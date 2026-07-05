//! 手順書/SOP/レシピ → `StepNavigator` + `ScalableTable`
//! (AGENTS.md §9.2「どこまでやった」「準備不足」「人数で分量が変わる」)。
//!
//! - 番号付きリスト (または「手順/作り方/工程」節配下のリスト) → `StepNavigator`。
//!   注意/危険キーワードを `caution`、失敗/ロールバック文言を `rollback` に拾う。
//! - 「必要なもの/準備/道具」節のリスト → `prerequisites`。
//! - 「材料」テーブル + 「N 人前」表記 → 数量セル scalable な `ScalableTable`。

use crate::generators::text::parse_quantity;
use crate::nodes::{
    Cell, Column, DomainNode, ScalableRow, ScalableTableNode, Step, StepNavigatorNode,
};
use crate::parser::{BlockKind, ParsedBlock, ParsedDoc};
use crate::seam::{NodeMeta, Quantity};

const CAUTION_KW: [&str; 6] = ["注意", "危険", "警告", "⚠", "禁止", "厳禁"];
const ROLLBACK_KW: [&str; 5] = ["ロールバック", "失敗した", "元に戻す", "復旧", "戻す場合"];
const PREREQ_HEADING_KW: [&str; 6] = ["必要", "準備", "道具", "前提", "用意", "持ち物"];
const STEP_HEADING_KW: [&str; 5] = ["手順", "作り方", "工程", "ステップ", "方法"];

/// 見出しテキストがキーワードのいずれかを含むか。
fn heading_matches(text: &str, kws: &[&str]) -> bool {
    kws.iter().any(|kw| text.contains(kw))
}

/// あるブロックの直後に続く最初のリストブロックを返す。
fn list_after(blocks: &[ParsedBlock], from: usize) -> Option<&ParsedBlock> {
    blocks[from + 1..]
        .iter()
        // 次の見出しに当たる前に現れるリストを探す。
        .take_while(|b| !matches!(b.kind, BlockKind::Heading { .. }))
        .find(|b| matches!(b.kind, BlockKind::List { .. }))
}

/// リスト項目から Step を組む。
fn step_from_item(index: u32, text: &str, range: crate::seam::SourceRange) -> Step {
    let title = text.lines().next().unwrap_or(text).trim().to_string();
    let caution = CAUTION_KW
        .iter()
        .any(|kw| text.contains(kw))
        .then(|| text.to_string());
    let rollback = ROLLBACK_KW
        .iter()
        .any(|kw| text.contains(kw))
        .then(|| text.to_string());
    let detail = (text.trim() != title).then(|| text.trim().to_string());
    Step {
        index,
        title,
        detail,
        duration: None,
        caution,
        rollback,
        source_range: Some(range),
    }
}

/// 手順ドキュメントから StepNavigator と ScalableTable を生成する。
pub fn generate_procedure(doc: &ParsedDoc) -> Vec<DomainNode> {
    let mut out = Vec::new();
    let blocks = &doc.blocks;

    // --- StepNavigator ---
    // 「手順/作り方」見出し配下のリストを優先、無ければ最初の番号付きリスト。
    let steps_list = blocks
        .iter()
        .enumerate()
        .find(|(_, b)| {
            matches!(b.kind, BlockKind::Heading { .. })
                && heading_matches(&b.text, &STEP_HEADING_KW)
        })
        .and_then(|(i, _)| list_after(blocks, i))
        .or_else(|| {
            blocks
                .iter()
                .find(|b| matches!(b.kind, BlockKind::List { ordered: true }))
        });

    if let Some(list) = steps_list {
        let steps: Vec<Step> = list
            .items
            .iter()
            .enumerate()
            .map(|(i, it)| step_from_item(i as u32 + 1, &it.text, it.range.clone()))
            .collect();

        if !steps.is_empty() {
            out.push(DomainNode::StepNavigator(StepNavigatorNode {
                meta: NodeMeta::rules(list.range.clone()),
                steps,
                prerequisites: collect_prerequisites(blocks),
            }));
        }
    }

    // --- ScalableTable (レシピの材料) ---
    if let Some(node) = scalable_ingredients(doc) {
        out.push(node);
    }

    out
}

/// 「必要なもの/準備/道具」節のリスト項目を集める。
fn collect_prerequisites(blocks: &[ParsedBlock]) -> Vec<String> {
    for (i, b) in blocks.iter().enumerate() {
        if matches!(b.kind, BlockKind::Heading { .. })
            && heading_matches(&b.text, &PREREQ_HEADING_KW)
            && let Some(list) = list_after(blocks, i)
        {
            return list.items.iter().map(|it| it.text.clone()).collect();
        }
    }
    Vec::new()
}

/// テキスト中の「N 人前 / N 人分」を検出して人数を返す。
fn detect_servings(text: &str) -> Option<f64> {
    for marker in ["人前", "人分"] {
        if let Some(pos) = text.find(marker) {
            // marker の直前にある連続数字を後ろから拾う。
            let head = &text[..pos];
            let digits: String = head
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            if let Ok(n) = digits.parse::<f64>() {
                return Some(n);
            }
        }
    }
    None
}

/// 材料テーブルを scalable な ScalableTable に変換する。
fn scalable_ingredients(doc: &ParsedDoc) -> Option<DomainNode> {
    let blocks = &doc.blocks;

    // 材料テーブル: ヘッダに「材料」または「分量/量」を含む表。
    let (idx, block) = blocks.iter().enumerate().find(|(_, b)| {
        b.kind == BlockKind::Table
            && b.table.as_ref().is_some_and(|t| {
                let h = t.header.concat();
                h.contains("材料") || h.contains("分量") || h.contains("量")
            })
    })?;
    let table = block.table.as_ref()?;

    // 人数はテーブル近傍の見出し/本文から推定 (「材料（2人前）」等)。
    let base = blocks[..idx]
        .iter()
        .rev()
        .take(3)
        .find_map(|b| detect_servings(&b.text))
        .or_else(|| detect_servings(&table.header.concat()));

    // 数量列を特定 (分量/量/数量)。
    let qty_col = table
        .header
        .iter()
        .position(|h| h.contains("分量") || h.contains("数量") || h.contains("量"))?;

    let columns: Vec<Column> = table
        .header
        .iter()
        .enumerate()
        .map(|(i, h)| Column {
            key: format!("c{i}"),
            label: h.clone(),
        })
        .collect();

    let rows: Vec<ScalableRow> = table
        .rows
        .iter()
        .map(|row| ScalableRow {
            cells: row
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    if i == qty_col
                        && let Some(mut q) = parse_quantity(v)
                    {
                        q.scalable = true; // 人数連動で再計算する
                        return Cell::Amount(q);
                    }
                    Cell::Text(v.clone())
                })
                .collect(),
        })
        .collect();

    Some(DomainNode::ScalableTable(ScalableTableNode {
        meta: NodeMeta::rules(block.range.clone()),
        columns,
        rows,
        base_scale: base.map(|n| Quantity::scalar(n, Some("人前".into()))),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantity::{scale_factor, scale_quantity};

    const RECIPE: &str = "\
# パンケーキ

## 必要なもの
- ボウル
- 泡立て器

## 材料（2人前）

| 材料 | 分量 |
|---|---|
| 小麦粉 | 200g |
| 牛乳 | 300ml |
| オーブン温度 | 180℃ |

## 作り方
1. 粉をふるう
2. 牛乳を混ぜる。⚠ 注意: ダマに気をつける
3. 焼く。失敗したら弱火でやり直す
";

    #[test]
    fn builds_step_navigator_with_caution_and_rollback() {
        let doc = ParsedDoc::parse(RECIPE);
        let nodes = generate_procedure(&doc);
        let nav = nodes
            .iter()
            .find_map(|n| match n {
                DomainNode::StepNavigator(s) => Some(s),
                _ => None,
            })
            .expect("StepNavigator");

        assert_eq!(nav.steps.len(), 3);
        assert_eq!(nav.steps[0].index, 1);
        assert_eq!(nav.prerequisites, vec!["ボウル", "泡立て器"]);
        assert!(nav.steps[1].caution.is_some(), "注意文を拾う");
        assert!(nav.steps[2].rollback.is_some(), "ロールバック文を拾う");
        // sourceRange が各ステップに紐づく。
        assert!(nav.steps[0].source_range.is_some());
    }

    #[test]
    fn builds_scalable_table_and_scales() {
        let doc = ParsedDoc::parse(RECIPE);
        let nodes = generate_procedure(&doc);
        let table = nodes
            .iter()
            .find_map(|n| match n {
                DomainNode::ScalableTable(t) => Some(t),
                _ => None,
            })
            .expect("ScalableTable");

        // 2 人前を検出。
        assert_eq!(table.base_scale.as_ref().unwrap().value, 2.0);

        // 小麦粉 200g が scalable。
        let flour = &table.rows[0].cells[1];
        let Cell::Amount(q) = flour else {
            panic!("expected amount cell")
        };
        assert!(q.scalable);
        assert_eq!(q.value, 200.0);

        // 2 → 5 人前で 500g。
        let scaled = scale_quantity(q, scale_factor(2.0, 5.0));
        assert_eq!(scaled.value, 500.0);

        // オーブン温度も "量" ヘッダ配下の数量列ではないので Text か、数量でも
        // scalable にはなるが値は人数比例。ここでは数量列 (分量) のみ scalable。
    }

    #[test]
    fn detect_servings_variants() {
        assert_eq!(detect_servings("材料（4人前）"), Some(4.0));
        assert_eq!(detect_servings("2人分の分量"), Some(2.0));
        assert_eq!(detect_servings("材料"), None);
    }
}
