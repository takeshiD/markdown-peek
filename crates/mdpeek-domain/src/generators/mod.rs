//! rules generator (AGENTS.md §3.4 の `RulesGenerator` 相当・オフライン既定)。
//!
//! Layer 3.5 のスコープは「構造的で rules が効く」文書 = 生産指示書・手順書を先行
//! (AGENTS.md §10)。散文系 (小説・契約) は LLM 依存が高いため後続で、ここでは
//! ドメインプリミティブの型 (`nodes.rs`) だけ用意し generator は繋ぎに留める。
//!
//! ここで生成するのは Layer 3.5 が *追加する* ドメインノードのみ:
//! - 生産指示書 → `ToleranceMeter` (公差メーター)
//! - 手順書/レシピ → `StepNavigator` (ステップナビ) + `ScalableTable` (材料スケール)
//!
//! サマリカード (`ConfigViewer`) / BOM (`DataTable`) / 検査 (`Checklist`) は
//! コア 12 種 (Layer 3 所有) の再利用なのでここでは生成しない (§9.3-1 2 層 registry)。

mod procedure;
mod production_order;
pub mod text;

pub use procedure::generate_procedure;
pub use production_order::generate_production_order;
