//! # mdpeek-domain — Layer 3.5: 非開発ドメインのドメインプリミティブ
//!
//! AGENTS.md §9.2 / §10「Layer 3.5」の実装。コア registry (Layer 3) が固まった後に
//! 乗せる *追加レイヤー* として、以下を提供する:
//!
//! 1. ドメインプリミティブ 6 種 (`nodes`): `Glossary` / `CharacterRoster` /
//!    `StepNavigator` / `ToleranceMeter` / `ScalableTable` / `ObligationMatrix`。
//! 2. rules generator (`generators`): 構造的な文書を先行 —
//!    生産指示書 → `ToleranceMeter`、手順書/レシピ → `StepNavigator` + `ScalableTable`。
//! 3. 横断要件: ネタバレ制御 (`visibility` = `Visibility::UntilRead`) と
//!    数値の operable 化 (`quantity` = 公差判定/材料スケーリング)。
//!
//! ## Layer 1/2/3 との干渉について
//!
//! このクレートはルート workspace に *登録していない* 独立クレートで、既存
//! `src/` やルート `Cargo.toml` を一切変更しない。共有 IR 型は [`seam`] に最小
//! 定義しており、Layer 3 マージ後は `mdpeek-core::ir` へ差し替える (README 参照)。

pub mod generators;
pub mod nodes;
pub mod parser;
pub mod quantity;
pub mod seam;
pub mod visibility;

pub use nodes::DomainNode;
pub use parser::ParsedDoc;
pub use seam::{NodeMeta, Origin, Quantity, SourceRange, Visibility};

/// Layer 3.5 が二層 registry の *外側* (domainRegistry) に足す kind の allowlist。
/// AGENTS.md §3.5「registry allowlist に無い kind は reject」/ §5.1 の domainRegistry
/// に対応する。web 側 `web/registry.ts` の `domainRegistry` と一致させること。
pub const DOMAIN_KINDS: [&str; 6] = nodes::DomainNode::KINDS;

/// ドメインノードを検証する (AGENTS.md §3.5 validate の Layer 3.5 分)。
/// - kind が allowlist にあること。
/// - sourceRange があれば行番号が 1 始まりで start <= end であること (捏造レンジ検出)。
///
/// 統合時は Layer 3 の validate に統合し、実ドキュメント Block との突合まで行う。
pub fn validate(node: &DomainNode) -> Result<(), String> {
    if !DOMAIN_KINDS.contains(&node.kind()) {
        return Err(format!("unknown kind: {}", node.kind()));
    }
    if let Some(sr) = &node.meta().source_range {
        if sr.start_line == 0 || sr.end_line == 0 {
            return Err("source_range line numbers are 1-based".into());
        }
        if (sr.start_line, sr.start_column) > (sr.end_line, sr.end_column) {
            return Err("source_range start after end".into());
        }
    }
    Ok(())
}
