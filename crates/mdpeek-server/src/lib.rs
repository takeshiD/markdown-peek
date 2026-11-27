mod adaptive;
mod assets;
pub use adaptive::{AdaptiveSetup, write_default_model};
mod explorer;
mod scrolly;

use anyhow::Result;
use axum::{
    Json, Router,
    body::Body,
    extract::{
        Path as AxumPath, Query, State,
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

use mdpeek_gui::LlmBackendConfig;
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
    rewatch: StdSender<WatchTarget>,
    /// Guided reading's LLM backend (`[llm]`). `None` = rules commentary only.
    llm: Arc<Option<LlmBackendConfig>>,
    /// Adaptive reading-aid selection (`/api/adaptive` + WS `adaptive`).
    adaptive: Arc<adaptive::Adaptive>,
}

/// What the server watches and re-renders on change: the active file.
enum WatchTarget {
    Single(PathBuf),
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

pub fn serve(
    watch_path: PathBuf,
    host: String,
    port: String,
    theme: Theme,
    llm: Option<LlmBackendConfig>,
    aids: AdaptiveSetup,
) {
    // Discover the enclosing repo/worktrees from where the user ran mdpeek so
    // the explorer sidebar (#14) works regardless of the file argument.
    let scan_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let roots = explorer::allowed_roots(&scan_root);
    // Pick the initial file: the given one if readable, else the first markdown
    // discovered under the repo, else fall back to the requested path.
    let active = explorer::initial_active(&watch_path, &scan_root).unwrap_or(watch_path);

    let (tx, _) = broadcast::channel::<Message>(16);
    let adaptive = Arc::new(adaptive::Adaptive::new(aids));
    let file_path = Arc::new(RwLock::new(active.clone()));
    let theme = Arc::new(RwLock::new(theme));
    let (rewatch_tx, rewatch_rx) = std::sync::mpsc::channel::<WatchTarget>();
    let state = AppState {
        tx: tx.clone(),
        file_path: Arc::clone(&file_path),
        theme: Arc::clone(&theme),
        roots: Arc::new(roots),
        scan_root: Arc::new(scan_root),
        rewatch: rewatch_tx,
        llm: Arc::new(llm),
        adaptive: Arc::clone(&adaptive),
    };
    let server = std::thread::spawn(move || run_server(state, host, port));

    // Watch loop: follow the active file, re-pointing when a selection arrives.
    // On each change (or selection) re-render and broadcast an in-place update
    // (#16) so the client updates without a full reload.
    let (mut handle, rx) = watch_channel();
    let mut watched: Vec<PathBuf> = Vec::new();
    let mut target = WatchTarget::Single(active);
    watch_target(&mut handle, &mut watched, &target);
    let rev = schedule_adaptive(&target, &adaptive, &tx);
    broadcast_for(&target, &tx, rev);
    loop {
        while let Ok(next) = rewatch_rx.try_recv() {
            watch_target(&mut handle, &mut watched, &next);
            target = next;
            let rev = schedule_adaptive(&target, &adaptive, &tx);
            broadcast_for(&target, &tx, rev);
        }
        match rx.recv_timeout(Duration::from_millis(300)) {
            Ok(()) => {
                let rev = schedule_adaptive(&target, &adaptive, &tx);
                broadcast_for(&target, &tx, rev);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = server.join();
}

/// Re-select the adaptive layout for the watched file.
/// Returns the revision the pending selection belongs to, so the body update
/// broadcast right after can tell the client which layout revision to expect.
fn schedule_adaptive(
    target: &WatchTarget,
    adaptive: &Arc<adaptive::Adaptive>,
    tx: &broadcast::Sender<Message>,
) -> u64 {
    let WatchTarget::Single(p) = target;
    adaptive.schedule(p.clone(), tx.clone())
}

fn target_paths(target: &WatchTarget) -> Vec<PathBuf> {
    let WatchTarget::Single(p) = target;
    vec![p.clone()]
}

/// Re-point the watcher: drop the previously watched paths and watch the new set.
fn watch_target(
    handle: &mut mdpeek_watcher::WatchHandle,
    watched: &mut Vec<PathBuf>,
    target: &WatchTarget,
) {
    for p in watched.iter() {
        handle.unwatch(p);
    }
    *watched = target_paths(target);
    for p in watched.iter() {
        handle.watch(p);
    }
}

/// Broadcast the appropriate update for the current watch target.
fn broadcast_for(target: &WatchTarget, tx: &broadcast::Sender<Message>, revision: u64) {
    let WatchTarget::Single(p) = target;
    broadcast_update(p, tx, revision)
}

/// Re-render `path` and broadcast an in-place update to connected clients (#16).
/// The message carries the full document payload (see [`doc_payload`]) so the
/// client can patch only the changed blocks, plus the adaptive revision whose
/// layout will follow (older layouts are then ignored by the client).
fn broadcast_update(path: &Path, tx: &broadcast::Sender<Message>, revision: u64) {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let mut msg = doc_payload(path, &content, revision);
            msg["type"] = serde_json::json!("update");
            let msg = msg.to_string();
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
        .route("/api/doc", get(doc_handler))
        .route("/api/select", post(select_handler))
        .route("/api/adaptive", get(adaptive_handler))
        .route("/api/adaptive/run", post(adaptive_run_handler))
        .route("/api/adaptive/stop", post(adaptive_stop_handler))
        .route("/api/models", get(models_handler))
        .route("/api/models/active", post(models_active_handler))
        .route("/api/models/default", post(models_default_handler))
        .route("/api/scrolly", get(scrolly_handler))
        .route("/api/scrolly/ask", post(scrolly_ask_handler))
        .route("/static/{*path}", get(static_handler))
        .route("/asset", get(asset_handler))
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
    let file_path = { state.file_path.read().unwrap().to_path_buf() };
    let revision = state.adaptive.current_revision();
    let boot = match tokio::fs::read_to_string(&file_path).await {
        Ok(content) => {
            info!("Loaded '{}'", file_path.display());
            doc_payload(&file_path, &content, revision)
        }
        Err(e) => {
            error!("Failed to read file '{}': {}", file_path.display(), e);
            let mut p = doc_payload(&file_path, "", revision);
            p["error"] = serde_json::json!(format!(
                "Failed to read {}: {}",
                file_path.display(),
                e.kind()
            ));
            p
        }
    };
    let theme = match *state.theme.read().unwrap() {
        Theme::Light => "light",
        Theme::Dark => "dark",
    };
    let title = boot["title"]
        .as_str()
        .unwrap_or("markdown-peek")
        .to_string();
    Html(render_shell(&title, theme, &boot))
}

/// Fill the app shell template. The boot payload is embedded as JSON inside a
/// `<script type="application/json">`, so the body renders on the first frame
/// (no fetch) and never waits for widget selection. `</` is escaped so document
/// text cannot close the script element.
fn render_shell(title: &str, theme: &str, boot: &serde_json::Value) -> String {
    let json = boot
        .to_string()
        .replace("</", "<\\/")
        .replace("<!--", "<\\u0021--");
    include_str!("../../../static/index.html")
        .replace(
            "{{ title }}",
            &escape_html_min(title).replace('"', "&quot;"),
        )
        .replace("{{ theme }}", theme)
        .replace("{{ boot }}", &json)
}

/// Everything the web UI needs to show one document: rendered body HTML, raw
/// front matter, the source line span of each top-level block (in the order the
/// blocks appear in `html`, used for source jumps / adaptive placement), and the
/// adaptive revision current when it was rendered.
fn doc_payload(path: &Path, content: &str, revision: u64) -> serde_json::Value {
    let (html, frontmatter) = render_markdown(content);
    let title = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("markdown-peek");
    serde_json::json!({
        "path": path.to_string_lossy(),
        "title": title,
        "html": html,
        "frontmatter": frontmatter.unwrap_or_default(),
        "blocks": block_lines(content),
        "revision": revision,
    })
}

/// `[start_line, end_line]` (1-based, inclusive) of every top-level block that
/// produces body HTML (front matter is excluded; it is shown separately).
fn block_lines(content: &str) -> Vec<[u32; 2]> {
    mdpeek_parser::BlockTree::parse(content)
        .blocks
        .iter()
        .filter(|b| !matches!(b.kind, mdpeek_parser::BlockKind::MetadataBlock))
        .map(|b| {
            let r = b.range;
            let end = if r.end_column == 1 && r.end_line > r.start_line {
                r.end_line - 1
            } else {
                r.end_line
            };
            [r.start_line, end]
        })
        .collect()
}

/// `GET /api/doc` — the active document payload (same shape as the boot JSON).
async fn doc_handler(State(state): State<AppState>) -> impl IntoResponse {
    let file_path = { state.file_path.read().unwrap().to_path_buf() };
    let revision = state.adaptive.current_revision();
    match tokio::fs::read_to_string(&file_path).await {
        Ok(content) => Json(doc_payload(&file_path, &content, revision)).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("cannot read file: {}", e.kind()) })),
        )
            .into_response(),
    }
}

/// Query params for `/api/scrolly`: `lang` = `auto` | `ja` | `en`.
#[derive(Debug, Deserialize)]
struct ScrollyQuery {
    #[serde(default = "default_scrolly_lang")]
    lang: String,
}

fn default_scrolly_lang() -> String {
    "auto".to_string()
}

/// `GET /api/scrolly?lang=` — Generative Scrollytelling guide for the active
/// file. Returns a [`scrolly::ScrollyGuide`] (`overview` + per-section
/// `commentary`, aligned to rendered heading anchors). LLM-only: 500 if no
/// backend is configured or generation fails. Runs on a blocking thread.
async fn scrolly_handler(
    State(state): State<AppState>,
    Query(query): Query<ScrollyQuery>,
) -> impl IntoResponse {
    let file_path = { state.file_path.read().unwrap().to_path_buf() };

    let markdown = match tokio::fs::read_to_string(&file_path).await {
        Ok(content) => content,
        Err(e) => {
            error!("scrolly: failed to read '{}': {e}", file_path.display());
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    info!(
        "scrolly: guiding '{}' [{}, lang={}]",
        file_path.display(),
        state
            .llm
            .as_ref()
            .as_ref()
            .map(|b| format!("LLM {:?}", b.provider))
            .unwrap_or_else(|| "no backend".to_string()),
        query.lang,
    );

    let started = std::time::Instant::now();
    let llm = (*state.llm).clone();
    let lang = query.lang.clone();
    let result = tokio::task::spawn_blocking(move || {
        // Cache under the cwd's `.cache/mdpeek/`.
        scrolly::generate(&markdown, llm.as_ref(), Path::new("."), &lang)
    })
    .await;

    match result {
        Ok(Ok(guide)) => {
            info!(
                "scrolly: {} section(s) for '{}' in {} ms",
                guide.sections.len(),
                file_path.display(),
                started.elapsed().as_millis(),
            );
            Json(guide).into_response()
        }
        Ok(Err(e)) => {
            error!(
                "scrolly: generation failed for '{}': {e}",
                file_path.display()
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
        Err(e) => {
            error!(
                "scrolly: generation task panicked for '{}': {e}",
                file_path.display()
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

/// `POST /api/scrolly/ask` — answer a reader's question about one section
/// (in-panel Q&A). Body: [`scrolly::AskRequest`]. Runs on a blocking thread.
async fn scrolly_ask_handler(
    State(state): State<AppState>,
    Json(req): Json<scrolly::AskRequest>,
) -> impl IntoResponse {
    let file_path = { state.file_path.read().unwrap().to_path_buf() };

    let markdown = match tokio::fs::read_to_string(&file_path).await {
        Ok(content) => content,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    info!(
        "scrolly: Q&A on '{}' section '{}' [lang={}]",
        file_path.display(),
        req.anchor,
        req.lang,
    );

    let llm = (*state.llm).clone();
    let result =
        tokio::task::spawn_blocking(move || scrolly::answer(&markdown, llm.as_ref(), &req)).await;

    match result {
        Ok(answer) => Json(answer).into_response(),
        Err(e) => {
            error!("scrolly: Q&A task panicked: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

/// Render markdown source into a `(body HTML, raw front matter)` pair. Shared by
/// the HTTP handler (initial page) and the live-update watch callback (#16) so
/// both produce identical markup. The front matter is returned raw (unescaped);
/// each caller escapes it as its transport requires.
fn render_markdown(content: &str) -> (String, Option<String>) {
    let parser = Parser::new_ext(content, mdpeek_gfm::parser_options());
    let parser = mdpeek_gfm::transform(parser);
    let mut emitter = HtmlEmitter::new(parser).with_asset_prefix(assets::ASSET_PREFIX);
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
/// `GET /api/adaptive` — the current file's AdaptiveLayout (R6). Never asks a
/// model: the last pushed layout for this revision, or the cached decisions
/// (see [`adaptive::Adaptive::for_get`]). Runs are started with
/// `POST /api/adaptive/run` and pushed over the WebSocket.
async fn adaptive_handler(State(state): State<AppState>) -> impl IntoResponse {
    let file_path = { state.file_path.read().unwrap().to_path_buf() };
    let adaptive = Arc::clone(&state.adaptive);
    // Reads the file and may read the disk cache: off the async runtime.
    let result = tokio::task::spawn_blocking(move || adaptive.for_get(&file_path)).await;
    match result {
        Ok(Ok(layout)) => Json(serde_json::to_value(layout).unwrap_or_default()).into_response(),
        Ok(Err(e)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("cannot read file: {}", e.kind()) })),
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// `POST /api/adaptive/run` — start (or resume) choosing aids.
async fn adaptive_run_handler(State(state): State<AppState>) -> impl IntoResponse {
    let file_path = { state.file_path.read().unwrap().to_path_buf() };
    state.adaptive.start(file_path, state.tx.clone());
    StatusCode::ACCEPTED
}

/// `POST /api/adaptive/stop` — stop the running selection.
async fn adaptive_stop_handler(State(state): State<AppState>) -> impl IntoResponse {
    let file_path = { state.file_path.read().unwrap().to_path_buf() };
    let adaptive = Arc::clone(&state.adaptive);
    let tx = state.tx.clone();
    let _ = tokio::task::spawn_blocking(move || adaptive.stop(&file_path, &tx)).await;
    StatusCode::ACCEPTED
}

/// `GET /api/models` — the models from the config and the one in use.
async fn models_handler(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.adaptive.models_info())
}

#[derive(Deserialize)]
struct ModelRequest {
    name: String,
}

fn model_error(msg: String) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg })),
    )
        .into_response()
}

/// `POST /api/models/active` — use another model for this session.
async fn models_active_handler(
    State(state): State<AppState>,
    Json(req): Json<ModelRequest>,
) -> impl IntoResponse {
    let file_path = { state.file_path.read().unwrap().to_path_buf() };
    let adaptive = Arc::clone(&state.adaptive);
    let tx = state.tx.clone();
    let res =
        tokio::task::spawn_blocking(move || adaptive.set_active(&req.name, file_path, tx)).await;
    match res {
        Ok(Ok(())) => Json(state.adaptive.models_info()).into_response(),
        Ok(Err(e)) => model_error(e),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// `POST /api/models/default` — save a model as the default in `config.toml`.
async fn models_default_handler(
    State(state): State<AppState>,
    Json(req): Json<ModelRequest>,
) -> impl IntoResponse {
    let adaptive = Arc::clone(&state.adaptive);
    let tx = state.tx.clone();
    let res = tokio::task::spawn_blocking(move || adaptive.set_default(&req.name, &tx)).await;
    match res {
        Ok(Ok(())) => Json(state.adaptive.models_info()).into_response(),
        Ok(Err(e)) => model_error(e),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

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
            if state.rewatch.send(WatchTarget::Single(abs)).is_err() {
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
        // The web UI (React + Tailwind, built from web/ by `npm run build`).
        "app.js" => Some(StaticAsset {
            bytes: include_bytes!("../../../web/dist/app.js"),
            content_type: "application/javascript; charset=utf-8",
        }),
        "app.css" => Some(StaticAsset {
            bytes: include_bytes!("../../../web/dist/app.css"),
            content_type: "text/css; charset=utf-8",
        }),
        "icons/github-mark.svg" => Some(StaticAsset {
            bytes: include_bytes!("../../../static/icons/github-mark.svg"),
            content_type: "image/svg+xml",
        }),
        "js/highlight/highlight.min.js" => Some(StaticAsset {
            bytes: include_bytes!("../../../static/js/highlight/highlight.min.js"),
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

#[derive(Deserialize)]
struct AssetQuery {
    path: String,
}

/// `GET /asset?path=<rel>` — a local image referenced by the active document
/// (#55, #56), resolved relative to the document's directory. Anything that is
/// not an existing image under the allowed roots is a 404.
async fn asset_handler(State(state): State<AppState>, Query(query): Query<AssetQuery>) -> Response {
    let doc = { state.file_path.read().unwrap().to_path_buf() };
    serve_asset(Arc::clone(&state.roots), doc, query.path).await
}

async fn serve_asset(roots: Arc<Vec<PathBuf>>, doc: PathBuf, rel: String) -> Response {
    let resolved = tokio::task::spawn_blocking(move || {
        assets::resolve(&roots, &doc, &rel, explorer::doc_root).ok_or(rel)
    })
    .await;
    let (path, content_type) = match resolved {
        Ok(Ok(found)) => found,
        Ok(Err(rel)) => {
            debug!("asset: rejected '{rel}'");
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let mut res = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
                // Images may be edited while previewing; always revalidate.
                .header(header::CACHE_CONTROL, "no-cache");
            if content_type == "image/svg+xml" {
                // Scripts in an SVG never run inside <img>, but the URL can
                // also be opened directly: sandbox it.
                res = res.header(
                    header::CONTENT_SECURITY_POLICY,
                    "default-src 'none'; style-src 'unsafe-inline'; sandbox",
                );
            }
            res.body(Body::from(bytes))
                .expect("asset response should be valid")
        }
        Err(e) => {
            debug!("asset: cannot read '{}': {e}", path.display());
            StatusCode::NOT_FOUND.into_response()
        }
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
            "app.js",
            "app.css",
            "icons/github-mark.svg",
            "js/highlight/highlight.min.js",
            "js/mermaid/mermaid.min.js",
        ] {
            let asset = embedded_static_asset(path).expect(path);
            assert!(!asset.bytes.is_empty(), "{path} should not be empty");
        }
    }

    #[test]
    fn unknown_static_asset_is_not_found() {
        assert!(embedded_static_asset("missing.css").is_none());
        // The old vanilla-JS viewer and its stylesheets are gone (R11).
        for old in ["js/main.js", "css/github-light.css", "gui/mdpeek-gui.js"] {
            assert!(embedded_static_asset(old).is_none(), "{old}");
        }
    }

    #[test]
    fn shell_embeds_boot_json_safely() {
        let boot = super::doc_payload(
            std::path::Path::new("/x/a.md"),
            "# T\n\n</script><script>alert(1)</script>\n",
            3,
        );
        let page = super::render_shell("a.md", "light", &boot);
        // The boot element ends at the first `</script>`; everything before it
        // must be the complete JSON payload (document text cannot close it).
        let start = page.find("id=\"mdpeek-boot\"").expect("boot script");
        let rest = &page[start..];
        let body_start = rest.find('>').unwrap() + 1;
        let close = rest.find("</script>").unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&rest[body_start..close]).expect("intact JSON");
        assert!(
            json["html"]
                .as_str()
                .unwrap()
                .contains("</script><script>alert(1)")
        );
        assert_eq!(json["revision"], 3);
    }

    #[test]
    fn block_lines_cover_top_level_blocks() {
        let lines = super::block_lines("---\na: 1\n---\n# H\n\npara\nmore\n\n- a\n- b\n");
        assert_eq!(lines, vec![[4, 4], [6, 7], [9, 10]]);
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
    fn render_markdown_routes_local_images_through_asset() {
        let (body, _) = super::render_markdown(
            "![a](img/a.png)\n\n<img src=\"path/to/img\" />\n\n![e](https://example.com/e.png)\n",
        );
        assert!(body.contains(r#"src="/asset?path=img/a.png""#), "{body}");
        assert!(body.contains(r#"src="/asset?path=path/to/img""#), "{body}");
        assert!(
            body.contains(r#"src="https://example.com/e.png""#),
            "{body}"
        );
    }

    #[tokio::test]
    async fn serve_asset_serves_images_and_rejects_escapes() {
        use axum::http::{StatusCode, header};
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let root = base.join("repo");
        std::fs::create_dir_all(root.join("img")).unwrap();
        std::fs::write(root.join("doc.md"), "![](img/a.svg)").unwrap();
        std::fs::write(root.join("img/a.svg"), "<svg/>").unwrap();
        std::fs::write(base.join("out.png"), b"x").unwrap();
        let roots = std::sync::Arc::new(vec![root.clone()]);
        let doc = root.join("doc.md");

        let ok = super::serve_asset(roots.clone(), doc.clone(), "img/a.svg".into()).await;
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(ok.headers()[header::CONTENT_TYPE], "image/svg+xml");
        assert_eq!(ok.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert!(ok.headers().contains_key(header::CONTENT_SECURITY_POLICY));

        for rel in ["../out.png", "/etc/passwd", "doc.md", "img/none.png"] {
            let res = super::serve_asset(roots.clone(), doc.clone(), rel.into()).await;
            assert_eq!(res.status(), StatusCode::NOT_FOUND, "{rel}");
        }
    }

    #[test]
    fn render_markdown_without_frontmatter_is_none() {
        use super::render_markdown;
        let (body, fm) = render_markdown("# Only heading\n");
        assert!(body.contains("Only heading"));
        assert!(fm.is_none());
    }
}
