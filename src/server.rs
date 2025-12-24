use crate::watcher::{self, spawn_watcher};
use axum::{
    Router,
    extract::{State, WebSocket, WebSocketUpgrade},
    response::{
        Html,
        sse::{Event, Sse},
    },
    routing::get,
};
use core::fmt;
use futures::{StreamExt, stream::Stream};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use pulldown_cmark::{Options, Parser};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use std::{convert::Infallible, path::PathBuf};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::services::ServeDir;
use tracing::{error, info};

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<()>,
    file_path: Arc<RwLock<PathBuf>>,
    theme: Arc<RwLock<Theme>>,
}

#[derive(Clone)]
enum Theme {
    GitHubLight,
    GitHubDark,
}

impl fmt::Display for Theme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Theme::GitHubLight => "github-light",
            Theme::GitHubDark => "github-dark",
        };
        write!(f, "{s}")
    }
}

pub async fn serve(path: PathBuf) {
    let (tx, mut rx) = broadcast::channel(16);
    let tx_notify = tx.clone();
    let path_clone = path.clone();
    let watch_path = if path_clone.is_dir() {
        path_clone.clone()
    } else {
        path_clone
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    };
    let target_file = if path.is_file() {
        Some(path.clone())
    } else {
        None
    };
    tokio::task::spawn(async move {
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| match res {
                Ok(event) => {
                    if event.kind.is_modify() || event.kind.is_create() {
                        if let Some(ref file) = target_file {
                            if !event.paths.iter().any(|p| p == file) {
                                return;
                            }
                        }
                        info!("File changed: {:?}", event.paths);
                        let _ = tx_notify.send(());
                    }
                }
                Err(e) => error!("Watch error: {:?}", e),
            },
            Config::default(),
        )
        .unwrap();
        let _ = watcher.watch(&watch_path, RecursiveMode::Recursive);
        loop {
            while let Ok(event) = rx.recv().await {
                info!("Yeah: {:#?}", event);
            }
            info!("Yeah");
        }
    });
    let state = AppState {
        tx: tx.clone(),
        file_path: Arc::new(RwLock::new(path)),
        theme: Arc::new(RwLock::new(Theme::GitHubLight)),
    };
    let static_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
    let static_files_service = ServeDir::new(static_dir).append_index_html_on_directories(true);
    let websocket_handler =
        async move |ws: WebSocketUpgrade| ws.on_upgrade(move |socket| websocket_connection(ws));
    let app = Router::new()
        .route("/", get(file_handler))
        .route("/ws", get(websocket_handler))
        .fallback_service(static_files_service)
        .with_state(state);
    // let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let listener = TcpListener::bind(addr).await.unwrap();
    info!("Listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn file_handler(State(state): State<AppState>) -> Html<String> {
    let filepath = {
        let filepath_lock = state.file_path.read().unwrap();
        filepath_lock.clone()
    };
    let markdown_content = match tokio::fs::read_to_string(filepath.clone()).await {
        Ok(content) => {
            info!("Loaded '{}'", filepath.display());
            content
        }
        Err(e) => {
            error!("Failed to read file: {e}");
            "Failed to read file".to_string()
        }
    };
    let mut options = Options::empty();
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(&markdown_content, options);
    let mut html_body = String::new();
    pulldown_cmark::html::push_html(&mut html_body, parser);
    let template = include_str!("../static/index.html");
    let page = template
        .replace("{{theme}}", "github")
        .replace("{{ content }}", &html_body);
    Html(page)
}

async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(async |result| match result {
        Ok(()) => {
            info!("SSE reload event");
            Some(Ok(Event::default().data("relod")))
        }
        Err(err) => {
            error!("SSE broadcast error: {:?}", err);
            None
        }
    });
    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(1))
            .text("keep-alive"),
    )
}

async fn websocket_connection(ws: WebSocket) {}
