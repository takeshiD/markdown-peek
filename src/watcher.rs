use notify_debouncer_mini::notify::RecursiveMode;
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use tracing::{error, info};

pub async fn rebuild_on_change(path: impl AsRef<Path>) {
    let path = path.as_ref().to_path_buf();
    let (tx, rx) = channel();
    let mut debouncer = match notify_debouncer_mini::new_debouncer(Duration::from_secs(1), tx) {
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
        let received = rx.recv().unwrap();
        match received {
            Ok(events) => {
                for event in events {
                    info!("{:#?}", event);
                }
            }
            Err(e) => {
                error!("{e}");
            }
        }
    }
}
