//! Semantic side panel (AGENTS.md §10 Layer 2 "サイドパネルに outline / TODO /
//! risk / open questions").
//!
//! This is the rules-stage view over a [`DocumentModel`]. Per the roadmap it is
//! *not* yet UI IR (Layer 3) — it is a plain data structure the web/TUI side
//! panels can render directly, with every entry carrying a [`SourceRangeLink`]
//! back to the original document (design思想「全 UI は sourceRange に紐づく」).

use crate::model::{BlockClass, DocumentModel};
use mdpeek_parser::{BlockId, BlockKind, BlockTree, SourceRange};
use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;

/// A jump-to-source reference: the block and its span in the document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceRangeLink {
    pub block_id: BlockId,
    pub range: SourceRange,
}

/// One row in a side-panel section.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PanelEntry {
    pub text: String,
    pub link: SourceRangeLink,
}

/// A TODO/FIXME-style marker, either a task-list item or an inline comment.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TodoItem {
    pub text: String,
    pub done: bool,
    /// Marker kind: `"task"`, `"todo"`, `"fixme"`, etc.
    pub marker: String,
    pub link: SourceRangeLink,
}

/// One outline row (heading) for the outline panel.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutlineRow {
    pub level: u8,
    pub title: String,
    pub link: SourceRangeLink,
}

/// The complete semantic side panel.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SemanticPanel {
    pub outline: Vec<OutlineRow>,
    pub todos: Vec<TodoItem>,
    pub risks: Vec<PanelEntry>,
    pub open_questions: Vec<PanelEntry>,
}

/// Matches inline `TODO`/`FIXME`/`XXX`/`HACK` markers, capturing the trailing note.
static MARKER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(TODO|FIXME|XXX|HACK)\b[:\s]*(.*)").expect("valid marker regex")
});

/// Build the side panel from a model and its parsed tree.
pub fn build(model: &DocumentModel, tree: &BlockTree) -> SemanticPanel {
    SemanticPanel {
        outline: outline_rows(model),
        todos: todos(model, tree),
        risks: entries_for_class(model, tree, BlockClass::Risk),
        open_questions: entries_for_class(model, tree, BlockClass::OpenQuestion),
    }
}

fn outline_rows(model: &DocumentModel) -> Vec<OutlineRow> {
    model
        .outline
        .iter()
        .map(|e| OutlineRow {
            level: e.level,
            title: e.title.clone(),
            link: SourceRangeLink {
                block_id: e.block_id,
                range: e.range,
            },
        })
        .collect()
}

/// TODOs come from two sources: task-list items and inline TODO/FIXME markers.
fn todos(model: &DocumentModel, tree: &BlockTree) -> Vec<TodoItem> {
    let mut out = Vec::new();

    // 1. Task-list items.
    for task in &model.tasks {
        out.push(TodoItem {
            text: task.text.clone(),
            done: task.checked,
            marker: "task".to_string(),
            link: SourceRangeLink {
                block_id: task.block_id,
                range: task.range,
            },
        });
    }

    // 2. Inline TODO/FIXME/XXX/HACK markers in block text. Headings are section
    //    titles (a `## TODO` heading is a section, not an inline note) and
    //    metadata is front matter, so skip both — task items are captured above.
    for block in tree.iter() {
        if matches!(
            block.kind,
            BlockKind::Heading { .. } | BlockKind::MetadataBlock
        ) {
            continue;
        }
        for caps in MARKER_RE.captures_iter(&block.text) {
            let marker = caps
                .get(1)
                .map(|m| m.as_str().to_lowercase())
                .unwrap_or_default();
            // Trim trailing comment closers so `<!-- TODO: x -->` yields "x".
            let note = caps
                .get(2)
                .map(|m| {
                    m.as_str()
                        .trim()
                        .trim_end_matches("-->")
                        .trim_end_matches("*/")
                        .trim()
                })
                .unwrap_or("");
            let text = if note.is_empty() {
                marker.to_uppercase()
            } else {
                note.to_string()
            };
            out.push(TodoItem {
                text,
                done: false,
                marker,
                link: SourceRangeLink {
                    block_id: block.id,
                    range: block.range,
                },
            });
        }
    }

    out
}

/// Collect panel entries for every block classified with `class`, using the
/// block's own text as the entry label.
fn entries_for_class(
    model: &DocumentModel,
    tree: &BlockTree,
    class: BlockClass,
) -> Vec<PanelEntry> {
    model
        .blocks
        .iter()
        .filter(|c| c.class == class)
        .filter_map(|c| {
            let block = tree.find(c.block_id)?;
            // Skip the section heading itself; surface the content beneath it.
            if matches!(block.kind, BlockKind::Heading { .. }) {
                return None;
            }
            let text = block.text.trim();
            if text.is_empty() {
                return None;
            }
            Some(PanelEntry {
                text: text.to_string(),
                link: SourceRangeLink {
                    block_id: c.block_id,
                    range: c.range,
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::build_model;
    use mdpeek_parser::BlockTree;

    fn panel_for(md: &str) -> SemanticPanel {
        let tree = BlockTree::parse(md);
        let model = build_model(md, &tree, None);
        build(&model, &tree)
    }

    #[test]
    fn outline_rows_have_links() {
        let panel = panel_for("# One\n\n## Two\n");
        assert_eq!(panel.outline.len(), 2);
        assert_eq!(panel.outline[0].title, "One");
        assert_eq!(panel.outline[0].link.range.start_line, 1);
    }

    #[test]
    fn todos_include_tasks_and_inline_markers() {
        let md = "# T\n\n- [ ] ship it\n- [x] plan it\n\nTODO: refactor this later\n";
        let panel = panel_for(md);
        // 2 task items + 1 inline TODO.
        assert_eq!(panel.todos.len(), 3);
        assert!(panel.todos.iter().any(|t| t.marker == "task" && !t.done));
        assert!(panel.todos.iter().any(|t| t.marker == "task" && t.done));
        let inline = panel.todos.iter().find(|t| t.marker == "todo").unwrap();
        assert_eq!(inline.text, "refactor this later");
    }

    #[test]
    fn inline_todo_in_html_comment_strips_closer() {
        let panel = panel_for("# T\n\n<!-- TODO: wire up retries -->\n");
        let todo = panel.todos.iter().find(|t| t.marker == "todo").unwrap();
        assert_eq!(todo.text, "wire up retries");
    }

    #[test]
    fn risks_and_open_questions_collected_from_sections() {
        let md = "# Design\n\n## Risks\n\nData loss on crash.\n\n## Open Questions\n\nWhich DB?\n";
        let panel = panel_for(md);
        assert!(panel.risks.iter().any(|e| e.text.contains("Data loss")));
        assert!(panel.open_questions.iter().any(|e| e.text.contains("Which DB")));
        // Links point back into the document.
        assert!(panel.risks[0].link.range.start_line >= 1);
    }

    #[test]
    fn empty_document_yields_empty_panel() {
        let panel = panel_for("");
        assert!(panel.outline.is_empty());
        assert!(panel.todos.is_empty());
        assert!(panel.risks.is_empty());
        assert!(panel.open_questions.is_empty());
    }
}
