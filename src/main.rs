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
        Mode::Serve { file, watch } => {
            let _ = watch;
            handle_serve(file)
        }
        Mode::Term { file, watch, theme } => handle_term(file, watch, theme),
    }
    Ok(())
}

fn handle_serve(root: PathBuf) {
    init_tracing();
    if root.exists() {
        serve(root);
    } else {
        error!("'{}' is not found.", root.display());
    }
}

fn handle_term(root: PathBuf, watch: bool, theme: ThemeChoice) {
    init_tracing();
    if root.exists() {
        if root.is_file() {
            if let Ok(rendered) = render_term(&root, theme) {
                println!("{rendered}");
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
        } else {
            error!("'{}' is not file and directory.", root.display());
        }
    } else {
        error!("'{}' is not found.", root.display());
    }
}

fn render_term(root: &PathBuf, theme: ThemeChoice) -> Result<String> {
    let markdown_content = std::fs::read_to_string(root)?;
    let mut options = Options::empty();
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_TABLES);
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
