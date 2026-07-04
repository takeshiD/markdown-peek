use anyhow::Result;
use axum::{
    Router,
    body::Body,
    extract::{
        Path as AxumPath, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use core::fmt;
use futures::{SinkExt, StreamExt};
use pulldown_cmark::Parser;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use mdpeek_render_html::HtmlEmitter;
use mdpeek_watcher::notify_on_change;

#[derive(Debug, Clone)]
struct AppState {
    tx: broadcast::Sender<Message>,
    file_path: Arc<RwLock<PathBuf>>,
    theme: Arc<RwLock<Theme>>,
}

/// Browser colour theme selected for the served page. The caller (the `mdpeek`
/// binary) maps its own config theme onto this so the server crate stays
/// independent of the binary's config types.
#[derive(Debug, Clone, Copy)]
pub enum Theme {
    Light,
    Dark,
}
impl fmt::Display for Theme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Theme::Light => "github-light",
            Theme::Dark => "github-dark",
        };
        write!(f, "{s}")
    }
}

pub fn serve(watch_path: PathBuf, host: String, port: String, theme: Theme) {
    let (tx, _) = broadcast::channel::<Message>(16);
    let tx_reload = tx.clone();
    let watch_path_clone = watch_path.clone();
    let server = std::thread::spawn(move || run_server(watch_path, tx_reload, host, port, theme));
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
    theme: Theme,
) -> Result<()> {
    let state = AppState {
        tx: tx_reload,
        file_path: Arc::new(RwLock::new(file_path.as_ref().to_path_buf())),
        theme: Arc::new(RwLock::new(theme)),
    };
    let app = Router::new()
        .route("/", get(file_handler))
        .route("/ws", get(websocket_handler))
        .route("/static/{*path}", get(static_handler))
        .with_state(state);
    let listener = match TcpListener::bind(format!("{host}:{port}")).await {
        Ok(listner) => listner,
        Err(_) => {
            warn!("Address '{host}:{port}' already in use.");
            TcpListener::bind(format!("{host}:0"))
                .await
                .unwrap_or_else(|e| panic!("failed to bind '{host}:0': {e}"))
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
            let error_html = include_str!("../../../static/index.html")
                .replace("{{theme}}", "")
                .replace("{{ title }}", "Error")
                .replace(
                    "{{ content }}",
                    &format!(
                        "<h1>Error</h1><p>Failed to read file <code>{}</code>: {}</p>",
                        file_path.display(),
                        e
                    ),
                )
                .replace("{{ frontmatter }}", "");
            return Html(error_html);
        }
    };
    let parser = Parser::new_ext(&markdown_content, mdpeek_gfm::parser_options());
    let parser = mdpeek_gfm::transform(parser);
    let parser = parser.inspect(|event| {
        debug!("{:#?}", event);
    });
    let mut emitter = HtmlEmitter::new(parser);
    let html_body = emitter.run();

    // Front matter panel (#19): surface the leading YAML/+++ block, which the
    // renderer otherwise hides. Escaped and stashed in a hidden element for the
    // client to display.
    let frontmatter_html =
        match mdpeek_parser::BlockTree::parse(&markdown_content).frontmatter() {
            Some(fm) if !fm.trim().is_empty() => {
                format!(
                    "<div id=\"mdpeek-frontmatter\" hidden>{}</div>",
                    escape_html_min(fm)
                )
            }
            _ => String::new(),
        };

    let template = include_str!("../../../static/index.html");
    let theme = state.theme.read().unwrap().to_string();
    let title = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_else(|| file_path.to_str().unwrap_or("markdown-peek"));
    let page = template
        .replace("{{theme}}", &theme)
        .replace("{{ title }}", title)
        .replace("{{ content }}", &html_body)
        .replace("{{ frontmatter }}", &frontmatter_html);
    Html(page)
}

/// Minimal HTML-body escaping (`&`, `<`, `>`) so arbitrary front matter text
/// can be embedded in a hidden element without breaking out of it. Newlines are
/// preserved for the client's front matter panel.
fn escape_html_min(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

struct StaticAsset {
    bytes: &'static [u8],
    content_type: &'static str,
}

fn embedded_static_asset(path: &str) -> Option<StaticAsset> {
    match path.trim_start_matches('/') {
        "css/github-dark.css" => Some(StaticAsset {
            bytes: include_bytes!("../../../static/css/github-dark.css"),
            content_type: "text/css; charset=utf-8",
        }),
        "css/github-light.css" => Some(StaticAsset {
            bytes: include_bytes!("../../../static/css/github-light.css"),
            content_type: "text/css; charset=utf-8",
        }),
        "icons/github-mark.svg" => Some(StaticAsset {
            bytes: include_bytes!("../../../static/icons/github-mark.svg"),
            content_type: "image/svg+xml",
        }),
        "js/highlight/github-dark.min.css" => Some(StaticAsset {
            bytes: include_bytes!("../../../static/js/highlight/github-dark.min.css"),
            content_type: "text/css; charset=utf-8",
        }),
        "js/highlight/github.min.css" => Some(StaticAsset {
            bytes: include_bytes!("../../../static/js/highlight/github.min.css"),
            content_type: "text/css; charset=utf-8",
        }),
        "js/highlight/highlight.min.js" => Some(StaticAsset {
            bytes: include_bytes!("../../../static/js/highlight/highlight.min.js"),
            content_type: "application/javascript; charset=utf-8",
        }),
        "js/main.js" => Some(StaticAsset {
            bytes: include_bytes!("../../../static/js/main.js"),
            content_type: "application/javascript; charset=utf-8",
        }),
        "js/mermaid/mermaid.min.js" => Some(StaticAsset {
            bytes: include_bytes!("../../../static/js/mermaid/mermaid.min.js"),
            content_type: "application/javascript; charset=utf-8",
        }),
        _ => None,
    }
}

async fn static_handler(AxumPath(path): AxumPath<String>) -> Response {
    match embedded_static_asset(&path) {
        Some(asset) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, asset.content_type)
            .body(Body::from(asset.bytes))
            .expect("static asset response should be valid"),
        None => StatusCode::NOT_FOUND.into_response(),
    }
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

#[cfg(test)]
mod tests {
    use super::embedded_static_asset;

    #[test]
    fn static_assets_are_embedded() {
        for path in [
            "css/github-light.css",
            "css/github-dark.css",
            "icons/github-mark.svg",
            "js/highlight/github.min.css",
            "js/highlight/github-dark.min.css",
            "js/highlight/highlight.min.js",
            "js/main.js",
            "js/mermaid/mermaid.min.js",
        ] {
            let asset = embedded_static_asset(path).expect(path);
            assert!(!asset.bytes.is_empty(), "{path} should not be empty");
        }
    }

    #[test]
    fn unknown_static_asset_is_not_found() {
        assert!(embedded_static_asset("missing.css").is_none());
    }

    #[test]
    fn escape_html_min_neutralizes_markup() {
        use super::escape_html_min;
        let escaped = escape_html_min("a </div><script> & b");
        assert!(!escaped.contains('<'));
        assert!(!escaped.contains('>'));
        assert_eq!(escaped, "a &lt;/div&gt;&lt;script&gt; &amp; b");
    }

    #[test]
    fn escape_html_min_preserves_newlines() {
        assert_eq!(super::escape_html_min("k: v\nk2: v2"), "k: v\nk2: v2");
    }
}
