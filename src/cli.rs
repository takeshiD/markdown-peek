use crate::config::{BrowserTheme, Config, DefaultMode};
use anyhow::Result;
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use std::{io::IsTerminal, path::PathBuf};

const DEFAULT_ROOT: &str = "README.md";
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: &str = "3030";

#[derive(Debug, Parser)]
#[command(author, name = "mdpeek", about, long_about = None, version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
    /// Target file by default "README.md"
    #[arg(value_name = "FILE")]
    pub root: Option<PathBuf>,
    #[arg(short = 'n', long = "host", value_name = "HOST")]
    pub host: Option<String>,
    #[arg(short = 'p', long = "port", value_name = "PORT")]
    pub port: Option<String>,
    /// Watch for file changes and re-render (term mode only; serve always watches)
    #[arg(short = 'w', long = "watch", global = true)]
    pub watch: bool,
    /// Path to a config file (overrides the default XDG location)
    #[arg(short = 'c', long = "config", value_name = "FILE", global = true)]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Serve hot reload previewer on your browser
    Serve(ServeArg),
    /// Display pretty rendered markdown on your terminal
    Term(TermArg),
    /// Repository-aware view: cross-check a document against the repo (Layer 4)
    Repo(RepoArg),
}

// Subcommand arguments are optional so that an unset flag can fall back to
// `config.toml`, then to the built-in default. clap defaults are intentionally
// omitted here and applied in `resolve_mode`.
#[derive(Debug, Args)]
pub struct ServeArg {
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,
    #[arg(long, value_name = "HOST")]
    pub host: Option<String>,
    #[arg(long, value_name = "PORT")]
    pub port: Option<String>,
}

#[derive(Debug, Args)]
pub struct TermArg {
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,
    #[arg(long, value_enum)]
    pub theme: Option<ThemeChoice>,
}

#[derive(Debug, Args)]
pub struct RepoArg {
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,
    /// Emit the report as JSON instead of the terminal view.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    Glow,
    Mono,
    Catputtin,
    Dracura,
    Solarized,
    Nord,
    Ayu,
}

pub enum Mode {
    /// `serve` always performs live-reload internally; no `watch` field needed.
    Serve {
        file: PathBuf,
        host: String,
        port: String,
        theme: BrowserTheme,
    },
    Term {
        file: PathBuf,
        watch: bool,
        theme: ThemeChoice,
        /// Pager override from config: `None` uses `$PAGER`/`less -R`, `Some("")`
        /// disables paging, `Some(cmd)` runs `cmd`.
        pager: Option<String>,
    },
    /// Repository-aware analysis of a document (Layer 4).
    Repo { file: PathBuf, json: bool },
}

impl Cli {
    pub fn parse_with_color() -> Result<Self, clap::Error> {
        pub const CLAP_STYLING: clap::builder::styling::Styles =
            clap::builder::styling::Styles::styled()
                .header(clap_cargo::style::HEADER)
                .usage(clap_cargo::style::USAGE)
                .literal(clap_cargo::style::LITERAL)
                .placeholder(clap_cargo::style::PLACEHOLDER)
                .error(clap_cargo::style::ERROR)
                .valid(clap_cargo::style::VALID)
                .invalid(clap_cargo::style::INVALID);
        // let cmd = Self::command().styles(STYLES);
        let cmd = Self::command().styles(CLAP_STYLING);
        Self::from_arg_matches(&cmd.get_matches())
    }
    /// Resolve the final run mode by merging, in order of precedence:
    /// CLI arguments, then `config.toml`, then built-in defaults.
    pub fn resolve_mode(self, config: &Config) -> Result<Mode> {
        let cfg_host = config.server.host.clone();
        let cfg_port = config.server.port.clone();
        let browser_theme = config.server.theme.unwrap_or(BrowserTheme::Light);
        let pager = config.term.pager.clone();

        match self.command {
            Some(Commands::Serve(arg)) => Ok(Mode::Serve {
                file: arg.file.unwrap_or_else(|| PathBuf::from(DEFAULT_ROOT)),
                host: arg
                    .host
                    .or(self.host)
                    .or(cfg_host)
                    .unwrap_or_else(|| DEFAULT_HOST.to_string()),
                port: arg
                    .port
                    .or(self.port)
                    .or(cfg_port)
                    .unwrap_or_else(|| DEFAULT_PORT.to_string()),
                theme: browser_theme,
            }),
            Some(Commands::Term(arg)) => Ok(Mode::Term {
                file: arg.file.unwrap_or_else(|| PathBuf::from(DEFAULT_ROOT)),
                watch: self.watch,
                theme: arg.theme.or(config.term.theme).unwrap_or(ThemeChoice::Glow),
                pager,
            }),
            Some(Commands::Repo(arg)) => Ok(Mode::Repo {
                file: arg
                    .file
                    .or(self.root)
                    .unwrap_or_else(|| PathBuf::from(DEFAULT_ROOT)),
                json: arg.json,
            }),
            None => {
                let root = self.root.unwrap_or_else(|| PathBuf::from(DEFAULT_ROOT));
                let host = self
                    .host
                    .or(cfg_host)
                    .unwrap_or_else(|| DEFAULT_HOST.to_string());
                let port = self
                    .port
                    .or(cfg_port)
                    .unwrap_or_else(|| DEFAULT_PORT.to_string());
                // Config wins over the stdout-tty heuristic when `default_mode`
                // is set; otherwise keep the original behaviour.
                let mode = config.default_mode.unwrap_or_else(|| {
                    if std::io::stdout().is_terminal() {
                        DefaultMode::Serve
                    } else {
                        DefaultMode::Term
                    }
                });
                match mode {
                    DefaultMode::Serve => Ok(Mode::Serve {
                        file: root,
                        host,
                        port,
                        theme: browser_theme,
                    }),
                    DefaultMode::Term => Ok(Mode::Term {
                        file: root,
                        watch: self.watch,
                        theme: config.term.theme.unwrap_or(ThemeChoice::Glow),
                        pager,
                    }),
                }
            }
        }
    }
}
