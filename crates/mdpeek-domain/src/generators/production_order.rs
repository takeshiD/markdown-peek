//! 生産指示書 → `ToleranceMeter` (AGENTS.md §9.2「公差が数値の羅列」→ 公差メーター)。
//!
//! 「規格値/中心 + 公差(±)」または「下限/上限」列を持つ検査・寸法テーブルを検出し、
//! 各行を `Quantity` (min/max/nominal) 付きのメーターに変換する。実測列があれば
//! それを value とし、公差内かどうかを renderer が `quantity::evaluate_tolerance`
//! で判定できるようにする。

use crate::generators::text::parse_quantity;
use crate::nodes::{DomainNode, ToleranceMeter, ToleranceMeterNode};
use crate::parser::{ParsedDoc, Table};
use crate::seam::{NodeMeta, Quantity};

/// 公差テーブルらしい表か。ヘッダのキーワード、または規格/公称列のセルに公差
/// 記号 (±) が含まれるかで判定する (規格値セルが "10±0.1" のケースを拾う)。
fn looks_like_tolerance_table(table: &Table) -> bool {
    let joined: String = table.header.concat();
    let has = |kw: &str| joined.contains(kw);
    if has("公差") || (has("下限") && has("上限")) || (has("規格") && (has("実測") || has("測定")))
    {
        return true;
    }
    // 規格/公称/中心列に ± を含むセルがあれば公差テーブル。
    if let Some(col) = find_col(&table.header, &["規格", "公称", "中心", "基準"]) {
        return table
            .rows
            .iter()
            .any(|r| r.get(col).is_some_and(|c| c.contains('±')));
    }
    false
}

/// ヘッダから、あるキーワード群のいずれかを含む列 index を探す。
fn find_col(header: &[String], keywords: &[&str]) -> Option<usize> {
    header
        .iter()
        .position(|h| keywords.iter().any(|kw| h.contains(kw)))
}

/// 1 行から ToleranceMeter を組む。
fn meter_from_row(header: &[String], row: &[String]) -> Option<ToleranceMeter> {
    let get = |i: Option<usize>| i.and_then(|i| row.get(i)).map(|s| s.as_str()).unwrap_or("");

    let label_col = find_col(header, &["項目", "検査", "寸法", "部位", "名称"]).unwrap_or(0);
    let label = row.get(label_col).cloned().unwrap_or_default();
    if label.trim().is_empty() {
        return None;
    }

    let nominal_col = find_col(header, &["規格", "中心", "基準", "公称"]);
    let tol_col = find_col(header, &["公差"]);
    let min_col = find_col(header, &["下限"]);
    let max_col = find_col(header, &["上限"]);
    let actual_col = find_col(header, &["実測", "測定"]);
    let unit_col = find_col(header, &["単位"]);

    let mut q: Quantity;

    if min_col.is_some() && max_col.is_some() {
        // 下限/上限スキーム。
        let min = parse_quantity(get(min_col))?.value;
        let max = parse_quantity(get(max_col))?.value;
        let nominal = parse_quantity(get(nominal_col)).map(|q| q.value);
        let value = parse_quantity(get(actual_col))
            .map(|q| q.value)
            .or(nominal)
            .unwrap_or((min + max) / 2.0);
        q = Quantity {
            value,
            unit: None,
            min: Some(min),
            max: Some(max),
            nominal,
            scalable: false,
        };
    } else if nominal_col.is_some() {
        // 規格値 (+ 公差) スキーム。規格セル自体が "10±0.1" のこともある。
        q = parse_quantity(get(nominal_col))?;
        if q.min.is_none()
            && let Some(tol) = parse_quantity(get(tol_col))
        {
            q.min = Some(q.value - tol.value);
            q.max = Some(q.value + tol.value);
            q.nominal = Some(q.value);
        }
        if let Some(actual) = parse_quantity(get(actual_col)) {
            q.value = actual.value;
        }
    } else {
        return None;
    }

    // 明示の単位列があれば上書き (規格セルから拾えなかったとき)。
    if q.unit.is_none() && !get(unit_col).trim().is_empty() {
        q.unit = Some(get(unit_col).trim().to_string());
    }

    Some(ToleranceMeter {
        label: label.trim().to_string(),
        quantity: q,
    })
}

/// 生産指示書ドキュメントから公差メーターノードを生成する。
pub fn generate_production_order(doc: &ParsedDoc) -> Vec<DomainNode> {
    let mut out = Vec::new();

    for block in doc.tables() {
        let Some(table) = &block.table else { continue };
        if !looks_like_tolerance_table(table) {
            continue;
        }
        let meters = build_meters(table);
        if !meters.is_empty() {
            out.push(DomainNode::ToleranceMeter(ToleranceMeterNode {
                meta: NodeMeta::rules(block.range.clone()),
                meters,
            }));
        }
    }

    out
}

fn build_meters(table: &Table) -> Vec<ToleranceMeter> {
    table
        .rows
        .iter()
        .filter_map(|row| meter_from_row(&table.header, row))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantity::{ToleranceStatus, evaluate_tolerance};

    #[test]
    fn extracts_tolerance_from_nominal_plus_pm() {
        let md = "\
# 検査基準

| 検査項目 | 規格値 | 実測 |
|---|---|---|
| 外径 | 10±0.1 mm | 10.05 |
| 全長 | 50±0.5 mm | 50.6 |
";
        let doc = ParsedDoc::parse(md);
        let nodes = generate_production_order(&doc);
        assert_eq!(nodes.len(), 1);
        let DomainNode::ToleranceMeter(n) = &nodes[0] else {
            panic!()
        };
        assert_eq!(n.meters.len(), 2);

        let outer = &n.meters[0];
        assert_eq!(outer.label, "外径");
        assert_eq!(outer.quantity.value, 10.05); // 実測で上書き
        assert_eq!(outer.quantity.min, Some(9.9));
        assert_eq!(
            evaluate_tolerance(&outer.quantity).status,
            ToleranceStatus::InSpec
        );

        // 全長は上限超過。
        assert_eq!(
            evaluate_tolerance(&n.meters[1].quantity).status,
            ToleranceStatus::AboveMax
        );
    }

    #[test]
    fn extracts_tolerance_from_min_max_cols() {
        let md = "\
| 部位 | 下限 | 上限 | 実測 | 単位 |
|---|---|---|---|---|
| 内径 | 4.8 | 5.2 | 5.0 | mm |
";
        let doc = ParsedDoc::parse(md);
        let nodes = generate_production_order(&doc);
        let DomainNode::ToleranceMeter(n) = &nodes[0] else {
            panic!()
        };
        let m = &n.meters[0];
        assert_eq!(m.quantity.min, Some(4.8));
        assert_eq!(m.quantity.max, Some(5.2));
        assert_eq!(m.quantity.value, 5.0);
        assert_eq!(m.quantity.unit.as_deref(), Some("mm"));
        assert_eq!(evaluate_tolerance(&m.quantity).position, Some(0.5));
    }

    #[test]
    fn ignores_non_tolerance_tables() {
        // BOM テーブル (公差列なし) は ToleranceMeter を生まない (DataTable の領分)。
        let md = "| 品目 | 数量 |\n|---|---|\n| ネジ | 4 |\n";
        let doc = ParsedDoc::parse(md);
        assert!(generate_production_order(&doc).is_empty());
    }
}
