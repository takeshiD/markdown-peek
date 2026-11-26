mod cli;
mod config;
mod tui;

use crate::cli::{Cli, Mode, ThemeChoice};
use crate::config::{BrowserTheme, Config};
use anyhow::Result;
use mdpeek_analyzer::GenerationConfig;
use mdpeek_render_term::{TerminalEmitter, Theme};
use mdpeek_server::serve;
use mdpeek_watcher::notify_on_change;
use pulldown_cmark::Parser;
use std::path::PathBuf;
use std::sync::Once;
use tracing::error;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    let cmd = Cli::parse_with_color()?;
    // A `--config` path overrides the default XDG location.
    let config = match &cmd.config {
        Some(path) => Config::load_explicit(path),
        None => Config::load(),
    };
    let mode = cmd.resolve_mode(&config)?;
    // Generation policy (rules-first vs LLM-first) is read from config at startup;
    // Layer 3's generator will consult it. Server mode is the generative-UI path,
    // so it is where the policy takes effect.
    let generation = config.generation_config();
    // The `/api/gui` LLM backend: only when `[llm] enabled = true`; otherwise the
    // server generates rules-only (offline, no key needed).
    let gui_llm = config.llm_enabled().then(|| config.llm_backend_config());
    match mode {
        Mode::Serve {
            file,
            host,
            port,
            theme,
            select,
        } => handle_serve(file, host, port, theme, generation, gui_llm, select),
        Mode::Term {
            file,
            watch,
            theme,
            pager,
        } => handle_term(file, watch, theme, pager),
        Mode::Gen {
            file,
            no_cache,
            llm,
        } => handle_gen(file, no_cache, llm)?,
        Mode::Select { file, select } => handle_select(file, select)?,
        Mode::SelectEval {
            dir,
            runs,
            select,
            json,
        } => handle_select_eval(dir, runs, select, json)?,
    }
    Ok(())
}

/// `mdpeek select`: print the AdaptiveLayout JSON for `file` on stdout. Never
/// fails because of the selector — problems degrade to rules and are reported
/// in `selector.fallback` (plus a one-line note on stderr, without content).
fn handle_select(file: PathBuf, select: mdpeek_gui::select::SelectConfig) -> Result<()> {
    init_stderr_tracing();
    if !file.is_file() {
        anyhow::bail!("'{}' is not a file.", file.display());
    }
    let markdown = std::fs::read_to_string(&file)?;
    let filename = file.file_name().and_then(|n| n.to_str());
    let cache = mdpeek_gui::select::SelectionCache::new(select.cache_entries);
    let layout = mdpeek_gui::select::adaptive_layout(&markdown, filename, &select, &cache, 1);
    if let Some(f) = &layout.selector.fallback {
        eprintln!(
            "mdpeek: select: {} unavailable ({f}); using rules",
            layout.selector.requested
        );
    }
    println!("{}", mdpeek_gui::select::layout_json(&layout));
    Ok(())
}

/// `mdpeek select-eval`: accuracy / none-appropriateness / p50-p95 per provider.
fn handle_select_eval(
    dir: PathBuf,
    runs: usize,
    select: mdpeek_gui::select::SelectConfig,
    json: bool,
) -> Result<()> {
    init_stderr_tracing();
    let report = mdpeek_gui::select::eval::evaluate(&dir, &select, runs)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", mdpeek_gui::select::eval::render_text(&report));
    }
    Ok(())
}

/// Tracing for the JSON-printing commands: stderr only (stdout stays pure JSON),
/// `warn` unless `RUST_LOG` says otherwise.
fn init_stderr_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            quiet_http(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"))),
        )
        .with_writer(std::io::stderr)
        .without_time()
        .try_init();
}

/// Generate Generative-UI IR (Layer 3) for `root` and print the JSON to stdout.
/// The cache lives under `.cache/mdpeek/` in the current directory. When `llm`
/// is set, the configured LLM backend is used (with a rules fallback).
fn handle_gen(
    root: PathBuf,
    no_cache: bool,
    llm: Option<mdpeek_gui::generator::llm::LlmBackendConfig>,
) -> Result<()> {
    if !root.is_file() {
        anyhow::bail!("'{}' is not a file.", root.display());
    }
    let markdown = std::fs::read_to_string(&root)?;
    let cache_root = if no_cache {
        None
    } else {
        Some(std::path::Path::new("."))
    };
    // Filename sharpens Layer 2 document-type inference (e.g. README.md).
    let filename = root.file_name().and_then(|n| n.to_str());
    let json = mdpeek_gui::generate_json(&markdown, filename, cache_root, llm.as_ref())?;
    println!("{json}");
    Ok(())
}

fn handle_serve(
    root: PathBuf,
    host: String,
    port: String,
    theme: BrowserTheme,
    generation: GenerationConfig,
    gui_llm: Option<mdpeek_gui::generator::llm::LlmBackendConfig>,
    select: mdpeek_gui::select::SelectConfig,
) {
    init_tracing();
    tracing::info!(
        "generation policy: {:?} (llm {}, confidence_threshold {})",
        generation.strategy,
        if generation.is_rules_only() {
            "disabled — rules only"
        } else {
            "enabled"
        },
        generation.confidence_threshold,
    );
    // Map the binary's config theme onto the server crate's own theme so the
    // server stays independent of the binary's config types.
    let theme = match theme {
        BrowserTheme::Light => mdpeek_server::Theme::Light,
        BrowserTheme::Dark => mdpeek_server::Theme::Dark,
    };
    // `serve` discovers the repo/worktree markdown tree (explorer mode, #14) and
    // picks a valid active file, so we hand off even when `root` doesn't exist
    // (e.g. the default README.md is absent) rather than bailing out here.
    serve(root, host, port, theme, gui_llm, select);
}

fn handle_term(root: PathBuf, watch: bool, theme: ThemeChoice, pager: Option<String>) {
    use std::io::IsTerminal;

    init_tracing();
    if !root.exists() {
        error!("'{}' is not found.", root.display());
        return;
    }
    if !root.is_file() {
        error!("'{}' is not a file.", root.display());
        return;
    }

    // Interactive live viewer: only when watching AND stdout is a real TTY.
    // Piped/redirected stdout falls through to the clear+reprint loop below.
    if watch && std::io::stdout().is_terminal() {
        if let Err(e) = tui::run_tui(root.clone(), theme) {
            error!("TUI viewer error: {e}");
        }
        return;
    }

    match render_term(&root, theme) {
        Ok(rendered) => {
            if watch {
                // Watch mode redraws continuously, so a pager would get in the way.
                println!("{rendered}");
            } else {
                display_term(&rendered, &pager);
            }
        }
        // Keep going in watch mode: the file may become readable again.
        Err(e) => error!("Failed to render '{}': {e}", root.display()),
    }
    if watch {
        let watch_path = root.clone();
        notify_on_change(watch_path, move || {
            if let Ok(rendered) = render_term(&root, theme) {
                clear_terminal();
                println!("{rendered}");
            }
        });
    }
}

/// Print the rendered output, launching a pager when it is too long to fit on
/// one screen. Falls back to a plain print when stdout is not a terminal or the
/// pager cannot be started.
fn display_term(content: &str, pager_cfg: &Option<String>) {
    use std::io::IsTerminal;

    let stdout = std::io::stdout();
    if !stdout.is_terminal() {
        // Piped or redirected: never page.
        println!("{content}");
        return;
    }

    // An explicit empty pager in config.toml disables paging entirely.
    if matches!(pager_cfg.as_deref(), Some("")) {
        println!("{content}");
        return;
    }

    // Page only when the output would overflow the visible screen.
    let rows = terminal_size::terminal_size()
        .map(|(_, h)| h.0 as usize)
        .unwrap_or(40);
    let line_count = content.lines().count();
    if line_count > rows && page(content, pager_cfg).is_ok() {
        return;
    }

    println!("{content}");
}

/// Pipe `content` through a pager. The pager is chosen with this precedence:
/// the `term.pager` config value, then `$PAGER`, then `less -R`.
fn page(content: &str, pager_cfg: &Option<String>) -> std::io::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let pager = match pager_cfg {
        Some(p) if !p.trim().is_empty() => p.clone(),
        _ => std::env::var("PAGER").unwrap_or_default(),
    };
    let mut cmd = if pager.trim().is_empty() {
        let mut c = Command::new("less");
        c.arg("-R"); // keep ANSI colour escapes intact
        c
    } else {
        let mut parts = pager.split_whitespace();
        // split_whitespace yields at least one item because pager is non-empty.
        let mut c = Command::new(parts.next().unwrap());
        c.args(parts);
        c
    };

    let mut child = cmd.stdin(Stdio::piped()).spawn()?;
    {
        // Take the handle so it is dropped (closing stdin → EOF) before we wait.
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("failed to open pager stdin"))?;
        stdin.write_all(content.as_bytes())?;
        stdin.write_all(b"\n")?;
    }
    child.wait()?;
    Ok(())
}

fn render_term(root: &PathBuf, theme: ThemeChoice) -> Result<String> {
    let markdown_content = std::fs::read_to_string(root)?;
    let parser = Parser::new_ext(&markdown_content, mdpeek_gfm::parser_options());
    let parser = mdpeek_gfm::transform(parser);
    let theme = match theme {
        ThemeChoice::Glow => Theme::glow(),
        ThemeChoice::Mono => Theme::mono(),
        ThemeChoice::Catputtin => Theme::catputtin(),
        ThemeChoice::Dracura => Theme::dracura(),
        ThemeChoice::Solarized => Theme::solarized(),
        ThemeChoice::Nord => Theme::nord(),
        ThemeChoice::Ayu => Theme::ayu(),
    };
    let mut emitter = TerminalEmitter::new(parser, theme);
    Ok(emitter.run())
}

fn clear_terminal() {
    use std::io::{self, Write};
    let mut stdout = io::stdout();
    let _ = write!(stdout, "\x1B[2J\x1B[H");
    let _ = stdout.flush();
}

/// HTTP client internals never log (they could carry request headers/bodies,
/// i.e. the API key or document excerpts), whatever `RUST_LOG` says.
fn quiet_http(f: EnvFilter) -> EnvFilter {
    [
        "hyper=off",
        "hyper_util=off",
        "reqwest=off",
        "h2=off",
        "rustls=off",
    ]
    .iter()
    .fold(f, |f, d| f.add_directive(d.parse().unwrap()))
}

fn init_tracing() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(
                quiet_http(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))),
            )
            .with_ansi(true)
            .without_time()
            .init();
    });
}
