//! reading-position aware / ネタバレ制御 (AGENTS.md §9.3-2)。
//!
//! 小説発の要件: 「既読位置より先の内容を要約・生成 UI に含めない」。
//! `Visibility::UntilRead { reveal_after_line }` を持つノードは、現在の既読位置
//! (`read_line`) がその行に達するまで renderer に渡さない。
//!
//! 判断は Rust core 側で行い、フィルタ済みの IR だけを web/TUI に送る
//! (境界を 1 か所に集約 — §1.1)。LLM への入力を絞る用途にも同じ関数を使える。

use crate::nodes::DomainNode;
use crate::seam::Visibility;

/// 現在の既読位置。`None` は「位置不明」= ネタバレ安全側 (UntilRead を隠す)。
pub type ReadPosition = Option<u32>;

/// 単一ノードが現在の既読位置で表示可能か。
pub fn is_visible(visibility: Visibility, read_line: ReadPosition) -> bool {
    match visibility {
        Visibility::Always => true,
        Visibility::UntilRead { reveal_after_line } => {
            // 位置不明なら安全側に倒して隠す。既読位置が到達していれば表示。
            read_line.is_some_and(|line| line >= reveal_after_line)
        }
    }
}

/// 既読位置に応じてノード列を絞り込む。ネタバレになるノードを落として返す。
pub fn filter_visible(nodes: Vec<DomainNode>, read_line: ReadPosition) -> Vec<DomainNode> {
    nodes
        .into_iter()
        .filter(|n| is_visible(n.meta().visibility, read_line))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::{GlossaryEntry, GlossaryNode};
    use crate::seam::{NodeMeta, Visibility};

    fn glossary(visibility: Visibility) -> DomainNode {
        DomainNode::Glossary(GlossaryNode {
            meta: NodeMeta {
                visibility,
                ..Default::default()
            },
            entries: vec![GlossaryEntry {
                term: "セカイの理".into(),
                definition: "作中の造語".into(),
                first_occurrence: None,
            }],
        })
    }

    #[test]
    fn always_visible_regardless_of_position() {
        assert!(is_visible(Visibility::Always, None));
        assert!(is_visible(Visibility::Always, Some(0)));
    }

    #[test]
    fn until_read_hidden_before_reveal_and_when_unknown() {
        let v = Visibility::UntilRead {
            reveal_after_line: 120,
        };
        assert!(!is_visible(v, None), "位置不明はネタバレ安全側で隠す");
        assert!(!is_visible(v, Some(119)));
        assert!(is_visible(v, Some(120)));
        assert!(is_visible(v, Some(200)));
    }

    #[test]
    fn filter_drops_spoiler_nodes() {
        let nodes = vec![
            glossary(Visibility::Always),
            glossary(Visibility::UntilRead {
                reveal_after_line: 50,
            }),
        ];
        // 既読 10 行目 → ネタバレ用語は落ちて 1 件だけ残る。
        assert_eq!(filter_visible(nodes.clone(), Some(10)).len(), 1);
        // 既読 60 行目 → 両方見える。
        assert_eq!(filter_visible(nodes, Some(60)).len(), 2);
    }
}
