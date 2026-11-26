//! Adaptive reading-aid layout over HTTP + WebSocket (design §3.7, R6).
//!
//! - `GET /api/adaptive` → the current file's `AdaptiveLayout`, **without
//!   waiting on the network**: the last computed layout for the current
//!   revision if there is one, otherwise (for a networked selector) an
//!   immediate rules layout marked `selector.fallback = "pending"` while the
//!   real selection runs in the background and is pushed over the WebSocket.
//! - On file change: debounce (`[select] debounce_ms`, default 800 ms), select,
//!   then broadcast `{type:"adaptive", revision, layout}`.
//! - Every change bumps a revision; a selection whose revision is no longer
//!   current when it finishes is discarded (never pushed).
//! - Single-flight: at most one selection runs at a time, so rapid saves do
//!   not stack overlapping external requests (and 429s).
//! - A networked selector is not called while no browser is connected.
//! - Decisions are cached **in memory only** (`SelectionCache`); unchanged
//!   sections reuse their decision. Nothing is written to disk.
//!
//! Logs carry revision, provider, fallback code, counts and timings only —
//! never document text or the API key.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::Message;
use mdpeek_gui::select::{
    AdaptiveLayout, Provider, SelectConfig, SelectionCache, adaptive_layout,
};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

pub struct Adaptive {
    pub cfg: SelectConfig,
    pub cache: SelectionCache,
    revision: AtomicU64,
    /// Last layout computed by the configured selector: `(path, layout)`.
    last: Mutex<Option<(PathBuf, AdaptiveLayout)>>,
    /// Held while a selection runs (single-flight).
    inflight: Mutex<()>,
}

impl Adaptive {
    pub fn new(cfg: SelectConfig) -> Self {
        let cache = SelectionCache::new(cfg.cache_entries);
        Adaptive {
            cfg,
            cache,
            revision: AtomicU64::new(0),
            last: Mutex::new(None),
            inflight: Mutex::new(()),
        }
    }

    pub fn current_revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    fn networked(&self) -> bool {
        self.cfg.provider == Provider::Jev
    }

    /// Compute the layout for `path` at revision `rev` (blocking; may call Jev).
    pub fn compute(&self, path: &Path, rev: u64) -> std::io::Result<AdaptiveLayout> {
        let markdown = std::fs::read_to_string(path)?;
        let filename = path.file_name().and_then(|n| n.to_str());
        let layout = adaptive_layout(&markdown, filename, &self.cfg, &self.cache, rev);
        log_layout("computed", &layout);
        Ok(layout)
    }

    /// Rules-only layout (no network, sub-millisecond), labelled as pending
    /// the configured selector.
    fn pending_layout(&self, path: &Path, rev: u64) -> std::io::Result<AdaptiveLayout> {
        let markdown = std::fs::read_to_string(path)?;
        let filename = path.file_name().and_then(|n| n.to_str());
        let rules = SelectConfig {
            provider: Provider::Rules,
            ..self.cfg.clone()
        };
        let mut layout = adaptive_layout(&markdown, filename, &rules, &self.cache, rev);
        layout.selector.requested = self.cfg.provider.as_str().to_string();
        layout.selector.fallback = Some("pending".into());
        Ok(layout)
    }

    fn remembered(&self, path: &Path, rev: u64) -> Option<AdaptiveLayout> {
        let last = self.last.lock().unwrap();
        match last.as_ref() {
            Some((p, l)) if p == path && l.revision == rev => Some(l.clone()),
            _ => None,
        }
    }

    fn remember(&self, path: &Path, layout: &AdaptiveLayout) {
        *self.last.lock().unwrap() = Some((path.to_path_buf(), layout.clone()));
    }

    /// `GET /api/adaptive` body. Never blocks on the network.
    pub fn for_get(
        self: &Arc<Self>,
        path: &Path,
        tx: &broadcast::Sender<Message>,
    ) -> std::io::Result<AdaptiveLayout> {
        let rev = self.current_revision();
        // Local selectors (rules/fake) are cheap and deterministic: compute
        // fresh so cache statistics reflect this request.
        if !self.networked() {
            return self.compute(path, rev);
        }
        if let Some(l) = self.remembered(path, rev) {
            return Ok(l);
        }
        let pending = self.pending_layout(path, rev)?;
        // Kick the real selection (single-flight) and push it when done.
        let me = Arc::clone(self);
        let path = path.to_path_buf();
        let tx = tx.clone();
        std::thread::spawn(move || me.run(&path, rev, &tx, "on-demand"));
        Ok(pending)
    }

    /// Select for `rev` under the single-flight lock and push if still current.
    fn run(&self, path: &Path, rev: u64, tx: &broadcast::Sender<Message>, why: &str) {
        let _guard = self.inflight.lock().unwrap();
        if self.current_revision() != rev {
            debug!("adaptive: revision {rev} superseded before {why} selection");
            return;
        }
        let reuse = if self.networked() { self.remembered(path, rev) } else { None };
        let layout = match reuse {
            Some(l) => l,
            None => match self.compute(path, rev) {
                Ok(l) => l,
                Err(e) => {
                    warn!("adaptive: cannot read file for revision {rev}: {}", e.kind());
                    return;
                }
            },
        };
        let now = self.current_revision();
        if now != rev {
            info!("adaptive: discarded stale revision {rev} (current {now})");
            return;
        }
        self.remember(path, &layout);
        let msg = serde_json::json!({
            "type": "adaptive",
            "revision": rev,
            "layout": layout,
        })
        .to_string();
        let _ = tx.send(Message::text(msg));
        info!("adaptive: pushed revision {rev} ({why})");
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
            // No browser connected: do not spend an external request. The next
            // GET /api/adaptive selects on demand.
            if me.networked() && tx.receiver_count() == 0 {
                debug!("adaptive: no client connected; deferring revision {rev}");
                return;
            }
            me.run(&path, rev, &tx, "watch");
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
