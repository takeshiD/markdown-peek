//! `RulesGenerator` — the offline, deterministic default (design doc §3.4).
//!
//! Walks the `pulldown-cmark` event stream (`into_offset_iter`, so every block
//! carries a byte range) and extracts the UI nodes that can be produced *without
//! an LLM*: task lists → `Checklist`, tables → `DataTable`, mermaid fences →
//! `Diagram`, config fences (json/yaml/toml/env) → `ConfigViewer`, and GFM alert
//! blockquotes → `Callout`. Each node is anchored to its `sourceRange`.
//!
//! Anything requiring judgement (risk extraction, doctype-specific layout, prose
//! summarisation) is deliberately *not* done here — that is the LLM generator's
//! job (`feature = "llm"`). Rules first keeps the default build offline and
//! reproducible (design §0 "rules 優先").

use anyhow::Result;
use pulldown_cmark::{
    BlockQuoteKind, CodeBlockKind, Event, Parser, Tag, TagEnd,
};

use mdpeek_gfm::parser_options;
use crate::ir::node::*;
use crate::ir::range::{LineIndex, SourceRange};

use super::traits::{GenInput, Generator};

/// Deterministic, offline UI IR generator.
#[derive(Debug, Default, Clone, Copy)]
pub struct RulesGenerator;

impl Generator for RulesGenerator {
    fn generate(&self, input: &GenInput<'_>) -> Result<Vec<UiNode>> {
        Ok(extract(input.markdown))
    }

    fn model_id(&self) -> String {
        "rules".to_string()
    }
}

/// Which buffer a `Text`/`Code` event should be routed to, innermost first.
fn extract(markdown: &str) -> Vec<UiNode> {
    let line_index = LineIndex::new(markdown);
    let mut out: Vec<UiNode> = Vec::new();

    // Accumulators shared across the single pass.
    let mut heading_buf = String::new();
    let mut in_heading = false;
    let mut last_heading: Option<String> = None;

    // Code block.
    let mut in_code = false;
    let mut code_lang = String::new();
    let mut code_buf = String::new();
    let mut code_range: Option<SourceRange> = None;

    // Task-list checklist (one node for the whole document).
    let mut checklist: Vec<ChecklistItem> = Vec::new();
    let mut item_stack: Vec<ItemState> = Vec::new();

    // Table.
    let mut table: Option<TableState> = None;

    // Blockquote alert (GFM `> [!WARNING]`).
    let mut alert: Option<AlertState> = None;

    for (ev, span) in Parser::new_ext(markdown, parser_options()).into_offset_iter() {
        match ev {
            Event::Start(Tag::Heading { .. }) => {
                in_heading = true;
                heading_buf.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                let h = heading_buf.trim().to_string();
                if !h.is_empty() {
                    last_heading = Some(h);
                }
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                in_code = true;
                code_buf.clear();
                code_range = Some(line_index.range(span.clone()));
                code_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.split_whitespace().next().unwrap_or("").to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code = false;
                if let Some(node) = code_block_node(&code_lang, &code_buf, code_range) {
                    out.push(node);
                }
                code_buf.clear();
            }

            Event::Start(Tag::Item) => {
                item_stack.push(ItemState {
                    is_task: false,
                    checked: false,
                    text: String::new(),
                    range: line_index.range(span.clone()),
                });
            }
            Event::TaskListMarker(checked) => {
                if let Some(item) = item_stack.last_mut() {
                    item.is_task = true;
                    item.checked = checked;
                }
            }
            Event::End(TagEnd::Item) => {
                if let Some(item) = item_stack.pop()
                    && item.is_task
                {
                    let title = item.text.trim().to_string();
                    if !title.is_empty() {
                        checklist.push(ChecklistItem {
                            title,
                            checked: item.checked,
                            category: last_heading.clone(),
                            source_range: Some(item.range),
                        });
                    }
                }
            }

            Event::Start(Tag::Table(_)) => {
                table = Some(TableState::new(line_index.range(span.clone())));
            }
            Event::Start(Tag::TableHead) => {
                if let Some(t) = table.as_mut() {
                    t.in_head = true;
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(t) = table.as_mut() {
                    t.in_head = false;
                }
            }
            Event::Start(Tag::TableRow) => {
                if let Some(t) = table.as_mut() {
                    t.current_row.clear();
                }
            }
            Event::End(TagEnd::TableRow) => {
                if let Some(t) = table.as_mut()
                    && !t.in_head
                {
                    let row = std::mem::take(&mut t.current_row);
                    t.rows.push(row);
                }
            }
            Event::Start(Tag::TableCell) => {
                if let Some(t) = table.as_mut() {
                    t.cell_buf.clear();
                    t.in_cell = true;
                }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(t) = table.as_mut() {
                    t.in_cell = false;
                    let cell = t.cell_buf.trim().to_string();
                    if t.in_head {
                        t.headers.push(cell);
                    } else {
                        t.current_row.push(cell);
                    }
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(t) = table.take()
                    && let Some(node) = t.into_node()
                {
                    out.push(node);
                }
            }

            Event::Start(Tag::BlockQuote(Some(kind))) => {
                alert = Some(AlertState {
                    severity: alert_severity(kind),
                    title: alert_title(kind).to_string(),
                    body: String::new(),
                    range: line_index.range(span.clone()),
                });
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                if let Some(a) = alert.take() {
                    let body = a.body.trim().to_string();
                    out.push(UiNode::Callout(CalloutNode {
                        meta: NodeMeta {
                            source_range: Some(a.range),
                            ..Default::default()
                        },
                        severity: a.severity,
                        title: Some(a.title),
                        body,
                    }));
                }
            }

            Event::Text(text) | Event::Code(text) => {
                // Route to the innermost active collector.
                if in_code {
                    code_buf.push_str(&text);
                } else if let Some(t) = table.as_mut().filter(|t| t.in_cell) {
                    t.cell_buf.push_str(&text);
                } else if in_heading {
                    heading_buf.push_str(&text);
                } else if let Some(a) = alert.as_mut() {
                    a.body.push_str(&text);
                } else if let Some(item) = item_stack.last_mut() {
                    item.text.push_str(&text);
                }
            }

            _ => {}
        }
    }

    // Emit the aggregated checklist (if any tasks were found), spanning all items.
    if !checklist.is_empty() {
        let range = checklist_span(&checklist);
        out.insert(
            checklist_insert_pos(&out),
            UiNode::Checklist(ChecklistNode {
                meta: NodeMeta {
                    source_range: range,
                    ..Default::default()
                },
                items: checklist,
            }),
        );
    }

    out
}

/// Keep the checklist near the top but after any leading node — simple and
/// deterministic. (Design leaves ordering to the planner; rules picks front.)
fn checklist_insert_pos(_out: &[UiNode]) -> usize {
    0
}

fn checklist_span(items: &[ChecklistItem]) -> Option<SourceRange> {
    let ranges: Vec<SourceRange> = items.iter().filter_map(|i| i.source_range).collect();
    let first = ranges.first()?;
    let last = ranges.last()?;
    Some(SourceRange {
        start_line: first.start_line,
        start_column: first.start_column,
        end_line: last.end_line,
        end_column: last.end_column,
    })
}

fn code_block_node(lang: &str, code: &str, range: Option<SourceRange>) -> Option<UiNode> {
    let meta = NodeMeta {
        source_range: range,
        ..Default::default()
    };
    let trimmed = code.trim_end_matches('\n').to_string();
    match lang.to_ascii_lowercase().as_str() {
        "mermaid" => Some(UiNode::Diagram(DiagramNode {
            meta,
            format: DiagramFormat::Mermaid,
            code: trimmed,
            title: None,
        })),
        "json" => cfg(meta, ConfigFormat::Json, trimmed),
        "yaml" | "yml" => cfg(meta, ConfigFormat::Yaml, trimmed),
        "toml" => cfg(meta, ConfigFormat::Toml, trimmed),
        "env" | "dotenv" => cfg(meta, ConfigFormat::Env, trimmed),
        _ => None,
    }
}

fn cfg(meta: NodeMeta, format: ConfigFormat, content: String) -> Option<UiNode> {
    Some(UiNode::ConfigViewer(ConfigViewerNode {
        meta,
        format,
        content,
        title: None,
    }))
}

fn alert_severity(kind: BlockQuoteKind) -> Severity {
    match kind {
        BlockQuoteKind::Warning | BlockQuoteKind::Caution => Severity::Warning,
        BlockQuoteKind::Important => Severity::Error,
        BlockQuoteKind::Note | BlockQuoteKind::Tip => Severity::Info,
    }
}

fn alert_title(kind: BlockQuoteKind) -> &'static str {
    match kind {
        BlockQuoteKind::Note => "Note",
        BlockQuoteKind::Tip => "Tip",
        BlockQuoteKind::Important => "Important",
        BlockQuoteKind::Warning => "Warning",
        BlockQuoteKind::Caution => "Caution",
    }
}

struct ItemState {
    is_task: bool,
    checked: bool,
    text: String,
    range: SourceRange,
}

struct AlertState {
    severity: Severity,
    title: String,
    body: String,
    range: SourceRange,
}

struct TableState {
    range: SourceRange,
    in_head: bool,
    in_cell: bool,
    cell_buf: String,
    headers: Vec<String>,
    current_row: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl TableState {
    fn new(range: SourceRange) -> Self {
        TableState {
            range,
            in_head: false,
            in_cell: false,
            cell_buf: String::new(),
            headers: Vec::new(),
            current_row: Vec::new(),
            rows: Vec::new(),
        }
    }

    fn into_node(self) -> Option<UiNode> {
        if self.headers.is_empty() {
            return None;
        }
        let columns: Vec<Column> = self
            .headers
            .iter()
            .enumerate()
            .map(|(i, label)| Column {
                key: column_key(label, i),
                label: label.clone(),
                col_type: None,
            })
            .collect();
        let rows = self
            .rows
            .iter()
            .map(|cells| {
                let mut map = serde_json::Map::new();
                for (col, cell) in columns.iter().zip(cells.iter()) {
                    map.insert(col.key.clone(), serde_json::Value::String(cell.clone()));
                }
                map
            })
            .collect();
        Some(UiNode::DataTable(DataTableNode {
            meta: NodeMeta {
                source_range: Some(self.range),
                ..Default::default()
            },
            columns,
            rows,
        }))
    }
}

/// Deterministic column key from a header label (lowercase, ascii-alnum),
/// falling back to `col{i}` when the label yields nothing usable.
fn column_key(label: &str, i: usize) -> String {
    let key: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let key = key.trim_matches('_').to_string();
    if key.is_empty() {
        format!("col{i}")
    } else {
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_gen(md: &str) -> Vec<UiNode> {
        RulesGenerator.generate(&GenInput::new(md)).unwrap()
    }

    #[test]
    fn extracts_task_list_into_checklist() {
        let md = "## Todo\n\n- [ ] first\n- [x] second\n";
        let nodes = run_gen(md);
        let cl = nodes
            .iter()
            .find_map(|n| match n {
                UiNode::Checklist(c) => Some(c),
                _ => None,
            })
            .expect("checklist");
        assert_eq!(cl.items.len(), 2);
        assert_eq!(cl.items[0].title, "first");
        assert!(!cl.items[0].checked);
        assert!(cl.items[1].checked);
        assert_eq!(cl.items[0].category.as_deref(), Some("Todo"));
        assert!(cl.items[0].source_range.is_some());
    }

    #[test]
    fn extracts_table_into_datatable() {
        let md = "| Name | Status |\n|------|--------|\n| a | ok |\n| b | fail |\n";
        let nodes = run_gen(md);
        let dt = nodes
            .iter()
            .find_map(|n| match n {
                UiNode::DataTable(d) => Some(d),
                _ => None,
            })
            .expect("datatable");
        assert_eq!(dt.columns.len(), 2);
        assert_eq!(dt.columns[0].key, "name");
        assert_eq!(dt.rows.len(), 2);
        assert_eq!(dt.rows[0].get("status").unwrap(), "ok");
    }

    #[test]
    fn mermaid_and_config_fences() {
        let md = "```mermaid\ngraph TD; A-->B;\n```\n\n```json\n{\"a\":1}\n```\n";
        let nodes = run_gen(md);
        assert!(nodes.iter().any(|n| matches!(n, UiNode::Diagram(_))));
        assert!(nodes.iter().any(
            |n| matches!(n, UiNode::ConfigViewer(c) if matches!(c.format, ConfigFormat::Json))
        ));
    }

    #[test]
    fn gfm_alert_into_callout() {
        let md = "> [!WARNING]\n> be careful here\n";
        let nodes = run_gen(md);
        let c = nodes
            .iter()
            .find_map(|n| match n {
                UiNode::Callout(c) => Some(c),
                _ => None,
            })
            .expect("callout");
        assert_eq!(c.severity, Severity::Warning);
        assert!(c.body.contains("careful"));
    }

    #[test]
    fn plain_prose_yields_nothing() {
        let nodes = run_gen("Just a paragraph of text.\n");
        assert!(nodes.is_empty());
    }
}
