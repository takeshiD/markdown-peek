use anyhow::Result;
use clap::{
    Args, CommandFactory, FromArgMatches, Parser, Subcommand,
    builder::{Styles, styling::AnsiColor},
};
use std::{io::IsTerminal, path::PathBuf};

#[derive(Debug, Parser)]
#[command(author, name = "mdpeek", about = "markdown viewer in browser and terminal", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
    #[arg(value_name = "FILE")]
    pub root: Option<PathBuf>,
    #[arg(short, long, global = true)]
    pub watch: bool,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Serve(FileArg),
    Term(FileArg),
}

#[derive(Debug, Args)]
pub struct FileArg {
    #[arg(value_name = "FILE")]
    pub file: PathBuf,
}

pub enum Mode {
    Serve { file: PathBuf, watch: bool },
    Term { file: PathBuf, watch: bool },
}

impl Cli {
    pub fn parse_with_color() -> Result<Self, clap::Error> {
        const STYLES: Styles = Styles::styled()
            .header(AnsiColor::Green.on_default().bold())
            .usage(AnsiColor::Green.on_default().bold())
            .literal(AnsiColor::Blue.on_default())
            .placeholder(AnsiColor::Cyan.on_default().bold());
        let cmd = Self::command().styles(STYLES);
        Self::from_arg_matches(&cmd.get_matches())
    }
    pub fn resolve_mode(self) -> Result<Mode> {
        match self.command {
            Some(Commands::Serve(arg)) => Ok(Mode::Serve {
                file: arg.file,
                watch: self.watch,
            }),
            Some(Commands::Term(arg)) => Ok(Mode::Term {
                file: arg.file,
                watch: self.watch,
            }),
            None => {
                let root = match self.root {
                    Some(root) => root,
                    None => PathBuf::from("README.md"),
                };
                if std::io::stdout().is_terminal() {
                    Ok(Mode::Serve {
                        file: root,
                        watch: self.watch,
                    })
                } else {
                    Ok(Mode::Term {
                        file: root,
                        watch: false,
                    })
                }
            }
        }
    }
}
