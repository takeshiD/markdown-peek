//! Per-block semantic classification (AGENTS.md §4.2 `BlockClass`).
//!
//! Rules only: a block's class comes from its own kind (code/table/task) or from
//! the section heading it sits under. Section membership is tracked by walking
//! blocks in document order and remembering the most recent recognised heading.

use crate::semantic::model::{BlockClass, ClassifiedBlock, OutlineEntry};
use crate::semantic::parser::{BlockKind, BlockTree};

/// Classify every block in the tree.
pub fn classify(tree: &BlockTree, _outline: &[OutlineEntry]) -> Vec<ClassifiedBlock> {
    let mut out = Vec::new();
    // The semantic class of the section we are currently inside.
    let mut current_section = BlockClass::Generic;

    for block in tree.iter() {
        let (class, confidence) = match &block.kind {
            BlockKind::Heading { .. } => {
                match section_class(&block.text) {
                    Some(section) => {
                        current_section = section;
                        (section, 0.8)
                    }
                    None => {
                        // Unrecognised heading opens a generic section.
                        current_section = BlockClass::Generic;
                        (BlockClass::Heading, 0.6)
                    }
                }
            }
            BlockKind::CodeBlock { .. } => (BlockClass::CodeExample, 0.9),
            BlockKind::Table => (BlockClass::Table, 0.9),
            BlockKind::List { task: true } | BlockKind::Item { checked: Some(_) } => {
                (BlockClass::Task, 0.9)
            }
            _ => {
                // Inherit the enclosing section (or Generic).
                let conf = if current_section == BlockClass::Generic {
                    0.3
                } else {
                    0.6
                };
                (current_section, conf)
            }
        };

        out.push(ClassifiedBlock {
            block_id: block.id,
            class,
            confidence,
            range: block.range,
        });
    }
    out
}

/// Map a heading title to the semantic class of the section it introduces.
pub fn section_class(title: &str) -> Option<BlockClass> {
    let t = title.to_lowercase();
    let has = |kws: &[&str]| kws.iter().any(|kw| t.contains(kw));

    // Order matters: more specific classes are checked before broad ones.
    if has(&["open question", "open issue", "未解決", "questions", "疑問"]) {
        Some(BlockClass::OpenQuestion)
    } else if has(&["risk", "リスク", "caveat", "warning", "注意", "落とし穴"]) {
        Some(BlockClass::Risk)
    } else if has(&["data model", "データモデル", "schema", "スキーマ", "型定義", "types"]) {
        Some(BlockClass::DataModel)
    } else if has(&["architecture", "アーキテクチャ", "構成", "design", "設計"]) {
        Some(BlockClass::Architecture)
    } else if has(&["consequence", "影響", "結果", "trade-off", "tradeoff"]) {
        Some(BlockClass::Consequence)
    } else if has(&["decision", "決定", "採用"]) {
        Some(BlockClass::Decision)
    } else if has(&["configuration", "config", "設定", "options"]) {
        Some(BlockClass::Configuration)
    } else if has(&["troubleshoot", "トラブル", "faq", "known issue"]) {
        Some(BlockClass::Troubleshooting)
    } else if has(&["usage", "使い方", "how to use", "getting started", "quick start", "使用方法"]) {
        Some(BlockClass::Usage)
    } else if has(&["step", "手順", "procedure", "instruction", "作り方", "how to"]) {
        Some(BlockClass::Step)
    } else if has(&["overview", "概要", "summary", "introduction", "はじめに", "要約"]) {
        Some(BlockClass::Overview)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::analyzer::outline;
    use crate::semantic::parser::{parse, BlockKind};

    #[test]
    fn section_class_maps_common_headings() {
        assert_eq!(section_class("Overview"), Some(BlockClass::Overview));
        assert_eq!(section_class("## Architecture"), Some(BlockClass::Architecture));
        assert_eq!(section_class("Open Questions"), Some(BlockClass::OpenQuestion));
        assert_eq!(section_class("リスク"), Some(BlockClass::Risk));
        assert_eq!(section_class("Random"), None);
    }

    #[test]
    fn paragraph_inherits_its_section() {
        let md = "# T\n\n## Risks\n\nThis is dangerous.\n\n## Usage\n\nRun it.\n";
        let tree = parse(md);
        let ol = outline(&tree);
        let classes = classify(&tree, &ol);

        // Find the paragraph under Risks.
        let risk_para = tree
            .iter()
            .find(|b| matches!(b.kind, BlockKind::Paragraph) && b.text.contains("dangerous"))
            .unwrap();
        let usage_para = tree
            .iter()
            .find(|b| matches!(b.kind, BlockKind::Paragraph) && b.text.contains("Run it"))
            .unwrap();

        let class_of = |id| classes.iter().find(|c| c.block_id == id).unwrap().class;
        assert_eq!(class_of(risk_para.id), BlockClass::Risk);
        assert_eq!(class_of(usage_para.id), BlockClass::Usage);
    }

    #[test]
    fn code_and_table_get_structural_classes() {
        let md = "## X\n\n```rust\nfn a(){}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        let tree = parse(md);
        let ol = outline(&tree);
        let classes = classify(&tree, &ol);
        assert!(classes.iter().any(|c| c.class == BlockClass::CodeExample));
        assert!(classes.iter().any(|c| c.class == BlockClass::Table));
    }
}
