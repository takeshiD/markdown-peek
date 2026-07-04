//! Terminal rendering of a [`RepoReport`](super::RepoReport).

use super::RepoReport;
use owo_colors::OwoColorize;
use std::fmt::Write;

/// Render the report to a coloured (or plain) string.
pub fn render_terminal(report: &RepoReport, color: bool) -> String {
    let mut out = String::new();
    let h = |s: &str| section_header(s, color);

    // Header.
    let _ = writeln!(
        out,
        "{}",
        paint(
            &format!("Repository-aware view — {}", report.document),
            color,
            Style::Title
        )
    );
    let _ = writeln!(out, "  root: {}", report.root.display());
    let git_state = if report.in_git_repo {
        paint("git repository", color, Style::Ok)
    } else {
        paint("not a git repository", color, Style::Warn)
    };
    let _ = writeln!(out, "  {git_state}");
    out.push('\n');

    // Worktrees.
    if !report.worktrees.is_empty() {
        let _ = writeln!(out, "{}", h("Worktrees"));
        for wt in &report.worktrees {
            let marker = if wt.is_current { "*" } else { " " };
            let branch = wt
                .branch
                .as_deref()
                .map(|b| b.to_string())
                .unwrap_or_else(|| {
                    if wt.is_detached {
                        "(detached)".into()
                    } else {
                        "-".into()
                    }
                });
            let line = format!("{marker} {}  [{}]", wt.path.display(), branch);
            if wt.is_current {
                let _ = writeln!(out, "  {}", paint(&line, color, Style::Ok));
            } else {
                let _ = writeln!(out, "  {line}");
            }
        }
        out.push('\n');
    }

    // Manifests.
    if !report.manifests.is_empty() {
        let _ = writeln!(out, "{}", h("Manifests"));
        for m in &report.manifests {
            let name = m.name.as_deref().unwrap_or("<unnamed>");
            let ver = m.version.as_deref().unwrap_or("?");
            let _ = writeln!(
                out,
                "  {} — {} v{}",
                paint(&m.file, color, Style::Emphasis),
                name,
                ver
            );
            if !m.binaries.is_empty() {
                let _ = writeln!(out, "      binaries: {}", m.binaries.join(", "));
            }
            if !m.scripts.is_empty() {
                let _ = writeln!(out, "      scripts:  {}", m.scripts.join(", "));
            }
            if let Some(readme) = &m.readme {
                if readme.exists {
                    let _ = writeln!(
                        out,
                        "      readme:   {}",
                        paint(&readme.path, color, Style::Ok)
                    );
                } else {
                    let _ = writeln!(
                        out,
                        "      readme:   {} {}",
                        paint(&readme.path, color, Style::Bad),
                        paint("(missing)", color, Style::Bad)
                    );
                }
            }
        }
        out.push('\n');
    }

    // Document references.
    let _ = writeln!(out, "{}", h("Document references"));
    let broken: Vec<_> = report.doc_refs.broken().collect();
    if report.doc_refs.refs.is_empty() {
        let _ = writeln!(out, "  no local file references found");
    } else {
        let _ = writeln!(
            out,
            "  {} resolved, {} broken",
            paint(&report.doc_refs.ok_count().to_string(), color, Style::Ok),
            paint(
                &broken.len().to_string(),
                color,
                if broken.is_empty() {
                    Style::Ok
                } else {
                    Style::Bad
                }
            ),
        );
        for r in broken {
            let _ = writeln!(
                out,
                "  {} {} ({})",
                paint("✗", color, Style::Bad),
                r.target,
                format!("{:?}", r.kind).to_lowercase()
            );
        }
    }
    out.push('\n');

    // ADRs.
    if !report.adrs.is_empty() {
        let _ = writeln!(out, "{}", h("Architecture Decision Records"));
        for adr in &report.adrs {
            let title = adr.title.as_deref().unwrap_or("");
            let _ = writeln!(
                out,
                "  {} {}",
                paint(&adr.path, color, Style::Emphasis),
                title
            );
            if let Some(c) = &adr.last_commit {
                let _ = writeln!(
                    out,
                    "      last: {} {} — {} ({} commit{})",
                    paint(&c.hash, color, Style::Dim),
                    c.date,
                    c.subject,
                    c.commit_count,
                    if c.commit_count == 1 { "" } else { "s" },
                );
            } else {
                let _ = writeln!(
                    out,
                    "      {}",
                    paint("untracked by git", color, Style::Warn)
                );
            }
        }
        out.push('\n');
    }

    // TODOs.
    let _ = writeln!(out, "{}", h("TODO / FIXME markers"));
    if report.todos.items.is_empty() {
        let _ = writeln!(out, "  none found");
    } else {
        let _ = writeln!(
            out,
            "  {} total — {} linked to an issue, {} orphaned",
            report.todos.items.len(),
            paint(&report.todos.linked().to_string(), color, Style::Ok),
            paint(&report.todos.orphaned().to_string(), color, Style::Warn),
        );
        // Show at most the first 20 to keep the report readable.
        for item in report.todos.items.iter().take(20) {
            let loc = format!("{}:{}", item.file, item.line);
            let issue = item
                .issue
                .as_deref()
                .map(|i| format!(" [{}]", paint(i, color, Style::Ok)))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "  {} {} {}{}",
                paint(&item.kind, color, Style::Emphasis),
                paint(&loc, color, Style::Dim),
                truncate(&item.text, 60),
                issue,
            );
        }
        if report.todos.items.len() > 20 {
            let _ = writeln!(out, "  … and {} more", report.todos.items.len() - 20);
        }
    }

    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max).collect();
        t.push('…');
        t
    }
}

#[derive(Clone, Copy)]
enum Style {
    Title,
    Ok,
    Bad,
    Warn,
    Emphasis,
    Dim,
    Header,
}

fn section_header(s: &str, color: bool) -> String {
    paint(&format!("── {s} ──"), color, Style::Header)
}

fn paint(s: &str, color: bool, style: Style) -> String {
    if !color {
        return s.to_string();
    }
    match style {
        Style::Title => s.bold().underline().to_string(),
        Style::Ok => s.green().to_string(),
        Style::Bad => s.red().bold().to_string(),
        Style::Warn => s.yellow().to_string(),
        Style::Emphasis => s.cyan().to_string(),
        Style::Dim => s.dimmed().to_string(),
        Style::Header => s.bold().to_string(),
    }
}
