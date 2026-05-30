use anyhow::Result;
use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::{Html, IntoResponse},
    routing::get,
};
use core::fmt;
use futures::{SinkExt, StreamExt};
use pulldown_cmark::{Options, Parser};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tower_http::services::ServeDir;
use tracing::{debug, error, info, warn};

use crate::emitter::HtmlEmitter;
use crate::watcher::notify_on_change;

#[derive(Debug, Clone)]
struct AppState {
    tx: broadcast::Sender<Message>,
    file_path: Arc<RwLock<PathBuf>>,
    theme: Arc<RwLock<Theme>>,
}

#[allow(dead_code)]
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

pub fn serve(watch_path: PathBuf, host: String, port: String) {
    let (tx, _) = broadcast::channel::<Message>(16);
    let tx_reload = tx.clone();
    let watch_path_clone = watch_path.clone();
    let server = std::thread::spawn(move || run_server(watch_path, tx_reload, host, port));
    let _: () = notify_on_change(watch_path_clone, move || {
        debug!("Callback Start");
        let result = tx.send(Message::text("reload"));
        debug!("Callback End!: {:#?}", result);
    });
    let _ = server.join();
}

#[tokio::main()]
async fn run_server(
    file_path: impl AsRef<Path>,
    tx_reload: broadcast::Sender<Message>,
    host: String,
    port: String,
) -> Result<()> {
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
    let listener = match TcpListener::bind(format!("{host}:{port}")).await {
        Ok(listner) => listner,
        Err(_) => {
            warn!("Address '{host}:{port}' already in use.");
            TcpListener::bind(format!("{host}:0"))
                .await
                .expect("failed to bind '{host}:0'")
        }
    };
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
            // Return early with an HTML error page rather than rendering bad markdown
            let error_html = include_str!("../static/index.html")
                .replace("{{theme}}", "")
                .replace("{{ title }}", "Error")
                .replace(
                    "{{ content }}",
                    &format!(
                        "<h1>Error</h1><p>Failed to read file <code>{}</code>: {}</p>",
                        file_path.display(),
                        e
                    ),
                );
            return Html(error_html);
        }
    };
    let mut options = Options::empty();
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_MATH);
    let parser = Parser::new_ext(&markdown_content, options);
    let parser = parser.inspect(|event| {
        debug!("{:#?}", event);
    });
    let mut emitter = HtmlEmitter::new(parser);
    let html_body = emitter.run();
    let template = include_str!("../static/index.html");
    let theme = state.theme.read().unwrap().to_string();
    let title = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_else(|| file_path.to_str().unwrap_or("markdown-peek"));
    let page = template
        .replace("{{theme}}", &theme)
        .replace("{{ title }}", title)
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

async fn websocket_connection(ws: WebSocket, tx_reload: broadcast::Sender<Message>) {
    let (mut tx_ws, mut rx_ws) = ws.split();
    let mut rx_reload = tx_reload.subscribe();
    debug!("Websocket got connection");
    loop {
        tokio::select! {
            result = rx_reload.recv() => {
                match result {
                    Ok(m) => {
                        debug!("Sending reload to client");
                        match tx_ws.send(m).await {
                            Ok(_) => {
                                debug!("Success reload");
                            }
                            Err(e) => {
                                error!("Error sending reload: {}", e);
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("FileWatcher broadcast lagged, skipped {} messages", n);
                        // Continue: skip lagged messages and keep the connection alive
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!("FileWatcher broadcast channel closed, closing WebSocket");
                        break;
                    }
                }
            }
            client_msg = rx_ws.next() => {
                match client_msg {
                    Some(Ok(Message::Close(_))) | None => {
                        debug!("Client closed WebSocket connection");
                        break;
                    }
                    Some(Ok(_)) => {
                        // Ignore other client messages
                    }
                    Some(Err(e)) => {
                        error!("WebSocket receive error: {}", e);
                        break;
                    }
                }
            }
        }
    }
}
