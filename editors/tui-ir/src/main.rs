//! `mdpeek-tui-ir` — an IR-driven ratatui renderer (Layer 5).
//!
//! Reads a UI IR JSON document (from a file or stdin) and renders it in the
//! terminal. This is the "TUI renderer (IR 対応)" half of Layer 5: the same IR
//! JSON the web renderer consumes is drawn here as a ratatui view, so analysis
//! is never re-implemented per frontend (AGENTS.md §1.1).
//!
//! Input accepts either a top-level array of nodes (`[ {"kind": ...}, ... ]`)
//! or a `GuiCacheEntry`-shaped object (`{ "ui_ir": [ ... ] }`, §4.3).
//!
//! Usage:
//!   mdpeek-tui-ir [FILE]                 # interactive viewer (reads stdin if no FILE)
//!   mdpeek-tui-ir --print [FILE]         # render one frame to stdout (no TTY needed)
//!   mdpeek-tui-ir --reveal-line N FILE   # reading position for spoiler control (§9.3)
//!
//! Keys (interactive): q/Esc quit · ↑/↓ or j/k scroll · PgUp/PgDn · g/G top/bottom.

mod ir;
mod render;

use std::io::{self, Read};

use anyhow::{Context, Result};
use ir::UiNode;
use render::RenderCtx;

struct Args {
    file: Option<String>,
    print: bool,
    reveal_line: Option<u32>,
}

fn parse_args() -> Result<Args> {
    let mut file = None;
    let mut print = false;
    let mut reveal_line = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--print" | "-p" => print = true,
            "--reveal-line" => {
                let v = it.next().context("--reveal-line needs a value")?;
                reveal_line = Some(v.parse().context("--reveal-line must be a number")?);
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other if other.starts_with('-') => {
                anyhow::bail!("unknown flag: {other}");
            }
            other => file = Some(other.to_string()),
        }
    }
    Ok(Args {
        file,
        print,
        reveal_line,
    })
}

fn print_help() {
    println!(
        "mdpeek-tui-ir — IR-driven ratatui renderer (Layer 5)\n\n\
         USAGE:\n  mdpeek-tui-ir [OPTIONS] [FILE]\n\n\
         OPTIONS:\n\
         \x20 -p, --print          Render one frame to stdout (no TTY required)\n\
         \x20     --reveal-line N   Reading position for spoiler control (§9.3)\n\
         \x20 -h, --help           Show this help\n\n\
         FILE defaults to stdin. Input is UI IR JSON: an array of nodes or\n\
         a {{ \"ui_ir\": [...] }} object."
    );
}

/// The IR JSON may be a bare array or a `{ "ui_ir": [...] }` envelope.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Document {
    Bare(Vec<UiNode>),
    Envelope { ui_ir: Vec<UiNode> },
}

impl Document {
    fn into_nodes(self) -> Vec<UiNode> {
        match self {
            Document::Bare(v) => v,
            Document::Envelope { ui_ir } => ui_ir,
        }
    }
}

fn load(args: &Args) -> Result<Vec<UiNode>> {
    let raw = match &args.file {
        Some(path) => std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {path}"))?,
        None => {
            let mut s = String::new();
            io::stdin()
                .read_to_string(&mut s)
                .context("failed to read stdin")?;
            s
        }
    };
    // Unknown `kind`s are rejected here (serde has no matching variant), which
    // enforces the registry allowlist — the renderer never sees a node it
    // cannot draw (DESIGN.md: 未知 component は reject).
    let doc: Document = serde_json::from_str(&raw).context("invalid UI IR JSON")?;
    Ok(doc.into_nodes())
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let nodes = load(&args)?;
    let ctx = RenderCtx {
        reveal_line: args.reveal_line,
    };

    if args.print {
        print_frame(&nodes, ctx);
        return Ok(());
    }
    tui::run(&nodes, ctx)
}

/// Non-interactive rendering: dump the rendered lines as plain text. Handy for
/// pipes, snapshots and CI where there is no TTY.
fn print_frame(nodes: &[UiNode], ctx: RenderCtx) {
    for line in render::render(nodes, ctx) {
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        println!("{}", text.trim_end());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bare_array_and_envelope() {
        let bare: Document = serde_json::from_str(
            r#"[{"kind":"Callout","severity":"info","body":"x"}]"#,
        )
        .unwrap();
        assert_eq!(bare.into_nodes().len(), 1);

        let env: Document = serde_json::from_str(
            r#"{"ui_ir":[{"kind":"Callout","severity":"info","body":"x"},
                        {"kind":"Callout","severity":"warning","body":"y"}]}"#,
        )
        .unwrap();
        assert_eq!(env.into_nodes().len(), 2);
    }

    #[test]
    fn bundled_fixture_parses() {
        let raw = include_str!("../fixtures/design-doc.json");
        let doc: Document = serde_json::from_str(raw).expect("fixture is valid IR");
        assert!(!doc.into_nodes().is_empty());
    }
}

/// Interactive ratatui viewer.
mod tui {
    use super::*;
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use crossterm::{execute, terminal};
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::Line;
    use ratatui::widgets::{Block, Borders, Paragraph};
    use ratatui::Terminal;
    use ratatui::prelude::CrosstermBackend;
    use std::time::Duration;

    pub fn run(nodes: &[UiNode], ctx: RenderCtx) -> Result<()> {
        let lines = render::render(nodes, ctx);

        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, terminal::EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut term = Terminal::new(backend)?;

        let res = event_loop(&mut term, &lines);

        // Always restore the terminal, even on error.
        terminal::disable_raw_mode().ok();
        execute!(term.backend_mut(), terminal::LeaveAlternateScreen).ok();
        term.show_cursor().ok();
        res
    }

    fn event_loop<B: ratatui::backend::Backend>(
        term: &mut Terminal<B>,
        lines: &[Line<'static>],
    ) -> Result<()> {
        let mut scroll: u16 = 0;
        loop {
            let mut viewport_height = 0u16;
            term.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(1), Constraint::Length(1)])
                    .split(f.area());
                // reserve 2 rows for the border.
                viewport_height = chunks[0].height.saturating_sub(2);

                let para = Paragraph::new(lines.to_vec())
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" mdpeek · Generated UI (IR) "),
                    )
                    .scroll((scroll, 0));
                f.render_widget(para, chunks[0]);

                let help = Paragraph::new(Line::from(
                    " q quit · ↑/↓ j/k scroll · PgUp/PgDn · g/G top/bottom ",
                ))
                .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM));
                f.render_widget(help, chunks[1]);
            })?;

            let max_scroll = (lines.len() as u16).saturating_sub(viewport_height.max(1));

            if !event::poll(Duration::from_millis(200))? {
                continue;
            }
            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                match k.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Down | KeyCode::Char('j') => {
                        scroll = (scroll + 1).min(max_scroll)
                    }
                    KeyCode::Up | KeyCode::Char('k') => scroll = scroll.saturating_sub(1),
                    KeyCode::PageDown => {
                        scroll = (scroll + viewport_height.max(1)).min(max_scroll)
                    }
                    KeyCode::PageUp => scroll = scroll.saturating_sub(viewport_height.max(1)),
                    KeyCode::Char('g') | KeyCode::Home => scroll = 0,
                    KeyCode::Char('G') | KeyCode::End => scroll = max_scroll,
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
