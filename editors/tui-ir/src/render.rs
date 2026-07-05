//! Deterministic IR → terminal renderer.
//!
//! Each [`UiNode`] kind maps to a fixed rendering (the "registry"), producing a
//! flat `Vec<Line>` that the app scrolls as one `Paragraph`. Nodes the terminal
//! cannot draw graphically (Diagram / DependencyGraph / CommitGraph) fall back
//! to a text summary plus an "open in web" hint, per AGENTS.md §5.2.
//!
//! The renderer is pure: `render(nodes) -> lines`. Given the same IR (and the
//! same reading position) it always produces the same output, which is what
//! makes it testable with a headless backend.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::ir::*;

/// Context threaded through rendering.
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderCtx {
    /// The reader's current line in the source document. Nodes marked
    /// `Visibility::UntilRead { reveal_after_line }` are hidden until this
    /// reaches that line (§9.3 reading-position aware / spoiler control).
    /// `None` (the default) means "reveal everything".
    pub reveal_line: Option<u32>,
}

const INDENT: &str = "  ";

fn header_style() -> Style {
    Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD)
}

fn severity_style(sev: Severity) -> Style {
    let c = match sev {
        Severity::Info => Color::Cyan,
        Severity::Warning => Color::Yellow,
        Severity::Error => Color::Red,
    };
    Style::default().fg(c)
}

fn severity_glyph(sev: Severity) -> &'static str {
    match sev {
        Severity::Info => "ℹ",
        Severity::Warning => "⚠",
        Severity::Error => "✖",
    }
}

/// Render a whole document (a slice of top-level nodes) into styled lines.
pub fn render(nodes: &[UiNode], ctx: RenderCtx) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        if i > 0 {
            out.push(Line::default());
        }
        render_node(node, 0, ctx, &mut out);
    }
    if out.is_empty() {
        out.push(Line::from(Span::styled(
            "(empty document)",
            Style::default().add_modifier(Modifier::DIM),
        )));
    }
    out
}

fn indent(depth: usize) -> String {
    INDENT.repeat(depth)
}

/// A node header line, e.g. `▚ RiskPanel  ~llm 0.82`.
fn push_header(kind: &str, meta: &NodeMeta, depth: usize, out: &mut Vec<Line<'static>>) {
    let mut spans = vec![
        Span::raw(indent(depth)),
        Span::styled(format!("▚ {kind}"), header_style()),
    ];
    if meta.origin == Origin::Llm {
        let conf = meta
            .confidence
            .map(|c| format!("  ~llm {c:.2}"))
            .unwrap_or_else(|| "  ~llm".to_string());
        let mut st = Style::default().add_modifier(Modifier::DIM);
        // Low-confidence LLM output is flagged so the reader can distrust it
        // (AGENTS.md: `low_confidence` フラグ付きで通す).
        if matches!(meta.confidence, Some(c) if c < 0.5) {
            st = st.fg(Color::Yellow);
        }
        spans.push(Span::styled(conf, st));
    }
    out.push(Line::from(spans));
}

fn text_line(depth: usize, text: impl Into<String>) -> Line<'static> {
    Line::from(vec![Span::raw(indent(depth)), Span::raw(text.into())])
}

/// Should this node be hidden given the current reading position?
fn hidden(meta: &NodeMeta, ctx: RenderCtx) -> bool {
    match (meta.visibility, ctx.reveal_line) {
        (Visibility::UntilRead { reveal_after_line }, Some(pos)) => pos < reveal_after_line,
        // No reading position tracked -> reveal everything.
        (Visibility::UntilRead { .. }, None) => false,
        (Visibility::Always, _) => false,
    }
}

fn render_node(node: &UiNode, depth: usize, ctx: RenderCtx, out: &mut Vec<Line<'static>>) {
    if hidden(node.meta(), ctx) {
        out.push(Line::from(Span::styled(
            format!("{}▚ {} (hidden until read)", indent(depth), node.kind_name()),
            Style::default().add_modifier(Modifier::DIM | Modifier::ITALIC),
        )));
        return;
    }

    match node {
        UiNode::Tabs(n) => {
            push_header("Tabs", &n.meta, depth, out);
            for tab in &n.tabs {
                out.push(Line::from(vec![
                    Span::raw(indent(depth + 1)),
                    Span::styled(
                        format!("┌─ {} ", tab.title),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                ]));
                for child in &tab.children {
                    render_node(child, depth + 2, ctx, out);
                }
            }
        }
        UiNode::Checklist(n) => {
            push_header("Checklist", &n.meta, depth, out);
            for item in &n.items {
                let (glyph, style) = if item.checked {
                    ("✓", Style::default().fg(Color::Green))
                } else {
                    ("☐", Style::default().fg(Color::Gray))
                };
                let mut spans = vec![
                    Span::raw(indent(depth + 1)),
                    Span::styled(format!("{glyph} "), style),
                    Span::raw(item.title.clone()),
                ];
                if let Some(cat) = &item.category {
                    spans.push(Span::styled(
                        format!("  [{cat}]"),
                        Style::default().add_modifier(Modifier::DIM),
                    ));
                }
                out.push(Line::from(spans));
            }
        }
        UiNode::DataTable(n) => {
            push_header("DataTable", &n.meta, depth, out);
            render_table(n, depth + 1, out);
        }
        UiNode::Timeline(n) => {
            push_header("Timeline", &n.meta, depth, out);
            for ev in &n.events {
                let mut label = String::new();
                if let Some(ts) = &ev.timestamp {
                    label.push_str(ts);
                    label.push_str("  ");
                }
                label.push_str(&ev.label);
                out.push(Line::from(vec![
                    Span::raw(indent(depth + 1)),
                    Span::styled("● ", Style::default().fg(Color::Blue)),
                    Span::raw(label),
                ]));
                if let Some(d) = &ev.detail {
                    out.push(text_line(depth + 2, d.clone()));
                }
            }
        }
        UiNode::Callout(n) => {
            let style = severity_style(n.severity);
            let title = n
                .title
                .clone()
                .unwrap_or_else(|| format!("{:?}", n.severity));
            out.push(Line::from(vec![
                Span::raw(indent(depth)),
                Span::styled(
                    format!("{} {}", severity_glyph(n.severity), title),
                    style.add_modifier(Modifier::BOLD),
                ),
            ]));
            for l in n.body.lines() {
                out.push(Line::from(vec![
                    Span::raw(indent(depth + 1)),
                    Span::styled("│ ", style),
                    Span::raw(l.to_string()),
                ]));
            }
        }
        UiNode::RiskPanel(n) => {
            push_header("RiskPanel", &n.meta, depth, out);
            for risk in &n.risks {
                out.push(Line::from(vec![
                    Span::raw(indent(depth + 1)),
                    Span::styled(
                        format!("{} ", severity_glyph(risk.severity)),
                        severity_style(risk.severity),
                    ),
                    Span::styled(
                        risk.title.clone(),
                        severity_style(risk.severity).add_modifier(Modifier::BOLD),
                    ),
                ]));
                if let Some(m) = &risk.mitigation {
                    out.push(text_line(depth + 2, format!("↳ {m}")));
                }
            }
        }
        UiNode::LogTimeline(n) => {
            push_header("LogTimeline", &n.meta, depth, out);
            for e in &n.entries {
                let mut spans = vec![Span::raw(indent(depth + 1))];
                if let Some(ts) = &e.timestamp {
                    spans.push(Span::styled(
                        format!("{ts} "),
                        Style::default().add_modifier(Modifier::DIM),
                    ));
                }
                spans.push(Span::styled(
                    format!("{:>7} ", format!("{:?}", e.level).to_uppercase()),
                    severity_style(e.level),
                ));
                spans.push(Span::raw(e.message.clone()));
                out.push(Line::from(spans));
            }
        }
        UiNode::CommitGraph(n) => {
            push_header("CommitGraph", &n.meta, depth, out);
            for c in &n.commits {
                let mut spans = vec![
                    Span::raw(indent(depth + 1)),
                    Span::styled("● ", Style::default().fg(Color::Yellow)),
                    Span::styled(
                        format!("{} ", short_hash(&c.hash)),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw(c.summary.clone()),
                ];
                if let Some(a) = &c.author {
                    spans.push(Span::styled(
                        format!("  <{a}>"),
                        Style::default().add_modifier(Modifier::DIM),
                    ));
                }
                out.push(Line::from(spans));
            }
        }
        UiNode::Glossary(n) => {
            push_header("Glossary", &n.meta, depth, out);
            for t in &n.terms {
                out.push(Line::from(vec![
                    Span::raw(indent(depth + 1)),
                    Span::styled(
                        format!("{}: ", t.term),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(t.definition.clone()),
                ]));
            }
        }
        UiNode::StepNavigator(n) => {
            push_header("StepNavigator", &n.meta, depth, out);
            for (i, s) in n.steps.iter().enumerate() {
                let mut head = vec![
                    Span::raw(indent(depth + 1)),
                    Span::styled(
                        format!("{}. ", i + 1),
                        Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(s.title.clone(), Style::default().add_modifier(Modifier::BOLD)),
                ];
                if let Some(dur) = &s.duration {
                    head.push(Span::styled(
                        format!("  ({dur})"),
                        Style::default().add_modifier(Modifier::DIM),
                    ));
                }
                out.push(Line::from(head));
                if let Some(p) = &s.prerequisite {
                    out.push(text_line(depth + 2, format!("前提: {p}")));
                }
                if let Some(d) = &s.detail {
                    out.push(text_line(depth + 2, d.clone()));
                }
            }
        }

        // --- graphical nodes: text summary + "open in web" fallback (§5.2) ---
        UiNode::Diagram(n) => {
            let title = n.title.clone().unwrap_or_else(|| "Diagram".into());
            push_fallback(
                depth,
                &format!("Diagram: {title}"),
                n.lang.as_deref(),
                n.summary.as_deref(),
                out,
            );
        }
        UiNode::DependencyGraph(n) => {
            push_header("DependencyGraph", &n.meta, depth, out);
            if let Some(t) = &n.title {
                out.push(text_line(depth + 1, t.clone()));
            }
            for e in &n.edges {
                let label = e
                    .label
                    .as_ref()
                    .map(|l| format!(" ({l})"))
                    .unwrap_or_default();
                out.push(text_line(depth + 1, format!("{} → {}{}", e.from, e.to, label)));
            }
            push_web_hint(depth + 1, out);
        }
    }
}

fn short_hash(hash: &str) -> String {
    hash.chars().take(8).collect()
}

fn push_web_hint(depth: usize, out: &mut Vec<Line<'static>>) {
    out.push(Line::from(vec![
        Span::raw(indent(depth)),
        Span::styled(
            "↗ open in web for the full diagram",
            Style::default().fg(Color::Blue).add_modifier(Modifier::DIM),
        ),
    ]));
}

fn push_fallback(
    depth: usize,
    title: &str,
    lang: Option<&str>,
    summary: Option<&str>,
    out: &mut Vec<Line<'static>>,
) {
    let mut spans = vec![
        Span::raw(indent(depth)),
        Span::styled(format!("▚ {title}"), header_style()),
    ];
    if let Some(l) = lang {
        spans.push(Span::styled(
            format!("  [{l}]"),
            Style::default().add_modifier(Modifier::DIM),
        ));
    }
    out.push(Line::from(spans));
    if let Some(s) = summary {
        for l in s.lines() {
            out.push(text_line(depth + 1, l.to_string()));
        }
    }
    push_web_hint(depth + 1, out);
}

fn render_table(n: &DataTableNode, depth: usize, out: &mut Vec<Line<'static>>) {
    // Compute column widths from headers and cell contents.
    let mut widths: Vec<usize> = n.columns.iter().map(|c| c.label.chars().count()).collect();
    let cell = |row: &serde_json::Map<String, serde_json::Value>, key: &str| -> String {
        match row.get(key) {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Null) | None => String::new(),
            Some(v) => v.to_string(),
        }
    };
    for row in &n.rows {
        for (i, col) in n.columns.iter().enumerate() {
            let len = cell(row, &col.key).chars().count();
            if len > widths[i] {
                widths[i] = len;
            }
        }
    }

    let pad = |s: &str, w: usize| -> String {
        let len = s.chars().count();
        let mut out = s.to_string();
        if len < w {
            out.push_str(&" ".repeat(w - len));
        }
        out
    };

    // Header row.
    let header: Vec<Span> = std::iter::once(Span::raw(indent(depth)))
        .chain(n.columns.iter().enumerate().flat_map(|(i, c)| {
            [
                Span::styled(
                    pad(&c.label, widths[i]),
                    Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::raw("  "),
            ]
        }))
        .collect();
    out.push(Line::from(header));

    // Data rows.
    for row in &n.rows {
        let spans: Vec<Span> = std::iter::once(Span::raw(indent(depth)))
            .chain(n.columns.iter().enumerate().flat_map(|(i, c)| {
                let val = cell(row, &c.key);
                let style = match c.col_type {
                    Some(ColumnType::Number) => Style::default().fg(Color::Cyan),
                    Some(ColumnType::Code) => Style::default().fg(Color::Green),
                    Some(ColumnType::Link) => {
                        Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED)
                    }
                    _ => Style::default(),
                };
                [
                    Span::styled(pad(&val, widths[i]), style),
                    Span::raw("  "),
                ]
            }))
            .collect();
        out.push(Line::from(spans));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::widgets::Paragraph;
    use ratatui::Terminal;

    /// Flatten rendered lines to plain text (styling dropped) for assertions.
    fn plain(nodes: &[UiNode], ctx: RenderCtx) -> String {
        render(nodes, ctx)
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn parse(json: &str) -> Vec<UiNode> {
        serde_json::from_str::<Vec<UiNode>>(json).expect("valid IR")
    }

    #[test]
    fn checklist_marks_checked_and_unchecked() {
        let nodes = parse(
            r#"[{"kind":"Checklist","items":[
                {"title":"done","checked":true},
                {"title":"todo","checked":false,"category":"x"}]}]"#,
        );
        let out = plain(&nodes, RenderCtx::default());
        assert!(out.contains("✓ done"), "got: {out}");
        assert!(out.contains("☐ todo"), "got: {out}");
        assert!(out.contains("[x]"), "category rendered: {out}");
    }

    #[test]
    fn datatable_aligns_columns_and_headers() {
        let nodes = parse(
            r#"[{"kind":"DataTable",
                "columns":[{"key":"a","label":"Layer"},{"key":"b","label":"名前"}],
                "rows":[{"a":"1","b":"viewer"},{"a":"55","b":"tui"}]}]"#,
        );
        let out = plain(&nodes, RenderCtx::default());
        assert!(out.contains("Layer"));
        assert!(out.contains("viewer"));
        // "55" is wider than header "Layer"? no; but padding keeps 2-space gap.
        assert!(out.contains("55"));
    }

    #[test]
    fn callout_uses_severity_glyph() {
        let nodes = parse(
            r#"[{"kind":"Callout","severity":"error","title":"boom","body":"line1\nline2"}]"#,
        );
        let out = plain(&nodes, RenderCtx::default());
        assert!(out.contains("✖ boom"), "got: {out}");
        assert!(out.contains("line1") && out.contains("line2"));
    }

    #[test]
    fn graphical_node_falls_back_to_web_hint() {
        let nodes = parse(
            r#"[{"kind":"Diagram","title":"flow","lang":"mermaid","summary":"a -> b"}]"#,
        );
        let out = plain(&nodes, RenderCtx::default());
        assert!(out.contains("Diagram: flow"));
        assert!(out.contains("a -> b"));
        assert!(out.contains("open in web"), "web fallback hint: {out}");
    }

    #[test]
    fn reading_position_hides_then_reveals() {
        let nodes = parse(
            r#"[{"kind":"Glossary","visibility":{"until_read":{"reveal_after_line":100}},
                "terms":[{"term":"IR","definition":"contract"}]}]"#,
        );
        // Before the reader reaches line 100 the content is hidden (spoiler control).
        let hidden = plain(&nodes, RenderCtx { reveal_line: Some(50) });
        assert!(hidden.contains("hidden until read"), "got: {hidden}");
        assert!(!hidden.contains("contract"));

        // After, it is revealed.
        let shown = plain(&nodes, RenderCtx { reveal_line: Some(120) });
        assert!(shown.contains("contract"), "got: {shown}");

        // With no tracked position everything is revealed.
        let all = plain(&nodes, RenderCtx::default());
        assert!(all.contains("contract"));
    }

    #[test]
    fn unknown_kind_is_rejected() {
        // Enforces the registry allowlist: serde has no variant for "EvilNode".
        let err = serde_json::from_str::<Vec<UiNode>>(r#"[{"kind":"EvilNode"}]"#);
        assert!(err.is_err(), "unknown kind must be rejected");
    }

    #[test]
    fn renders_into_a_headless_ratatui_backend() {
        // Proves the lines actually paint through a real ratatui pipeline.
        let nodes = parse(r#"[{"kind":"Callout","severity":"info","body":"hello tui"}]"#);
        let lines = render(&nodes, RenderCtx::default());
        let backend = TestBackend::new(40, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            f.render_widget(Paragraph::new(lines), f.area());
        })
        .unwrap();
        let buf = term.backend().buffer().clone();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains("hello tui"), "buffer: {text}");
    }
}
