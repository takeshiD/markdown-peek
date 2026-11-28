//! Adaptive reading aids over HTTP + WebSocket (design §3.7, R6).
//!
//! - `GET /api/adaptive` → the current file's layout **without asking a
//!   model**: the last layout for this revision if there is one, otherwise the
//!   decisions already in the cache (memory, then disk).
//! - `POST /api/adaptive/run` → start (or resume) choosing aids for the current
//!   file. Sections are asked a batch at a time; each batch is cached and its
//!   layout pushed (`{type:"adaptive", revision, layout}`), so a stopped run
//!   resumes with only what is still missing.
//! - `POST /api/adaptive/stop` → cancel the run (an in-flight request is
//!   abandoned) and stop following edits until the next run.
//! - `GET /api/models`, `POST /api/models/active`, `POST /api/models/default`
//!   → the models from the config, the one in use for this session, and
//!   saving a new default (`[model] use` in `config.toml`).
//! - On file change: debounce, then run again while the reader wants aids
//!   (unchanged sections come from the cache). Every change bumps a revision;
//!   a run whose revision is no longer current is cancelled and never pushed.
//!
//! Logs carry revision, model name, error code, counts and timings only —
//! never document text or a key.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use axum::extract::ws::Message;
use mdpeek_gui::select::{
    AdaptiveLayout, JobState, ModelSpec, Models, SelectConfig, SelectionCache, build_selector,
    run_selection, snapshot,
};
use serde_json::json;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

/// Everything the binary resolves from the config for reading aids.
pub struct AdaptiveSetup {
    pub cfg: SelectConfig,
    pub models: Models,
    /// Model to use at start instead of the default (`serve --model`).
    pub start_with: Option<String>,
    /// Problems with the config as a whole (shown in the UI).
    pub config_errors: Vec<String>,
    /// Where "make default" writes `[model] use`.
    pub config_path: Option<PathBuf>,
    /// `[genui] default = "on"`.
    pub aids_default_on: bool,
    /// `[genui] auto_generate`.
    pub auto_generate: bool,
    pub cache: SelectionCache,
}

pub struct Adaptive {
    cfg: SelectConfig,
    cache: SelectionCache,
    models: Models,
    default_model: RwLock<String>,
    active: RwLock<String>,
    config_errors: Vec<String>,
    config_path: Option<PathBuf>,
    aids_default_on: bool,
    auto_generate: bool,
    revision: AtomicU64,
    /// Last layout pushed: `(path, model name, layout)`.
    last: Mutex<Option<(PathBuf, String, AdaptiveLayout)>>,
    /// Held while a run is in progress (single-flight).
    inflight: Mutex<()>,
    /// Cancel flag of the current run.
    cancel: Mutex<Arc<AtomicBool>>,
    /// The reader wants aids for the current file (set by run, cleared by stop).
    wanted: AtomicBool,
    /// The reader stopped the run for the current revision.
    stopped: AtomicBool,
    running: AtomicBool,
    /// File of the last scheduled revision (to tell edits from switches).
    current: Mutex<Option<PathBuf>>,
}

impl Adaptive {
    pub fn new(s: AdaptiveSetup) -> Self {
        let default_model = s.models.default.clone();
        let active = s
            .start_with
            .filter(|n| s.models.get(n).is_some())
            .unwrap_or_else(|| default_model.clone());
        Adaptive {
            cfg: s.cfg,
            cache: s.cache,
            models: s.models,
            default_model: RwLock::new(default_model),
            active: RwLock::new(active),
            config_errors: s.config_errors,
            config_path: s.config_path,
            aids_default_on: s.aids_default_on,
            auto_generate: s.auto_generate,
            revision: AtomicU64::new(0),
            last: Mutex::new(None),
            inflight: Mutex::new(()),
            cancel: Mutex::new(Arc::new(AtomicBool::new(false))),
            wanted: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            running: AtomicBool::new(false),
            current: Mutex::new(None),
        }
    }

    pub fn current_revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    fn active_spec(&self) -> ModelSpec {
        let name = self.active.read().unwrap().clone();
        self.models
            .get(&name)
            .cloned()
            .unwrap_or_else(|| self.models.list[0].clone())
    }

    /// `GET /api/models` body. Problems never include a key value.
    pub fn models_info(&self) -> serde_json::Value {
        let models: Vec<_> = self
            .models
            .list
            .iter()
            .map(|m| {
                json!({
                    "name": m.name,
                    "kind": m.kind,
                    "api": m.api.as_str(),
                    "model": m.label(),
                    "problem": m.problem(),
                })
            })
            .collect();
        json!({
            "default": *self.default_model.read().unwrap(),
            "active": *self.active.read().unwrap(),
            "models": models,
            "configErrors": self.config_errors,
            "configPath": self.config_path.as_ref().map(|p| p.display().to_string()),
            "aids": { "defaultOn": self.aids_default_on, "autoGenerate": self.auto_generate },
            "cacheDir": self.cache.disk_dir().map(|p| p.display().to_string()),
        })
    }

    fn state_now(&self) -> JobState {
        if self.running.load(Ordering::SeqCst) {
            JobState::Running
        } else if self.stopped.load(Ordering::SeqCst) {
            JobState::Stopped
        } else {
            JobState::Idle
        }
    }

    /// The layout from the cache only (no model call).
    fn cached_layout(&self, path: &Path, rev: u64) -> std::io::Result<AdaptiveLayout> {
        let markdown = std::fs::read_to_string(path)?;
        let filename = path.file_name().and_then(|n| n.to_str());
        Ok(snapshot(
            &markdown,
            filename,
            &self.cfg,
            &self.active_spec(),
            &self.cache,
            rev,
            self.state_now(),
        ))
    }

    /// `GET /api/adaptive` body. Never calls a model.
    pub fn for_get(&self, path: &Path) -> std::io::Result<AdaptiveLayout> {
        let rev = self.current_revision();
        let model = self.active.read().unwrap().clone();
        if let Some((p, m, l)) = self.last.lock().unwrap().as_ref()
            && p == path
            && *m == model
            && l.revision == rev
        {
            return Ok(l.clone());
        }
        self.cached_layout(path, rev)
    }

    fn push(&self, path: &Path, layout: &AdaptiveLayout, tx: &broadcast::Sender<Message>) {
        let model = self.active.read().unwrap().clone();
        *self.last.lock().unwrap() = Some((path.to_path_buf(), model, layout.clone()));
        let msg = json!({ "type": "adaptive", "revision": layout.revision, "layout": layout });
        let _ = tx.send(Message::text(msg.to_string()));
    }

    fn push_models(&self, tx: &broadcast::Sender<Message>) {
        let msg = json!({ "type": "models", "models": self.models_info() });
        let _ = tx.send(Message::text(msg.to_string()));
    }

    /// Push the cache-only layout for the current revision (state changes).
    fn push_snapshot(&self, path: &Path, tx: &broadcast::Sender<Message>) {
        let rev = self.current_revision();
        match self.cached_layout(path, rev) {
            Ok(l) => self.push(path, &l, tx),
            Err(e) => warn!(
                "adaptive: cannot read file for revision {rev}: {}",
                e.kind()
            ),
        }
    }

    /// `POST /api/adaptive/run`: choose aids for `path` (resume if stopped).
    pub fn start(self: &Arc<Self>, path: PathBuf, tx: broadcast::Sender<Message>) {
        self.wanted.store(true, Ordering::SeqCst);
        self.stopped.store(false, Ordering::SeqCst);
        let rev = self.current_revision();
        let me = Arc::clone(self);
        std::thread::spawn(move || me.run(&path, rev, &tx, "start"));
    }

    /// `POST /api/adaptive/stop`.
    pub fn stop(&self, path: &Path, tx: &broadcast::Sender<Message>) {
        self.wanted.store(false, Ordering::SeqCst);
        self.stopped.store(true, Ordering::SeqCst);
        self.cancel.lock().unwrap().store(true, Ordering::SeqCst);
        if !self.running.load(Ordering::SeqCst) {
            self.push_snapshot(path, tx);
        }
        info!("adaptive: stopped by the reader");
    }

    /// `POST /api/models/active`: use `name` for this session.
    pub fn set_active(
        self: &Arc<Self>,
        name: &str,
        path: PathBuf,
        tx: broadcast::Sender<Message>,
    ) -> Result<(), String> {
        if self.models.get(name).is_none() {
            return Err(format!("no model named \"{name}\" in the config"));
        }
        *self.active.write().unwrap() = name.to_string();
        // Results of the previous model must not be shown under this one.
        self.cancel.lock().unwrap().store(true, Ordering::SeqCst);
        *self.last.lock().unwrap() = None;
        self.push_models(&tx);
        info!("adaptive: model {name} for this session");
        if self.wanted.load(Ordering::SeqCst) {
            self.stopped.store(false, Ordering::SeqCst);
            let rev = self.current_revision();
            let me = Arc::clone(self);
            std::thread::spawn(move || me.run(&path, rev, &tx, "model switch"));
        } else {
            self.push_snapshot(&path, &tx);
        }
        Ok(())
    }

    /// `POST /api/models/default`: save `name` as `[model] use`.
    pub fn set_default(&self, name: &str, tx: &broadcast::Sender<Message>) -> Result<(), String> {
        if self.models.get(name).is_none() {
            return Err(format!("no model named \"{name}\" in the config"));
        }
        let Some(path) = &self.config_path else {
            return Err(
                "no config file location (set XDG_CONFIG_HOME or HOME, or pass --config)".into(),
            );
        };
        write_default_model(path, name)?;
        *self.default_model.write().unwrap() = name.to_string();
        self.push_models(tx);
        info!("adaptive: default model saved as {name}");
        Ok(())
    }

    /// Run for `rev` under the single-flight lock; push progress and the result
    /// while `rev` is current.
    fn run(&self, path: &Path, rev: u64, tx: &broadcast::Sender<Message>, why: &str) {
        let _guard = self.inflight.lock().unwrap();
        if self.current_revision() != rev || !self.wanted.load(Ordering::SeqCst) {
            debug!("adaptive: revision {rev} superseded before {why} run");
            return;
        }
        let markdown = match std::fs::read_to_string(path) {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    "adaptive: cannot read file for revision {rev}: {}",
                    e.kind()
                );
                return;
            }
        };
        let cancel = Arc::new(AtomicBool::new(false));
        *self.cancel.lock().unwrap() = Arc::clone(&cancel);
        // A stop that landed before the new flag was installed.
        if !self.wanted.load(Ordering::SeqCst) {
            return;
        }
        let spec = self.active_spec();
        self.running.store(true, Ordering::SeqCst);
        // Say "running" right away so the reader sees the spinner.
        self.push_snapshot(path, tx);
        let filename = path.file_name().and_then(|n| n.to_str());
        let current =
            |me: &Self| me.current_revision() == rev && *me.active.read().unwrap() == spec.name;
        let layout = run_selection(
            &markdown,
            filename,
            &self.cfg,
            &spec,
            build_selector(&spec),
            &self.cache,
            rev,
            &cancel,
            &mut |l| {
                if current(self) {
                    self.push(path, l, tx);
                }
            },
        );
        self.running.store(false, Ordering::SeqCst);
        log_layout(&layout, why);
        if !current(self) {
            info!("adaptive: discarded stale revision {rev}");
            return;
        }
        self.push(path, &layout, tx);
    }

    /// A file changed (or another file was opened): bump the revision, cancel
    /// the running run, and after the debounce run again if the reader wants
    /// aids (a newly opened file starts only with `auto_generate`).
    pub fn schedule(self: &Arc<Self>, path: PathBuf, tx: broadcast::Sender<Message>) -> u64 {
        let rev = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        self.cancel.lock().unwrap().store(true, Ordering::SeqCst);
        let switched = {
            let mut cur = self.current.lock().unwrap();
            let s = cur.as_deref() != Some(path.as_path());
            *cur = Some(path.clone());
            s
        };
        if switched {
            self.stopped.store(false, Ordering::SeqCst);
            if !self.auto_generate {
                self.wanted.store(false, Ordering::SeqCst);
            }
        }
        let me = Arc::clone(self);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(me.cfg.debounce_ms));
            if me.current_revision() != rev {
                debug!("adaptive: revision {rev} superseded during debounce");
                return;
            }
            // No browser connected: nobody to show aids to. The next page load
            // starts a run itself.
            if tx.receiver_count() == 0 {
                debug!("adaptive: no client connected; deferring revision {rev}");
                return;
            }
            if me.wanted.load(Ordering::SeqCst) {
                me.run(&path, rev, &tx, "watch");
            } else {
                me.push_snapshot(&path, &tx);
            }
        });
        rev
    }
}

/// Set `[model] use = "<name>"`, keeping the rest of the file (comments,
/// order) as it is. Creates the file when it does not exist yet.
pub fn write_default_model(path: &Path, name: &str) -> Result<(), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("cannot read {}: {}", path.display(), e.kind())),
    };
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|_| format!("{} is not valid TOML; fix it first", path.display()))?;
    if !doc.contains_key("model") {
        doc["model"] = toml_edit::table();
    }
    let Some(t) = doc["model"].as_table_like_mut() else {
        return Err("[model] in the config is not a table".into());
    };
    t.insert("use", toml_edit::value(name));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {}", dir.display(), e.kind()))?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, doc.to_string())
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|e| format!("cannot write {}: {}", path.display(), e.kind()))
}

fn log_layout(l: &AdaptiveLayout, why: &str) {
    info!(
        "adaptive: {why} revision {} model {} state {:?} error {} placements {} skipped {} done {}/{} cache hits {} misses {} requests {} select {}ms",
        l.revision,
        l.selector.name,
        l.progress.state,
        l.selector.error.as_deref().unwrap_or("none"),
        l.placements.len(),
        l.skipped.len(),
        l.progress.done,
        l.progress.total,
        l.cache.hits,
        l.cache.misses,
        l.selector.requests,
        l.timing_ms.select
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_is_written_without_touching_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.toml");
        std::fs::write(
            &p,
            "# my settings\n[server]\nport = \"4000\"\n\n[model]\nuse = \"jev\" # recommended\ntimeout_ms = 9000\n",
        )
        .unwrap();
        write_default_model(&p, "sonnet").unwrap();
        let t = std::fs::read_to_string(&p).unwrap();
        assert!(
            t.starts_with("# my settings\n[server]\nport = \"4000\"\n"),
            "{t}"
        );
        assert!(t.contains("use = \"sonnet\""), "{t}");
        assert!(t.contains("timeout_ms = 9000"), "{t}");

        let fresh = dir.path().join("sub/config.toml");
        write_default_model(&fresh, "jev").unwrap();
        assert!(
            std::fs::read_to_string(&fresh)
                .unwrap()
                .contains("[model]\nuse = \"jev\"")
        );
    }
}
