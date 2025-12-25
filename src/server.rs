use anyhow::Result;
use axum::{
    Router,
    extract::{
        State,
        ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
    },
    response::{Html, IntoResponse},
    routing::get,
};
use core::fmt;
use futures::{SinkExt, StreamExt};
use pulldown_cmark::{Options, Parser, Event, Tag};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tower_http::services::ServeDir;
use tracing::{error, info};

use crate::watcher::rebuild_on_change;
use crate::renderer::html;

#[derive(Debug, Clone)]
struct AppState {
    tx: broadcast::Sender<()>,
    file_path: Arc<RwLock<PathBuf>>,
    theme: Arc<RwLock<Theme>>,
}

#[derive(Debug, Clone)]
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

pub async fn serve(watch_path: PathBuf) {
    let (tx, _) = broadcast::channel(16);
    let tx_reload = tx.clone();
    let server = tokio::spawn(run_server(watch_path.clone(), tx_reload));
    let watcher = tokio::spawn(rebuild_on_change(watch_path));
    tokio::select! {
        r = server => {
            eprintln!("server finished: {:?}", r);
        }
        r = watcher => {
            eprintln!("watcher finished: {:?}", r);
        }
    }
}

async fn run_server(file_path: impl AsRef<Path>, tx_reload: broadcast::Sender<()>) -> Result<()> {
    let state = AppState {
        tx: tx_reload,
        file_path: Arc::new(RwLock::new(file_path.as_ref().to_path_buf())),
        theme: Arc::new(RwLock::new(Theme::GitHubLight)),
    };
    let static_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
    let static_files_service = ServeDir::new(static_dir).append_index_html_on_directories(true);
    let app = Router::new()
        .route("/", get(file_handler))
        .route("/ws", get(websocket_handler))
        .nest_service("/static", static_files_service)
        .with_state(state);
    // let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(addr).await.unwrap();
    info!("Listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
    info!("End of serve");
    Ok(())
}

async fn file_handler(State(state): State<AppState>) -> impl IntoResponse {
    let file_path = {
        let file_path_guard = state.file_path.read().unwrap();
        file_path_guard.to_path_buf()
    };
    let markdown_content = match tokio::fs::read_to_string(file_path.clone()).await {
        Ok(content) => {
            info!("Loaded '{}'", file_path.display());
            content
        }
        Err(e) => {
            error!("Failed to read file '{}': {}", file_path.display(), e);
            "Failed to read file".to_string()
        }
    };
    let mut options = Options::empty();
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(&markdown_content, options);
    let events: Vec<Event> = parser.map(|e| e).collect();
    println!("{:#?}", events);
    let mut html_body = String::new();
    // pulldown_cmark::html::push_html(&mut html_body, parser);
    let template = include_str!("../static/index.html");
    let theme = state.theme.read().unwrap().to_string();
    let page = template
        .replace("{{theme}}", &theme)
        .replace("{{ content }}", &html_body);
    Html(page)
}

async fn websocket_handler(
    ws_upgrade: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let tx = state.tx.clone();
    ws_upgrade.on_upgrade(move |socket| websocket_connection(socket, tx))
}

async fn websocket_connection(ws: WebSocket, tx_reload: broadcast::Sender<()>) {
    let (mut tx_ws, _) = ws.split();
    let mut rx_reload = tx_reload.subscribe();
    info!("websocket got connection");
    if let Ok(_m) = rx_reload.recv().await {
        info!("notify of reload");
        let _ = tx_ws
            .send(Message::Text(Utf8Bytes::from_static("reload")))
            .await;
    }
}
