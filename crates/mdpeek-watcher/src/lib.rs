use notify::poll::PollWatcher;
use notify::{self, RecursiveMode};
use notify_debouncer_mini::{Config, DebounceEventResult, Debouncer, new_debouncer_opt};
use std::path::Path;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;
use tracing::{debug, error, info};

pub fn notify_on_change(path: impl AsRef<Path>, callback: impl Fn()) {
    let path = path.as_ref().to_path_buf();
    let (tx, rx) = channel();
    let backend_config = notify::Config::default().with_poll_interval(Duration::from_secs(1));
    let debouncer_config = Config::default().with_notify_config(backend_config);
    let mut debouncer =
        match new_debouncer_opt::<_, notify::poll::PollWatcher>(debouncer_config, tx) {
            Ok(d) => d,
            Err(e) => {
                error!("Error while trying to watch the files:\n\n\t{:?}", e);
                std::process::exit(1)
            }
        };
    let watcher = debouncer.watcher();
    let _ = watcher.watch(&path, RecursiveMode::NonRecursive);
    info!("Watching: {:?}", path);
    loop {
        debug!("Waiting Receive...");
        let received = rx.recv().unwrap();
        debug!("Received!!");
        match received {
            Ok(events) => {
                for event in events {
                    debug!("{:#?}", event);
                    callback();
                }
            }
            Err(e) => {
                debug!("{e}");
            }
        }
    }
}

/// A controllable debounced watcher whose set of watched paths can change at
/// runtime. Change batches are coalesced into `()` signals on the paired
/// [`Receiver`]. Keep the handle alive for as long as you want events — dropping
/// it stops the watcher and closes the receiver.
///
/// Used by the server's explorer mode (#14) to re-point the watcher at whichever
/// file the user selects, and to watch worktree roots for tree changes.
pub struct WatchHandle {
    debouncer: Debouncer<PollWatcher>,
}

impl WatchHandle {
    /// Start watching a single file (non-recursive).
    pub fn watch(&mut self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        if let Err(e) = self
            .debouncer
            .watcher()
            .watch(path, RecursiveMode::NonRecursive)
        {
            debug!("watch({path:?}) failed: {e}");
        } else {
            info!("Watching: {path:?}");
        }
    }

    /// Start watching a directory tree recursively (e.g. a worktree root).
    pub fn watch_recursive(&mut self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        if let Err(e) = self
            .debouncer
            .watcher()
            .watch(path, RecursiveMode::Recursive)
        {
            debug!("watch_recursive({path:?}) failed: {e}");
        }
    }

    /// Stop watching a path previously passed to [`watch`](Self::watch).
    pub fn unwatch(&mut self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        if let Err(e) = self.debouncer.watcher().unwatch(path) {
            debug!("unwatch({path:?}) failed: {e}");
        }
    }
}

/// Create a controllable watcher plus a `()` change-signal receiver. No paths
/// are watched initially; call [`WatchHandle::watch`] to add them. Each batch of
/// debounced changes yields a single `()` on the receiver.
pub fn watch_channel() -> (WatchHandle, Receiver<()>) {
    let (raw_tx, raw_rx) = channel::<DebounceEventResult>();
    let backend_config = notify::Config::default().with_poll_interval(Duration::from_secs(1));
    let debouncer_config = Config::default().with_notify_config(backend_config);
    let debouncer = match new_debouncer_opt::<_, PollWatcher>(debouncer_config, raw_tx) {
        Ok(d) => d,
        Err(e) => {
            error!("Error while creating file watcher:\n\n\t{:?}", e);
            std::process::exit(1);
        }
    };
    let (out_tx, out_rx) = channel::<()>();
    // Forward debouncer batches as coalesced `()` signals. The debouncer (and so
    // `raw_tx`) is owned by the returned handle, so this loop ends when the
    // handle is dropped.
    std::thread::spawn(move || {
        for res in raw_rx {
            match res {
                Ok(events) if !events.is_empty() => {
                    if out_tx.send(()).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(e) => debug!("watch error: {e}"),
            }
        }
    });
    (WatchHandle { debouncer }, out_rx)
}

/// Spawn a debounced file watcher on a background thread and return a receiver
/// that yields a `()` for each batch of detected changes.
///
/// Unlike [`notify_on_change`], this is non-blocking: it hands back a
/// [`Receiver`] so the caller can `select`/`try_recv` on change events
/// alongside other event sources (e.g. terminal input in the TUI). The
/// debouncer is kept alive by moving it into the spawned thread, which loops
/// forwarding events until the receiver is dropped.
pub fn watch_events(path: impl AsRef<Path>) -> Receiver<()> {
    let path = path.as_ref().to_path_buf();
    // Channel delivering the caller-facing `()` change signals.
    let (out_tx, out_rx) = channel::<()>();
    std::thread::spawn(move || {
        // Internal channel from the debouncer to this thread.
        let (tx, rx) = channel();
        let backend_config = notify::Config::default().with_poll_interval(Duration::from_secs(1));
        let debouncer_config = Config::default().with_notify_config(backend_config);
        let mut debouncer =
            match new_debouncer_opt::<_, notify::poll::PollWatcher>(debouncer_config, tx) {
                Ok(d) => d,
                Err(e) => {
                    error!("Error while trying to watch the files:\n\n\t{:?}", e);
                    return;
                }
            };
        let watcher = debouncer.watcher();
        let _ = watcher.watch(&path, RecursiveMode::NonRecursive);
        info!("Watching (events): {:?}", path);
        // `debouncer` is kept alive for the lifetime of this loop.
        loop {
            match rx.recv() {
                Ok(Ok(events)) => {
                    for event in events {
                        debug!("{:#?}", event);
                    }
                    // Coalesce a batch into a single change signal. Stop the
                    // thread once the consumer has gone away.
                    if out_tx.send(()).is_err() {
                        break;
                    }
                }
                Ok(Err(e)) => {
                    debug!("{e}");
                }
                // Debouncer dropped its sender: nothing more to watch.
                Err(e) => {
                    debug!("watch channel closed: {e}");
                    break;
                }
            }
        }
    });
    out_rx
}
