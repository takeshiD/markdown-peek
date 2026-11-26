//! Adaptive reading-aid layout over HTTP + WebSocket (design §3.7, R6).
//!
//! - `GET /api/adaptive` → the current file's `AdaptiveLayout`.
//! - On file change: debounce (`[select] debounce_ms`, default 800 ms), select,
//!   then broadcast `{type:"adaptive", revision, layout}`.
//! - Every change bumps a revision; a selection whose revision is no longer
//!   current when it finishes is discarded (never pushed).
//! - Decisions are cached **in memory only** (`SelectionCache`); unchanged
//!   sections reuse their decision. Nothing is written to disk.
//!
//! Logs carry revision, provider, fallback code, counts and timings only —
//! never document text or the API key.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::extract::ws::Message;
use mdpeek_gui::select::{AdaptiveLayout, SelectConfig, SelectionCache, adaptive_layout};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

pub struct Adaptive {
    pub cfg: SelectConfig,
    pub cache: SelectionCache,
    revision: AtomicU64,
}

impl Adaptive {
    pub fn new(cfg: SelectConfig) -> Self {
        let cache = SelectionCache::new(cfg.cache_entries);
        Adaptive {
            cfg,
            cache,
            revision: AtomicU64::new(0),
        }
    }

    pub fn current_revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    /// Compute the layout for `path` at revision `rev` (blocking; may call Jev).
    pub fn compute(&self, path: &Path, rev: u64) -> std::io::Result<AdaptiveLayout> {
        let markdown = std::fs::read_to_string(path)?;
        let filename = path.file_name().and_then(|n| n.to_str());
        let layout = adaptive_layout(&markdown, filename, &self.cfg, &self.cache, rev);
        log_layout("computed", &layout);
        Ok(layout)
    }

    /// A file changed (or a new file was selected): bump the revision and, after
    /// the debounce, select and push — unless a newer revision arrived.
    pub fn schedule(self: &Arc<Self>, path: PathBuf, tx: broadcast::Sender<Message>) {
        let rev = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        let me = Arc::clone(self);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(me.cfg.debounce_ms));
            if me.current_revision() != rev {
                debug!("adaptive: revision {rev} superseded during debounce");
                return;
            }
            let layout = match me.compute(&path, rev) {
                Ok(l) => l,
                Err(e) => {
                    warn!("adaptive: cannot read file for revision {rev}: {}", e.kind());
                    return;
                }
            };
            let now = me.current_revision();
            if now != rev {
                info!("adaptive: discarded stale revision {rev} (current {now})");
                return;
            }
            let msg = serde_json::json!({
                "type": "adaptive",
                "revision": rev,
                "layout": layout,
            })
            .to_string();
            let _ = tx.send(Message::text(msg));
            info!("adaptive: pushed revision {rev}");
        });
    }
}

fn log_layout(what: &str, l: &AdaptiveLayout) {
    info!(
        "adaptive: {what} revision {} provider {} (requested {}) fallback {} placements {} skipped {} cache hits {} misses {} requests {} select {}ms",
        l.revision,
        l.selector.provider,
        l.selector.requested,
        l.selector.fallback.as_deref().unwrap_or("none"),
        l.placements.len(),
        l.skipped.len(),
        l.cache.hits,
        l.cache.misses,
        l.selector.requests,
        l.timing_ms.select
    );
}
