use anyhow::Result;
use clap::{
    Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum,
    builder::{Styles, styling::AnsiColor},
};
use std::{io::IsTerminal, path::PathBuf};

#[derive(Debug, Parser)]
#[command(author, name = "mdpeek", about, long_about = None, version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
    /// Target file by default "README.md"
    #[arg(value_name = "FILE")]
    pub root: Option<PathBuf>,
    #[arg(short, long, global = true)]
    pub watch: bool,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Serve hot reload previewer on your browser
    Serve(FileArg),
    /// Display pretty rendered markdown on your terminal
    Term(TermArg),
}

#[derive(Debug, Args)]
pub struct FileArg {
    #[arg(value_name = "FILE", default_value = "README.md")]
    pub file: PathBuf,
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
            None => PathBuf::from("README.md"),
        };
        match self.command {
            Some(Commands::Serve(arg)) => Ok(Mode::Serve {
                file: arg.file,
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
