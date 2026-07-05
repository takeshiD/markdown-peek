//! # Seam — 共有 IR 型 (Layer 3 の `mdpeek-core::ir` への接続点)
//!
//! Layer 3.5 は Layer 3 が定めるコア IR (`SourceRange` / `NodeMeta` / `Origin` /
//! `Visibility` / `Quantity` / `UiNode`) の *上に乗る* 追加レイヤーである
//! (AGENTS.md §9.3-1「2 層 registry」)。しかし Layer 3 はまだ別 worktree で
//! 実装中でありコードが未マージのため、Layer 3.5 を単体でコンパイル・テスト
//! できるよう、AGENTS.md §4.1 と **1:1 一致する最小定義** をここに置く。
//!
//! ⚠ 統合時 (Layer 3 マージ後) はこのファイルを削除し、
//!   `use mdpeek_core::ir::{SourceRange, NodeMeta, Origin, Visibility, Quantity};`
//! に差し替えるだけでよい。ドメインノード (`nodes.rs`) は Layer 3 の `UiNode`
//! enum のドメイン variant にそのまま移設する (README「統合手順」参照)。

use serde::{Deserialize, Serialize};

/// 原文中の範囲。全 UI ノードは根拠としてこれを持つ (DESIGN.md 思想: 全 UI は
/// sourceRange に紐づく)。AGENTS.md §4.1 と同一。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRange {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// 生成ノードの出所。rules 既定、LLM は `feature = "llm"` 経路。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    #[default]
    Rules,
    Llm,
}

/// ノードの可視条件。小説などで「既読位置より先の内容を要約・生成に出さない」
/// ための制御 (AGENTS.md §9.3 reading-position aware)。既定は常に可視。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Always,
    /// `reveal_after_line` 行目まで既読になった場合のみ表示 (ネタバレ防止)。
    UntilRead { reveal_after_line: u32 },
}

/// 全ノード共通メタ。各ノードに `#[serde(flatten)]` で載せる (AGENTS.md 論点 D:
/// flatten 推奨)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NodeMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_range: Option<SourceRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub origin: Origin,
    #[serde(default)]
    pub visibility: Visibility,
}

impl NodeMeta {
    /// rules 生成 (confidence=1.0, origin=Rules, 常に可視) のメタを作る近道。
    pub fn rules(source_range: SourceRange) -> Self {
        Self {
            source_range: Some(source_range),
            confidence: Some(1.0),
            origin: Origin::Rules,
            visibility: Visibility::Always,
        }
    }
}

/// 数値を「読む」ではなく「使える」形にするための共通型
/// (AGENTS.md §9.3 quantity operable)。公差メーター・材料スケーリング・チャートが
/// 共通で利用する。AGENTS.md §4.1 と同一。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quantity {
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// 公差/許容 下限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// 公差/許容 上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// 規格中心値。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nominal: Option<f64>,
    /// 人数スケーリング等で連動再計算するか。
    #[serde(default)]
    pub scalable: bool,
}

impl Quantity {
    /// 単位のみを伴う素の数値。
    pub fn scalar(value: f64, unit: Option<String>) -> Self {
        Self {
            value,
            unit,
            min: None,
            max: None,
            nominal: None,
            scalable: false,
        }
    }
}
