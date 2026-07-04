# Generative UI Markdown Viewer — アーキテクチャ設計書 (v0.2)

> このドキュメントは [`DESIGN.md`](DESIGN.md) の構想を、現行の Rust 実装 (`pulldown-cmark` + `axum` + TUI) の上に**実装可能な形**へ落とし込んだ設計書のたたき台です。`DESIGN.md` = *what / why*、本書 = *how* という役割分担です。
>
> 論点 A–F は **2026-07-05 に決定済み**(§0.1)。以降の記述はその決定を反映しています。

## 0. 確定した基本方針

先の議論で確定した3つの分岐:

| 論点 | 決定 |
|------|------|
| Renderer 戦略 | **コンポーネント FW を埋め込み**。Preact + `@preact/signals` を採用し、IR 駆動の component registry でクライアント描画。バンドルは単一バイナリに embed。 |
| LLM 連携 | **trait 化 + Claude 既定 + rules 優先**。`RulesGenerator`(オフライン既定) + `ClaudeGenerator`(`feature = "llm"`)。 |
| 対象文書タイプ | **全部**(技術設計書 / README / ADR / 議事録 / 作業手順書 / ログ調査メモ / changelog / git log)。ただしロードマップで着手順を規定。 |

`DESIGN.md` の「重要な設計思想」10項目は不変の制約として本書全体に効かせる(特に *Markdown 本文は変更しない* / *LLM は UI IR だけを生成* / *renderer は決定論的* / *全 UI は sourceRange に紐づく* / *任意コード実行禁止*)。

### 0.1 論点 A–F の決定 (2026-07-05)

| 論点 | 決定 | 要旨 |
|------|------|------|
| **A** SSR と Preact の同居 | **全面 Preact** | web の SSR HTML 描画(`HtmlEmitter`)は廃止。本文ペインも生成 UI ペインも Preact が描画し、描画系統を1つに統一。SourceRangeLink/ライブハイライトが同一コンポーネントツリー内で解決できる。 |
| **B** workspace 化の時期 | **今すぐ** | Layer 1 の段階で `mdpeek-core` を含む workspace へ再編する(後回しにしない)。 |
| **C** `web/dist` の扱い | **コミット + CI 鮮度チェック** | ビルド成果物を in-tree コミットして `include_bytes!`。CI で再ビルド→`git diff --exit-code web/dist` により stale を検出。`cargo install` は JS ツールチェイン不要のまま。 |
| **D** ノードのメタ表現 | **入れ子 `meta`** | 共通メタ(source_range/confidence/origin/visibility)を各ノードの `meta: NodeMeta` に**入れ子**で持つ(`#[serde(flatten)]` は使わない)。serde × 内部タグ enum × `ts-rs` の相性問題を回避。 |
| **E** 生成物の永続化 | **保存しない** | 生成 UI IR をディスクにキャッシュ/コミットしない。実行中プロセスの**メモリ内再利用のみ**。恒久キャッシュ(`.gui.json`)・sidecar・`--emit-gui` は採用しない。 |
| **F** LLM 呼び出しの位置 | **両方** | rules-first で即描画 → LLM 依存ノードは server 内 async でプログレッシブに後追い(#16 と統合)。加えて `mdpeek gen` を**一回きりの明示エクスポート**(stdout/指定ファイル)として提供。E に従い管理キャッシュは作らない。 |

> **E と F の整合**: 恒久キャッシュを持たない(E)ため、`serve` 再起動や別プロセスでは LLM を再実行する。これは *rules-first(LLM 依存ノードだけ生成)* と *同一プロセス内メモリ再利用* で吸収する、というトレードオフを受け入れる。`mdpeek gen` はユーザーが出力先を指定する一回きりのエクスポートであり、自動管理される cache ではない。

---

## 1. 全体アーキテクチャ

核心は **「重い処理・判断は Rust core に集約し、UI IR を web/TUI 共通の契約(wire format)にする」**こと。フロントエンドはこの IR を決定論的に描画するだけの薄い層に保つ。

```
        ┌──────────────────────── Rust core (mdpeek-core) ───────────────────────┐
        │                                                                        │
document.md ─▶ parser ─▶ model ─▶ analyzer ─▶ planner ─▶ generator ─▶ ir(validate)
        │      (pulldown  (block   (rules +   (semantic   (rules/     (schema +              │
        │       -cmark     tree +   LLM)       model →     LLM →       sourceRange            │
        │       OffsetIter offsets)            UI plan)    UI IR)      検証。永続 cache なし E)│
        └──────────────────────────────────────────┬───────────────────────────┘
                                                    │  UI IR (JSON, serde)
                        ┌───────────────────────────┼───────────────────────────┐
                        ▼                                                         ▼
              web renderer (Preact)                                    tui renderer (ratatui)
              registry[kind] → component                               registry[kind] → widget
              (全 UiNode 対応)                                          (部分集合を段階対応)
```

### 1.1 なぜ IR を Rust 側の source of truth にするか

- **web と TUI で解析を二重実装しない**。両 renderer は同じ IR JSON を食べる。
- **セキュリティ境界が 1 か所に集約**する(LLM 出力 → schema validation → sourceRange 検証 が Rust core だけに存在)。
- **差分再生成(メモリ内)**を core に閉じ込められる(#16 のライブ更新と統合。永続キャッシュは持たない=論点 E)。
- TS 型は Rust 型から**自動生成**(`ts-rs`)し、二重定義を防ぐ。`DESIGN.md` の TS 型定義は「wire format の仕様」として採用するが、正本は Rust。

### 1.2 現行コードとの接続

| 現行 | 本設計での位置づけ |
|------|------------------|
| `src/gfm.rs` (`parser_options`, `transform`) | `mdpeek-core::parser` にそのまま移設。Layer 1 として維持。 |
| `src/emitter/html.rs` | **web では廃止**(論点 A: 全面 Preact)。本文描画も `BlockTree`/IR から Preact が行う。※ 静的 HTML エクスポート(`mdpeek gen --html` 等)の用途にのみ残置を検討。 |
| `src/emitter/term.rs` | TUI Layer 1 描画として存続(TUI は Preact 化しない)。 |
| `src/server.rs` (`file_path`/`theme` が `Arc<RwLock>`) | `BlockTree`/IR を JSON で配る API を足す土台として再利用。HTML テンプレ注入は Preact 配信へ置換。 |
| `src/watcher.rs` (単一パス blocking) | チャネル化し再生成トリガに接続(#12/#16 と共通)。 |
| `static/js/main.js` (素の JS) | Preact 全面移行に伴い**撤去**。テーマ切替 / mermaid / hljs / TOC / WS は Preact 側へ移す。 |

> **✅ 論点 A(決定)**: **全面 Preact**。web の SSR HTML 描画は廃止し、本文ペインも生成 UI ペインも Preact が描画する。本文ペインは `BlockTree` から Preact のマークダウンレンダラで描画し、各ブロックに `data-block-id` を付与。SourceRangeLink とライブハイライトを**同一コンポーネントツリー内**で解決する。TUI(ratatui)は対象外で、term emitter を使い続ける。

---

## 2. ディレクトリ / クレート構成

単一バイナリクレートから **Cargo workspace** へ再編する。core を独立させることで、CLI / server / TUI / (将来の neovim・LSP) が同じ core を共有できる。

```
markdown-peek/
├── Cargo.toml                      # [workspace]
├── crates/
│   ├── mdpeek-core/                # ★ 解析〜IR〜キャッシュの心臓部(no I/O 依存を最小化)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── parser/             # markdown -> block tree (+ byte offset)
│   │   │   │   ├── mod.rs
│   │   │   │   ├── gfm.rs          # ← 現 src/gfm.rs を移設
│   │   │   │   └── tree.rs         # OffsetIter -> BlockTree(sourceRange 付き)
│   │   │   ├── model/              # semantic model(解析中間表現)
│   │   │   │   ├── mod.rs
│   │   │   │   ├── document.rs     # DocumentType, DocumentModel
│   │   │   │   └── block.rs        # BlockClass, Block
│   │   │   ├── analyzer/           # 分類・抽出(rules + LLM の入口)
│   │   │   │   ├── mod.rs
│   │   │   │   ├── doctype.rs      # 文書種別推定
│   │   │   │   ├── block_class.rs  # ブロック種別推定
│   │   │   │   ├── code.rs         # コードブロック意図(bash/json/sql/…)
│   │   │   │   ├── table.rs        # 表の意味推定(status列など)
│   │   │   │   └── tasks.rs        # task list 抽出
│   │   │   ├── planner/            # semantic model -> UI plan
│   │   │   │   ├── mod.rs
│   │   │   │   └── doctype/        # 文書タイプ別プランナ
│   │   │   │       ├── design_doc.rs
│   │   │   │       ├── readme.rs
│   │   │   │       ├── adr.rs
│   │   │   │       ├── minutes.rs
│   │   │   │       ├── runbook.rs
│   │   │   │       ├── investigation.rs
│   │   │   │       ├── changelog.rs
│   │   │   │       ├── novel.rs          # 小説・物語
│   │   │   │       ├── production_order.rs # 生産/製造指示書
│   │   │   │       ├── procedure.rs      # 手順書/SOP(runbook を汎用化)
│   │   │   │       └── recipe.rs         # レシピ(手順書の身近な例)
│   │   │   ├── generator/          # UI plan -> UI IR
│   │   │   │   ├── mod.rs
│   │   │   │   ├── rules.rs        # RulesGenerator(既定・オフライン)
│   │   │   │   ├── llm/            # feature = "llm"
│   │   │   │   │   ├── mod.rs
│   │   │   │   │   ├── claude.rs   # Anthropic adapter
│   │   │   │   │   └── prompt.rs   # プロンプト & few-shot
│   │   │   │   └── traits.rs       # Analyzer / Generator trait
│   │   │   ├── ir/                 # ★ UI IR 型定義(正本)
│   │   │   │   ├── mod.rs
│   │   │   │   ├── node.rs         # UiNode enum
│   │   │   │   ├── range.rs        # SourceRange
│   │   │   │   ├── validate.rs     # schema + sourceRange 検証
│   │   │   │   └── registry.rs     # 許可コンポーネント名の allowlist
│   │   │   └── security/           # command safety, link/image policy
│   │   │       # 注: 永続 cache モジュールは持たない(論点 E)。生成物はメモリ内のみ。
│   │   │       ├── mod.rs
│   │   │       ├── command.rs      # 危険コマンド検出
│   │   │       └── policy.rs       # 外部リンク/画像ポリシー
│   │   └── Cargo.toml
│   ├── mdpeek-cli/                 # バイナリ `mdpeek`(現 src/main.rs, cli.rs, config.rs)
│   │   ├── src/{main.rs, cli.rs, config.rs}
│   │   └── Cargo.toml
│   ├── mdpeek-server/              # axum server(現 src/server.rs, emitter/html.rs)
│   │   ├── src/{lib.rs, routes.rs, ws.rs, html.rs}
│   │   └── Cargo.toml
│   └── mdpeek-tui/                 # ratatui viewer(現 emitter/term.rs + #12)
│       ├── src/{lib.rs, app.rs, render.rs}
│       └── Cargo.toml
├── web/                            # ★ Preact フロントエンド(ビルドして server に embed)
│   ├── package.json
│   ├── vite.config.ts
│   ├── src/
│   │   ├── main.tsx
│   │   ├── ir.ts                   # ← ts-rs 生成(コミットする)
│   │   ├── registry.ts             # componentRegistry
│   │   ├── layout/                 # SplitPane, Outline, Tabs...
│   │   ├── components/             # UiNode 描画コンポーネント群(§5)
│   │   └── live/                   # WS 差分更新 + 変更ハイライト(#16)
│   └── dist/                       # ビルド成果物(server が include_bytes! で埋め込む)
├── static/                         # Layer 1 の既存アセット(css 等)
└── docs/
    └── architecture.md             # 本書
```

> **✅ 論点 B(決定)**: **今すぐ workspace 化**する。Layer 1 の段階で `mdpeek-core`(lib)を抜き出し、バイナリ `mdpeek` がそれに依存する形へ再編する(細かい server/tui/cli 分割は必要に応じて後続)。`release.yml`・バイナリ名 `mdpeek` の追従を同時に行う。
>
> **✅ 論点 C(決定)**: `web/dist` を **リポジトリにコミット**して `include_bytes!` で埋め込む。`cargo install` は JS ツールチェイン不要のまま。**CI で web を再ビルドし `git diff --exit-code web/dist` を回して stale コミットを検出**する(`build.rs` で `npm build` はしない)。

---

## 3. パイプライン各段の責務

`DESIGN.md` の「処理フロー」12ステップを、各モジュールの責務に割り付ける。

### 3.1 parser — `markdown -> BlockTree`

- `pulldown-cmark` の `Parser::into_offset_iter()` を使い、**各イベントに byte range** を持たせる。
- byte offset → `SourceRange { start_line, start_col, end_line, end_col }` へ変換(行頭 offset テーブルを 1 パスで構築)。
- イベント列を**トップレベルブロック単位の軽量ツリー** `BlockTree` に畳む(見出し階層、段落、コードブロック、表、リスト)。既存 emitter を壊さないため、pulldown-cmark からの乗り換え(comrak 等)は**しない**。

```rust
pub struct BlockTree {
    pub blocks: Vec<Block>,
    pub line_index: LineIndex,   // byte offset <-> (line,col)
}

pub struct Block {
    pub id: BlockId,             // ツリー内の安定 ID(差分再生成・ハイライト用)
    pub kind: BlockKind,         // Heading{level}, Paragraph, CodeBlock{lang}, Table, List{task}, ...
    pub range: SourceRange,
    pub children: Vec<Block>,
    pub text: String,            // 表示/解析用の抽出テキスト
}
```

### 3.2 analyzer — 分類・抽出

`DESIGN.md` の「Rules で処理すべきこと / LLM に任せること」に厳密に従い、**deterministic に確定できるものは全部 rules**、曖昧なものだけ LLM 候補にする。

| 処理 | 担当 |
|------|------|
| heading outline / code lang 検出 / task 抽出 / table 抽出 / link・image 抽出 / frontmatter / mermaid・diff・HTTP method 検出 / JSON・YAML・TOML parse / TODO・FIXME | **rules(必須・確定)** |
| 文書種別推定 / 曖昧見出しの意味解釈 / 設計書の観点抽出 / risk・open question 抽出 / コードブロック意図推定 / ログのクラスタリング / 図化可能構造の検出 / TODO の意味分類 / README の usage/config/troubleshooting 分離 | **LLM(あれば) or rules ヒューリスティック(なければ)** |

- 文書種別推定 (`doctype.rs`) はまず **rules**(ファイル名 `README*`/`ADR-*`/`CHANGELOG*`、frontmatter、見出しパターン)で信頼度付き推定。低信頼のときだけ LLM へ。
- analyzer の出力は `DocumentModel`(§4.2)。

### 3.3 planner — semantic model → UI plan

- 文書タイプ別の `planner/doctype/*.rs` が `DocumentModel` を受け取り、「どの UiNode をどの順で出すか」の **UI plan** を決める。
- 例: 技術設計書 → `[Tabs(Overview, Architecture, DataModel, Risks, OpenQuestions, TODO)]` + `RiskPanel` + `OpenQuestionsPanel` + `ReviewChecklist`(missing section 検出)。
- planner は **どの UiNode を出すか**を決めるだけ。中身の生成(rules or LLM)は generator。

### 3.4 generator — UI plan → UI IR

```rust
pub trait Generator {
    /// UI plan と文書モデルから UI IR を生成する。
    async fn generate(&self, plan: &UiPlan, model: &DocumentModel) -> Result<Vec<UiNode>>;
}
```

- `RulesGenerator`: 決定論的に埋められるノード(DataTable, Checklist, Timeline(構造的), Callout, Diagram(既存 mermaid), ConfigViewer)を生成。**オフライン既定**。
- `ClaudeGenerator`(`feature = "llm"`): rules で埋まらない/低信頼のノードだけを LLM に投げ、**UI IR(JSON)だけ**を返させる。§7。
- generator は **rules で全部埋まれば LLM を呼ばない**(コスト・再現性)。
- **rules-first + async progressive(論点 F)**: `serve` はまず rules 出力を即返し、LLM 依存ノードは server 内 async で後追い生成 → 到着次第 WS で push(#16 と統合)。生成物は永続化せず(論点 E)、同一プロセス内のメモリでのみ再利用する。

### 3.5 validate — schema + sourceRange 検証

- serde でのデシリアライズ(型が合わない IR は reject)。
- `registry.rs` の allowlist に無い `kind` は reject(`DESIGN.md`: 未知 component は reject)。
- **sourceRange 検証**: すべての `sourceRange` が実ドキュメントの範囲内で、かつ対応 Block と矛盾しないこと。捏造レンジは reject(hallucination 検出)。
- `confidence` が閾値未満のノードは `low_confidence` フラグ付きで通す(renderer が明示表示)。

### 3.6 生成物の非永続方針

論点 E により、生成 UI IR はディスクに保存しない。詳細は §6。

---

## 4. 型定義

### 4.1 UI IR(正本 = Rust、`ir/node.rs`)

`DESIGN.md` の TS 型を Rust の serde 型として定義。`#[serde(tag = "kind")]` で TS の discriminated union と 1:1 対応させ、`ts-rs` で `web/src/ir.ts` を自動生成する。

```rust
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRange {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// 生成 UI の 1 ノード。renderer は kind で registry を引く。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum UiNode {
    Tabs(TabsNode),
    Timeline(TimelineNode),
    Checklist(ChecklistNode),
    DataTable(DataTableNode),
    Diagram(DiagramNode),
    Callout(CalloutNode),
    RiskPanel(RiskPanelNode),
    ApiExplorer(ApiExplorerNode),
    ConfigViewer(ConfigViewerNode),
    DependencyGraph(DependencyGraphNode),
    LogTimeline(LogTimelineNode),
    CommitGraph(CommitGraphNode),

    // --- ドメインプリミティブ(§5.1 の 2 層 registry・外側層) ---
    Glossary(GlossaryNode),                 // 用語集: 小説の世界観語 / 契約の定義語
    CharacterRoster(CharacterRosterNode),   // 登場人物パネル(初出ジャンプ + 一言要約)
    StepNavigator(StepNavigatorNode),       // 手順の 1 ステップずつナビ(前提/所要時間)
    ToleranceMeter(ToleranceMeterNode),     // 公差/許容範囲のビジュアルバー(Quantity)
    ScalableTable(ScalableTableNode),       // 数量連動テーブル(材料の人数スケーリング等)
    ObligationMatrix(ObligationMatrixNode), // 当事者 × 義務/権利マトリクス(契約/規程)
}

/// 全ノード共通のメタ(sourceRange + 信頼度 + 出所 + 可視条件)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMeta {
    pub source_range: Option<SourceRange>,
    pub confidence: Option<f32>,          // 0.0–1.0, LLM 生成時
    #[serde(default)]
    pub origin: Origin,                   // Rules | Llm
    #[serde(default)]
    pub visibility: Visibility,           // ネタバレ/読書位置制御(§9.3)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    #[default]
    Rules,
    Llm,
}

/// ノードの可視条件。小説などで「既読位置より先の内容を要約・生成に出さない」
/// ための制御(§9.3 reading-position aware)。既定は常に可視。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Always,
    /// 指定行以降を既読にした場合のみ表示(ネタバレ防止)。
    UntilRead { reveal_after_line: u32 },
}

/// 数値を「読む」ではなく「使える」形にするための共通型(§9.3 quantity operable)。
/// 公差メーター・材料スケーリング・チャートが共通で利用する。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quantity {
    pub value: f64,
    pub unit: Option<String>,             // "mm", "個", "g", "min", ...
    pub min: Option<f64>,                 // 公差/許容 下限
    pub max: Option<f64>,                 // 公差/許容 上限
    pub nominal: Option<f64>,             // 規格中心値
    #[serde(default)]
    pub scalable: bool,                   // 人数スケーリング等で連動再計算するか
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabsNode {
    pub meta: NodeMeta,   // 入れ子(flatten しない = 論点 D)
    pub tabs: Vec<Tab>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab { pub title: String, pub children: Vec<UiNode> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistNode {
    pub meta: NodeMeta,   // 入れ子(flatten しない = 論点 D)
    pub items: Vec<ChecklistItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub title: String,
    pub checked: bool,
    pub category: Option<String>,
    pub source_range: Option<SourceRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTableNode {
    pub meta: NodeMeta,   // 入れ子(flatten しない = 論点 D)
    pub columns: Vec<Column>,
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub col_type: Option<ColumnType>,     // text | number | status | link | code
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity { Info, Warning, Error }

// TimelineNode / DiagramNode / CalloutNode / RiskPanelNode / ApiExplorerNode /
// ConfigViewerNode / DependencyGraphNode / LogTimelineNode / CommitGraphNode、
// および各ドメインプリミティブ Node も同様に定義。
```

> **✅ 論点 D(決定)**: 共通メタは各ノードの **入れ子 `meta: NodeMeta`** として持つ(`#[serde(flatten)]` は使わない)。wire 形は `{"kind":"Tabs","meta":{…},"tabs":[…]}`。serde × 内部タグ enum(`tag="kind"`) × `ts-rs` の相性問題を避けつつ、全ノードがメタを一様に持つ(renderer のバッジ/ネタバレ/ジャンプ処理が一様化)。

### 4.2 semantic model(`model/`)

```rust
pub struct DocumentModel {
    pub doc_type: Classified<DocumentType>,   // 種別 + confidence
    pub blocks: Vec<ClassifiedBlock>,
    pub frontmatter: Option<serde_json::Value>,
    pub outline: Vec<OutlineEntry>,
    pub links: Vec<Link>,
    pub tasks: Vec<Task>,
}

pub enum DocumentType {
    // 開発文書
    DesignDoc, Readme, Adr, Minutes, Runbook,
    Investigation, Changelog, GitLog,
    // 非開発ドメイン(§9.2)
    Novel, ProductionOrder, Procedure, Recipe, Contract, Paper, Faq,
    Generic,
}

pub struct Classified<T> { pub value: T, pub confidence: f32, pub by: Origin }

pub struct ClassifiedBlock {
    pub block: BlockRef,          // BlockTree への参照(id)
    pub class: BlockClass,        // Overview | Architecture | Risk | OpenQuestion | Step | ...
    pub confidence: f32,
}
```

### 4.3 生成結果の保持(非永続)

論点 E により、生成 UI IR は**ディスクに永続化しない**。実行中プロセスがメモリ内にのみ保持する軽量な状態:

```rust
/// serve プロセスがファイルごとに保持する in-memory 状態(永続化しない)。
pub struct DocGenState {
    pub content_hash: u64,             // 現在の本文 hash(変更検知用)
    pub ui_ir: Vec<UiNode>,            // 生成済み UI IR(rules + LLM 後追い分)
    pub block_index: BlockIndex,       // BlockId -> ノード対応(差分再生成用)
    pub model: &'static str,           // "rules" | "claude-…"
}
```

- プロセス終了で破棄。再起動時は rules 出力から再構築し、LLM 依存分は再生成(§7)。
- `mdpeek gen` はこの状態をユーザー指定先へ**一回きり書き出す**エクスポートであって、自動管理される cache ではない(§6)。

---

## 5. Component Registry と Renderer

### 5.1 Web (Preact)

web は**全面 Preact**(論点 A)。本文ペインは `BlockTree` を描画する Preact マークダウンレンダラ、生成 UI ペインは `UiNode.kind` → コンポーネントの写像(`registry.ts`)。`DESIGN.md` の `componentRegistry` をそのまま実装。

```ts
import type { UiNode } from "./ir";           // ts-rs 生成

// 2 層 registry: 汎用コア(どの文書でも使う) + ドメインプリミティブ(特定ドメイン)。
const coreRegistry = {
  Tabs, Timeline, DataTable, Checklist, Diagram, Callout,
  RiskPanel, ApiExplorer, ConfigViewer, DependencyGraph,
  LogTimeline, CommitGraph,
};
const domainRegistry = {
  Glossary, CharacterRoster, StepNavigator,
  ToleranceMeter, ScalableTable, ObligationMatrix,
};
const registry = { ...coreRegistry, ...domainRegistry } as const;

export function Render({ node }: { node: UiNode }) {
  const C = registry[node.kind];
  if (!C) return null;                          // 未知 kind は描画しない(reject 済みのはずだが二重防御)
}
```

- **2 層 registry**: `coreRegistry`(汎用12種)＋`domainRegistry`(ドメインプリミティブ)。文書タイプが増えてもコアは不変で、ドメイン層に数個足すだけで済む(§9.3 の発見)。未知 `kind` は描画しない。
- **信頼度表示**: `confidence` が低い / `origin === "llm"` のノードには「生成・要確認」バッジを出す(`DESIGN.md` 思想 8, 判断は人間)。
- **SourceRangeLink**: 各ノードから原文へジャンプ。本文ペインも Preact 描画なので、`data-block-id` 付きの同一コンポーネントツリー内でスクロール&ハイライトを解決(全面 Preact の利点)。`sourceRange` 必須の根拠表示。
- **ライブ更新 (#16 と統合)**: `@preact/signals` で BlockTree/IR を signal 化。WS で届いた**変更ノード/ブロックだけ**差し替え、`origin`/変更フラグからハイライトアニメーション。full reload しない(素の JS 版 `main.js` の全リロードは廃止)。

想定コンポーネント一覧は `DESIGN.md` §「想定 UI コンポーネント」を registry の実装対象とする(基本 / 文書理解 / コード・設定 / 図・構造 / 表・データ / ログ・調査 / Git)。

### 5.2 TUI (ratatui)

- 同じ IR を食べ、**描画可能なノードの部分集合**を ratatui widget にマップ(Tabs→タブ、Checklist→リスト、DataTable→Table、Callout→枠、Timeline→リスト)。
- Diagram/DependencyGraph 等のグラフィカルノードは、TUI では**要約テキスト + 「web で開く」導線**にフォールバック。
- `DESIGN.md` の TUI 3 ペイン例(TOC / Content / Generated UI)をレイアウトの目標にする。

### 5.3 レイアウト(3 ペイン)

`DESIGN.md`「複数ビュー表示」の 3 ペイン(Outline / Content / Generated UI)。**3 ペインとも Preact が描画**(論点 A: 全面 Preact)。Content ペインは `BlockTree` から Preact のマークダウンレンダラで描画し、各ブロックに `data-block-id` を付けて SourceRangeLink とライブハイライトを同一ツリー内で解決する。

---

## 6. 生成物の非永続方針(論点 E)

生成 UI IR は**ディスクに一切永続化しない**。恒久キャッシュ(`.cache/*.gui.json`)・sidecar(`document.md.gui.json`)・`--emit-gui` は採用しない。

- **メモリ内のみ**: `serve` プロセスがファイルごとに `DocGenState`(§4.3)を保持。プロセス終了で破棄。
- **差分再生成(メモリ内)**: watcher (#12/#16 でチャネル化) の変更イベントで、変わった Block を含むノードだけ再生成し、残りはメモリ内の既存ノードを流用。これで #16 のライブ更新を支える(ディスクは介さない)。
- **トレードオフ**: `serve` 再起動・別プロセスでは LLM を再実行する。これを *rules-first(LLM 依存ノードだけ生成)* と *同一プロセス内メモリ再利用* で吸収する(§0.1 の E–F 整合)。
- **明示エクスポート**: `mdpeek gen <file>` はユーザー指定先(stdout / 指定ファイル)へ**一回きり書き出す**。自動管理される cache ではなく、CI/バッチ/共有用の手動出力。

---

## 7. LLM 連携設計 (`generator/llm/`)

- **trait**: `Analyzer`(分類・抽出)と `Generator`(IR 生成)を分離。どちらも rules 実装が既定、LLM 実装は `feature = "llm"`。
- **provider**: Anthropic Claude(最新の Claude モデル)を既定 adapter に。`ANTHROPIC_API_KEY` 未設定なら自動で rules-only にフォールバック(オフラインで壊れない)。
- **入出力契約**:
  - 入力: 文書モデルの必要部分 + 「この plan のこのノードを埋めよ」という指示 + **UI IR の JSON schema**。
  - 出力: **UI IR(JSON)のみ**。React/HTML/JS/任意テキストは一切受け付けない(§8, `DESIGN.md` 思想 3・9)。
  - Claude の **tool use / structured output** で schema を強制し、パース失敗はリトライ→最終的に rules フォールバック。
- **sourceRange 強制**: 生成ノードには必ず対象 Block の range を持たせるようプロンプト設計し、validate で実レンジと突合(捏造は reject)。
- **コスト制御**: rules で埋まる分は投げない / メモリ内に既存ノードがあれば投げない / 変更 Block を含む差分ノードだけ投げる(永続キャッシュは持たない=論点 E)。

> **✅ 論点 F(決定)**: **両方**。主経路は **server 内 async**(rules-first で即描画 → LLM 依存ノードをプログレッシブに後追いし WS で push、#16 と統合)。加えて **`mdpeek gen <file>`** を一回きりの明示エクスポート(stdout/指定ファイル)として提供(CI/バッチ/事前生成用)。`ANTHROPIC_API_KEY` 未設定や `--llm` 無効時は rules-only に degrade。論点 E に従い、いずれも恒久キャッシュは作らない。

---

## 8. セキュリティ設計

`DESIGN.md`「セキュリティ設計」を実装制約として明文化:

- LLM 出力は **UI IR のみ**。任意 JS/HTML を生成させない。→ serde 型 + registry allowlist で構造的に不可能にする。
- renderer は **固定 registry からのみ**選択。未知 `kind` は reject。
- **bash/コードブロックは自動実行しない**。`CommandSafetyPanel` は preview + 危険度表示のみ(`security/command.rs` が `rm -rf` / `curl | sh` 等を検出)。
- **外部リンク・remote image** は policy 管理(既定は展開せず明示許可)。
- **Mermaid / 埋め込み HTML は sandbox**(iframe sandbox or DOMPurify 相当)。
- Preact 採用で attack surface が増えるため、CSP を維持し、IR→DOM 生成は `dangerouslySetInnerHTML` を使わず**テキストノードとして描画**する(コードは `<pre>` にエスケープ挿入)。
- confidence が低い生成結果は UI 上で明示。

---

## 9. 文書タイプ別ハンドラ(全タイプ実装)

各 `planner/doctype/*.rs` の初期スコープ。生成 UI は常に**「その文書を読む目的(job-to-be-done)」から逆算**する。読むときの苦痛を特定し、それを解く操作可能な affordance を出す。散文系(小説・契約)は rules が効きにくく LLM 依存が高い/構造的な文書(生産指示書・手順書)は rules で大半を抽出できる、という軸も設計判断に効く。

### 9.1 開発文書

`DESIGN.md`「想定する文書タイプと生成 UI」に対応。

| タイプ | rules で出せる UI | LLM が要る UI |
|--------|------------------|--------------|
| 技術設計書 | outline / section map / task→checklist / mermaid 図 | architecture diagram 生成 / risk・open question 抽出 / missing section 検出 / review checklist |
| README | install 抽出 / command palette / config viewer / links | usage・config・troubleshooting の意味分離 / unsafe command 判定補助 |
| ADR | Context/Decision/Alternatives/Consequences 分割 / timeline | decision graph / superseded 関係推定 / impact map |
| 議事録 | task 抽出 / 発言者抽出(パターン) | 決定事項/論点/次回確認の意味分類 |
| 作業手順書 | step-by-step / checklist / env var 抽出 / command copy | dangerous op 判定 / rollback 手順抽出 |
| ログ調査メモ | log severity grouping / error focus | error cluster / hypotheses / next check points |
| Changelog | version timeline / セクション分類 | breaking changes 抽出 / migration guide 生成 |
| Git log | commit/branch view / refactor・feat・fix 分類(rules 一次) | 意図別グルーピング / 関連コミットクラスタ |

### 9.2 非開発ドメイン

「普通の Markdown を目的別 UI に」という思想は開発文書に限らない。読む苦痛 → 生成 UI で整理する。

**小説・物語**(散文中心・LLM 依存が高い / markdown 構造は乏しい)

| 読む苦痛 | 生成 UI | 使うコンポーネント |
|---|---|---|
| 人物を見失う | 登場人物パネル(名前抽出＋初出ジャンプ＋一言要約) | `CharacterRoster` |
| 関係が複雑 | 相関図(共起・明示関係からエッジ) | `DependencyGraph` |
| 時系列が混乱(回想・並行) | 物語タイムライン(章/場面＋時間標識、回想検出) | `Timeline` |
| 視点が切替わる | POV トラッカー(場面ごとの視点人物) | `Timeline`/`DataTable` |
| 伏線が気になる | 未回収の問いパネル(※断定せず候補・要確認、判断は読者) | `OpenQuestionsPanel` |
| 独自世界観 | 用語集(造語＋初出定義) | `Glossary` |

> 小説固有の課題: **ネタバレ境界**。「既読位置より先を要約・生成に出さない」制御が要る(§9.3 reading-position aware)。開発文書には無い新要件。

**生産指示書・製造指示書**(構造的・rules がよく効く)

| 読む苦痛 | 生成 UI | 使うコンポーネント |
|---|---|---|
| 指示要点が散在 | 指示サマリカード(品番/品名/数量/納期/ライン/ロット) | `ConfigViewer` |
| 部材が多い | BOM/部材テーブル(品目・数量・仕様、不足ハイライト、sort/filter) | `DataTable` |
| 工程順・設備割当 | 工程フロー(順序＋設備＋標準時間＋担当、流れ図) | `SequenceView`/`DependencyGraph` |
| 品質基準 | 検査チェックリスト(検査項目・規格値・判定基準) | `Checklist` |
| 公差が数値の羅列 | 公差メーター(上下限を視覚バー、規格中心からの位置) | `ToleranceMeter`(+`Quantity`) |
| 安全 | 安全/注意 callout(保護具・危険工程の警告表示のみ) | `Callout` |
| 前後工程・ロット追跡 | トレーサビリティリンク(前工程/後工程/図番/図面参照) | `SourceRangeLink`/`DependencyGraph` |

**手順書・SOP**(IT 運用に限らず / レシピを身近な例に)

| 読む苦痛 | 生成 UI | 使うコンポーネント |
|---|---|---|
| どこまでやった | ステップナビ(1 ステップずつ＋進捗＋所要時間) | `StepNavigator` |
| 準備不足 | 必要物一覧(工具/材料/前提条件を冒頭に) | `DataTable`/`Checklist` |
| 「もし〜なら」の分岐 | 分岐フロー/決定木 | `Diagram` |
| 危険操作 | 注意/禁止 callout | `Callout` |
| 失敗した | ロールバック手順を隣接表示 | `StepNavigator` |
| (レシピ)人数で分量が変わる | 材料の人数スケーリング(分量が連動再計算) | `ScalableTable`(+`Quantity`) |
| (レシピ)並行作業 | 工程タイムライン(並列レーン)＋タイマー候補 | `Timeline` |

**その他(同じ枠組みで乗る)**

- 契約書・規程: 定義語＋用語集(`Glossary`)、条項アウトライン(`Outline`)、当事者×義務/権利(`ObligationMatrix`)、期限・金額抽出(`Quantity`)、参照条項ジャンプ(`SourceRangeLink`)、曖昧条項フラグ(`RiskPanel`)、改定 diff。
- 論文・技術記事: 要約、図表インデックス、引用・参考文献リンク、主張→根拠マップ、数式索引。
- FAQ/ナレッジ: Q&A アコーディオン＋検索＋カテゴリ＋関連リンク(`Tabs`/`Search`)。
- 旅行しおり/イベント進行表: タイムライン、場所、持ち物チェックリスト、連絡先カード。

### 9.3 横断要件(文書タイプ非依存)

上記の棚卸しから、文書タイプを増やしても registry が爆発しないこと、および共通で必要な仕組みが 3 点見えた。

1. **2 層 registry**(§5.1 反映済み): 生成 UI の大半は汎用コア12種(`Timeline`/`DataTable`/`Checklist`/`Callout`/`DependencyGraph`/`Diagram` 等)の**再構成**で表現できる。ドメイン固有で新規に要るのは数個だけ — `CharacterRoster` / `ToleranceMeter` / `StepNavigator` / `ScalableTable` / `ObligationMatrix` / `Glossary`。
2. **reading-position aware(ネタバレ制御)** — 小説発。「既読位置より先の内容を要約・生成 UI に含めない」。`NodeMeta.visibility: Visibility::UntilRead { reveal_after_line }`(§4.1)で表現し、renderer は現在の既読位置に応じて描画を抑止する。
3. **数値の operable 化** — 生産・レシピ・健康記録発。「数値＋単位＋制約(公差/許容/中心/スケーラブル)」を専用の `Quantity` 型(§4.1)で扱い、公差メーター・材料スケーリング・チャートが共通利用する。「読む」を「使える」に変える affordance の核。

---

## 10. 開発ロードマップ

`DESIGN.md` の 5 レイヤーに沿い、既存 issue #12–#16 を Layer 1 の一部として組み込む。

### Layer 1 — 強い Markdown viewer(基盤・一部既存)
- 既存: GFM / highlight / mermaid / math / footnotes / task list / table / theme。
- **#12** term ライブ更新 TUI / **#13** TOC トグル / **#14** repo+worktree エクスプローラ / **#15** diff / **#16** ライブ更新差分ハイライト。
- 追加(本設計の前提整備): **parser を `into_offset_iter` 化して SourceRange 取得**(全レイヤーの土台)。fuzzy search / backlinks / outline パネル。
- **workspace 化を今すぐ実施(論点 B)**: `mdpeek-core`(lib)を抜き出し、バイナリ `mdpeek` を依存させる。`release.yml` を追従。
- **成果物**: `mdpeek-core` workspace、`mdpeek-core::parser::BlockTree`。

### Layer 2 — Semantic viewer(rules 中心)
- `analyzer`(doctype/block_class/code/table/tasks)+ `model` を rules で実装。
- サイドパネルに outline / TODO / risk(rules 版) / open questions を表示(まだ UI IR 化しなくてよい)。
- **成果物**: `DocumentModel`、doctype 推定(rules)、`SourceRangeLink`。
- **新 issue 候補**: 「SourceRange 対応 parser」「DocumentModel と rules analyzer」「semantic サイドパネル」。

### Layer 3 — Generated UI(IR + renderer + LLM)
- `ir` 型定義 + validate + registry allowlist。
- `web/` Preact 導入、component registry、3 ペインレイアウト。
- `RulesGenerator` → 続いて `ClaudeGenerator`(`feature = "llm"`, server 内 async + `mdpeek gen`=論点 F)。
- 生成物は非永続(論点 E)。#16 差分再生成は **メモリ内 `DocGenState`** で行い、ディスクキャッシュは作らない。
- 文書タイプは §9 の rules 列から着手 → 技術設計書 / README を最初の縦に、順次全タイプ。
- コア12種 registry を先に固め、ドメインプリミティブ(§9.3-1)と横断要件(reading-position / `Quantity`)は各ドメイン着手時に追加。
- **成果物**: 動く Generative UI(まず技術設計書・README、その後 ADR/議事録/手順書/ログ/changelog/gitlog)。

### Layer 3.5 — 非開発ドメイン(§9.2)
- コア registry が固まった後、ドメインプリミティブ(`CharacterRoster`/`ToleranceMeter`/`StepNavigator`/`ScalableTable`/`ObligationMatrix`/`Glossary`)を追加。
- 生産指示書・手順書(構造的で rules が効く)を先行、小説・契約(散文で LLM 依存)を後続。
- ネタバレ制御(`Visibility::UntilRead`)と `Quantity` operable UI をこの段で実装。

### Layer 4 — Repository-aware viewer
- #14 の worktree スキャン基盤の上に、README↔実ファイル対応 / docs-code 整合 / Cargo.toml・package.json 参照 / ADR↔git history / TODO↔issue。

### Layer 5 — Editor/TUI 統合
- TUI renderer(#12 の上に IR 対応)、Neovim plugin、GitHub preview 連携。

### 依存関係(クリティカルパス)
```
SourceRange parser ──┬─▶ Layer2 analyzer ──▶ Layer3 IR/generator ──▶ Layer3 renderer
                     └─▶ #16 差分 (BlockId 安定化)
#14 worktree ────────────────────────────────────────────────────▶ Layer4
```

---

## 11. 次アクション

論点 A–F は決定済み(§0.1)。残作業:

1. **issue の整合**: #20(workspace 化)を Layer 1 へ移動(論点 B: 今すぐ)。#26 のスコープから恒久キャッシュを外し「RulesGenerator + メモリ内差分再生成」に修正(論点 E)。#25 に「本文ペインも Preact 描画(全面 Preact)」「素の `main.js` 撤去」を追記(論点 A)。#27 に `mdpeek gen` を追記(論点 F)。
2. **UI IR 第一版**: 技術設計書・README の 2 タイプに絞って `UiNode`/`NodeMeta`(入れ子 meta=論点 D)を確定 → `ts-rs` 生成の PoC。
3. **全面 Preact の PoC**: `BlockTree` → Preact マークダウンレンダラ(`data-block-id` 付き)+ WS 差分更新の骨組み。
4. **CI**: `web/dist` の鮮度チェックジョブ(論点 C)。

> 決定は §0.1 に記録。以降の設計変更もここに追記していく。
