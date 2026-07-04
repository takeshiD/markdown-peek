//! Rules analyser (AGENTS.md §3.2, Layer 2).
//!
//! Deterministic classification and extraction: everything that can be decided
//! by rules is decided here. Ambiguous judgements are left for Layer 3's LLM
//! path — this module never guesses beyond documented heuristics and always
//! reports a `confidence`.

pub mod block_class;
pub mod code;
pub mod doctype;
pub mod table;
pub mod tasks;

use crate::semantic::model::{DocumentModel, OutlineEntry};
use crate::semantic::parser::{BlockKind, BlockTree};

/// Run the full rules pipeline over a parsed tree, producing a [`DocumentModel`].
pub fn build_model(tree: &BlockTree, filename: Option<&str>) -> DocumentModel {
    let outline = outline(tree);
    let tasks = tasks::extract(tree);
    let doc_type = doctype::classify(filename, tree, &outline);
    let blocks = block_class::classify(tree, &outline);

    DocumentModel {
        doc_type,
        blocks,
        frontmatter: tree.frontmatter.clone(),
        outline,
        links: tree.links.clone(),
        tasks,
    }
}

/// Extract the heading outline in document order.
pub fn outline(tree: &BlockTree) -> Vec<OutlineEntry> {
    tree.iter()
        .filter_map(|b| match b.kind {
            BlockKind::Heading { level } => Some(OutlineEntry {
                level,
                title: b.text.clone(),
                block_id: b.id,
                range: b.range,
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::model::DocumentType;
    use crate::semantic::parser::parse;

    #[test]
    fn build_model_populates_all_sections() {
        let md = "---\ntype: readme\n---\n\
                  # My Project\n\n\
                  Intro paragraph with [a link](https://example.com).\n\n\
                  ## Usage\n\n\
                  - [ ] install\n- [x] configure\n\n\
                  ```bash\nmake build\n```\n";
        let tree = parse(md);
        let model = build_model(&tree, Some("README.md"));

        assert_eq!(model.doc_type.value, DocumentType::Readme);
        assert!(model.frontmatter.is_some());
        assert_eq!(model.outline.len(), 2);
        assert_eq!(model.links.len(), 1);
        assert_eq!(model.tasks.len(), 2);
        assert!(!model.blocks.is_empty());
    }

    #[test]
    fn outline_preserves_levels_and_order() {
        let tree = parse("# A\n\n## B\n\n### C\n\n## D\n");
        let outline = outline(&tree);
        let levels: Vec<u8> = outline.iter().map(|e| e.level).collect();
        let titles: Vec<&str> = outline.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(levels, vec![1, 2, 3, 2]);
        assert_eq!(titles, vec!["A", "B", "C", "D"]);
    }
}
