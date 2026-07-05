//! Task-list extraction (AGENTS.md §3.2 "task list 抽出").
//!
//! Every GFM task-list item (`- [ ]` / `- [x]`) becomes a [`Task`] carrying its
//! checked state and source range, so the side panel can list open TODOs with
//! jump-to-source links.

use crate::model::Task;
use mdpeek_parser::{BlockKind, BlockTree};

/// Extract all task-list items in document order.
pub fn extract(tree: &BlockTree) -> Vec<Task> {
    tree.iter()
        .filter_map(|b| match b.kind {
            BlockKind::Item {
                task: Some(checked),
            } => Some(Task {
                text: b.text.clone(),
                checked,
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
    use mdpeek_parser::BlockTree;

    #[test]
    fn extracts_checked_and_unchecked() {
        let md = "- [ ] write tests\n- [x] design api\n- not a task\n";
        let tasks = extract(&BlockTree::parse(md));
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].text, "write tests");
        assert!(!tasks[0].checked);
        assert_eq!(tasks[1].text, "design api");
        assert!(tasks[1].checked);
    }

    #[test]
    fn ignores_plain_lists() {
        let tasks = extract(&BlockTree::parse("- one\n- two\n"));
        assert!(tasks.is_empty());
    }

    #[test]
    fn tasks_carry_source_range() {
        let md = "- [ ] first line task\n";
        let tasks = extract(&BlockTree::parse(md));
        assert_eq!(tasks[0].range.start_line, 1);
    }
}
