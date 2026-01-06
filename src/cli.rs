use anyhow::Result;
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
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
    #[arg(short = 'w', long = "watch", global = true)]
    pub watch: bool,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Serve hot reload previewer on your browser
    Serve(ServeArg),
    /// Display pretty rendered markdown on your terminal
    Term(TermArg),
}

#[derive(Debug, Args)]
pub struct ServeArg {
    #[arg(value_name = "FILE", default_value = DEFAULT_ROOT)]
    pub file: PathBuf,
    #[arg(long, value_name = "HOST", default_value = DEFAULT_HOST)]
    pub host: String,
    #[arg(long, value_name = "PORT", default_value = DEFAULT_PORT)]
    pub port: String,
}

#[derive(Debug, Args)]
pub struct TermArg {
    #[arg(value_name = "FILE", default_value = "README.md")]
    pub file: PathBuf,
    #[arg(long, value_enum, default_value = "glow")]
    pub theme: ThemeChoice,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
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
    Serve {
        file: PathBuf,
        host: String,
        port: String,
        watch: bool,
    },
    Term {
        file: PathBuf,
        watch: bool,
        theme: ThemeChoice,
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
    pub fn resolve_mode(self) -> Result<Mode> {
        let root = match self.root {
            Some(root) => root,
            None => PathBuf::from(DEFAULT_ROOT),
        };
        let host = match self.host {
            Some(host) => host,
            None => DEFAULT_HOST.to_string(),
        };
        let port = match self.port {
            Some(port) => port,
            None => DEFAULT_PORT.to_string(),
        };
        match self.command {
            Some(Commands::Serve(arg)) => Ok(Mode::Serve {
                file: arg.file,
                host: arg.host,
                port: arg.port,
                watch: self.watch,
            }),
            Some(Commands::Term(arg)) => Ok(Mode::Term {
                file: arg.file,
                watch: self.watch,
                theme: arg.theme,
            }),
            None => {
                if std::io::stdout().is_terminal() {
                    Ok(Mode::Serve {
                        file: root,
                        host,
                        port,
                        watch: self.watch,
                    })
                } else {
                    Ok(Mode::Term {
                        file: root,
                        watch: false,
                        theme: ThemeChoice::Glow,
                    })
                }
            }
        }
    }
}
