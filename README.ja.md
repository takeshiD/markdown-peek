# Markdown Peek

[English](README.md) | 日本語

Markdown Peek（`mdpeek`）は、リポジトリを理解する軽量な CLI ツールです。Markdown の変更を監視し、ブラウザまたは対話型のターミナルビューアにライブで表示します。

- [機能](#機能)
- [クイックスタート](#クイックスタート)
- [インストール](#インストール)
- [設定](#設定)
- [対応状況](#対応状況)
- [ライセンス](#ライセンス)

# 機能

### 現在使えるもの
- ⚡ 高速 — すべてのアセットを埋め込んだ単一バイナリ
- 🔄 その場でのライブ更新 — 変更したブロックだけを再描画し（ページ全体は再読み込みしない）、スクロール位置を保ち、変更箇所を一時的に強調表示
- 🌐 ブラウザプレビュー — Read / Adaptive の 2 モード、出典にリンクした読書補助と、**本文内に表示するチャート・ノードグラフ・比較カード**、コマンドパレット（Ctrl/⌘K）、現在のブランチのファイルエクスプローラ、アウトラインと見出しのあいまい検索、front matter パネル、light / dark / system テーマ、キーボードだけでの操作
- 🖥️ 対話型ターミナルビューア（TUI） — スクロール、折り返し、vim 風の検索、ちらつきのないライブ更新
- 📝 GitHub Flavored Markdown（GFM）。GitLab Flavored Markdown（GLFM）は対応予定
- ⚙️ TOML（XDG）で簡単に設定

### 予定（ロードマップ）
mdpeek は、高機能な Markdown ビューアから、文書を理解して文書ごとに合った UI を描画するツールへと発展しています。今後のレイヤー:

- 🧠 **セマンティックビュー** — 文書種別を推定する文書モデルと、アウトライン / TODO / リスク / 未解決の問いのサイドパネル
- 🎛️ **生成 UI** — 文書種別（設計書、README、ADR、議事録、手順書、changelog など）ごとに生成される UI。まずルールで、次に Claude（LLM）で生成
- 📚 **開発文書以外の文書種別** — 小説・契約書・製造 / 手順書向けのドメイン widget（登場人物一覧、公差メーター、ステップナビ、義務マトリクス、用語集）とネタバレ制御
- 🔗 **リポジトリと連携した検査** — README ↔ コードのリンク、ドキュメントとコードの整合性、ADR ↔ git 履歴、TODO ↔ issue の相互参照
- 🧩 **エディタ / ツール統合** — IR 対応の TUI レンダラ、Neovim プラグイン、GitHub プレビュー連携


# クイックスタート

## ブラウザでプレビュー
引数なしで `mdpeek` を実行します。現在のブランチの作業ツリーにある Markdown を見つけ（他の worktree は一覧に出しません）、ファイルエクスプローラ付きのブラウザプレビューを開きます:
```sh
mdpeek
 INFO mdpeek::server: Listening on http://127.0.0.1:3030
```
単一のファイルを指定することもできます: `mdpeek serve path/to/file.md`

編集すると、プレビューは**その場で**更新されます。変更されたブロックだけが再描画され（読んでいる位置は保たれます）、一時的に印が付きます。

### ブラウザ UI

![Adaptive モード: 本文内のチェックリストとステップの操作、ノードグラフで RETRY の行き先をたどる、アウトラインをフローティングにしてドラッグ、比較カードを観点で絞る、棒グラフの指標切り替え、折れ線グラフの読み取り、Read モードへの切り替え](docs/design/media/adaptive-tour.gif)

画面は、上部バー（リポジトリ / ファイルのパンくず、選択器の状態、表示モード、**View** メニュー、コマンドパレット、テーマ）、左の **Files**、文書本体、右の **Outline** で構成されます。Adaptive モードでは、文書の中に*読書補助*が入ります。読書補助は文書から作った小さな widget（ステップ、チェックリスト、コマンド、チャート、ノードグラフ、比較カード、リスク、アクションアイテムなど）で、**抽出元の行の直後に本文の一部として**表示され、それぞれその行にリンクしています。読書補助用のサイドペインはありません。

| モード | キー | 表示内容 |
|---|---|---|
| **Read** | `1` | 本文のみ |
| **Adaptive** | `2` | 読書補助を含んだ本文。表の下にチャート、リストの下にステップナビ、コードの下にコピーできるコマンド。文書全体にかかる補助（役割別アウトライン、用語集など）は、冒頭のセクションの後に折り畳んで置きます |

- **チャート**: 列が数値（`120 ms`、`$4.20`、`35%`、`1,200`）の表は棒グラフに、行が日付・月・四半期・バージョンの表は折れ線グラフになります。値はすべて表のセルを読み取ったもので、推定した値はありません。ツールチップとスクリーンリーダー向けの表には、セルの文字列をそのまま表示します。単位が混在する列、行番号 / ID の列、文字列の列はグラフにしません。数値の列が複数ある表は、切り替えスイッチで 1 つずつ表示します（2 軸のグラフにはしません）。折れ線グラフはフォーカスして ← / → で値を読めます。
- **ノードグラフ**: リストに矢印で書いた流れ（`Planning (Orchestrator) → Frontend, Backend`、`Verification --PASS--> Release`、`A -->|RETRY| B`）や、*from* / *to* 列を持つ表を、上から下へのグラフとして描きます。本流の後に書かれ、ループを閉じる辺（やり直しなど）は破線になります。グラフの下のチップ（またはノード自体）でノードを選ぶと、文書に書かれた説明（`- **名前**: …` または `名前 (補足)`）、どこから来てどこへ行くかを表示します。ラベル付きの出口を持つノードには切り替え（例: *PASS / RETRY*）が付き、その出口の行き先を強調します。
- **比較カード**: 2〜4 個の選択肢をスコア付きの観点（5 / 10 / 100 点満点の数値、★★★☆☆、◎ ○ △ ×）で比べた表を、選択肢ごとのカードにし、スコアをバーで表示します。それ以外の行（所要時間、価格など）はそのまま表示します。大きな数字は選択肢のスコアの単純平均で、平均であることを明記します。*Focus* で観点を 1 つ選ぶと、そのセルを表示して最も良い選択肢に印を付けます。**Recommended** バッジは、文書がどの選択肢を推奨しているかを書いている場合（例: `… Agent Swarm を推奨する`、見出しの `(推奨)`）に限って表示し、その文をカードの下に引用します。

- 読書補助は本文の写しではなく、**操作できる**ものです: チェックリストやアクションアイテムにチェックを付ける（進捗メーター付き）、手順を 1 ステップずつ進める（Step n of N、前へ / 次へ）、コマンドを 1 行ずつまたはまとめてコピーする（コマンドを実行することはありません）、表をソート・絞り込む。チェックと現在のステップは文書ごとにこのブラウザ内にだけ保存されます。**Markdown ファイルは一切変更されません**。*Reset to file* でファイルの内容どおりの状態に戻せます。
- widget の行範囲（例: `L21–26`）をクリックすると、その行までスクロールして強調表示します。スクロールに合わせて、現在のセクションが Outline とその読書補助で強調されます。読書補助はタイトルから折り畳めます。
- widget の ⓘ ボタンで、**なぜ選ばれたか**を確認できます: 選択器とモデル、confidence（確信度）、どれだけ役立つか（helps）、他の候補、fallback の有無。上部バーのドットは、選択器の状態（rules / jev / fake、選択中、fallback、エラー）を常に示します。
- 読書補助を**固定（pin）**すると本文の先頭（タイトルの下）に移り、**非表示（hide）**にすると隠れます。どちらも文書ごとに記憶されます。隠したものは、本文の上の *n hidden* またはコマンドパレットから戻せます。
- 編集後に新しい選択が実行されている間も、前回の widget は表示されたままです。古いバージョンのファイルに対する結果が表示されることはありません。
- **Outline**（右）: 見出しのあいまい絞り込みと、スクロールに合わせた現在のセクションの強調。ヘッダーのボタンで文書の上に**フローティング**させられ（ヘッダーをドラッグ、またはグリップにフォーカスして矢印キーで移動）、サイドバーに**ドッキング**し直すこともできます。`]` で表示・非表示を切り替えます。
- 左右のサイドバーは、区切りをドラッグするか、区切りにフォーカスして ← / → を押すとサイズを変えられます。`[` で Files を折り畳めます。モード、テーマ、サイズ、折り畳み状態、フローティング時の位置は再読み込み後も保たれます。
- **Files** には現在のブランチの Markdown が並びます。パンくずにはリポジトリ名とブランチ名を表示します。
- **View** メニューには、**Guided reading**（`g`）、*Follow live changes*（変更箇所に追従）、ショートカット一覧があります。
- モードを切り替えたりサイドバーを折り畳んだりしても、文書内の読んでいる位置は保たれます。
- スマートフォン（狭い画面）では、読書補助を含んだ文書が画面いっぱいに表示され、Files / Outline はシートとして開きます。

**キーボードショートカット**（アプリ内では `?` で表示。1 文字のキーはテキスト入力欄にフォーカスがないときに使えます）:

| キー | 操作 |
|---|---|
| `Ctrl`+`K`（macOS では `⌘K`） | コマンドパレット: ファイルを開く、見出しへジャンプ、モードやテーマの切り替え、読書補助の選び直し、読書補助の固定 / 非表示 / 再表示 |
| `1` / `2` | Read / Adaptive |
| `[` | Files の折り畳み・展開 |
| `]` | Outline の表示・非表示（ドッキング / フローティングとも） |
| `/` | Outline の見出しを絞り込む |
| `g` | Guided reading |
| `t` | 次のテーマ（light → dark → system） |
| `?` | ショートカット一覧 |
| フォーカスした区切りで `←` / `→` | ペインのサイズ変更 |
| `Esc` | ダイアログ、シート、ポップオーバーを閉じる |

既定では、読書補助は `rules` によってオフラインで選ばれます。**Jev はオプトイン**です: `[select]` で `provider = "jev"` **と** `allow_network = true` を設定し、`TYPESAFE_API_KEY` を export してください（[適応型の読書補助](#適応型の読書補助-select)を参照）。そうしない限り、何も外部へ送信されません。Jev が使えない場合は、UI がその旨（fallback）を表示し、ルールによる選択を表示します。

### Web UI のビルド

UI は `web/` にある React 19 + Tailwind CSS v4 + shadcn/ui のアプリで、`web/dist/app.{js,css}` にビルドされ、バイナリに埋め込まれます（ビルド済みファイルはコミットされているので、`cargo install` に Node は不要です）。`web/src` を変更したら:

```sh
cd web && npm ci && npm run build     # web/dist を再ビルド（再現性あり: scripts/check-web-dist.sh）
npm run test:e2e                      # 実際の mdpeek に対する Playwright E2E（fake 選択器、キー不要、ネットワーク不要）
```

E2E はシステムの Chrome を使います（パスは `MDPEEK_CHROME` で上書き可能。Playwright 自身のブラウザダウンロードは使いません）。また、`docs/design/screenshots/` の参照スクリーンショットも更新します。

## ターミナルでプレビュー
`term` サブコマンドを使います。TTY 上では、ファイルの変更に合わせてライブ更新する対話型の全画面ビューアが開きます:
```sh
mdpeek term             # 対話型ビューア（TTY では既定で監視）
mdpeek term --no-watch  # 一度だけ描画して終了（パイプ時も既定でこちら）
```

### ターミナルビューアのキー操作
| キー                 | 操作                        |
| -----                | --------                    |
| `q` / `Ctrl-c`       | 終了                        |
| `j` / `k`、`↓` / `↑` | 1 行スクロール              |
| `Ctrl-d` / `Ctrl-u`  | 半ページ下 / 上             |
| `PgDn` / `PgUp`      | 1 ページ下 / 上             |
| `g` / `G`            | 先頭 / 末尾へ移動           |
| `/`                  | 検索                        |
| `n` / `N`            | 次 / 前の一致               |
| `Esc`                | 検索を解除                  |
| `?`                  | キー操作のヘルプを切り替え  |

# インストール
## `cargo`
```
cargo install markdown-peek
```

## `nix`（予定）
```
nix-shell -p markdown-peek --command mdpeek
```

## `npm`（予定）
```
npm install -g markdown-peek
```

## ビルド済みバイナリのダウンロード
```
curl -SL https://github.com/takeshid/markdown-peek
```


# 設定
`mdpeek` は、[XDG Base Directory](https://specifications.freedesktop.org/basedir-spec/latest/) 仕様に従って、任意の設定ファイルを読み込みます:

```
$XDG_CONFIG_HOME/mdpeek/config.toml
```

`XDG_CONFIG_HOME` が未設定の場合は `~/.config/mdpeek/config.toml` を使います。ファイルは任意で、書かれていないキーには組み込みの既定値が使われます。設定は次の優先順位で決まります:

```
CLI 引数  >  設定ファイル  >  組み込みの既定値
```

`-c` / `--config` で別の設定ファイルを読み込めます（既定の XDG の場所より優先されます）:

```sh
mdpeek --config ./my-config.toml
mdpeek -c ./my-config.toml term README.md
```

## オプション

| キー | 値 | 既定値 | 説明 |
|-----|--------|---------|-------------|
| `default_mode` | `serve` \| `term` | 自動（TTY なら `serve`、それ以外は `term`） | サブコマンドなしで `mdpeek` を実行したときのモード |
| `server.host` | IP 文字列 | `127.0.0.1` | ブラウザプレビューが待ち受けるアドレス |
| `server.port` | ポート文字列 | `3030` | ブラウザプレビューが待ち受けるポート |
| `server.theme` | `light` \| `dark` | `light` | ブラウザプレビューの既定テーマ |
| `term.theme` | `glow` \| `mono` \| `catputtin` \| `dracura` \| `solarized` \| `nord` \| `ayu` | `glow` | ターミナルの既定の配色テーマ |
| `term.pager` | コマンド文字列 | `$PAGER`、なければ `less -R` | 長いターミナル出力用のページャ。`""` でページングを無効化 |

## 例
コメント付きの完全な例は [`config.example.toml`](config.example.toml) を参照してください。始めるには:

```sh
mkdir -p ~/.config/mdpeek
cp config.example.toml ~/.config/mdpeek/config.toml
```

```toml
default_mode = "serve"

[server]
host = "127.0.0.1"
port = "3030"
theme = "light"

[term]
theme = "glow"
pager = "less -R"
```

## 適応型の読書補助 (`[select]`)

`mdpeek select <file>` は **AdaptiveLayout** JSON を出力します。セクションごとに、どの読書補助 widget（ステップナビ、チェックリスト、表ビュー、チャート、ノードグラフ、比較カード、コマンド一覧、タイムライン、リスク callout など）を表示するか、または表示しない理由（`skipped`）が入っています。`mdpeek serve` は同じレイアウトを `GET /api/adaptive` で返し、ファイルが変わるたびに WebSocket で `{"type":"adaptive","revision":N,"layout":{…}}` を送ります（debounce あり、既定 800 ms。古い revision の結果は破棄され、変更のないセクションはメモリ内の判断を再利用します。ディスクには何も書き込みません）。

widget の*内容*は常に文書から決定的に抽出され、ソース行の範囲を持ちます。選択器は、そのセクションにすでに適合している候補の中から選ぶだけです。

| キー（`[select]`） | 既定値 | 説明 |
|---|---|---|
| `provider` | `rules` | `rules`（オフライン）、`jev`（TypeSafe AI の Jev）、`fake`（台本どおりに答える。テスト / E2E 用） |
| `allow_network` | `false` | `jev` が送信するには `true` が必要 |
| `model` / `base_url` | `jev-1.14.0` / `https://api.typesafe.ai` | Jev のモデルとエンドポイント |
| `timeout_ms` | `4000` | リクエスト全体の期限（429 / 529 は `retry-after` を尊重して最大 1 回再試行） |
| `helps_threshold` / `confidence_threshold` | `0.5` / `0.6` | Jev が役に立たないと判断した widget / 確信が持てない widget を隠す |
| `debounce_ms` | `800` | `serve`: 変更後、選び直すまでの待ち時間 |
| `fake_script` | – | `provider = "fake"` 用の JSON 台本 |

**外部送信はオプトインです。** `provider = "jev"` **かつ** `allow_network = true`（または `mdpeek select --provider jev --allow-network`）でない限り、何も外部へ送信されません。送信する場合、Jev には文書ごとに 1 回、見出し、セクションごとの短い抜粋（600 文字以内）、数値の特徴量を送ります。全文は送りません。機密文書は `rules` のままにしてください。

**API キーは、`mdpeek` プロセスの環境変数 `TYPESAFE_API_KEY` からのみ読み込みます。** `config.toml` やコマンドラインでは設定できず、表示・ログ出力・ブラウザへの返却も一切しません。キーがない場合、またはタイムアウト、401、429 / 529、5xx、不正な応答、ネットワーク不通の場合、`mdpeek` は `rules` にフォールバックし、その理由を `selector.fallback` に記録します。

```sh
mdpeek select README.md                                  # rules、オフライン
TYPESAFE_API_KEY=… mdpeek select README.md --provider jev --allow-network
mdpeek select README.md --provider fake --fake-script tests/fake-select.json   # 台本どおり
mdpeek select-eval tests/fixtures/select                 # 一致率 / none の適切さ / p50・p95
```

### fake 選択器の台本（`provider = "fake"`）

キーなしでテストや Web E2E を行うために、`fake` は JSON の台本に従って答えます（例: [`tests/fake-select.json`](tests/fake-select.json)）。ネットワークには一切接続せず、同じ文書には毎回同じ答えを返します。

```json
{
  "model": "fake-e2e",
  "delay_ms": 0,
  "fail": null,
  "doc_type": { "value": "runbook", "confidence": 0.95 },
  "document": { "widget": "ActionItems", "confidence": 0.9, "helps": 0.9 },
  "sections": {
    "Steps": { "widget": "StepNavigator", "confidence": 0.9, "helps": 0.9 },
    "Commands": { "helps": 0.1 }
  },
  "default": "rules"
}
```

- `sections` のキーはセクションの見出し（または位置による id `s0`、`s1`、…）です。各エントリには任意で `widget`（`StepNavigator`、`ConfigViewer:commands`、`DataTable:status`、`Callout:risk` などの候補 id。省略するとルールの選択）、`helps`、`confidence`（0〜1、既定 1）を指定できます。通常の閾値が適用されるので、`helps` / `confidence` が低いと widget は隠れます。そのセクションの候補にない id はルールにフォールバックします（`fallback_invalid_answer`）。
- `document`: 文書レベルのパネル（`SemanticOutline`、`ActionItems`、`RiskPanel` など）。
- `default`: 台本にないセクションの扱い。`"rules"`（既定）、`"missing"`（不正な回答として扱う）、またはエントリのオブジェクト。
- `fail`: すべての呼び出しを、指定した fallback コード（例: `"timeout"`）で失敗させます。`delay_ms`: 疑似的な遅延（`timeout_ms` を超えると `timeout`）。
- 台本がない・壊れている場合は `selector.fallback = "fake_script_error"` となり、理由（I/O エラーの種類または JSON エラーの位置）を stderr に出力します。


# 対応状況
## ビューア
- [x] 変更ブロックを強調表示する、その場でのライブ更新
- [x] 現在のブランチの作業ツリーを表示するファイルエクスプローラ
- [x] リポジトリ名とブランチ名を示すパンくず
- [x] 見出しのあいまい検索付きアウトラインパネル
- [x] front matter パネル
- [x] 対話型ターミナルビューア（スクロール、折り返し、vim 風の検索、ライブ更新）

## [GFM](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax)
- [x] [表](https://github.github.com/gfm/#tables-extension-)
- [x] [タスクリスト](https://github.github.com/gfm/#task-list-items-extension-)
- [x] [取り消し線](https://github.github.com/gfm/#strikethrough-extension-)
- [x] [フェンスコードブロック](https://github.github.com/gfm/#fenced-code-blocks)
- [x] シンタックスハイライト
- [x] [絵文字](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#using-emojis)
- [x] [アラート](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#alerts)
- [x] MathJax
- [x] [カラーモデル](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#supported-color-models)
- [x] 自動リンク
- [x] ページ内リンク
- [x] 脚注
- [x] 目次
- [x] テーマ切り替え（ライト / ダーク）

## [GLFM（予定）](https://docs.gitlab.com/user/markdown/)
- [ ] 表
- [ ] 取り消し線
- [ ] タスクリスト
- [ ] フェンスコードブロック
- [ ] シンタックスハイライト
- [ ] 自動リンク
- [ ] 絵文字
- [ ] [アラート](https://docs.gitlab.com/user/markdown/#alerts)
- [ ] 数式

## ターミナル
- [x] [表](https://github.github.com/gfm/#tables-extension-)
- [x] [タスクリスト](https://github.github.com/gfm/#task-list-items-extension-)
- [x] [取り消し線](https://github.github.com/gfm/#strikethrough-extension-)
- [x] [フェンスコードブロック](https://github.github.com/gfm/#fenced-code-blocks)
- [x] シンタックスハイライト
- [x] [絵文字](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#using-emojis)
- [x] [アラート](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#alerts)
- [x] 数式
- [x] [カラーモデル](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#supported-color-models)
- [x] 脚注

# ライセンス
MIT License。
[LICENSE](LICENSE)
