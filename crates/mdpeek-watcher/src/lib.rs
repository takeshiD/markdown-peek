use notify::{self, RecursiveMode};
use notify_debouncer_mini::{Config, new_debouncer_opt};
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
