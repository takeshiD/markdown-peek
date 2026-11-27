# Jev Adaptive UI — 設計メモ (v1, 2026-11-27)

> 対象ブランチ: `feat/jev-adaptive-ui`(起点 = `origin/worktree-layer3-generative-ui` @ `308e537`)。
> 上位設計: [`AGENTS.md`](../../AGENTS.md)(論点 A–L)、Layer 3 実装メモ: [`docs/layer3.md`](../layer3.md)。
> 本書 = *how*(Jev による UI 選択 + Tailwind/shadcn 化した Web UI)。実装は kenshu-loop の発注 → 納品 → 検収で進め、結果を §11 に記録する。

## 0. 前提と一次資料(確認日 2026-11-27)

| 事実 | 出典 |
|---|---|
| Jev は TypeSafe AI の "System One" モデル。state + 型付き質問 → 構造化回答。**テキスト/コード生成をしない** | https://docs.typesafe.ai/introduction , https://docs.typesafe.ai/introduction/coding-agents.md |
| `POST https://api.typesafe.ai/v1/systemone`、`GET /v1/models`。`Authorization: Bearer <key>` | https://docs.typesafe.ai/api/system-one.md , OpenAPI https://api.typesafe.ai/openapi.json |
| 質問型: `choice`(1–255 options, `criteria` = option→説明)、`score`(1–10 段階, 0 起点)、`noul`(yes/no 確率)、`box`(画像) | https://docs.typesafe.ai/primitives.md |
| **none/abstain 型は無い**。「該当なし」は同一リクエスト内の companion `noul` で判定し、yes の時だけ choice を採用するのが推奨 | https://docs.typesafe.ai/patterns/companion-noul.md |
| 応答: `answers.<name>` に `choice`+`confidence`+`probabilities` / `score`+`confidence`+`legend` / `noul`(確率)。reasoning フィールドは無い。`usage` あり | system-one.md, quickstart |
| エラー: 400(`max_tokens_exceeded`) / 401 / 413(>32MiB) / 415 / 422 / 429 / 529。429/529 は指数バックオフ + `retry-after` | https://docs.typesafe.ai/api/errors.md , https://docs.typesafe.ai/models.md |
| コンテキスト 64k tok/req, 32k tok/question。SDK 既定 timeout 10s、retry 2 回 | models.md, SDK RetryPolicy |
| SDK は `TYPESAFE_API_KEY` / `TYPESAFE_BASE_URL` / `TYPESAFE_DEFAULT_MODEL`(既定 `jev-latest`)を読む。**Rust SDK は無い**(HTTP 直叩きが公式に案内されている) | https://docs.typesafe.ai/sdk/python/api/constants.md , https://docs.typesafe.ai/sdk.md |
| モデル: `jev-1.14.0`(= `jev-latest`/`jev-preview` 現在)、`jev-1.13.0`。$0.042 / 1M input tok、output 無料。顧客データで学習しない | https://docs.typesafe.ai/models.md |
| CopilotKit cookbook: `choice` で panel 種別、候補ごと `score` で fit 順位。カタログ・ID・ラベル・Zod 検証はアプリ側(決定的) | https://docs.copilotkit.ai/cookbook/jev-generative-ui |
| json-render × Jev(jevforagents.com)は第三者。統合コードは非公開 | https://jevforagents.com/builds/json-render-jev-generative-ui |

**実 API 疎通(2026-11-27, 本ブランチ作業者が実施)**: 無害な fixture(README の Installation 節 + 3 候補)で `GET /v1/models` = 200、`POST /v1/systemone` = 200 / 0.14s、`model: "jev-1.14.0"`、choice/score/noul の応答形が上表と一致。このとき companion `noul`=0.15(「plain より役立つ widget は無い」寄り)なのに choice は `confidence 1.0` で 1 候補を選んだ。**choice の confidence だけでは「出さない」を判断できない**ことを実測で確認した → companion noul を必須にする(§3.3)。

未確認: API key 形式、無料枠、非 enterprise のデータ保持期間(DPA 未読)、サーバ側 timeout、組織別の rate limit 数値。

## 1. 目的 / 非目的

目的
- 文書・セクションごとに「どの読書補助 widget を出すか/出さないか」を **Jev の型付き選択**で決め、内容は決定的抽出で作る。
- Web UI を Tailwind CSS + shadcn/ui(React 19、§6.1)で作り直し、落ち着いた高密度 UI を提供する(当初 Read / Split / Adaptive の 3 モード。§11.7 で Read / Adaptive の 2 モード + 本文内表示に変更)。
- オフライン・単一バイナリ・既存機能(diff, watch, explorer, GFM, scrolly, TUI)を壊さない。

非目的
- Jev に HTML/JS/CSS クラス/色/自由配置/文書にない事実を生成させること(構造的に不可能にする)。
- json-render の導入(既存の型付き IR + registry で足りる。§9)。
- 生成物のディスク永続化(論点 E)。**新しい選択パイプラインはメモリ内キャッシュのみ**。既存 `/api/gui` の `.cache/mdpeek` は論点 E と不整合だが、互換のため今回は触らない(後続 issue)。

## 2. 現状(調査結果)

- `main` には Layer 1/2(parser / analyzer / viewer)。UI IR・generator・Preact island は本ブランチ系(`worktree-layer3-generative-ui`)にのみ存在。
- Web は **SSR テンプレート + 素の `static/js/main.js`(1300 行)** が本体ビュー(テーマ、TOC + fuzzy filter + scrollspy、front matter、#16 ブロック差分パッチ + flash + autoscroll、mermaid/hljs/MathJax、worktree explorer + breadcrumb、#15 diff)を担い、Preact は Generated UI パネルと scrolly パネルの island だけ。論点 A(全面 Preact)は未達。
- `web/` = Preact 10 + @preact/signals + Vite 5 + TS、CSS は `styles.css`/`panel.css`/`scrolly.css` に GitHub 風 tokens が重複。バンドル(ベースライン): `mdpeek-gui.js` 30.49 kB(gzip 10.52 kB)、`mdpeek-gui.css` 10.24 kB(gzip 2.45 kB)。
- HTML emitter はブロックに `data-block-id` / ソース行を出していない → ソースジャンプ/同期の土台が無い。
- 既存 LLM backend(`claude_code` / `codex` / `anthropic_api`)は lens 生成と scrolly に使用。**互換維持**。
- ベースライン: `cargo test --workspace` 全 pass(48s)、`npm run build` 成功、dist は再現ビルドで差分なし。

## 3. 選択パイプライン

```
markdown ──▶ mdpeek-parser BlockTree (stable BlockId, SourceRange)
         └─▶ mdpeek-analyzer DocumentModel (doctype, BlockClass, tasks, tables, code intent)
                     │
                     ▼
        ① Section split + feature extraction (決定的)
                     ▼
        ② Candidate registry + eligibility prefilter (決定的)
           CandidateSpec{kind, variant, scope, required data, tui_fallback, safety}
                     ▼                 ┌──────────────── cache hit? (section hash × registry/policy/model ver)
        ③ Selector (trait) ───────────┤ Jev  : 1 request = 1 document state, 質問を一括(choice + companion noul + score)
                     │                 │ Rules: 決定的スコア(オフライン既定 / fallback)
                     │                 └ Fake : テスト・E2E 用の台本化 selector
                     ▼
        ④ Composer (決定的) — threshold / 最大数 / 位置 / 「出さない」 / 重複排除
                     ▼
        ⑤ validate_nodes(allowlist + sourceRange 検証) ─▶ AdaptiveLayout (versioned wire format)
                     ▼
            Web (React registry) / TUI(要約 fallback)
```

### 3.1 Section split
- 見出し(H1–H3)でセクション化。各セクション: `section_id`(先頭見出しの `BlockId`)、`range`、`hash`(本文の content hash)、`features`(code block 数と言語/intent、table 行列数・status 列、ordered list 長、task 数、callout 種別、definition 風パターン数、日時/バージョン行の数、文字数)、`class`(analyzer の BlockClass)。
- Jev に渡す state は **見出し + 先頭 N 文字(既定 600)+ features** のみ。全文は送らない。文書全体でも上限(既定 24k 文字)を超えたら後方セクションの本文を切り、features のみにする。

### 3.2 Candidate registry(widget カタログ)

| widget (`kind`/variant) | scope | 適用条件(prefilter, 決定的) | データ provenance | TUI fallback |
|---|---|---|---|---|
| `StepNavigator` | section | ordered list ≥3 項目 or Step class | list item テキスト + range | 番号付きリスト |
| `Checklist` | section | task item ≥2 | task + checked + range | `[ ]` リスト |
| `DataTable`/`sortable` | section | GFM table ≥3 行 | セル値(原文) | 表 |
| `DataTable`/`status` | section | table に status 列 | 同上 + status 列判定 | 表 |
| `Chart` (bar / line) | section | table ≥3 行に「単位が揃った数値列」≥1 + ラベル列(行番号・ID・年だけの列は数値扱いしない) | セルを parse した数値 + セル原文(tooltip) | 表 |
| `ConfigViewer`/`commands` | section | shell/command code ≥2 | code 本文(実行しない) | コードそのまま |
| `ConfigViewer`/`config` | section | json/yaml/toml code | parse 結果 | コード |
| `Timeline` | section | 日付/バージョン見出し or 行 ≥3 | 日付 + 行 + range | リスト |
| `Callout`/`risk` | section | Risk class or WARNING/CAUTION alert | 原文の文 | 枠付きテキスト |
| `SemanticOutline` | document | 見出し ≥4 | outline + class | TOC |
| `RiskPanel` | document | risk 文 ≥1 | planner 抽出 | リスト |
| `OpenQuestions` | document | `?` で終わる文 / Open Questions 節 | planner 抽出 | リスト |
| `ActionItems` | document | task / TODO ≥1 | planner 抽出 | リスト |
| `DecisionLog` | document | Decision class(ADR/議事録) | planner 抽出 | リスト |
| `Glossary` | document | 定義パターン ≥3 | planner 抽出 | 定義リスト |

- すべての内容は決定的抽出で、各ノードは `source_range` を持つ。Jev は **どれを出すか(と doc 種別/優先度)だけ**を選ぶ。
- registry に `REGISTRY_VERSION` を持ち、cache key に含める。新 widget の追加はこの表 + web registry の 2 か所のみ。

### 3.3 Jev 質問パック(1 リクエスト / 文書)

```jsonc
{
  "model": "jev-1.14.0",                      // 既定は版固定(閾値チューニングの再現性)。config で変更可
  "state": {
    "document": { "filename_hint": "README.md", "headings": ["Install", "Usage", "Config"], "rules_doc_type": "readme" },
    "sections": [
      { "id": "s3", "heading": "Install", "excerpt": "Run `cargo install …`", "features": { "shell_blocks": 2, "ordered_items": 0 } }
    ],
    "guidance": "You choose reading aids for a Markdown viewer. Never assume facts beyond the given excerpts."
  },
  // 注: リクエスト直下に instructions は置けない(OpenAPI 0.2.0 = model/state/questions/assets。実 API は 400)。全体方針は state.guidance に入れる(§11.3)
  "questions": {
    "doc_type":  { "type": "choice", "criteria": { "readme": "Project README …", "design_doc": "…", "adr": "…", "runbook": "…", "minutes": "…", "changelog": "…", "generic": "…" } },
    "s3_helps":  { "type": "noul",   "instructions": "Section s3: would one of its candidate widgets help a reader more than the plain text?" },
    "s3_widget": { "type": "choice", "instructions": "Section s3: which widget fits best?", "criteria": { "ConfigViewer:commands": "Copyable ordered shell commands", "StepNavigator": "Step-by-step navigation" } },
    "doc_helps": { "type": "noul",   "instructions": "Would a document-level overview panel help?" },
    "doc_widget":{ "type": "choice", "criteria": { "SemanticOutline": "…", "ActionItems": "…" } }
  }
}
```

- 候補が 1 つのセクションは `sN_widget` を省き `sN_helps` だけ聞く。候補 0 のセクションは質問しない(prefilter)。
- 質問数の上限(既定 40)を超えたらセクションを features の強い順に切り、残りは rules。
- 応答 → `Decision`:
  - `helps.noul < helps_threshold(0.5)` → **None(出さない)**。
  - `widget.confidence < confidence_threshold(0.6, 論点 K)` → **None**(低信頼で出さない)。
  - 未知 option / 欠損 / 型不一致 → その質問は rules にフォールバック(`fallback_reason: "invalid_answer"`)。

### 3.4 Composer(決定的)
- 1 セクション最大 1 widget、文書レベル最大 3。文書順に並べ、doc-level は先頭。
- `AdaptiveLayout` を構築 → `validate_nodes` → 送信。理由(why)は selector/provider、confidence、noul 値、alternatives(probabilities 上位)、`reason_code` の **列挙値のみ**(自由文なし)。

### 3.5 Rust インタフェース(`crates/mdpeek-gui/src/select/`)

```rust
pub trait Selector: Send + Sync {
    fn id(&self) -> SelectorId;                       // { provider: "jev"|"rules"|"fake", model: String }
    fn select(&self, req: &SelectionRequest, deadline: Instant) -> Result<SelectionDecisions, SelectError>;
}
pub enum SelectError { Disabled, MissingKey, Timeout, RateLimited { retry_after: Option<Duration> },
                       Overloaded, Http(u16), InvalidResponse(String), Network(String) }
pub struct SelectionRequest { pub doc: DocState, pub sections: Vec<SectionState>, pub candidates: Vec<CandidateSet> }
pub enum Decision { Choose { candidate: CandidateId, confidence: f32, helps: f32, alternatives: Vec<(CandidateId, f32)> },
                    None { reason: NoneReason } }
pub fn adaptive_layout(markdown: &str, filename: Option<&str>, selector: &dyn Selector,
                       cache: &SelectionCache, revision: u64) -> AdaptiveLayout;  // 失敗時は rules で必ず埋める
```

### 3.6 Wire format(`AdaptiveLayout`, version 1)

```jsonc
{
  "version": 1, "revision": 7, "doc_hash": "…",
  "selector": { "provider": "jev", "model": "jev-1.14.0", "fallback": null },   // fallback: "timeout"|"offline"|"rate_limited"|"invalid_answer"|"disabled"|"missing_key"
  "doc_type": { "value": "readme", "confidence": 0.93, "by": "jev" },
  "placements": [
    { "scope": "section", "anchor": { "block_id": "…", "heading": "Install", "range": {…} },
      "node": { "kind": "ConfigViewer", "meta": { "source_range": {…}, "origin": "rules", … }, … },
      "why": { "by": "jev", "confidence": 0.97, "helps": 0.81, "reason_code": "chosen", "alternatives": [["StepNavigator", 0.03]] } }
  ],
  "skipped": [ { "anchor": {…}, "reason_code": "low_helps" } ],
  "timing_ms": { "select": 140, "total": 152 }
}
```

### 3.7 Live 更新・revision・cache
- server はファイルごとに `revision`(変更ごと +1)を持つ。watch 変更 → **debounce 800ms** → 選択。Jev 呼び出し中に新 revision が来たら結果を破棄(旧 revision 拒否)。client も `revision` が現在より古い応答を捨てる。
- cache(メモリ): key = `(section.hash, REGISTRY_VERSION, POLICY_VERSION, selector id/model)`。変更セクションだけ再質問し、未変更セクションの decision を流用。LRU 上限(既定 256 エントリ)。
- 生成中も前回 layout を表示し続ける(ちらつき防止)。初期表示は Markdown 本文 + rules layout を即時、Jev 結果を後追い(論点 F)。

## 4. セキュリティ / プライバシー / 脅威モデル

| 脅威 | 対策 |
|---|---|
| 意図しない外部送信 | 既定 provider = `rules`。Jev は `[select] provider = "jev"` **かつ** `allow_network = true` の明示 opt-in。UI に送信中/送信先を表示 |
| 秘密情報漏洩 | key は **サーバプロセスの env `TYPESAFE_API_KEY` のみ**(config/CLI 引数/ログに置かない)。ブラウザに返さない |
| 文書内容のログ残り | ログは件数・latency・HTTP status・fallback 理由のみ。本文/抜粋/質問文を出さない(テストで検証) |
| prompt injection | 出力空間が列挙値のみ。prefilter 済み候補しか選べず、内容は決定的抽出。最悪でも「許可済み widget の誤選択」 |
| 不正コンポーネント/任意アクション | allowlist + `validate_nodes`。widget はコマンドを実行しない(copy のみ) |
| 機密文書 | `[select] provider = "rules"` で完全ローカル。`--select rules` で一時上書き |
| DoS/コスト | debounce、cache、質問上限、timeout(既定 4s, 1 リトライ, `retry-after` 尊重)、429/529 → rules fallback |

## 5. 設定 / CLI

```toml
[select]
provider = "rules"            # "rules" | "jev" | "fake"(テスト用)
allow_network = false         # jev の外部送信を明示許可
model = "jev-1.14.0"          # TYPESAFE_DEFAULT_MODEL 相当。版固定推奨
base_url = "https://api.typesafe.ai"
timeout_ms = 4000
confidence_threshold = 0.6
helps_threshold = 0.5
max_questions = 40
debounce_ms = 800
# key は env TYPESAFE_API_KEY のみ
```

`mdpeek select <file> [--provider rules|jev|fake] [--json]` で layout を出力(デバッグ/評価用)。既存 `mdpeek gen` は不変。

## 6. Web UI 設計

### 6.1 方式決定: Tailwind + shadcn/ui、UI ランタイムは React 19(Preact から変更)
- **Tailwind CSS v4**(`@tailwindcss/vite`、ビルド時に静的 CSS を生成 → `web/dist` に embed。ランタイム/CDN 依存なし)。
- **shadcn/ui はソース取り込み**(公式 CLI `shadcn@4.23.1` の `add` で `components/ui/*` を生成。依存: `radix-ui@1.8.1`、`cmdk@1.1.1`、`react-resizable-panels@4.24.3`、`class-variance-authority`、`cn@0.5.2`(shadcn 公式 = clsx + tailwind-merge 互換)、`lucide-react`)。
- アイコンは `lucide-react` に統一。インライン SVG の手書きは廃止。

**互換性の実測(2026-11-27, Chrome 158 + Playwright, 同一のテストアプリ・同一のキーボード操作シナリオ)**

| 検査 | Preact 10.29 + compat | Preact 10 + ref-as-prop shim | Preact 11.2 + compat | Preact 11.2 + shim | React 19.2.9 |
|---|---|---|---|---|---|
| Dialog が開く | ✗ `getComputedStyle` 例外(Radix Presence が ref を prop で受ける前提) | ✓ | ✓ | ✓ | ✓ |
| Dialog focus trap / Esc / focus 復帰 | ✗ | ✓ | 不安定(1 回目 ✗, 2 回目 ✓) | ✓ | ✓ |
| Tabs 矢印キー(automatic activation) | — | — | ✓ | ✓ | ✓ |
| DropdownMenu Enter→↓→Enter で選択 + focus 復帰 | — | ✗ | 不安定(1 回目 ✓, 2 回目 ✗) | ✗ | ✓ |
| Tooltip がキーボード focus で開く | — | ✗ | ✗ | ✗ | ✓ |
| Command(cmdk)入力で絞り込み → Enter | — | ✓ | 不安定 | ✗(絞り込まれない) | ✓ |
| Resizable separator を矢印キーでリサイズ | — | ✓ | ✓ | ✓ | ✓ |
| JS(minified / gzip) | — | 234 kB / 81 kB | 234 kB / 81 kB | 235 kB / 81 kB | 410 kB / 133 kB |

結論と根拠:
- shadcn v4 系(`radix-ui` 1.x の React 19 前提: ref-as-prop、focus/pointer イベント順序)は `preact/compat` で **複数のキーボード操作が壊れ、しかも実行ごとに結果が揺れる**。shim で一部は直るが Tooltip/Dropdown/cmdk が残る。アクセシビリティ要件(キーボード完結)を満たせない。
- 代替案のコスト比較: (a) Preact のまま Radix を使わず primitives を自作 → shadcn を「採用」できず、focus trap / roving focus / typeahead を自前実装・保守(高コスト・高リスク)。(b) Preact + compat + shim で個別回避 → 上表のとおり未解決項目が残り、上流更新のたびに再検証が必要。(c) **React 19 へ移行** → 既存 web/src は約 1.7k 行(Preact + signals)で、`class`→`className` と signals → `useSyncExternalStore` ベースの小さな store への置換が主。増分は JS +176 kB(gzip +52 kB)だが、localhost から単一バイナリ埋め込みで配信するため実害は小さい。
- **(c) を選択**。論点 A の「全面 Preact」は「全面 React(単一の component tree)」に読み替える(単一描画系統という意図は維持)。AGENTS.md §0.1 に追記した。
- 生データ: スクラッチ領域の `spike-result-*.json`(テストスクリプトと共に、本メモの記録として要約を残す)。
- Tailwind preflight は **アプリ chrome にのみ**効かせ、`.markdown-body`(GFM 本文)には GitHub 由来の本文スタイルを維持(`@layer` 順序と `:where()` で隔離)。
- tokens は CSS variables(semantic: `--background`, `--foreground`, `--muted`, `--border`, `--ring`, `--accent`, `--change` …)を **単一ソース**にし、Tailwind `@theme` に写像。`styles.css`/`panel.css` の重複は撤去。

### 6.2 デザイン計画(frontend-design 手順: 計画 → ブリーフ照合 → 実装 → 批評)

主題: ローカルリポジトリの Markdown(設計書・README・手順書)を読むエンジニア。主目的は **本文を快適に読むこと**、副目的は出典に紐づいた読書補助で要点に素早く到達すること。

- Color(light): paper `#FBFBFA` / ink `#1B1D22` / graphite(副文) `#5C616B` / rule `#E4E4E0` / accent(cobalt, 操作と focus のみ) `#2D5BD7` / change-marker `#F3DA78`(live 更新の差分だけ)。
  Color(dark): `#14161A` / surface `#1A1D22` / ink `#E6E7EA` / graphite `#9AA0AA` / rule `#2A2E35` / accent `#86A6FF` / marker `#5E5220`。
- Type: 外部フォント無し(オフライン + CJK)。UI = system-ui 13/14px、本文 = 16px / line-height 1.7 / measure ≤ 72ch、見出しスケール 1.25。等幅はコードのみ(ラベルに等幅を使わない)。
- Layout:
  ```
  Split (≥1280)
  ┌ topbar 44 ─ [≡] repo › docs/design.md ─────── [Read|Split|Adaptive] [⌘K] [◐] ┐
  ├ rail 256 ──┬──────── body (≤72ch, 中央) ─────────┬──── widgets 360 ─────┤
  │ Files      │ # Payment Service Design           │ Readme · jev 0.93    │
  │ Outline    │ ## Overview …                      │ ▸ Risks   L53–55  ⓘ  │
  │            │                                    │ ▸ Steps   L12–20  📌 │
  └────────────┴────────────────────────────────────┴──────────────────────┘
  Adaptive (≥1280): body の右余白に「欄外注」レール。widget はアンカー節の高さに揃い、本文側の左端に細い tick で対応を示す。
  Read: rail と widgets を閉じ、本文のみ(topbar は残す)。
  Mobile (<768): 単一ペイン。本文が既定。Files/Outline/Widgets は bottom sheet(Dialog)で開く。
  ```
- Principles: (1) 本文が主役、chrome は退く(hairline の rule、影は overlay のみ)。(2) 大胆さは 1 か所 = **欄外注レールと source tick**。widget はカードではなく「注記」— 左端の出典アンカー(`L53–55`)と見出しだけの軽い構造。(3) accent は操作可能なものにだけ。(4) motion はユーザー操作への応答と live 変更マーカーだけ、`prefers-reduced-motion` で無効。
- ブリーフ照合で修正した点: 当初案の「widget を角丸カードで縦積み」は SaaS カードキットの既定(frontend-design の calibration #4)なので **注記スタイル**へ変更。群見出しの ALL CAPS ラベル(既存 panel.css)は削除。メタ情報の `A · B · C` 連結は使わず、ラベル付きの短い行にする。`→` を付けたリンクも使わない。

### 6.3 UX 要件(画面状態と操作)
- モード: Read / Split / Adaptive(`1`/`2`/`3`、palette からも)。layout(モード、ペイン幅、折り畳み、pin/hide)を localStorage に保存(失敗しても描画は壊さない)。
- ペイン: rail / widgets はドラッグとキーボード(矢印)でリサイズ、`[` / `]` で折り畳み。
- Command palette(`⌘K` / `Ctrl+K`): ファイル切替(explorer と同じ候補)、見出しジャンプ、モード、テーマ(light/dark/system)、widget の pin/hide、再選択。
- Source jump & sync: widget の出典チップ → 本文の該当ブロックへスクロール + 一時ハイライト。本文スクロール位置に応じて outline と widget の現在節を強調。
- Why this widget: popover に selector/provider、confidence、helps、alternatives、fallback 理由。
- 状態: empty(候補なし → 「この文書には追加の読書補助はありません」+ rules/Jev 状態)、loading(前回結果を保持したまま細いプログレス)、error(原因と対処: 「Jev に接続できません。rules で表示中」)、stale(旧 revision 破棄)。
- 既存機能の保持: explorer(worktree/branch グループ、breadcrumb、compare → diff[source/rendered × unified/split, swap, live re-diff])、TOC filter + scrollspy、front matter、#16 差分パッチ + flash + autoscroll、mermaid/hljs/MathJax、scrolly(guided reading)。
- アクセシビリティ: 全操作キーボード可、可視 focus ring、skip link、landmark、コントラスト 4.5:1、`aria-live` で生成状態通知、44px タッチターゲット(mobile)。

ui-ux-pro-max(ローカル DB)で確認した適用ルール: Keyboard Navigation(High)/ Focus States(High)/ Skip Links / Line Length 65–75ch / Line Height 1.5–1.75 / Reduced Motion(High)/ Excessive Motion: 1–2 要素まで / Truncation + expand。スタイル推奨は「Minimalism & Swiss Style」(dashboards・documentation 向け)。同ツールの配色推奨(teal + orange)と Inter(Google Fonts 取得)は、オフライン制約と frontend-design の「既定を避ける」方針により不採用。

## 7. 評価

- 評価セット: `tests/fixtures/select/*.md`(README / 設計書 / ADR / 手順書 / 議事録 / changelog / 小説抜粋 / 製造指示書)に、人手の期待ラベル(section ごとに widget か none)を付ける。
- 指標: 一致率、**none の適切さ**(出すべきでない節で出さない率)、p50/p95 選択遅延、1 文書あたり input tokens(→ 費用)、同一入力での安定性(3 回)。
- 比較: rules-only vs Jev vs(参考)既存 LLM lens。**未測定の値は断定しない**。

## 8. 段階的実装計画と受入条件

1. 発注 1(kenshu-loop): Jev 選択バックエンド — selector trait、rules/jev/fake、composer、validate、`/api/adaptive` + WS push、revision/debounce/cache、config、`mdpeek select`。受入: cargo test、fake/mock HTTP による timeout/429/invalid/旧 revision/ログに本文なし の検証、実 API(key があれば)疎通。
2. 発注 2(kenshu-loop): Web UI — Tailwind + shadcn(React 19)、3 モード、explorer/outline、palette、source jump、why、pin/hide、永続化、既存機能の移植。受入: Playwright(desktop/mobile, light/dark, keyboard, offline, live edit, diff, explorer)+ スクリーンショット + 独立レビュー。
3. 統合・検収: README / config.example 更新、ベンチ(バンドルサイズ、選択遅延)、PR。

## 9. json-render を導入しない理由
既存の Rust 正本 IR(serde tag + allowlist + sourceRange 検証)が json-render の担う「カタログ制約付きツリー」を既に満たし、Rust 側で検証できる。json-render は JS 側スキーマ(Zod)を正本にするため、検証境界が二重化する。Jev 統合部分も公開コードが無い(第三者記事)。

## 10. 未決事項 / ブロッカー
- 非 enterprise のデータ保持(DPA)未確認 → README に「外部送信は opt-in、機密文書は rules」を明記。
- MathJax の CDN 依存(既存)。オフライン数式は KaTeX/MathJax embed の後続 issue。
- 既存 `/api/gui` のディスクキャッシュ(論点 E 不整合)は後続 issue。

## 11. 実装記録(kenshu-loop の結果をここに追記)

### 11.1 発注 1(選択バックエンド)— 第1回納品 (2026-11-27)

実装: `crates/mdpeek-gui/src/select/`(`sections`=①②、`mod`=③④⑤ + wire format、`jev`、`fake`、`cache`、`eval`)、`crates/mdpeek-server/src/adaptive.rs`(`GET /api/adaptive` + WS push)、`mdpeek select` / `mdpeek select-eval`、`[select]` 設定。

設計からの変更点と理由:

- **selector.provider は「実際に決めた側」**を表し、設定値は `selector.requested` に分けた(fallback 時に `provider:"rules", requested:"jev", fallback:"timeout"` となり UI が誤解しない)。`selector.requests` に外部送信回数を出す(送信有無を UI/テストで観察可能にするため)。
- **キャッシュは raw 回答(noul/confidence/choice)を保存**し、閾値は composer で毎回適用する(閾値変更でキャッシュ無効化が不要)。キーは §3.7 の要素 + 候補 id 列。文書レベル質問(doc_type / doc_widget)は「見出し列 + rules doc_type + doc 候補 + ファイル名」のハッシュをキーにし、本文だけの変更では再質問しない。Jev 失敗時の rules 結果はキャッシュしない(次回は Jev を再試行)。
- **未変更セクションは質問自体を送らない**: 全セクションがキャッシュ命中なら HTTP 0 回、一部変更なら変更セクションだけを 1 リクエストで送る。
- **Jev の文書レベルは 1 widget**(`doc_helps` + `doc_widget`)。rules は従来どおり最大 3。複数選択を noul×候補で聞く案は質問数が増えるため見送り。
- **`noul` 回答形**は一次資料で「確率」とだけあり形が未確定のため、数値そのもの / `{noul|probability|yes|p: 数値}` を受理し、それ以外は型違い(`invalid_answer`)とした。**実 API での形確認は発注者の疎通試験で要確認**。
- **429/529**: `retry-after`(秒)が全体 deadline 内に収まるときだけ 1 回再試行。無ければ 500ms 待機で 1 回。deadline を超える場合は即 fallback。SDK 既定(10s, 2 回)より短いのは対話用途(§4 DoS/UX)のため。
- `ConfigViewer` に `format: "shell"` を追加(`ConfigViewer:commands` 用。コマンドは表示/コピーのみで実行しない)。web registry 側の表示対応は発注 2。
- `validate_nodes` の入れ子 range 検査に ActionItems / OpenQuestions / DecisionLog / SemanticOutline を追加。文書レベル widget の `meta.sourceRange` は項目 range の包含範囲。
- list/heading 由来の widget テキストは parser の平坦化テキストではなく**ソース行から marker を除いた原文**を使う(インライン markup を保持し、言い換えを生まない)。
- 設定ファイルのパースエラー表示は**メッセージと行番号のみ**に変更(従来は該当行を表示しており、誤って書かれた秘密値が stderr に出るため)。`api_key` は `[select]` の未知キーとしてエラー(従来どおり警告 + 既定値)。
- HTTP クライアント内部(hyper/reqwest/h2/rustls)の tracing は `RUST_LOG` に関わらず off(ヘッダ・本文をログに出さないため)。select/eval の tracing は stderr(stdout は純 JSON)。

検証(すべてキー無し・ローカル模擬サーバ): `tests/select_jev.rs`(送信内容・閾値・invalid/missing・401/5xx/壊れた JSON/timeout 既定 4s/429 再試行 1 回・retry-after 尊重/529 deadline 超過/接続不可/キー無し/未許可/設定にキー)、`tests/adaptive_serve.rs`(GET・WS push・debounce・旧 revision 破棄・キャッシュ再利用で再送信は変更節のみ・ディスク非保存・ログに本文/キー無し)、`tests/select_cli.rs`(wire format・fake・eval・原文抽出 + range)。`cargo build --no-default-features` では `jev` が `fallback:"not_built"` で rules に degrade。

評価(`mdpeek select-eval tests/fixtures/select`, 8 fixture: README / 設計書 / ADR / 手順書(runbook)/ 議事録 / changelog / 小説抜粋 / 製造指示書, ラベル付き 36 節):
rules = 一致率 1.000、none 適切率 1.000、widget recall 1.000、p50 0.73ms / p95 1.01ms(release, 5 runs/doc, 2026-11-27 受注側環境)。**注意: fixture・ラベル・rules スコアを同一作業者が作成しており楽観的な値**。独立したラベル付けと実文書での再評価が必要。**Jev は未実行**(キー無し)— 数値なし。

既知の制約: watcher(既存 PollWatcher)は mtime を秒単位で比較するため、同一秒内の連続保存は 1 回の変更として扱われる(既存挙動、未変更)。

### 11.2 第2回納品(差し戻し対応, 2026-11-27)

- **R10 不適合の修正**: `Callout:risk` の body が空だった(GFM alert ブロックの parser 平坦化テキストが空のため)。alert はソース行から `>` と `[!WARNING]` 等の marker 行を除いた原文、risk 段落はソース行そのものを body にした。sourceRange は引用ブロック/段落の行範囲。抽出テスト(`tests/select_cli.rs`)に Callout(非空・原文・range が本文行を含む)と RiskPanel(非空・原文)を追加。
- `selector.requests` は失敗時も実際の送信回数(429 再試行を含む)を表示する(`Selector::last_requests`)。
- fake 台本の書式を README に記載し、例 `tests/fake-select.json` を追加。台本の読込/パース失敗時は理由(I/O 種別・JSON の位置)を stderr に出す。
- 未使用依存 `tokio-stream`(mdpeek-server)を削除。

### 11.3 実 Jev サービスでの検証(発注者, 2026-11-27)

キーはシークレット入力カード経由のファイルからテスト時だけ `TYPESAFE_API_KEY` に渡し、出力・ログ・引数・Git には残していない(標準出力/標準エラーをキー文字列で走査して非検出を確認)。対象は無害な fixture(`tests/fixtures/select/*` と合成 runbook)。

- **発見した不具合**: 受入済みの実装(と本メモ §3.3 の当初の例)はリクエスト直下に `instructions` を置いており、実 API は `400 {"detail":{"error_type":"api_usage_error","message":"Invalid request."}}` を返した(模擬サーバはスキーマを検証しないため検収で見逃された)。要素を 1 つずつ外して切り分け、OpenAPI 0.2.0 の `SystemOneRequest` が `model/state/questions/assets` だけを持つことを確認。→ 全体方針を `state.guidance` へ移し、リクエスト形を OpenAPI の形に限定する単体テスト `request_matches_system_one_schema_shape` を追加。失敗ログに HTTP ステータスを出すようにした(秘密ではない)。fallback は設計どおり働き、修正前も UI/CLI は rules で正常応答していた。
- **疎通**: `mdpeek select <runbook> --provider jev --allow-network` → `status 200 in 145ms, attempts 1, sections 3, questions 6`、`model: jev-1.14.0`、`doc_type: runbook (0.98, by jev)`。Prerequisites → `Checklist`(helps 0.74)、Steps → `StepNavigator`(helps 0.72)、Commands / 文書レベル overview は `low_helps` で出さない。
- **noul の応答形**: 実 API は `{"type":"noul","noul":0.57}`(確率を `noul` キーで返す)で、§11.1 の受理形に含まれる。
- **評価**(`mdpeek select-eval tests/fixtures/select --runs 3`, 実 API, 2026-11-27):

  | provider | accuracy | none_ok | widget_recall | p50 | p95 |
  |---|---|---|---|---|---|
  | rules | 1.000(楽観値: ラベルと rules が同一作業者) | 1.000 | 1.000 | 0.34 ms | 1.82 ms |
  | jev (jev-1.14.0) | 0.943 | 1.000 | 0.833 | 139.7 ms | 152.3 ms |

  Jev の不一致: CHANGELOG「Changelog」節(期待 Timeline → none)、README「Installation」節(期待 ConfigViewer:commands → none)。いずれも「出さない」側に倒れている(過剰表示は 0)。
- **トークン/費用**(実 API の `usage`, 1 文書 1 リクエスト): input 588–1,606 tokens(質問 1–7 問)、output 88–238。単価 $0.042 / 1M input(output 無料, models.md)から **1 文書あたり約 $0.00002–0.00007**(計算値。請求額は未確認)。
- 未確認: 組織別 rate limit の数値、429/529 の実挙動(模擬サーバでのみ検証)、大きな文書(64k tok 近傍)での挙動。

### 11.4 独立レビュー(バックエンド)と修正(発注者, 2026-11-27)

私の結論を渡さずに、別エージェントへ読み取り専用のレビューを依頼した。報告 14 件のうち次を修正し、回帰テストを追加した(`tests/select_jev.rs` / `tests/select_cli.rs` / `tests/adaptive_serve.rs`)。

| # | 内容 | 対応 |
|---|---|---|
| 1 | Jev の `probabilities` キー(モデル由来の任意文字列)が `why.alternatives` としてブラウザまで届く | 候補 id だけを残す(jev.rs の回答解析 + composer の二重防御)。`<img onerror>` を混ぜた模擬応答で非出力を検証 |
| 2 | `retry-after: 1e300` などで `Duration` 変換が panic(exit 101) | deadline と f64 で比較してから `try_from_secs_f64`。1e300 / 1e18 / NaN / -5 / inf で exit 0 を検証 |
| 3 | 見出しが状態上限に数えられず無制限(1.86 MB の request を確認) | 見出しを 120 文字に切り、文書見出し・節見出しも `max_state_chars` に計上 |
| 4 | `GET /api/adaptive` が Jev を同期待ちし、最大 3 回再計算・重複送信 | GET はネットワークを待たない: 計算済みならそれを、無ければ rules レイアウト(`fallback: "pending"`)を即返し、Jev 結果は WS で push。選択は single-flight |
| 5 | 1 回の失敗で、未変更節のキャッシュ済み判断まで rules に戻る | 失敗時は今回質問した節だけ rules に戻す |
| 6 | 設定パースエラーで値(秘密の可能性)が stderr に出る | エラーメッセージ内の引用部分を伏せる |
| 8 | 閾値の NaN / 範囲外 | NaN は既定値、範囲外は 0..1 に clamp |
| 9 | `max_questions` が文書レベル質問に効かない | 予算内に入らなければ文書レベルは rules |
| 10 | CRLF の `\r` がコピー用コマンドに残る | 行末 `\r` を除去 |
| 11 | ブラウザ未接続でも保存のたびに Jev を呼ぶ | 接続クライアントが無いときは延期(次の GET で選択) |
| 13 | `http://` の base_url に Bearer キーを平文送信しうる | https 以外は loopback のみ許可(`insecure_base_url` で fallback、送信 0 回) |
| 14 | 空の `> [!WARNING]` が空本文の Callout になる | 本文が空なら候補にしない |

未対応(既知の制約として記録): #7 親節の Timeline 特徴が入れ子見出しの変更でキャッシュキーに反映されない(影響は helps の小さな変化)、#12 Jev クライアントは独自 runtime を作るため tokio コンテキスト内から直接呼ぶと panic する(現行の呼び出し元はすべて OS スレッドで安全)。
||||||| parent of b5d30c4 (feat(web): Tailwind v4 + shadcn/ui (React 19) adaptive viewer UI)

### 11.5 発注 2(Web UI)— 納品 (2026-11-27)

実装: `web/`(React 19 + Tailwind v4 + shadcn/ui ソース取り込み + lucide-react、単一 bundle `web/dist/app.{js,css}`)、`static/index.html`(app shell。初回文書を `#mdpeek-boot` に JSON で埋め込み、本文は widget 選択を待たず初回フレームで描画)、`mdpeek-server` に `GET /api/doc` と WS `update` の拡張(`path` / `blocks` / `revision`)。旧 `static/js/main.js`・`github-{light,dark}.css`・hljs テーマ CSS・`web/src/{styles,panel,scrolly}.css`・Preact island は削除。

設計からの変更点と理由:
- **本文ブロック ↔ ソース行**: server が top-level block の行範囲 `blocks: [[start,end],…]` を HTML と同じ順で返し、client が `data-line-start/-end` を付ける(GFM 変換を二重実装しない論点 G を維持)。widget の出典チップ・scrollspy・Adaptive 配置はこの属性で解決する。
- **Adaptive の欄外注**: widget は記事内の slot に React portal で描画し、列幅 ≥ 980px では float で右余白へ(見出しと同じ高さ)、狭い列ではセクション直後に表示(CSS container query)。文書レベルの widget は欄外では折り畳み表示にし、セクションの注が自分の見出しからずれないようにした。GFM 本文のスタイルは `:not(.mp-note-slot *)` で注に及ばない。
- **ライブ更新**: front/back trim + LCS で変更ブロックだけ差し替える(旧実装は 2 か所の編集の間を全部差し替えていた)。画面上の未変更ブロックの位置を基準に読書位置を保つ。WS `update` は次に来る layout の `revision` を持ち、client はそれより古い layout を捨てる(選択中は前回の widget を表示し続ける)。
- **トークン**: 色・余白・タイポは `web/src/app.css` の `:root` / `.dark` だけで定義し、shadcn の semantic 名(`--primary` = cobalt accent、`--accent` = hover 面)へ写像。Tailwind のクラス走査は `source(none)` + `@source "./"` で `web/src` に限定(チェックアウト位置に依存せず `npm ci && npm run build` が同一出力になる。`scripts/check-web-dist.sh`)。
- **テーマ既定**: 保存値が無ければ `system`。ただし mdpeek 設定の `theme = "dark"` なら dark を既定にする(従来の設定を尊重)。
- **pane の永続化**: react-resizable-panels v4 の `onLayoutChanged` のうち `isUserInteraction` のものだけ保存(mount 時や制約再計算で保存値を潰さない)。折り畳みは `[` / `]` / ボタンで prefs に直接書く。キーボードで最小幅より縮めると折り畳まれる(ライブラリ既定)。
- UI の文言は英語(既存 UI と README に合わせた)。文書は日本語を含めて検証している。

検証: `cd web && npm run test:e2e`(Playwright 40 本、fake provider・キー無し・ネットワーク無し、システム Chrome)。axe-core(wcag2a/aa + best-practice)で serious/critical 0(light/dark × split/adaptive/popover/palette/mobile)。スクリーンショットは `docs/design/screenshots/`(light/dark × desktop(read/split/adaptive)/mobile(read/adaptive/widgets sheet)、状態: widget なし・選択中(ライブ更新直後)・更新完了・エラー・fallback)。静的検査は `scripts/check-web-assets.sh`。

既知の制約: MathJax は従来どおり CDN(発注範囲外)。Guided reading は LLM 専用のため、LLM 未設定では panel に失敗理由が出る(既存挙動)。

### 11.6 受入後の自己批評と独立 UX レビュー(発注者, 2026-11-27)

kenshu-loop で受入済みの UI を、frontend-design の「批評」手順と ui-ux-pro-max のチェックリストでスクリーンショットから批評し、さらに私の結論を渡さない独立レビュー(実ブラウザで 1440/1280/900/390px、light/dark、キーボード、ライブ編集を試行)を 1 回入れた。

自己批評で直した点(`83ede85`, `1ecd13c`):
- 読書補助が本文の写しになっていた → **操作できる補助**に: Checklist / Action items はチェックと進捗(ブラウザ内のみ保存。ファイルは変更しない。「Reset to file」)、Steps は Step n of N のステッパー、Commands は行ごと / 全部コピー(実行はしない)、表はソートと絞り込み。
- 行番号チップを graphite にし、項目単位のチップは行 hover / focus 時のみ。上部バーの補助トグル 4 つを「View」メニューに集約。選択器状態に短い理由と `pending` を表示。document.title をファイル切替に追従。
- 文書レベルの補助が節の補助と同じ行を指すときは出さない(`covered_by_section`)。
- モード / ペイン切替で読書位置が先頭に戻るバグ → 先頭に見えているブロックを source 行で覚えて復元。

独立レビュー(初回 6.5/10)の指摘と対応(`dbd9de4`):

| 重要度 | 指摘 | 対応 |
|---|---|---|
| High | 補助を非表示にすると文書内の全チェック・ステップ位置が消える | `hideWidget` が他の per-document 設定を落としていた。修正 + E2E |
| High | 900px / スマホで上部バーが重なる | shadcn Button の `shrink-0` を解除、<1024px でリポジトリ区画を隠しモード / View をアイコン化、<1280px で状態はドットのみ |
| High | 1280px + レールで Adaptive が本文直下の重複表示になる | 820px 以上の文書列で 232px の欄外注を使う中間段、インライン fallback は折り畳み開始 |
| High | 本文を言い換えるだけの補助(README の Outline by role、alert の重複 Callout、定義のない Glossary、17 個の補助) | 本文で強調済みの alert には Callout を出さない / 意味のある outline のみ / 定義のある用語のみ / ほぼ完了済みの一覧は Action items にしない / 文書レベル 2・合計 8 個まで(`over_limit`) |
| Medium | ライブ更新の強調が強すぎる | 左の細いバー + 薄い背景のフェード |
| Medium | リスク・問いが赤太字 | 本文色・通常の太さ、error のみ細い danger 罫線 |
| Medium | ボタンのフォーカスリングが薄い(約 2.2:1) | 2px の solid outline(Tailwind v4 の `outline-none` 対策で `outline-solid` を併記) |
| Medium | 日本語の組版(lang=en、見出しの分割) | かなを含む文書は `lang="ja"`、見出しは `word-break: auto-phrase`、`line-break: strict`、CJK フォント優先 |
| Medium | Files / パレットが `.gitignore` を無視(node_modules の README が混入) | `ignore` crate で gitignore を尊重。本リポジトリで 190 件 → 23 件(うち 167 件が node_modules) |
| Low | 注記ごとの操作アイコンが多い / 用語の不統一 / 表のバッククォート / エラー時に古い補助が最新に見える | hover・focus 時のみ表示、隣接時は節名を省略、「reading aid」に統一、表セルのインラインコード、エラー時は薄く表示し「前回の補助を表示中」 |

未対応(後続): 補助と本文の状態の同期(チェックを本文側にも反映するか)、パレットでのファイル切替がサーバ全体(全タブ)に効く既存仕様、MathJax の CDN 依存(KaTeX 置換は論点 H の後続)、§11.4 の #7 / #12。

### 11.7 追加要望: 本文内表示・チャート・ブランチ比較の削除(2026-11-27)

ユーザー要望「ブランチと比較する機能は削除、現在のブランチだけでよい」「Generative UI は右サイドバーではなく本文内に」「チャートも生成したい」への対応。

- **ブランチ比較の削除**: explorer は現在の作業ツリー(`git rev-parse --show-toplevel`)だけを列挙し、他の worktree は一覧にも `allowed_roots` にも入れない(他 worktree のファイルの `/api/select` は 403)。2 ファイル diff(`/api/diff`、`similar` 依存、diff CSS、DiffView)と worktree/branch のグループ切替を撤去。パンくずのリポジトリ表示は `repo (branch)` の静的ラベルになった。
- **本文内表示**: Split モード(右ペイン)と欄外注を廃止し、モードは Read / Adaptive の 2 つ(`1` / `2`)。Adaptive では各補助を **抽出元ブロックの直後** に 1 つの slot として記事内に挿入する(表の直後にチャート、リストの直後にステップ)。文書レベルの補助は冒頭セクションの後に折り畳みで、pin した補助はタイトル直下へ移動する。補助は本文と同じ measure に収め、hairline 枠 + 1 段明るい面で本文と区別する(影・グラデーションなし)。保存済み `mode: "split"` は Adaptive に移行し、旧 `widgetsSize` / `groupBy` は捨てる。右側シートだった「Generated UI」(`/api/gui` のレンズ)も UI から外した(API と `mdpeek gen` は残す。文書レベルの rules レンズは Adaptive の文書レベル候補として本文内に出る)。
- **チャート**(`UiNode::Chart`, `select/chart.rs`): 表の各列を「全非空セルが同じ単位で数値に parse できる」ときだけ指標列とみなし、ラベル列が日付・月・四半期・バージョンなら折れ線、それ以外は横棒。値はセルの parse 結果のみで、セル原文を `cells` に保持して tooltip / スクリーンリーダー用の表に出す(推定値なし)。単位混在・行番号・ID・年だけの列は描画しない。指標が複数でも 2 軸にせず、切替で 1 つずつ。dataviz スキルの指針(棒 ≤24px・データ端 4px 角丸・2px 線・hairline grid・単一系列は凡例なし・テキストは系列色を使わない・hover/キーボードで値)に従い、色は `--series-1`(light `#257de0` / dark `#338ced`)。rules スコアは bar 0.7 / line 0.75(+0.05 if ≥6 行)で DataTable:sortable(0.4/0.65)より上、Jev には他候補と同じく id だけを選ばせる。`REGISTRY_VERSION` を 2 に上げた。
- **検証**: Rust 全テスト、web build、Playwright E2E 36 件 + スクリーンショット 12 件が通過。sr-only 表が記事外へはみ出してページ root がスクロールする不具合をスクリーンショットで見つけて修正(`.gui-chart { position: relative }`、E2E で root がスクロールしないことを確認)。

## 12. ベンチマーク(実測, 2026-11-27)

環境: NixOS / Chrome 158(Playwright, headless)/ release ビルド。ベースライン = `308e537`(起点ブランチ)。数値は 7 回の中央値。`docs/sample-design-doc.md`、rules。

| 指標 | ベースライン | 本ブランチ |
|---|---|---|
| release バイナリ | 20.59 MB | 21.94 MB(+1.35 MB: React + Tailwind + shadcn の埋め込み) |
| ページ転送量(初回表示, MathJax CDN 含む) | 4,891 KB | 4,406 KB |
| 外部リクエスト数 | 5(MathJax 一式) | 1(MathJax 本体のみ) |
| 本文が見えるまで | 25 ms(SSR) | 78 ms(hljs / mermaid のオンデマンド読込後。読込前は 284 ms) |
| First Contentful Paint | 28 ms | 84 ms |
| DOMContentLoaded | 289 ms | 36 ms |
| Web バンドル | 30.5 kB JS + 10.2 kB CSS(island のみ) + main.js 44.7 kB | 539 kB JS(gzip 171 kB) + 76 kB CSS(gzip 14 kB) |

選択の遅延(`mdpeek select-eval`、§11.3): rules p50 0.34 ms / p95 1.82 ms、Jev(実 API)p50 139.7 ms / p95 152.3 ms。UI では GET はネットワークを待たず(rules を `pending` で即時表示)、Jev の結果は WebSocket で後から差し替わる(実測: 選択 149–153 ms)。

注: 本文表示はベースラインの SSR より約 50 ms 遅い(React バンドルの解析)。100 ms 未満で体感上は即時だが、SSR 相当に戻すにはサーバ側の初回本文プリレンダが必要(後続)。

## 13. 使用したスキルと手順の記録

- **kenshu-loop**(`~/.claude/skills/kenshu-loop`, home-manager 管理): 発注 1(選択バックエンド)は 2 ラウンドで受入($35.67 / 47 分)、発注 2(Web UI)は 4 ラウンドで反復上限 → 1 ラウンド追加して 5 ラウンド目で受入($138.67 / 132 分)。受注 AI と別セッションの検収 AI が利用者試行と技術審査を行い、各ラウンドで実在の不具合(Callout 本文空、パレットの誤実行、シートの重なり、フォーカス不可視)を差し戻した。納品・受入記録は `docs/design/kenshu/`。git worktree では `.git` がファイルのため `init` が失敗する不具合があり、専用 clone で回避した(スキル側の修正候補)。
- **frontend-design**(公式 marketplace の plugin。未導入だったため `claude plugin install frontend-design@claude-plugins-official` で導入, v1.1.1): §6.2 の計画 → ブリーフ照合(カード積みや ALL CAPS ラベル等の既定を排除)→ 実装 → スクリーンショットでの批評(§11.6)の手順で使用。受注 AI のセッションでも利用可能だった。
- **ui-ux-pro-max**(`~/.claude/skills/ui-ux-pro-max`): `--design-system`(Minimalism & Swiss を採用、配色と Google Fonts は不採用、§6.2)と、keyboard / focus / line length / reduced motion / tap target の各ルール検索を設計と検収の基準に使用。
