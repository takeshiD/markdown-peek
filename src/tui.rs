//! Interactive full-screen viewer for `term -w` on a TTY.
//!
//! Renders the same ANSI-coloured output as the non-interactive path (via
//! [`crate::render_term`]), converts it into ratatui [`Text`] with
//! `ansi-to-tui`, and re-renders in place on file changes without the
//! flicker/scroll-loss of the clear+reprint fallback.

use crate::cli::ThemeChoice;
use crate::render_term;
use ansi_to_tui::IntoText;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::text::Text;
use ratatui::widgets::Paragraph;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::Once;
use std::time::Duration;

/// Clamp a scroll offset so it never scrolls past the last visible line.
///
/// `total_lines` is the number of rendered lines and `viewport_height` the
/// number of rows currently visible. The maximum offset keeps at least one
/// screenful of content in view (or 0 when everything fits).
pub fn clamp_scroll(offset: u16, total_lines: usize, viewport_height: u16) -> u16 {
    let max_offset = total_lines.saturating_sub(viewport_height as usize);
    let max_offset = u16::try_from(max_offset).unwrap_or(u16::MAX);
    offset.min(max_offset)
}

/// RAII guard owning the terminal in raw + alternate-screen mode. Dropping it
/// restores the terminal, so early returns and errors cannot leave the shell
/// in a broken state.
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
        let _ = self.terminal.show_cursor();
    }
}

/// Best-effort restoration of the terminal. Safe to call more than once.
fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

/// Install a panic hook (once) that restores the terminal before the default
/// hook prints the panic message, so a panic mid-render is still readable.
fn install_panic_hook() {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let original = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal();
            original(info);
        }));
    });
}

/// Render the file and convert its ANSI output to ratatui `Text`, degrading to
/// a plain-text error message so the viewer stays open on transient failures.
fn load(path: &PathBuf, theme: ThemeChoice) -> Text<'static> {
    match render_term(path, theme) {
        Ok(rendered) => rendered
            .into_text()
            .unwrap_or_else(|e| Text::raw(format!("Failed to parse rendered output: {e}"))),
        Err(e) => Text::raw(format!("Failed to render '{}': {e}", path.display())),
    }
}

/// Run the interactive viewer until the user quits with `q` (or Ctrl-C).
pub fn run_tui(path: PathBuf, theme: ThemeChoice) -> Result<()> {
    install_panic_hook();
    // Start watching before taking over the screen so no early change is lost.
    let changes = mdpeek_watcher::watch_events(&path);
    let mut guard = TerminalGuard::new()?;

    let mut text = load(&path, theme);
    let mut scroll: u16 = 0;

    loop {
        let viewport = guard.terminal.size()?.height;
        // Clamp against the current content/viewport (preserves position across
        // re-renders and honours `G`, which parks the offset at u16::MAX).
        scroll = clamp_scroll(scroll, text.lines.len(), viewport);

        guard.terminal.draw(|frame| {
            let paragraph = Paragraph::new(text.clone()).scroll((scroll, 0));
            frame.render_widget(paragraph, frame.area());
        })?;

        // Short poll so file-change events are picked up promptly on timeout.
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()?
                && key.kind != KeyEventKind::Release
            {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('q'), _) => break,
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                    (KeyCode::Char('j') | KeyCode::Down, _) => {
                        scroll = scroll.saturating_add(1);
                    }
                    (KeyCode::Char('k') | KeyCode::Up, _) => {
                        scroll = scroll.saturating_sub(1);
                    }
                    (KeyCode::Char('g'), _) => scroll = 0,
                    (KeyCode::Char('G'), _) => scroll = u16::MAX,
                    (KeyCode::PageDown, _) => scroll = scroll.saturating_add(viewport),
                    (KeyCode::PageUp, _) => scroll = scroll.saturating_sub(viewport),
                    _ => {}
                }
            }
        } else {
            // Timed out: drain any pending change events and re-render once.
            let mut changed = false;
            while changes.try_recv().is_ok() {
                changed = true;
            }
            if changed {
                text = load(&path, theme);
            }
        }
    }

    // `guard` drops here, restoring the terminal.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::clamp_scroll;

    #[test]
    fn clamp_zero_when_content_fits() {
        // 5 lines, 10 rows visible: nothing to scroll.
        assert_eq!(clamp_scroll(3, 5, 10), 0);
        assert_eq!(clamp_scroll(0, 5, 10), 0);
    }

    #[test]
    fn clamp_within_range_is_unchanged() {
        // 100 lines, 20 rows: max offset is 80.
        assert_eq!(clamp_scroll(50, 100, 20), 50);
        assert_eq!(clamp_scroll(80, 100, 20), 80);
    }

    #[test]
    fn clamp_caps_at_max_offset() {
        // Requesting past the end (e.g. `G` -> u16::MAX) parks at last screen.
        assert_eq!(clamp_scroll(u16::MAX, 100, 20), 80);
        assert_eq!(clamp_scroll(1000, 100, 20), 80);
    }

    #[test]
    fn clamp_handles_zero_viewport() {
        assert_eq!(clamp_scroll(u16::MAX, 100, 0), 100);
    }

    #[test]
    fn clamp_saturates_over_u16() {
        // More lines than u16 can hold: max offset saturates, no overflow.
        let total = usize::from(u16::MAX) + 1_000;
        assert_eq!(clamp_scroll(u16::MAX, total, 10), u16::MAX);
    }
}
