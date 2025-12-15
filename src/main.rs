mod cli;
mod server;
mod watcher;

use crate::cli::{Cli, Mode};
use anyhow::Result;
use std::path::PathBuf;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    let cmd = Cli::parse_with_color()?;
    let mode = cmd.resolve_mode()?;
    match mode {
        Mode::Serve { file, watch } => handle_serve(file),
        Mode::Term { file, watch } => unimplemented!(),
    }
    Ok(())
}

fn handle_serve(root: PathBuf) {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_ansi(true)
        .init();
    if root.exists() {
        if root.is_file() {
            info!("watching file '{}'", root.display());
        } else if root.is_dir() {
            info!("watching dir '{}'", root.display());
        } else {
            error!("'{}' is not file and directory.", root.display());
        }
    } else {
        error!("'{}' is not found.", root.display());
    }
}

fn handle_term(root: PathBuf) {
    if root.exists() {
        if root.is_file() {
            info!("watching file '{}'", root.display());
        } else {
            error!("'{}' is not file and directory.", root.display());
        }
    } else {
        error!("'{}' is not found.", root.display());
    }
}
