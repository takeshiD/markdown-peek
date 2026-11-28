use crate::config::{BrowserTheme, Config, DefaultMode};
use anyhow::Result;
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use mdpeek_gui::select::{ModelSpec, SelectConfig};
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
    /// Disable watch/live mode for `term` (one-shot render). `term` watches by
    /// default when stdout is a TTY.
    #[arg(long = "no-watch", global = true)]
    pub no_watch: bool,
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
    /// Choose reading-aid widgets per section and print the AdaptiveLayout JSON
    Select(SelectArg),
    /// Evaluate widget selection against labelled fixtures (accuracy, none, p50/p95)
    SelectEval(SelectEvalArg),
}

#[derive(Debug, Args)]
pub struct SelectArg {
    #[arg(value_name = "FILE")]
    pub file: PathBuf,
    /// Model to ask: a `[models.<name>]` entry (default: `[model] use`).
    #[arg(long, value_name = "NAME")]
    pub model: Option<String>,
    /// Accepted for symmetry with the design doc; output is always JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SelectEvalArg {
    /// Directory with `*.md` fixtures and `labels.json`.
    #[arg(value_name = "DIR", default_value = "tests/fixtures/select")]
    pub dir: PathBuf,
    /// Cold-cache runs per document for the latency percentiles.
    #[arg(long, default_value_t = 5)]
    pub runs: usize,
    /// Models to evaluate (`[models.<name>]`, repeatable; default: `[model] use`).
    /// A model that cannot run (e.g. its key is not set) is reported as not run.
    #[arg(long = "model", value_name = "NAME")]
    pub models: Vec<String>,
    /// Print the report as JSON instead of a table.
    #[arg(long)]
    pub json: bool,
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
    /// Model for reading aids in this session (`[models.<name>]`); the
    /// default stays `[model] use`.
    #[arg(long, value_name = "NAME")]
    pub model: Option<String>,
}

#[derive(Debug, Args)]
pub struct TermArg {
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,
    #[arg(long, value_enum)]
    pub theme: Option<ThemeChoice>,
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
        /// `--model` for this session.
        model: Option<String>,
    },
    Term {
        file: PathBuf,
        watch: bool,
        theme: ThemeChoice,
        /// Pager override from config: `None` uses `$PAGER`/`less -R`, `Some("")`
        /// disables paging, `Some(cmd)` runs `cmd`.
        pager: Option<String>,
    },
    Select {
        file: PathBuf,
        select: SelectConfig,
        model: ModelSpec,
    },
    SelectEval {
        dir: PathBuf,
        runs: usize,
        select: SelectConfig,
        models: Vec<ModelSpec>,
        json: bool,
    },
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
        let select_cfg = config.select_config();

        // `term` watches (interactive TUI) by default on a TTY; `--no-watch`
        // forces a one-shot render and `-w` forces watching (e.g. over a pipe).
        let term_watch = if self.no_watch {
            false
        } else if self.watch {
            true
        } else {
            std::io::stdout().is_terminal()
        };

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
                model: arg.model,
            }),
            Some(Commands::Term(arg)) => Ok(Mode::Term {
                file: arg.file.unwrap_or_else(|| PathBuf::from(DEFAULT_ROOT)),
                watch: term_watch,
                theme: arg.theme.or(config.term.theme).unwrap_or(ThemeChoice::Glow),
                pager,
            }),
            Some(Commands::Select(arg)) => {
                let models = config.models();
                let name = arg.model.unwrap_or_else(|| models.default.clone());
                let Some(model) = models.get(&name).cloned() else {
                    anyhow::bail!("{}", models.unavailable(&name));
                };
                Ok(Mode::Select {
                    file: arg.file,
                    select: select_cfg,
                    model,
                })
            }
            Some(Commands::SelectEval(arg)) => {
                let models = config.models();
                let names = if arg.models.is_empty() {
                    vec![models.default.clone()]
                } else {
                    arg.models
                };
                let mut list = Vec::new();
                for n in names {
                    let Some(m) = models.get(&n).cloned() else {
                        anyhow::bail!("{}", models.unavailable(&n));
                    };
                    list.push(m);
                }
                Ok(Mode::SelectEval {
                    dir: arg.dir,
                    runs: arg.runs,
                    select: select_cfg,
                    models: list,
                    json: arg.json,
                })
            }
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
                        model: None,
                    }),
                    DefaultMode::Term => Ok(Mode::Term {
                        file: root,
                        watch: term_watch,
                        theme: config.term.theme.unwrap_or(ThemeChoice::Glow),
                        pager,
                    }),
                }
            }
        }
    }
}
