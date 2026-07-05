//! Interactive full-screen viewer for `term` watch mode on a TTY.
//!
//! Renders the same ANSI-coloured output as the non-interactive path (via
//! [`crate::render_term`]), converts it into ratatui [`Text`] with
//! `ansi-to-tui`, and re-renders in place on file changes without the
//! flicker/scroll-loss of the clear+reprint fallback. Supports wrapping,
//! half-page scrolling, an in-app help overlay, and a vim-style `/` search with
//! match highlighting and `n`/`N` navigation.

use crate::cli::ThemeChoice;
use crate::render_term;
use ansi_to_tui::IntoText;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::Once;
use std::time::Duration;

/// Clamp a scroll offset so it never scrolls past the last visible line.
///
/// `total_lines` is the number of rendered (wrapped) lines and `viewport_height`
/// the number of rows currently visible. The maximum offset keeps at least one
/// screenful of content in view (or 0 when everything fits).
pub fn clamp_scroll(offset: u16, total_lines: usize, viewport_height: u16) -> u16 {
    let max_offset = total_lines.saturating_sub(viewport_height as usize);
    let max_offset = u16::try_from(max_offset).unwrap_or(u16::MAX);
    offset.min(max_offset)
}

/// Approximate the number of wrapped rows a logical line occupies at `width`.
fn wrapped_height(text: &str, width: u16) -> usize {
    if width == 0 {
        return 1;
    }
    text.chars().count().max(1).div_ceil(width as usize)
}

/// The wrapped-row offset at which logical line `idx` begins (sum of the wrapped
/// heights of the preceding lines). Used to scroll a search match into view.
fn wrapped_row_of(plain: &[String], idx: usize, width: u16) -> u16 {
    let rows: usize = plain
        .iter()
        .take(idx)
        .map(|l| wrapped_height(l, width))
        .sum();
    u16::try_from(rows).unwrap_or(u16::MAX)
}

/// Total wrapped rows for the whole document.
fn total_wrapped(plain: &[String], width: u16) -> usize {
    plain.iter().map(|l| wrapped_height(l, width)).sum()
}

/// Line indices that contain `query` (ASCII case-insensitive). Empty query → no
/// matches.
fn find_matches(plain: &[String], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }
    let q = query.to_ascii_lowercase();
    plain
        .iter()
        .enumerate()
        .filter(|(_, l)| l.to_ascii_lowercase().contains(&q))
        .map(|(i, _)| i)
        .collect()
}

/// The plain text of each line, for searching (spans concatenated).
fn plain_lines(text: &Text) -> Vec<String> {
    text.lines
        .iter()
        .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect()
}

/// Rebuild a styled line with every occurrence of `query` highlighted, splitting
/// spans at match boundaries and overlaying the highlight style onto the
/// original one. `current` picks a distinct colour for the active match's line.
fn highlight_line(line: &Line<'static>, query: &str, current: bool) -> Line<'static> {
    let q = query.to_ascii_lowercase();
    if q.is_empty() {
        return line.clone();
    }
    let full: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let lower = full.to_ascii_lowercase(); // same byte length (ASCII fold)

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut from = 0;
    while let Some(p) = lower[from..].find(&q) {
        let s = from + p;
        let e = s + q.len();
        ranges.push((s, e));
        from = e;
    }
    if ranges.is_empty() {
        return line.clone();
    }

    let hl = if current {
        Style::default().bg(Color::LightRed).fg(Color::Black)
    } else {
        Style::default().bg(Color::Yellow).fg(Color::Black)
    };

    let mut out: Vec<Span<'static>> = Vec::new();
    let mut base = 0usize; // byte offset of the current span within `full`
    for span in &line.spans {
        let content = span.content.as_ref();
        let span_start = base;
        let span_end = base + content.len();
        let mut pos = span_start;
        while pos < span_end {
            let in_match = ranges.iter().find(|(ms, me)| pos >= *ms && pos < *me);
            let (seg_end, style) = match in_match {
                Some((_, me)) => ((*me).min(span_end), span.style.patch(hl)),
                None => {
                    let next = ranges
                        .iter()
                        .map(|(ms, _)| *ms)
                        .filter(|ms| *ms > pos)
                        .min()
                        .unwrap_or(span_end)
                        .min(span_end);
                    (next, span.style)
                }
            };
            let seg = &content[(pos - span_start)..(seg_end - span_start)];
            out.push(Span::styled(seg.to_string(), style));
            pos = seg_end;
        }
        base = span_end;
    }
    Line::from(out)
}

/// Whether the viewer is accepting scroll/nav keys or typing a search query.
enum Mode {
    Normal,
    Search,
}

struct App {
    text: Text<'static>,
    plain: Vec<String>,
    scroll: u16,
    show_help: bool,
    mode: Mode,
    input: String,
    query: Option<String>,
    matches: Vec<usize>,
    current: usize,
}

impl App {
    fn new(text: Text<'static>) -> Self {
        let plain = plain_lines(&text);
        Self {
            text,
            plain,
            scroll: 0,
            show_help: false,
            mode: Mode::Normal,
            input: String::new(),
            query: None,
            matches: Vec::new(),
            current: 0,
        }
    }

    /// Replace the document (on file change) and refresh any active search.
    fn reload(&mut self, text: Text<'static>) {
        self.plain = plain_lines(&text);
        self.text = text;
        if let Some(q) = self.query.clone() {
            self.matches = find_matches(&self.plain, &q);
            if self.current >= self.matches.len() {
                self.current = 0;
            }
        }
    }

    /// Scroll so the given logical line sits near the top with a little context.
    fn scroll_to(&mut self, line_idx: usize, width: u16) {
        self.scroll = wrapped_row_of(&self.plain, line_idx, width).saturating_sub(2);
    }

    fn move_match(&mut self, delta: i32, width: u16) {
        if self.matches.is_empty() {
            return;
        }
        let n = self.matches.len() as i32;
        self.current = (((self.current as i32 + delta) % n + n) % n) as usize;
        let line = self.matches[self.current];
        self.scroll_to(line, width);
    }

    /// The document with search matches highlighted (or the plain styled text).
    fn display(&self) -> Text<'static> {
        match &self.query {
            Some(q) if !q.is_empty() => {
                let cur_line = self.matches.get(self.current).copied();
                let ql = q.to_ascii_lowercase();
                let lines: Vec<Line> = self
                    .text
                    .lines
                    .iter()
                    .enumerate()
                    .map(|(i, line)| {
                        if self.plain[i].to_ascii_lowercase().contains(&ql) {
                            highlight_line(line, q, Some(i) == cur_line)
                        } else {
                            line.clone()
                        }
                    })
                    .collect();
                Text::from(lines)
            }
            _ => self.text.clone(),
        }
    }
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

const HELP_LINES: &[(&str, &str)] = &[
    ("q", "quit"),
    ("j / k, ↓ / ↑", "scroll one line"),
    ("Ctrl-d / Ctrl-u", "half page down / up"),
    ("PgDn / PgUp", "page down / up"),
    ("g / G", "top / bottom"),
    ("/", "search"),
    ("n / N", "next / previous match"),
    ("Esc", "clear search"),
    ("?", "toggle this help"),
];

fn help_widget() -> Paragraph<'static> {
    let lines: Vec<Line> = HELP_LINES
        .iter()
        .map(|(keys, desc)| {
            Line::from(vec![
                Span::styled(format!("  {keys:<16}"), Style::default().fg(Color::Cyan)),
                Span::raw((*desc).to_string()),
            ])
        })
        .collect();
    Paragraph::new(Text::from(lines)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Keybindings "),
    )
}

/// A rectangle of the given size centred within `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

/// The bottom status line: search input while typing, match count when a search
/// is active, otherwise a short hint.
fn status_line(app: &App) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    match app.mode {
        Mode::Search => Line::from(format!("/{}", app.input)),
        Mode::Normal => match &app.query {
            Some(q) if !app.matches.is_empty() => Line::from(vec![Span::styled(
                format!("/{}  [{}/{}]", q, app.current + 1, app.matches.len()),
                dim,
            )]),
            Some(q) => Line::from(vec![Span::styled(format!("/{q}  [no matches]"), dim)]),
            None => Line::from(vec![Span::styled(
                "q quit · j/k scroll · Ctrl-d/u half-page · / search · ? help".to_string(),
                dim,
            )]),
        },
    }
}

/// Handle one key event. Returns `true` when the viewer should quit.
fn handle_key(app: &mut App, key: KeyEvent, viewport: u16, width: u16) -> bool {
    if app.show_help {
        match key.code {
            KeyCode::Char('q') => return true,
            _ => app.show_help = false,
        }
        return false;
    }

    match app.mode {
        Mode::Search => {
            match key.code {
                KeyCode::Enter => {
                    if app.input.is_empty() {
                        app.mode = Mode::Normal;
                    } else {
                        let q = std::mem::take(&mut app.input);
                        app.matches = find_matches(&app.plain, &q);
                        app.current = 0;
                        app.query = Some(q);
                        app.mode = Mode::Normal;
                        if let Some(&line) = app.matches.first() {
                            app.scroll_to(line, width);
                        }
                    }
                }
                KeyCode::Esc => {
                    app.mode = Mode::Normal;
                    app.input.clear();
                }
                KeyCode::Backspace => {
                    app.input.pop();
                }
                KeyCode::Char(c) => app.input.push(c),
                _ => {}
            }
            false
        }
        Mode::Normal => {
            match (key.code, key.modifiers) {
                (KeyCode::Char('q'), _) => return true,
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => return true,
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    app.scroll = app.scroll.saturating_add(viewport / 2);
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                    app.scroll = app.scroll.saturating_sub(viewport / 2);
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    app.scroll = app.scroll.saturating_add(1);
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    app.scroll = app.scroll.saturating_sub(1);
                }
                (KeyCode::Char('g'), _) => app.scroll = 0,
                (KeyCode::Char('G'), _) => app.scroll = u16::MAX,
                (KeyCode::PageDown, _) => app.scroll = app.scroll.saturating_add(viewport),
                (KeyCode::PageUp, _) => app.scroll = app.scroll.saturating_sub(viewport),
                (KeyCode::Char('/'), _) => {
                    app.mode = Mode::Search;
                    app.input.clear();
                }
                (KeyCode::Char('n'), _) => app.move_match(1, width),
                (KeyCode::Char('N'), _) => app.move_match(-1, width),
                (KeyCode::Char('?'), _) => app.show_help = true,
                (KeyCode::Esc, _) => {
                    app.query = None;
                    app.matches.clear();
                }
                _ => {}
            }
            false
        }
    }
}

/// Run the interactive viewer until the user quits with `q` (or Ctrl-C).
pub fn run_tui(path: PathBuf, theme: ThemeChoice) -> Result<()> {
    install_panic_hook();
    // Start watching before taking over the screen so no early change is lost.
    let changes = mdpeek_watcher::watch_events(&path);
    let mut guard = TerminalGuard::new()?;

    let mut app = App::new(load(&path, theme));

    loop {
        let size = guard.terminal.size()?;
        let width = size.width;
        let content_height = size.height.saturating_sub(1);
        let total = total_wrapped(&app.plain, width);
        app.scroll = clamp_scroll(app.scroll, total, content_height);

        let display = app.display();
        guard.terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);
            let content = chunks[0];
            let status = chunks[1];

            let paragraph = Paragraph::new(display)
                .wrap(Wrap { trim: false })
                .scroll((app.scroll, 0));
            frame.render_widget(paragraph, content);
            frame.render_widget(Paragraph::new(status_line(&app)), status);

            if let Mode::Search = app.mode {
                // Place the cursor after the "/" + typed query.
                let cx = status.x + 1 + app.input.chars().count() as u16;
                frame.set_cursor_position(Position::new(cx.min(status.x + status.width), status.y));
            }

            if app.show_help {
                let popup = centered_rect(48, HELP_LINES.len() as u16 + 2, area);
                frame.render_widget(Clear, popup);
                frame.render_widget(help_widget(), popup);
            }
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()?
                && key.kind != KeyEventKind::Release
                && handle_key(&mut app, key, content_height, width)
            {
                break;
            }
        } else {
            // Timed out: drain any pending change events and re-render once.
            let mut changed = false;
            while changes.try_recv().is_ok() {
                changed = true;
            }
            if changed {
                app.reload(load(&path, theme));
            }
        }
    }

    // `guard` drops here, restoring the terminal.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_zero_when_content_fits() {
        assert_eq!(clamp_scroll(3, 5, 10), 0);
        assert_eq!(clamp_scroll(0, 5, 10), 0);
    }

    #[test]
    fn clamp_within_range_is_unchanged() {
        assert_eq!(clamp_scroll(50, 100, 20), 50);
        assert_eq!(clamp_scroll(80, 100, 20), 80);
    }

    #[test]
    fn clamp_caps_at_max_offset() {
        assert_eq!(clamp_scroll(u16::MAX, 100, 20), 80);
        assert_eq!(clamp_scroll(1000, 100, 20), 80);
    }

    #[test]
    fn clamp_handles_zero_viewport() {
        assert_eq!(clamp_scroll(u16::MAX, 100, 0), 100);
    }

    #[test]
    fn clamp_saturates_over_u16() {
        let total = usize::from(u16::MAX) + 1_000;
        assert_eq!(clamp_scroll(u16::MAX, total, 10), u16::MAX);
    }

    #[test]
    fn wrapped_height_counts_rows() {
        assert_eq!(wrapped_height("", 10), 1);
        assert_eq!(wrapped_height("abc", 10), 1);
        assert_eq!(wrapped_height("abcdefghij", 10), 1);
        assert_eq!(wrapped_height("abcdefghijk", 10), 2);
        assert_eq!(wrapped_height("anything", 0), 1);
    }

    #[test]
    fn wrapped_row_of_sums_preceding_lines() {
        let plain = vec!["a".to_string(), "b".repeat(25), "c".to_string()];
        // line 0 at row 0; line 1 at row 1; line 2 after 1 + ceil(25/10)=3 => row 4.
        assert_eq!(wrapped_row_of(&plain, 0, 10), 0);
        assert_eq!(wrapped_row_of(&plain, 1, 10), 1);
        assert_eq!(wrapped_row_of(&plain, 2, 10), 4);
    }

    #[test]
    fn find_matches_is_case_insensitive() {
        let plain = vec![
            "The Quick brown".to_string(),
            "fox jumps".to_string(),
            "over the lazy dog".to_string(),
        ];
        assert_eq!(find_matches(&plain, "the"), vec![0, 2]);
        assert_eq!(find_matches(&plain, "FOX"), vec![1]);
        assert!(find_matches(&plain, "zzz").is_empty());
        assert!(find_matches(&plain, "").is_empty());
    }

    #[test]
    fn highlight_line_marks_match_and_preserves_text() {
        let line = Line::from("hello world hello");
        let out = highlight_line(&line, "hello", false);
        // Text is preserved end to end.
        let joined: String = out.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "hello world hello");
        // Two occurrences get a background; the middle text does not.
        let highlighted = out
            .spans
            .iter()
            .filter(|s| s.style.bg == Some(Color::Yellow))
            .count();
        assert_eq!(highlighted, 2);
    }

    #[test]
    fn highlight_line_current_uses_distinct_colour() {
        let line = Line::from("find me");
        let out = highlight_line(&line, "me", true);
        assert!(
            out.spans
                .iter()
                .any(|s| s.style.bg == Some(Color::LightRed))
        );
    }
}
