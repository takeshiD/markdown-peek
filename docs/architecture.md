# Generative UI Markdown Viewer — アーキテクチャ設計書 (v0 draft)

> このドキュメントは [`DESIGN.md`](../DESIGN.md) の構想を、現行の Rust 実装 (`pulldown-cmark` + `axum` + TUI) の上に**実装可能な形**へ落とし込んだ設計書のたたき台です。`DESIGN.md` = *what / why*、本書 = *how* という役割分担です。
>
> 一緒にブラッシュアップする前提の v0 です。未確定点は各所の **⚠ 論点** で明示しています。

## 0. 確定した基本方針

先の議論で確定した3つの分岐:

| 論点 | 決定 |
|------|------|
| Renderer 戦略 | **コンポーネント FW を埋め込み**。Preact + `@preact/signals` を採用し、IR 駆動の component registry でクライアント描画。バンドルは単一バイナリに embed。 |
| LLM 連携 | **trait 化 + Claude 既定 + rules 優先**。`RulesGenerator`(オフライン既定) + `ClaudeGenerator`(`feature = "llm"`)。 |
| 対象文書タイプ | **全部**(技術設計書 / README / ADR / 議事録 / 作業手順書 / ログ調査メモ / changelog / git log)。ただしロードマップで着手順を規定。 |

`DESIGN.md` の「重要な設計思想」10項目は不変の制約として本書全体に効かせる(特に *Markdown 本文は変更しない* / *LLM は UI IR だけを生成* / *renderer は決定論的* / *全 UI は sourceRange に紐づく* / *任意コード実行禁止*)。

---

## 1. 全体アーキテクチャ

核心は **「重い処理・判断は Rust core に集約し、UI IR を web/TUI 共通の契約(wire format)にする」**こと。フロントエンドはこの IR を決定論的に描画するだけの薄い層に保つ。

```
        ┌──────────────────────── Rust core (mdpeek-core) ───────────────────────┐
        │                                                                        │
document.md ─▶ parser ─▶ model ─▶ analyzer ─▶ planner ─▶ generator ─▶ ir(validate) ─▶ cache
        │      (pulldown  (block   (rules +   (semantic   (rules/     (schema +      (hash)   │
        │       -cmark     tree +   LLM)       model →     LLM →       sourceRange            │
        │       OffsetIter offsets)            UI plan)    UI IR)      検証)                   │
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
- **キャッシュ・差分再生成**を core に閉じ込められる(#16 のライブ更新と統合)。
- TS 型は Rust 型から**自動生成**(`ts-rs`)し、二重定義を防ぐ。`DESIGN.md` の TS 型定義は「wire format の仕様」として採用するが、正本は Rust。

### 1.2 現行コードとの接続

| 現行 | 本設計での位置づけ |
|------|------------------|
| `src/gfm.rs` (`parser_options`, `transform`) | `mdpeek-core::parser` にそのまま移設。Layer 1 として維持。 |
| `src/emitter/html.rs` | Layer 1(素の HTML)経路として存続。Generative UI は**別レイヤー**として上に重ねる(共存)。 |
| `src/emitter/term.rs` | 同上(TUI Layer 1 描画)。 |
| `src/server.rs` (`file_path`/`theme` が `Arc<RwLock>`) | IR / ファイル選択 API を足す土台として再利用。 |
| `src/watcher.rs` (単一パス blocking) | チャネル化しキャッシュ無効化トリガに接続(#12/#16 と共通)。 |
| `static/js/main.js` (素の JS) | Layer 1 の enhancer として存続。Generated UI ペインだけ Preact island に置換。 |

> **⚠ 論点 A**: 既存の `HtmlEmitter` 経路(Layer 1)と Preact 経路(Layer 3)を **同一ページで共存**させるか、`--generative` で切替えるか。推奨は「Content ペイン=既存 SSR HTML、右の Generated UI ペイン=Preact island」の**共存**。

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
│   │   │   │       └── changelog.rs
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
│   │   │   ├── cache/              # 生成 UI キャッシュ
│   │   │   │   ├── mod.rs
│   │   │   │   ├── key.rs          # content hash + model/version
│   │   │   │   └── store.rs        # .cache/<hash>.gui.json
│   │   │   └── security/           # command safety, link/image policy
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
│   │   ├── ir.ts                   # ← ts-rs 生成(gitignore or commit)
│   │   ├── registry.ts             # componentRegistry
│   │   ├── layout/                 # SplitPane, Outline, Tabs...
│   │   ├── components/             # UiNode 描画コンポーネント群(§5)
│   │   └── live/                   # WS 差分更新 + 変更ハイライト(#16)
│   └── dist/                       # ビルド成果物(server が include_bytes! で埋め込む)
├── static/                         # Layer 1 の既存アセット(css 等)
└── docs/
    └── architecture.md             # 本書
```

> **⚠ 論点 B**: workspace 化は現在のリリース CI (`release.yml`) とバイナリ名 `mdpeek` に影響する。crate 分割は Layer 2 着手時にまとめて行い、Layer 1 (#12–#16) は現構成のまま進める、という段取りを推奨。
>
> **⚠ 論点 C**: `web/dist` を **リポジトリにコミット**して `include_bytes!` するか、ビルド時に生成するか。単一バイナリ配布・`cargo install` 一発を守るなら、`build.rs` で `npm build` を叩くのは重い。**dist をコミット**(生成物 in-tree)が現実的。

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

### 3.5 validate — schema + sourceRange 検証

- serde でのデシリアライズ(型が合わない IR は reject)。
- `registry.rs` の allowlist に無い `kind` は reject(`DESIGN.md`: 未知 component は reject)。
- **sourceRange 検証**: すべての `sourceRange` が実ドキュメントの範囲内で、かつ対応 Block と矛盾しないこと。捏造レンジは reject(hallucination 検出)。
- `confidence` が閾値未満のノードは `low_confidence` フラグ付きで通す(renderer が明示表示)。

### 3.6 cache — 生成 UI キャッシュ

§6。

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
}

/// 全ノード共通のメタ(sourceRange + 信頼度 + 出所)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMeta {
    pub source_range: Option<SourceRange>,
    pub confidence: Option<f32>,          // 0.0–1.0, LLM 生成時
    #[serde(default)]
    pub origin: Origin,                   // Rules | Llm { model }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    #[default]
    Rules,
    Llm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabsNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub tabs: Vec<Tab>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab { pub title: String, pub children: Vec<UiNode> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
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
    #[serde(flatten)]
    pub meta: NodeMeta,
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
// ConfigViewerNode / DependencyGraphNode / LogTimelineNode / CommitGraphNode も同様に定義。
```

> **⚠ 論点 D**: `NodeMeta` を全ノードに `#[serde(flatten)]` で持たせるか、`DESIGN.md` のように各ノードが個別に `sourceRange` を持つか。共通メタ(confidence/origin を必ず載せたい)を考えると flatten 推奨。TS 生成との相性は要検証。

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
    DesignDoc, Readme, Adr, Minutes, Runbook,
    Investigation, Changelog, GitLog, Generic,
}

pub struct Classified<T> { pub value: T, pub confidence: f32, pub by: Origin }

pub struct ClassifiedBlock {
    pub block: BlockRef,          // BlockTree への参照(id)
    pub class: BlockClass,        // Overview | Architecture | Risk | OpenQuestion | Step | ...
    pub confidence: f32,
}
```

### 4.3 キャッシュエントリ(`cache/`)

`DESIGN.md`「キャッシュすべき内容」をそのまま:

```rust
#[derive(Serialize, Deserialize)]
pub struct GuiCacheEntry {
    pub document_type: DocumentType,
    pub block_classification: Vec<ClassifiedBlockLite>,
    pub ui_ir: Vec<UiNode>,
    pub source_ranges: Vec<SourceRange>,   // 検証済みレンジ一覧
    pub confidence: f32,                   // 全体信頼度
    pub model: String,                     // "rules" | "claude-…"
    pub generated_at: String,              // RFC3339
    pub content_hash: String,              // 入力の hash(= ファイル名の一部)
}
```

---

## 5. Component Registry と Renderer

### 5.1 Web (Preact)

`web/src/registry.ts` が `UiNode.kind` → Preact コンポーネントの写像を持つ。`DESIGN.md` の `componentRegistry` をそのまま実装。

```ts
import type { UiNode } from "./ir";           // ts-rs 生成
const registry = {
  Tabs, Timeline, DataTable, Checklist, Diagram, Callout,
  RiskPanel, ApiExplorer, ConfigViewer, DependencyGraph,
  LogTimeline, CommitGraph,
} as const;

export function Render({ node }: { node: UiNode }) {
  const C = registry[node.kind];
  if (!C) return null;                          // 未知 kind は描画しない(reject 済みのはずだが二重防御)
}
```

- **信頼度表示**: `confidence` が低い / `origin === "llm"` のノードには「生成・要確認」バッジを出す(`DESIGN.md` 思想 8, 判断は人間)。
- **SourceRangeLink**: 各ノードから原文へジャンプ(Content ペインの該当行へスクロール&ハイライト)。`sourceRange` 必須の根拠表示。
- **ライブ更新 (#16 と統合)**: `@preact/signals` で IR を signal 化。WS で届いた**変更ノードだけ**差し替え、`origin`/変更フラグからハイライトアニメーション。full reload しない。

想定コンポーネント一覧は `DESIGN.md` §「想定 UI コンポーネント」を registry の実装対象とする(基本 / 文書理解 / コード・設定 / 図・構造 / 表・データ / ログ・調査 / Git)。

### 5.2 TUI (ratatui)

- 同じ IR を食べ、**描画可能なノードの部分集合**を ratatui widget にマップ(Tabs→タブ、Checklist→リスト、DataTable→Table、Callout→枠、Timeline→リスト)。
- Diagram/DependencyGraph 等のグラフィカルノードは、TUI では**要約テキスト + 「web で開く」導線**にフォールバック。
- `DESIGN.md` の TUI 3 ペイン例(TOC / Content / Generated UI)をレイアウトの目標にする。

### 5.3 レイアウト(3 ペイン)

`DESIGN.md`「複数ビュー表示」の 3 ペイン(Outline / Content / Generated UI)。Content は Layer 1 の既存 HTML、Generated UI が Preact island(論点 A の共存案)。

---

## 6. キャッシュ設計

```
.cache/mdpeek/<content-hash>.gui.json     # 既定(リポジトリ内 .gitignore 推奨)
```

- **鍵** = `hash(正規化 Markdown 本文) + generator 種別 + prompt/schema version`。本文が変わればミス、generator や schema を上げてもミス。
- **無効化**: watcher (#12/#16 でチャネル化) の変更イベントで該当ハッシュを破棄 → 再生成。
- **差分再生成**: `sourceRange` 単位で「変わった Block を含むノードだけ」再生成し、他はキャッシュ流用(LLM コスト削減 + #16 のライブ更新に直結)。
- **sidecar 形式** (`document.md.gui.json`) も選べるが、既定は `.cache/` 集約。

> **⚠ 論点 E**: LLM 生成物をリポジトリにコミット可能にする(レビュー・再現)か、常にローカルキャッシュに留めるか。既定は `.gitignore`、`--emit-gui document.md.gui.json` で明示エクスポート、が無難。

---

## 7. LLM 連携設計 (`generator/llm/`)

- **trait**: `Analyzer`(分類・抽出)と `Generator`(IR 生成)を分離。どちらも rules 実装が既定、LLM 実装は `feature = "llm"`。
- **provider**: Anthropic Claude(最新の Claude モデル)を既定 adapter に。`ANTHROPIC_API_KEY` 未設定なら自動で rules-only にフォールバック(オフラインで壊れない)。
- **入出力契約**:
  - 入力: 文書モデルの必要部分 + 「この plan のこのノードを埋めよ」という指示 + **UI IR の JSON schema**。
  - 出力: **UI IR(JSON)のみ**。React/HTML/JS/任意テキストは一切受け付けない(§8, `DESIGN.md` 思想 3・9)。
  - Claude の **tool use / structured output** で schema を強制し、パース失敗はリトライ→最終的に rules フォールバック。
- **sourceRange 強制**: 生成ノードには必ず対象 Block の range を持たせるようプロンプト設計し、validate で実レンジと突合(捏造は reject)。
- **コスト制御**: rules で埋まる分は投げない / キャッシュ命中は投げない / 差分ノードだけ投げる。

> **⚠ 論点 F**: LLM 呼び出しは server プロセス内(Rust)から直接か、`mdpeek` とは別の生成ステップ(`mdpeek gen document.md`)に分けるか。ライブ用途を考えると server 内 async 呼び出し(tokio)推奨だが、レイテンシとキー管理の観点で CLI 事前生成も併用したい。

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

各 `planner/doctype/*.rs` の初期スコープ。`DESIGN.md`「想定する文書タイプと生成 UI」に対応。

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

---

## 10. 開発ロードマップ

`DESIGN.md` の 5 レイヤーに沿い、既存 issue #12–#16 を Layer 1 の一部として組み込む。

### Layer 1 — 強い Markdown viewer(基盤・一部既存)
- 既存: GFM / highlight / mermaid / math / footnotes / task list / table / theme。
- **#12** term ライブ更新 TUI / **#13** TOC トグル / **#14** repo+worktree エクスプローラ / **#15** diff / **#16** ライブ更新差分ハイライト。
- 追加(本設計の前提整備): **parser を `into_offset_iter` 化して SourceRange 取得**(全レイヤーの土台)。fuzzy search / backlinks / outline パネル。
- **成果物**: workspace 化(論点 B に従い Layer 2 直前でも可)、`mdpeek-core::parser::BlockTree`。

### Layer 2 — Semantic viewer(rules 中心)
- `analyzer`(doctype/block_class/code/table/tasks)+ `model` を rules で実装。
- サイドパネルに outline / TODO / risk(rules 版) / open questions を表示(まだ UI IR 化しなくてよい)。
- **成果物**: `DocumentModel`、doctype 推定(rules)、`SourceRangeLink`。
- **新 issue 候補**: 「SourceRange 対応 parser」「DocumentModel と rules analyzer」「semantic サイドパネル」。

### Layer 3 — Generated UI(IR + renderer + LLM)
- `ir` 型定義 + validate + registry allowlist。
- `web/` Preact 導入、component registry、3 ペインレイアウト。
- `RulesGenerator` → 続いて `ClaudeGenerator`(`feature = "llm"`)。
- `cache` 実装 + #16 差分再生成と統合。
- 文書タイプは §9 の rules 列から着手 → 技術設計書 / README を最初の縦に、順次全タイプ。
- **成果物**: 動く Generative UI(まず技術設計書・README、その後 ADR/議事録/手順書/ログ/changelog/gitlog)。

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

## 11. 次アクション(このドラフトの磨き込み)

1. **論点 A–F** の合意(特に A: Layer1/Layer3 共存方式、C: dist コミット、F: LLM 呼び出し位置)。
2. Layer 2 の新 issue 3 本(SourceRange parser / DocumentModel+rules analyzer / semantic サイドパネル)を切るか。
3. UI IR の第一版スキーマを技術設計書・README の 2 タイプに絞って確定 → `ts-rs` 生成の PoC。
4. workspace 化のタイミング(論点 B)。

> フィードバック歓迎。合意した論点は本書に反映し、Layer 2 の issue に落とします。
