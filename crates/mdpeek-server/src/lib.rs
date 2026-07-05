mod explorer;

use anyhow::Result;
use axum::{
    Json, Router,
    body::Body,
    extract::{
        Path as AxumPath, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use core::fmt;
use futures::{SinkExt, StreamExt};
use pulldown_cmark::Parser;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender as StdSender;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use mdpeek_render_html::HtmlEmitter;
use mdpeek_watcher::watch_channel;

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<Message>,
    file_path: Arc<RwLock<PathBuf>>,
    theme: Arc<RwLock<Theme>>,
    /// Canonical roots a selected file must live under (#14 path safety).
    roots: Arc<Vec<PathBuf>>,
    /// Directory discovery starts from when (re)building the explorer tree.
    scan_root: Arc<PathBuf>,
    /// Tells the watch loop to re-point at a newly selected file.
    rewatch: StdSender<PathBuf>,
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
    // Discover the enclosing repo/worktrees from where the user ran mdpeek so
    // the explorer sidebar (#14) works regardless of the file argument.
    let scan_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let roots = explorer::allowed_roots(&scan_root);
    // Pick the initial file: the given one if readable, else the first markdown
    // discovered under the repo, else fall back to the requested path.
    let active = explorer::initial_active(&watch_path, &scan_root).unwrap_or(watch_path);

    let (tx, _) = broadcast::channel::<Message>(16);
    let file_path = Arc::new(RwLock::new(active.clone()));
    let theme = Arc::new(RwLock::new(theme));
    let (rewatch_tx, rewatch_rx) = std::sync::mpsc::channel::<PathBuf>();
    let state = AppState {
        tx: tx.clone(),
        file_path: Arc::clone(&file_path),
        theme: Arc::clone(&theme),
        roots: Arc::new(roots),
        scan_root: Arc::new(scan_root),
        rewatch: rewatch_tx,
    };
    let server = std::thread::spawn(move || run_server(state, host, port));

    // Watch loop: follow the active file, re-pointing when a selection arrives.
    // On each change (or selection) re-render and broadcast a block-diff update
    // (#16) so the client patches in place instead of full-reloading.
    let (mut handle, rx) = watch_channel();
    let mut current = active;
    handle.watch(&current);
    broadcast_update(&current, &tx);
    loop {
        while let Ok(next) = rewatch_rx.try_recv() {
            if next != current {
                handle.unwatch(&current);
                handle.watch(&next);
                current = next;
            }
            broadcast_update(&current, &tx);
        }
        match rx.recv_timeout(Duration::from_millis(300)) {
            Ok(()) => broadcast_update(&current, &tx),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = server.join();
}

/// Re-render `path` and broadcast an in-place update to connected clients (#16).
fn broadcast_update(path: &Path, tx: &broadcast::Sender<Message>) {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let (body, frontmatter) = render_markdown(&content);
            let msg = serde_json::json!({
                "type": "update",
                "html": body,
                "frontmatter": frontmatter.unwrap_or_default(),
            })
            .to_string();
            let result = tx.send(Message::text(msg));
            debug!("Pushed live update for {}: {:#?}", path.display(), result);
        }
        Err(e) => error!("Failed to read '{}' on change: {e}", path.display()),
    }
}

#[tokio::main()]
async fn run_server(state: AppState, host: String, port: String) -> Result<()> {
    let app = Router::new()
        .route("/", get(file_handler))
        .route("/ws", get(websocket_handler))
        .route("/api/tree", get(tree_handler))
        .route("/api/select", post(select_handler))
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
    let (html_body, frontmatter) = render_markdown(&markdown_content);

    // Front matter panel (#19): surface the leading YAML/+++ block, which the
    // renderer otherwise hides. Escaped and stashed in a hidden element for the
    // client to display.
    let frontmatter_html = frontmatter
        .map(|fm| {
            format!(
                "<div id=\"mdpeek-frontmatter\" hidden>{}</div>",
                escape_html_min(&fm)
            )
        })
        .unwrap_or_default();

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

/// Render markdown source into a `(body HTML, raw front matter)` pair. Shared by
/// the HTTP handler (initial page) and the live-update watch callback (#16) so
/// both produce identical markup. The front matter is returned raw (unescaped);
/// each caller escapes it as its transport requires.
fn render_markdown(content: &str) -> (String, Option<String>) {
    let parser = Parser::new_ext(content, mdpeek_gfm::parser_options());
    let parser = mdpeek_gfm::transform(parser);
    let mut emitter = HtmlEmitter::new(parser);
    let body = emitter.run();
    let frontmatter = mdpeek_parser::BlockTree::parse(content)
        .frontmatter()
        .filter(|fm| !fm.trim().is_empty())
        .map(|fm| fm.to_string());
    (body, frontmatter)
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

/// `GET /api/tree` — the discovered repo/worktree markdown tree plus the
/// currently active file, for the explorer sidebar (#14).
async fn tree_handler(State(state): State<AppState>) -> impl IntoResponse {
    let tree = explorer::build_tree(state.scan_root.as_ref());
    let active = state
        .file_path
        .read()
        .unwrap()
        .to_string_lossy()
        .to_string();
    Json(serde_json::json!({ "tree": tree, "active": active }))
}

#[derive(Deserialize)]
struct SelectRequest {
    path: String,
}

/// `POST /api/select {path}` — switch the active/watched file. The path must
/// canonicalize to a markdown file under one of the discovered roots, else it is
/// rejected (directory-traversal guard, #14).
async fn select_handler(
    State(state): State<AppState>,
    Json(req): Json<SelectRequest>,
) -> impl IntoResponse {
    match explorer::resolve_within(&state.roots, &req.path) {
        Some(abs) => {
            *state.file_path.write().unwrap() = abs.clone();
            // Re-point the watcher; it re-renders and broadcasts the new file.
            if state.rewatch.send(abs).is_err() {
                error!("watch loop is gone; cannot switch file");
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
            StatusCode::OK
        }
        None => {
            warn!("Rejected select for '{}' (outside roots)", req.path);
            StatusCode::FORBIDDEN
        }
    }
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

    #[test]
    fn render_markdown_emits_body_and_frontmatter() {
        use super::render_markdown;
        let (body, fm) = render_markdown("---\ntitle: Hi\n---\n\n# Heading\n\ntext\n");
        assert!(body.contains("<h1"), "body should contain rendered heading");
        assert_eq!(fm.as_deref(), Some("title: Hi"));
    }

    #[test]
    fn render_markdown_without_frontmatter_is_none() {
        use super::render_markdown;
        let (body, fm) = render_markdown("# Only heading\n");
        assert!(body.contains("Only heading"));
        assert!(fm.is_none());
    }
}
