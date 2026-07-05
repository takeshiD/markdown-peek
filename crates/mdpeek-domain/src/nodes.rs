//! ドメインプリミティブ (AGENTS.md §4.1 の `UiNode` ドメイン variant / §5.1 の
//! 外側 `domainRegistry`)。文書タイプが増えてもコア 12 種は不変で、ここに数個
//! 足すだけで済む、というのが §9.3-1「2 層 registry」の設計判断。
//!
//! ⚠ 統合時: 下記 6 型は Layer 3 の `mdpeek-core::ir::UiNode` enum の
//! ドメイン variant にそのまま吸収する。`DomainNode` enum は `UiNode` に統合され
//! 消える (README「統合手順」参照)。variant 名 = §4.1 の `kind` と一致させてある。

use serde::{Deserialize, Serialize};

use crate::seam::{NodeMeta, Quantity, SourceRange};

/// Layer 3.5 が追加するドメイン UI ノードの集合。
///
/// serde 表現は `#[serde(tag = "kind")]` で AGENTS.md §4.1 の `UiNode` と同型。
/// 例: `{"kind":"Glossary", "source_range":{...}, "entries":[...]}`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DomainNode {
    /// 用語集: 小説の世界観語 / 契約の定義語 (初出ジャンプ + 定義)。
    Glossary(GlossaryNode),
    /// 登場人物パネル (名前 + 初出ジャンプ + 一言要約)。
    CharacterRoster(CharacterRosterNode),
    /// 手順の 1 ステップずつナビ (前提/所要時間/ロールバック)。
    StepNavigator(StepNavigatorNode),
    /// 公差/許容範囲のビジュアルバー (規格中心からの位置)。
    ToleranceMeter(ToleranceMeterNode),
    /// 数量連動テーブル (材料の人数スケーリング等)。
    ScalableTable(ScalableTableNode),
    /// 当事者 × 義務/権利マトリクス (契約/規程)。
    ObligationMatrix(ObligationMatrixNode),
}

impl DomainNode {
    /// registry allowlist (AGENTS.md §3.5 / §5.1) 用の kind 文字列。
    pub fn kind(&self) -> &'static str {
        match self {
            DomainNode::Glossary(_) => "Glossary",
            DomainNode::CharacterRoster(_) => "CharacterRoster",
            DomainNode::StepNavigator(_) => "StepNavigator",
            DomainNode::ToleranceMeter(_) => "ToleranceMeter",
            DomainNode::ScalableTable(_) => "ScalableTable",
            DomainNode::ObligationMatrix(_) => "ObligationMatrix",
        }
    }

    /// 共通メタへの参照 (visibility フィルタ・sourceRange 検証で使う)。
    pub fn meta(&self) -> &NodeMeta {
        match self {
            DomainNode::Glossary(n) => &n.meta,
            DomainNode::CharacterRoster(n) => &n.meta,
            DomainNode::StepNavigator(n) => &n.meta,
            DomainNode::ToleranceMeter(n) => &n.meta,
            DomainNode::ScalableTable(n) => &n.meta,
            DomainNode::ObligationMatrix(n) => &n.meta,
        }
    }

    /// Layer 3.5 が提供する全 kind (二層 registry の外側層 allowlist)。
    pub const KINDS: [&'static str; 6] = [
        "Glossary",
        "CharacterRoster",
        "StepNavigator",
        "ToleranceMeter",
        "ScalableTable",
        "ObligationMatrix",
    ];
}

// ---------------------------------------------------------------------------
// Glossary — 用語集 (小説の造語 / 契約の定義語)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlossaryNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub entries: Vec<GlossaryEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlossaryEntry {
    pub term: String,
    pub definition: String,
    /// 初出位置。原文へジャンプする根拠 (SourceRangeLink)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_occurrence: Option<SourceRange>,
}

// ---------------------------------------------------------------------------
// CharacterRoster — 登場人物パネル
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterRosterNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub characters: Vec<Character>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Character {
    pub name: String,
    /// 一言要約 (※ 断定せず候補・要確認。判断は読者 — DESIGN.md 思想 8)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// 初出位置 (初出ジャンプ)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_occurrence: Option<SourceRange>,
}

// ---------------------------------------------------------------------------
// StepNavigator — 手順の 1 ステップずつナビ (手順書/SOP/レシピ)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepNavigatorNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub steps: Vec<Step>,
    /// 冒頭にまとめる必要物 (工具/材料/前提条件)。§9.2「準備不足」対策。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prerequisites: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// 1 始まりのステップ番号。
    pub index: u32,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 所要時間 (§9.2「1 ステップずつ＋所要時間」)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<Quantity>,
    /// 危険操作の警告 (表示のみ・自動実行しない — セキュリティ §8)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caution: Option<String>,
    /// 失敗時のロールバック手順 (§9.2「ロールバックを隣接表示」)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_range: Option<SourceRange>,
}

// ---------------------------------------------------------------------------
// ToleranceMeter — 公差/許容メーター (生産指示書)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToleranceMeterNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub meters: Vec<ToleranceMeter>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToleranceMeter {
    /// 測定項目名 (例: 「外径」)。
    pub label: String,
    /// 実測/規格値と上下限・中心を保持する Quantity。
    pub quantity: Quantity,
}

// ---------------------------------------------------------------------------
// ScalableTable — 数量連動テーブル (レシピの人数スケーリング等)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScalableTableNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub columns: Vec<Column>,
    pub rows: Vec<ScalableRow>,
    /// 基準となる分量 (例: 2 人前)。renderer はこれを基準にスケールする。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_scale: Option<Quantity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScalableRow {
    pub cells: Vec<Cell>,
}

/// テーブルセル。数量セルは `scalable: true` の `Quantity` を持ち、renderer 側で
/// スケール係数に応じ再計算される (AGENTS.md §9.3 quantity operable)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Cell {
    Amount(Quantity),
    Text(String),
}

// ---------------------------------------------------------------------------
// ObligationMatrix — 当事者 × 義務/権利 (契約/規程)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObligationMatrixNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    /// 当事者 (例: 「甲」「乙」)。
    pub parties: Vec<String>,
    pub obligations: Vec<Obligation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Obligation {
    /// 当事者名 (`parties` のいずれか)。
    pub party: String,
    pub kind: ObligationKind,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_range: Option<SourceRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationKind {
    Obligation,
    Right,
}
