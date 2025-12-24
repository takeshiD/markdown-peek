use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use tokio::sync::broadcast;
use tracing::{error, info};

pub async fn spawn_watcher(
    path: impl AsRef<Path>,
    tx: broadcast::Sender<()>,
) -> notify::Result<RecommendedWatcher> {
    let path = path.as_ref().to_path_buf();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| match res {
            Ok(event) => {
                if event.kind.is_modify() || event.kind.is_create() {
                    info!("File changed: {:?}", event.paths);
                    let _ = tx.send(());
                }
            }
            Err(e) => error!("Watch error: {:?}", e),
        },
        Config::default(),
    )?;
    watcher.watch(&path, RecursiveMode::Recursive)?;
    info!("Watching: {:?}", path);
    Ok(watcher)
}
