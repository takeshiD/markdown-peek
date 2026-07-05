//! serde wire format の検証。web の `web/ir.ts` (手書き TS 型) と一致すべき
//! JSON 表現を固定する。統合時は Rust 型から `ts-rs` 生成に置換予定 (§1.1)。

use mdpeek_domain::generators::{generate_procedure, generate_production_order};
use mdpeek_domain::nodes::DomainNode;
use mdpeek_domain::{ParsedDoc, validate};

#[test]
fn tolerance_meter_wire_format() {
    let md = "| 検査項目 | 規格値 |\n|---|---|\n| 外径 | 10±0.1 mm |\n";
    let doc = ParsedDoc::parse(md);
    let node = &generate_production_order(&doc)[0];
    let v = serde_json::to_value(node).unwrap();

    // kind タグ + flatten された meta が同一階層に出る (論点 D: flatten)。
    assert_eq!(v["kind"], "ToleranceMeter");
    assert_eq!(v["origin"], "rules");
    assert_eq!(v["meters"][0]["label"], "外径");
    assert_eq!(v["meters"][0]["quantity"]["min"], 9.9);
    assert_eq!(v["meters"][0]["quantity"]["nominal"], 10.0);
    assert!(v["source_range"]["start_line"].is_number());
}

#[test]
fn scalable_table_untagged_cells() {
    let md = "## 材料（2人前）\n\n| 材料 | 分量 |\n|---|---|\n| 小麦粉 | 200g |\n";
    let doc = ParsedDoc::parse(md);
    let node = generate_procedure(&doc)
        .into_iter()
        .find(|n| matches!(n, DomainNode::ScalableTable(_)))
        .unwrap();
    let v = serde_json::to_value(&node).unwrap();

    assert_eq!(v["kind"], "ScalableTable");
    // Text セルは素の文字列、Amount セルは Quantity オブジェクト (untagged)。
    assert_eq!(v["rows"][0]["cells"][0], "小麦粉");
    assert_eq!(v["rows"][0]["cells"][1]["value"], 200.0);
    assert_eq!(v["rows"][0]["cells"][1]["scalable"], true);
    assert_eq!(v["base_scale"]["value"], 2.0);
}

#[test]
fn visibility_until_read_wire_format() {
    use mdpeek_domain::nodes::{GlossaryEntry, GlossaryNode};
    use mdpeek_domain::{NodeMeta, Visibility};

    let node = DomainNode::Glossary(GlossaryNode {
        meta: NodeMeta {
            visibility: Visibility::UntilRead {
                reveal_after_line: 42,
            },
            ..Default::default()
        },
        entries: vec![GlossaryEntry {
            term: "理".into(),
            definition: "造語".into(),
            first_occurrence: None,
        }],
    });
    let v = serde_json::to_value(&node).unwrap();
    assert_eq!(v["visibility"]["until_read"]["reveal_after_line"], 42);
}

#[test]
fn round_trips_and_validates() {
    let md = "| 検査項目 | 下限 | 上限 |\n|---|---|---|\n| 内径 | 4.8 | 5.2 |\n";
    let doc = ParsedDoc::parse(md);
    let node = generate_production_order(&doc).remove(0);
    validate(&node).unwrap();

    let json = serde_json::to_string(&node).unwrap();
    let back: DomainNode = serde_json::from_str(&json).unwrap();
    assert_eq!(node, back);
}
