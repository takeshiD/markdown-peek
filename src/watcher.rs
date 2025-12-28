use notify::{self, RecursiveMode};
use notify_debouncer_mini::{Config, new_debouncer_opt};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use tracing::{debug, error, info};

pub fn rebuild_on_change(path: impl AsRef<Path>, callback: impl Fn()) {
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
