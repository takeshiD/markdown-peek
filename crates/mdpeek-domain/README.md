# mdpeek-domain — Layer 3.5: 非開発ドメインのドメインプリミティブ

AGENTS.md の開発ロードマップ **Layer 3.5「非開発ドメイン (§9.2)」** の実装。
コア registry (Layer 3) が固まった後に *上に重ねる追加レイヤー* として、以下を提供する。

| 成果物 | 対応 (AGENTS.md) | 実装 |
|---|---|---|
| ドメインプリミティブ 6 種 | §4.1 `UiNode` ドメイン variant / §5.1 domainRegistry | `src/nodes.rs`, `web/registry.ts` |
| 生産指示書・手順書の rules generator (先行) | §10「構造的で rules が効く」 | `src/generators/` |
| ネタバレ制御 (`Visibility::UntilRead`) | §9.3-2 reading-position aware | `src/visibility.rs` |
| 数値の operable 化 (`Quantity`) | §9.3-3 quantity operable | `src/quantity.rs` |

ドメインプリミティブは 6 種:
`Glossary` / `CharacterRoster` / `StepNavigator` / `ToleranceMeter` / `ScalableTable` / `ObligationMatrix`
(AGENTS.md §9.3-1 で「ドメインを増やしても新規に要るのはこの数個だけ」と特定されたもの)。

生成器は Layer 3.5 が *追加する* ドメインノードのみを出す:

- **生産指示書** → `ToleranceMeter` (公差メーター)。規格値±公差 / 下限・上限の
  検査テーブルを検出し、`Quantity` (min/max/nominal) 付きメーターに変換。
- **手順書・レシピ** → `StepNavigator` (ステップナビ + 前提物 + 注意/ロールバック) と
  `ScalableTable` (「N 人前」を検出し数量セルを人数連動 `scalable` 化)。

> サマリカード (`ConfigViewer`) / BOM (`DataTable`) / 検査 (`Checklist`) は
> コア 12 種 (Layer 3 所有) の再利用なので本クレートでは生成しない
> (§9.3-1 の 2 層 registry: コアは不変、ドメイン層に数個足すだけ)。

散文系 (小説・契約) は LLM 依存が高いため後続。型 (`Glossary`/`CharacterRoster`/
`ObligationMatrix`) と web コンポーネントは用意済みで、rules/LLM generator を後で足せる。

## ビルド・テスト・デモ

このクレートは **あえてルート workspace に登録していない** (下記「干渉回避」参照)。

```sh
# ビルド / テスト
cargo test  --manifest-path crates/mdpeek-domain/Cargo.toml

# 生成デモ (埋め込みサンプルで両ジェネレータを実行し UI IR JSON を出力)
cargo run   --manifest-path crates/mdpeek-domain/Cargo.toml --example generate

# 任意ファイルで
cargo run   --manifest-path crates/mdpeek-domain/Cargo.toml --example generate \
            -- production crates/mdpeek-domain/examples/production_order.md
```

## Layer 1/2/3 との干渉回避

Layer 1–3 は別 worktree で並行実装中で、次を触る:
`src/` (→ `mdpeek-core` 化) / ルート `Cargo.toml` (→ workspace 化) / `web/` (Preact 導入)。

衝突を避けるため本クレートは **完全に独立** させてある:

- `crates/mdpeek-domain/` に閉じ、既存 `src/`・ルート `Cargo.toml`・`static/` を一切変更しない。
- 独自 `Cargo.toml` を持ち、ルート workspace に **登録しない**
  (Layer 2 が workspace 化するので、そのとき members に追加すればよい)。
- 共有 IR 型 (`SourceRange`/`NodeMeta`/`Origin`/`Visibility`/`Quantity`) は
  [`src/seam.rs`](src/seam.rs) に AGENTS.md §4.1 と 1:1 一致で最小定義。
- web も `web/` サブツリーに閉じ、Layer 3 の `web/src/` とは別ファイル。

## 統合手順 (Layer 3 マージ後)

Layer 3 の `mdpeek-core` (IR + registry) がマージされたら、次の機械的な差し替えで統合できる。

1. **seam の削除**: `src/seam.rs` を削除し、各 `use crate::seam::…` を
   `use mdpeek_core::ir::…` に置換。§4.1 と同型なのでフィールドは一致。
2. **ノードの吸収**: `src/nodes.rs` の 6 型を `mdpeek_core::ir::UiNode` の
   ドメイン variant (§4.1 に既に列挙済み) に移し、`DomainNode` enum を廃止。
   `kind()` 文字列 = variant 名は変えないこと。
3. **パーサ差し替え**: `src/parser.rs` を Layer 1 の
   `mdpeek_core::parser::BlockTree` に置換。generator の入力を `&ParsedDoc` から
   `&BlockTree`/`&DocumentModel` に変える (抽出ロジックは流用可)。
4. **generator 登録**: `generate_production_order` / `generate_procedure` を
   Layer 3 の `RulesGenerator` (§3.4) から `DocumentType::{ProductionOrder,Procedure,Recipe}`
   に対して呼ぶよう配線。
5. **validate 合流**: `lib.rs::validate` を Layer 3 の validate (§3.5) に統合し、
   実 Block との sourceRange 突合まで行う。
6. **web registry 合流**: `web/registry.ts` の `domainRegistry` を Layer 3 の
   `web/src/registry.ts` に `{ ...coreRegistry, ...domainRegistry }` として合流。
   `web/ir.ts` は `ts-rs` 生成物に置換 (`web/components/*` はそのまま流用可)。
7. **workspace 登録**: ルート `Cargo.toml` の `[workspace] members` に
   `crates/mdpeek-domain` (または `mdpeek-core` に統合) を追加。

いずれも追加・移設が中心で、Layer 3 のコア設計 (§4.1 の variant 名 / §5.1 の 2 層
registry) にそのまま乗る形になっている。

## ファイル構成

```
crates/mdpeek-domain/
├── Cargo.toml              # 独立クレート (workspace 未登録)
├── src/
│   ├── lib.rs              # 公開 API + DOMAIN_KINDS + validate
│   ├── seam.rs             # 共有 IR 型 (統合時に mdpeek-core へ差し替え)
│   ├── nodes.rs            # ドメインプリミティブ 6 種 + DomainNode
│   ├── quantity.rs         # 公差判定 / スケーリング (operable)
│   ├── visibility.rs       # reading-position フィルタ (ネタバレ制御)
│   ├── parser.rs           # 軽量 MD パーサ (統合時 BlockTree へ)
│   └── generators/
│       ├── text.rs         # 数量文字列パース等のヘルパ
│       ├── production_order.rs
│       └── procedure.rs
├── examples/
│   ├── generate.rs         # デモ / 手動検証 CLI
│   ├── production_order.md
│   └── recipe.md
├── tests/
│   └── wire_format.rs      # serde wire format = web/ir.ts の固定
└── web/                    # Preact 側 (Layer 3 web に合流予定)
    ├── ir.ts               # 手書き TS 型 (統合時 ts-rs 生成へ)
    ├── quantity.ts         # operable ロジック web 版
    ├── registry.ts         # domainRegistry
    └── components/*.tsx     # 6 コンポーネント + SourceRangeLink
```
