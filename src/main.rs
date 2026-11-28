mod cli;
mod config;
mod tui;

use crate::cli::{Cli, Mode, ThemeChoice};
use crate::config::{BrowserTheme, Config};
use anyhow::Result;
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
    // Guided reading's LLM: only when `[llm] enabled = true`; otherwise its
    // commentary comes from rules (offline, no key needed).
    let gui_llm = config.llm_enabled().then(|| config.llm_backend_config());
    match mode {
        Mode::Serve {
            file,
            host,
            port,
            theme,
            model,
        } => {
            let aids = adaptive_setup(&config, model);
            handle_serve(file, host, port, theme, gui_llm, aids)
        }
        Mode::Term {
            file,
            watch,
            theme,
            pager,
        } => handle_term(file, watch, theme, pager),
        Mode::Select {
            file,
            select,
            model,
        } => handle_select(file, select, model)?,
        Mode::SelectEval {
            dir,
            runs,
            select,
            models,
            json,
        } => handle_select_eval(dir, runs, select, models, json)?,
    }
    Ok(())
}

/// Everything `serve` needs for reading aids, resolved from the config.
fn adaptive_setup(config: &Config, start_with: Option<String>) -> mdpeek_server::AdaptiveSetup {
    let models = config.models();
    let mut config_errors = config.errors(&models);
    if let Some(n) = &start_with
        && models.get(n).is_none()
    {
        config_errors.push(format!(
            "--model {n}: {}; using \"{}\"",
            models.unavailable(n),
            models.default
        ));
    }
    let c = config.cache_settings();
    let disk = c.dir.as_ref().and_then(|d| {
        match mdpeek_gui::select::DiskCache::open(d.clone(), c.ttl_days, c.max_size_mb) {
            Ok(dc) => Some(dc),
            Err(e) => {
                config_errors.push(format!(
                    "cache directory {} cannot be used ({}); decisions are kept in memory only",
                    d.display(),
                    e.kind()
                ));
                None
            }
        }
    });
    for e in &config_errors {
        eprintln!("mdpeek: warning: {e}");
    }
    let (aids_default_on, auto_generate) = config.genui();
    mdpeek_server::AdaptiveSetup {
        cfg: config.select_config(),
        models,
        start_with,
        config_errors,
        config_path: config.path.clone().or_else(config::config_path),
        aids_default_on,
        auto_generate,
        cache: mdpeek_gui::select::SelectionCache::with_disk(c.memory_entries, disk),
    }
}

/// `mdpeek select`: print the AdaptiveLayout JSON for `file` on stdout. Never
/// fails because of the model — problems are reported in `selector.error` /
/// `selector.message` (plus a one-line note on stderr, without content). The
/// cache is memory-only here, so every run asks the model.
fn handle_select(
    file: PathBuf,
    select: mdpeek_gui::select::SelectConfig,
    model: mdpeek_gui::select::ModelSpec,
) -> Result<()> {
    init_stderr_tracing();
    if !file.is_file() {
        anyhow::bail!("'{}' is not a file.", file.display());
    }
    let markdown = std::fs::read_to_string(&file)?;
    let filename = file.file_name().and_then(|n| n.to_str());
    let cache = mdpeek_gui::select::SelectionCache::new(256);
    let layout =
        mdpeek_gui::select::adaptive_layout(&markdown, filename, &select, &model, &cache, 1);
    if let Some(m) = &layout.selector.message {
        eprintln!(
            "mdpeek: select: model {} could not answer: {m}",
            layout.selector.name
        );
    }
    println!("{}", mdpeek_gui::select::layout_json(&layout));
    Ok(())
}

/// `mdpeek select-eval`: accuracy / none-appropriateness / p50-p95 per model.
fn handle_select_eval(
    dir: PathBuf,
    runs: usize,
    select: mdpeek_gui::select::SelectConfig,
    models: Vec<mdpeek_gui::select::ModelSpec>,
    json: bool,
) -> Result<()> {
    init_stderr_tracing();
    let report = mdpeek_gui::select::eval::evaluate(&dir, &select, &models, runs)?;
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
        .with_env_filter(quiet_http(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        ))
        .with_writer(std::io::stderr)
        .without_time()
        .try_init();
}

fn handle_serve(
    root: PathBuf,
    host: String,
    port: String,
    theme: BrowserTheme,
    gui_llm: Option<mdpeek_gui::LlmBackendConfig>,
    aids: mdpeek_server::AdaptiveSetup,
) {
    init_tracing();
    // Map the binary's config theme onto the server crate's own theme so the
    // server stays independent of the binary's config types.
    let theme = match theme {
        BrowserTheme::Light => mdpeek_server::Theme::Light,
        BrowserTheme::Dark => mdpeek_server::Theme::Dark,
    };
    // `serve` discovers the repo/worktree markdown tree (explorer mode, #14) and
    // picks a valid active file, so we hand off even when `root` doesn't exist
    // (e.g. the default README.md is absent) rather than bailing out here.
    serve(root, host, port, theme, gui_llm, aids);
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
            .with_env_filter(quiet_http(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
            ))
            .with_ansi(true)
            .without_time()
            .init();
    });
}
