//! One JSON POST with an overall deadline and a cancel flag, shared by every
//! HTTP model. 429/529 are retried at most once, honouring `retry-after` only
//! when it fits inside the deadline. The key travels in one header and is never
//! logged; errors carry status codes only.

use std::sync::atomic::{AtomicBool, AtomicU32};
use std::time::Instant;

use serde_json::Value;

use super::SelectError;

#[cfg_attr(not(feature = "llm"), allow(dead_code))]
pub struct Post<'a> {
    pub url: &'a str,
    /// `(header name, value)`; the auth header is built by the caller.
    pub headers: Vec<(&'static str, String)>,
    pub body: &'a Value,
    pub deadline: Instant,
    pub cancel: &'a AtomicBool,
    /// Requests actually sent (incl. a retry).
    pub sent: &'a AtomicU32,
}

#[cfg(feature = "llm")]
pub fn post(p: Post<'_>) -> Result<(u16, u32, Value), SelectError> {
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| SelectError::Network)?;
    rt.block_on(async {
        let remaining = p.deadline.saturating_duration_since(Instant::now());
        let cancel = p.cancel;
        let watch = async move {
            loop {
                if cancel.load(Ordering::SeqCst) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        };
        tokio::select! {
            r = tokio::time::timeout(remaining, post_async(&p)) => match r {
                Ok(r) => r,
                Err(_) => Err(SelectError::Timeout),
            },
            _ = watch => Err(SelectError::Cancelled),
        }
    })
}

#[cfg(feature = "llm")]
async fn post_async(p: &Post<'_>) -> Result<(u16, u32, Value), SelectError> {
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let client = reqwest::Client::builder()
        .user_agent("mdpeek-select")
        .build()
        .map_err(|_| SelectError::Network)?;
    let net = |e: reqwest::Error| {
        if e.is_timeout() {
            SelectError::Timeout
        } else {
            SelectError::Network
        }
    };
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        p.sent.fetch_add(1, Ordering::SeqCst);
        let mut req = client.post(p.url).json(p.body);
        for (k, v) in &p.headers {
            req = req.header(*k, v);
        }
        let resp = req.send().await.map_err(net)?;
        let status = resp.status().as_u16();
        match status {
            200..=299 => {
                let bytes = resp.bytes().await.map_err(net)?;
                let v: Value =
                    serde_json::from_slice(&bytes).map_err(|_| SelectError::InvalidResponse)?;
                return Ok((status, attempt, v));
            }
            401 | 403 => return Err(SelectError::Unauthorized),
            402 => return Err(SelectError::PaymentRequired),
            404 => return Err(SelectError::NotFound),
            429 | 529 => {
                let err = if status == 429 {
                    SelectError::RateLimited
                } else {
                    SelectError::Overloaded
                };
                if attempt >= 2 {
                    return Err(err);
                }
                let secs = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .filter(|s| s.is_finite() && *s >= 0.0)
                    .unwrap_or(0.5);
                // Compare in f64 *before* building a Duration, so a malformed
                // or hostile header (e.g. 1e300) can neither panic nor
                // overflow, and we never wait past our own deadline.
                let remaining = p.deadline.saturating_duration_since(Instant::now());
                if secs >= remaining.as_secs_f64() {
                    return Err(err);
                }
                let Ok(wait) = Duration::try_from_secs_f64(secs) else {
                    return Err(err);
                };
                tokio::time::sleep(wait).await;
            }
            500..=599 => return Err(SelectError::Server(status)),
            _ => return Err(SelectError::Http(status)),
        }
    }
}

#[cfg(not(feature = "llm"))]
pub fn post(_p: Post<'_>) -> Result<(u16, u32, Value), SelectError> {
    Err(SelectError::NotBuilt)
}
