mod cache;
mod cli;
mod config;
mod emitter;
mod generator;
mod gfm;
mod gui;
mod ir;
mod server;
mod watcher;

use crate::cli::{Cli, Mode, ThemeChoice};
use crate::config::{BrowserTheme, Config};
use crate::emitter::{TerminalEmitter, Theme};
use crate::server::serve;
use crate::watcher::notify_on_change;
use anyhow::Result;
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
    match mode {
        Mode::Serve {
            file,
            host,
            port,
            theme,
        } => handle_serve(file, host, port, theme),
        Mode::Term {
            file,
            watch,
            theme,
            pager,
        } => handle_term(file, watch, theme, pager),
        Mode::Gen { file, no_cache } => handle_gen(file, no_cache)?,
    }
    Ok(())
}

/// Generate Generative-UI IR (Layer 3) for `root` and print the JSON to stdout.
/// The cache lives under `.cache/mdpeek/` in the current directory.
fn handle_gen(root: PathBuf, no_cache: bool) -> Result<()> {
    if !root.is_file() {
        anyhow::bail!("'{}' is not a file.", root.display());
    }
    let markdown = std::fs::read_to_string(&root)?;
    let cache_root = if no_cache {
        None
    } else {
        Some(std::path::Path::new("."))
    };
    let json = gui::generate_json(&markdown, cache_root)?;
    println!("{json}");
    Ok(())
}

fn handle_serve(root: PathBuf, host: String, port: String, theme: BrowserTheme) {
    init_tracing();
    if root.exists() {
        serve(root, host, port, theme);
    } else {
        error!("'{}' is not found.", root.display());
    }
}

fn handle_term(root: PathBuf, watch: bool, theme: ThemeChoice, pager: Option<String>) {
    init_tracing();
    if !root.exists() {
        error!("'{}' is not found.", root.display());
        return;
    }
    if !root.is_file() {
        error!("'{}' is not a file.", root.display());
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
    let parser = Parser::new_ext(&markdown_content, crate::gfm::parser_options());
    let parser = crate::gfm::transform(parser);
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

fn init_tracing() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .with_ansi(true)
            .without_time()
            .init();
    });
}
