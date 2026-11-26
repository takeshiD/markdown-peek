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
- Web UI を Tailwind CSS + shadcn/ui(Preact 上)で作り直し、Read / Split / Adaptive の 3 モードで落ち着いた高密度 UI を提供する。
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
            Web (Preact registry) / TUI(要約 fallback)
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
    ]
  },
  "instructions": "You choose reading aids for a Markdown viewer. Never assume facts beyond the given excerpts.",
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

### 6.1 方式決定: Tailwind + shadcn/ui を Preact 上で
- **Tailwind CSS v4**(`@tailwindcss/vite`、ビルド時に静的 CSS を生成 → `web/dist` に embed。ランタイム/CDN 依存なし)。
- **shadcn/ui はソース取り込み**(`components/ui/*`)。Radix 依存は `preact/compat` alias(`react`, `react-dom`, `react/jsx-runtime`)で動かす。**互換性を実測**(Dialog/Command/Tabs/Tooltip/DropdownMenu/Resizable のキーボード操作・フォーカストラップ・portal)し、問題があれば React 移行とのコスト比較を §11 に記録して最小変更を選ぶ。
- アイコンは `lucide`(Preact 版 `lucide-preact` もしくは shadcn 標準の `lucide-react` を compat 経由)に統一。インライン SVG の手書きは廃止。
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
2. 発注 2(kenshu-loop): Web UI — Tailwind + shadcn(Preact)、3 モード、explorer/outline、palette、source jump、why、pin/hide、永続化、既存機能の移植。受入: Playwright(desktop/mobile, light/dark, keyboard, offline, live edit, diff, explorer)+ スクリーンショット + 独立レビュー。
3. 統合・検収: README / config.example 更新、ベンチ(バンドルサイズ、選択遅延)、PR。

## 9. json-render を導入しない理由
既存の Rust 正本 IR(serde tag + allowlist + sourceRange 検証)が json-render の担う「カタログ制約付きツリー」を既に満たし、Rust 側で検証できる。json-render は JS 側スキーマ(Zod)を正本にするため、検証境界が二重化する。Jev 統合部分も公開コードが無い(第三者記事)。

## 10. 未決事項 / ブロッカー
- 非 enterprise のデータ保持(DPA)未確認 → README に「外部送信は opt-in、機密文書は rules」を明記。
- MathJax の CDN 依存(既存)。オフライン数式は KaTeX/MathJax embed の後続 issue。
- 既存 `/api/gui` のディスクキャッシュ(論点 E 不整合)は後続 issue。

## 11. 実装記録(kenshu-loop の結果をここに追記)
