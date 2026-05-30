mod cli;
mod emitter;
mod server;
mod watcher;

use crate::cli::{Cli, Mode, ThemeChoice};
use crate::emitter::{TerminalEmitter, Theme};
use crate::server::serve;
use crate::watcher::notify_on_change;
use anyhow::Result;
use pulldown_cmark::{Options, Parser};
use std::path::PathBuf;
use std::sync::Once;
use tracing::error;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    let cmd = Cli::parse_with_color()?;
    let mode = cmd.resolve_mode()?;
    match mode {
        Mode::Serve {
            file,
            watch,
            host,
            port,
        } => {
            let _ = watch;
            handle_serve(file, host, port)
        }
        Mode::Term { file, watch, theme } => handle_term(file, watch, theme),
    }
    Ok(())
}

fn handle_serve(root: PathBuf, host: String, port: String) {
    init_tracing();
    if root.exists() {
        serve(root, host, port);
    } else {
        error!("'{}' is not found.", root.display());
    }
}

fn handle_term(root: PathBuf, watch: bool, theme: ThemeChoice) {
    init_tracing();
    if !root.exists() {
        error!("'{}' is not found.", root.display());
        return;
    }
    if !root.is_file() {
        error!("'{}' is not file and directory.", root.display());
        return;
    }
    if let Ok(rendered) = render_term(&root, theme) {
        if watch {
            // Watch mode redraws continuously, so a pager would get in the way.
            println!("{rendered}");
        } else {
            display_term(&rendered);
        }
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
fn display_term(content: &str) {
    use std::io::IsTerminal;

    let stdout = std::io::stdout();
    if !stdout.is_terminal() {
        // Piped or redirected: never page.
        println!("{content}");
        return;
    }

    // Page only when the output would overflow the visible screen.
    let rows = terminal_size::terminal_size()
        .map(|(_, h)| h.0 as usize)
        .unwrap_or(40);
    let line_count = content.lines().count();
    if line_count > rows && page(content).is_ok() {
        return;
    }

    println!("{content}");
}

/// Pipe `content` through the user's pager (`$PAGER`, or `less -R`).
fn page(content: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let pager = std::env::var("PAGER").unwrap_or_default();
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
    let mut options = Options::empty();
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_MATH);
    options.insert(Options::ENABLE_FOOTNOTES);
    let parser = Parser::new_ext(&markdown_content, options);
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
