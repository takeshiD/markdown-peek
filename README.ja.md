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
- 🌐 ブラウザプレビュー — 出典にリンクした GenUI（**本文内に表示するチャート・ノードグラフ・比較カード**など）を 1 つのトグルでオン / オフ。GenUI は選んだモデルが決めます: **Jev**（推奨）、他の Decision Model（GPT-6 Luna Decision、Gemini Decision、Microsoft Decision）、通常の LLM（Claude、GPT、GLM、DeepSeek など）。生成中のスピナー、中止 / 再開、ディスクへの判断キャッシュ付き。ほかに、コマンドパレット（Ctrl/⌘K）、現在のブランチのファイルエクスプローラ、アウトラインと見出しのあいまい検索、front matter パネル、light / dark / system テーマ、キーボードだけでの操作
- 🖥️ 対話型ターミナルビューア（TUI） — スクロール、折り返し、vim 風の検索、ちらつきのないライブ更新
- 📝 GitHub Flavored Markdown（GFM）。GitLab Flavored Markdown（GLFM）は対応予定
- ⚙️ TOML（XDG）で簡単に設定

### 予定（ロードマップ）
mdpeek は、高機能な Markdown ビューアから、文書を理解して文書ごとに合った UI を描画するツールへと発展しています。今後のレイヤー:

- 🧠 **セマンティックビュー** — 文書種別を推定する文書モデルと、アウトライン / TODO / リスク / 未解決の問いのサイドパネル
- 🎛️ **生成 UI** — LLM に GenUI を選ばせるだけでなく、中身まで書かせる機能（[#58](https://github.com/takeshiD/markdown-peek/issues/58) で検討中。以前の `mdpeek gen` / `/api/gui` はそれまで削除）
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

![GenUI がスピナーとともに 1 つずつ入ってくる様子、本文内のチェックリストとステップの操作、ノードグラフで RETRY の行き先をたどる、比較カードを観点で絞る、このセッションだけモデルを切り替えてデフォルトにする、GenUI のオフ / オン](docs/design/media/adaptive-tour.gif)

画面は、上部バー（リポジトリ / ファイルのパンくず、**GenUI の操作**、コマンドパレット、テーマ）、左の **Files**、文書本体、右の **Outline** で構成されます。GenUI をオンにすると、文書の中に小さな widget（ステップ、チェックリスト、表、チャート、ノードグラフ、比較カード、ディレクトリツリー、リスク、アクションアイテムなど）が入ります。widget は文書から作ったもので、**抽出元の行の直後に本文の一部として**表示され、それぞれその行にリンクしています。表の下にチャート、リストの下にステップナビ、段階を順に説明する節の下に段階フロー、という配置です。**並べ替えできる表と操作できるチェックリストは、元の表 / タスクリストの位置にそれを置き換えて**表示します（同じ内容を 2 回出しません）。*Original* ボタンでその部分だけ Markdown の表示に戻せ、GenUI をオフにするとすべて元に戻ります。本文のコードブロックには**コピーボタン**があります。文書全体にかかる補助（役割別アウトライン、用語集など）は、冒頭のセクションの後に折り畳んで置きます。GenUI 用のサイドペインはありません。

GenUI の操作（左から順に）:

| 操作 | 内容 |
|---|---|
| **GenUI** トグル（`a`） | GenUI のオン / オフ。オフなら本文のみ。モデルが選んでいる間は、トグルに**スピナー**が出ます |
| **モデル**メニュー | 設定ファイルのモデル（`[models.<名前>]`）の一覧。ここで選んだモデルは**このセッションだけ**有効です。設定に問題があるモデルは理由付きで表示され、選べません |
| **Make default** | デフォルト以外のモデルを使っているときだけ表示されます。押すと、そのモデルを設定ファイルの `[model] use` に保存します（コメントやほかの設定はそのまま残ります） |
| 状態 | Choosing *n/N*、Ready、Stopped、Failed。ポップオーバーには、判断したモデル、送った内容、文書の種類、キャッシュの場所を表示します |
| **Stop** / **Resume** | 選んでいる途中で止めます（それまでに選ばれた補助は残ります）。Resume では、まだ答えのないセクションだけを問い合わせます（判断はセクションごとにキャッシュされます） |

設定に問題がある場合（キーの環境変数が未設定、設定ファイルにキーを直接書いた、未知の `api` など）は、問題の内容と設定ファイルの場所を**通知**で知らせます。

- **チャート**: 列が数値（`120 ms`、`$4.20`、`35%`、`1,200`）の表は棒グラフに、行が日付・月・四半期・バージョンの表は折れ線グラフになります。値はすべて表のセルを読み取ったもので、推定した値はありません。ツールチップとスクリーンリーダー向けの表には、セルの文字列をそのまま表示します。単位が混在する列、行番号 / ID の列、文字列の列はグラフにしません。数値の列が複数ある表は、切り替えスイッチで 1 つずつ表示します（2 軸のグラフにはしません）。折れ線グラフはフォーカスして ← / → で値を読めます。
- **ノードグラフ**: リストに矢印で書いた流れ（`Planning (Orchestrator) → Frontend, Backend`、`Verification --PASS--> Release`、`A -->|RETRY| B`）や、*from* / *to* 列を持つ表を、上から下へのグラフとして描きます。本流の後に書かれ、ループを閉じる辺（やり直しなど）は破線になります。グラフの下のチップ（またはノード自体）でノードを選ぶと、文書に書かれた説明（`- **名前**: …` または `名前 (補足)`）、どこから来てどこへ行くかを表示します。ラベル付きの出口を持つノードには切り替え（例: *PASS / RETRY*）が付き、その出口の行き先を強調します。
- **段階フロー**: 処理の段階を小見出しで順に説明している節（`### 3.1 parser — markdown -> BlockTree`、`### 3.2 analyzer — …`）に、段階を上から下へつないだフロー図を節の本文の後に差し込みます。見出しに入力と出力（`A -> B`）が書いてあれば矢印のラベルにし、各ノードから該当する小見出しへ移動できます。
- **ディレクトリツリー**: `├──` / `└──` / `│`（または `|--`、`` `-- ``）で構成を描いたコードブロックを、フォルダを開閉できるツリーとして表示します（矢印キー、Home / End、Enter で該当行へ移動）。`# コメント` と ★ の注記も表示します。
- **比較カード**: 2〜4 個の選択肢をスコア付きの観点（5 / 10 / 100 点満点の数値、★★★☆☆、◎ ○ △ ×）で比べた表を、選択肢ごとのカードにし、スコアをバーで表示します。それ以外の行（所要時間、価格など）はそのまま表示します。大きな数字は選択肢のスコアの単純平均で、平均であることを明記します。*Focus* で観点を 1 つ選ぶと、そのセルを表示して最も良い選択肢に印を付けます。**Recommended** バッジは、文書がどの選択肢を推奨しているかを書いている場合（例: `… Agent Swarm を推奨する`、見出しの `(推奨)`）に限って表示し、その文をカードの下に引用します。

- GenUI は本文の写しではなく、**操作できる**ものです: チェックリストやアクションアイテムにチェックを付ける（進捗メーター付き）、手順を 1 ステップずつ進める（Step n of N、前へ / 次へ）、表をソート・絞り込む。コードは各コードブロックのコピーボタンでコピーします（コマンドを実行することはありません）。チェックと現在のステップは文書ごとにこのブラウザ内にだけ保存されます。**Markdown ファイルは一切変更されません**。*Reset to file* でファイルの内容どおりの状態に戻せます。
- widget の行範囲（例: `L21–26`）をクリックすると、その行までスクロールして強調表示します。スクロールに合わせて、現在のセクションが Outline とその GenUI で強調されます。GenUI はタイトルから折り畳めます。
- widget の ⓘ ボタンで、**なぜ選ばれたか**を確認できます: モデル、confidence（確信度）、どれだけ役立つか（helps）、他の候補。
- モデルが新しく選んだ補助は、**少し行き過ぎて戻る動きで本文に入り**、薄い枠の光が消えていきます。何が追加されたかが分かります。キャッシュから再利用した補助は動かずに表示され、OS の「視差効果を減らす」設定では枠の光だけになります。
- GenUI を**固定（pin）**すると、タイトルの下の欄に移り、**読み進めても文書の上端に固定されて見え続けます**（高さに上限があり、欄の中でスクロールします）。**非表示（hide）**にすると通知に **Undo** が出ます。後からでも、本文の上の *n hidden* またはコマンドパレットから戻せます。どちらも文書ごとに記憶されます。
- 編集後にモデルが選び直している間も、前回の widget は表示されたままです。古いバージョンのファイルに対する結果が表示されることはありません。
- **Outline**（右）: 見出しのあいまい絞り込みと、スクロールに合わせた現在のセクションの強調。ヘッダーのボタンで文書の上に**フローティング**させられ（ヘッダーをドラッグ、またはグリップにフォーカスして矢印キーで移動）、サイドバーに**ドッキング**し直すこともできます。`]` で表示・非表示を切り替えます。
- 左右のサイドバーは、区切りをドラッグするか、区切りにフォーカスして ← / → を押すとサイズを変えられます。`[` で Files を折り畳めます。GenUI のオン / オフ、テーマ、サイズ、折り畳み状態、フローティング時の位置は再読み込み後も保たれます。
- **Files** には現在のブランチの Markdown が並びます。パンくずにはリポジトリ名とブランチ名を表示します。
- **Guided reading**（`g`）、*Follow live changes*（変更箇所に追従）、ショートカット一覧（`?`）はコマンドパレットから使えます。
- GenUI をオン / オフしたりサイドバーを折り畳んだりしても、文書内の読んでいる位置は保たれます。
- スマートフォン（狭い画面）では、GenUI を含んだ文書が画面いっぱいに表示され、Files / Outline はシートとして開きます。

**キーボードショートカット**（アプリ内では `?` で表示。1 文字のキーはテキスト入力欄にフォーカスがないときに使えます）:

| キー | 操作 |
|---|---|
| `Ctrl`+`K`（macOS では `⌘K`） | コマンドパレット: ファイルを開く、見出しへジャンプ、GenUI のオン / オフ、中止 / 再開、ほかのモデルを使う、それをデフォルトにする、テーマ、GenUI の固定 / 非表示 / 再表示、Guided reading、変更箇所への追従、キーボードショートカット |
| `a` | GenUI のオン / オフ |
| `[` | Files の折り畳み・展開 |
| `]` | Outline の表示・非表示（ドッキング / フローティングとも） |
| `/` | Outline の見出しを絞り込む |
| `g` | Guided reading |
| `t` | 次のテーマ（light → dark → system） |
| `?` | ショートカット一覧 |
| フォーカスした区切りで `←` / `→` | ペインのサイズ変更 |
| `Esc` | ダイアログ、シート、ポップオーバーを閉じる |

GenUI は**モデル**が選びます（既定は Jev。[GenUI とモデル](#genui-とモデル)を参照）。GenUI がオンのとき、`mdpeek` は選んだモデルに見出し、セクションごとの短い抜粋、数値の特徴量を送ります。ファイル全体は送りません。外に出してはいけない文書では、GenUI をオフにしてください（`[genui] default = "off"` で最初からオフにもできます）。モデルが答えられない場合、GenUI は表示されず、UI がその理由を示します。

### Web UI のビルド

UI は `web/` にある React 19 + Tailwind CSS v4 + shadcn/ui のアプリで、`web/dist/app.{js,css}` にビルドされ、バイナリに埋め込まれます（ビルド済みファイルはコミットされているので、`cargo install` に Node は不要です）。`web/src` を変更したら:

```sh
cd web && npm ci && npm run build     # web/dist を再ビルド（再現性あり: scripts/check-web-dist.sh）
npm run test:e2e                      # 実際の mdpeek に対する Playwright E2E（テスト専用の fake モデル、キー不要、ネットワーク不要）
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

## GenUI とモデル

GenUI のコンポーネントの一覧（元の Markdown と画像、操作するものは GIF）は [docs/components.md](docs/components.md) にあります。

`mdpeek serve` は、モデルが選んだ GenUI を表示します。`mdpeek select <file>` は同じ選択を **AdaptiveLayout** JSON として出力します。JSON には、セクションごとにどの widget（ステップナビ、チェックリスト、表ビュー、チャート、ノードグラフ、段階フロー、比較カード、ディレクトリツリー、タイムライン、リスク callout など）を表示するか、または表示しない理由が入っています。

**モデルは選ぶだけで、内容は書きません。** widget はすべて文書から決定的に抽出したもので、抽出元の行範囲を持ちます。モデルは、各セクションに当てはまる候補の中から選び、どれだけ役立つかを答えるだけです。

### モデルの種類

| `api` | 種類 | 例 | エンドポイント（`base_url` + …） | キーのヘッダー |
|---|---|---|---|---|
| `systemone` | Decision Model | **TypeSafe Jev（推奨）**、GPT-6 Luna Decision、Gemini Decision 1、Microsoft Decision | `/v1/systemone` | `Authorization: Bearer`（Azure では `auth = "api-key"`） |
| `anthropic` | LLM | Claude Haiku / Sonnet / Opus | `/v1/messages` | `x-api-key`（OpenCode 経由では `auth = "bearer"`） |
| `openai` | LLM | GPT-6 Luna / Sol（Responses API） | `/v1/responses` | `Authorization: Bearer` |
| `openai_chat` | LLM | GLM、DeepSeek、Kimi など（OpenAI 互換） | `/v1/chat/completions` | `Authorization: Bearer` |
| `claude_code` / `codex` | LLM | このマシンでサインイン済みの `claude` / `codex` CLI | – | – |

Decision Model は質問に直接答えます（yes / no の質問には確率、選択の質問には選んだ候補と確信度）。LLM には同じ質問を JSON のプロンプトとして渡し、同じ形の JSON で答えてもらいます。JSON のない文章だけの回答は不正な回答として扱います。LLM に GenUI の中身まで書かせる機能はありません（issue #58 で検討中）。

### 設定

```toml
[genui]
default = "on"           # トグルの初期状態: "on" | "off"
auto_generate = true     # 文書を開いたら選び始める。false なら利用者が開始するまで待つ

[model]
use = "jev"              # デフォルトのモデル（[models] のキー）
timeout_ms = 30000       # 1 リクエストの上限時間
batch_sections = 8       # 1 リクエストで問い合わせるセクション数（中止 / 再開の単位）
excerpt_chars = 600      # セクションごとにモデルへ送る抜粋の上限
max_state_chars = 24000  # 1 リクエストで送る総量の上限
helps_threshold = 0.5    # モデルが「役立たない」と見た補助を隠す…
confidence_threshold = 0.6  # …または自信のない補助を隠す
debounce_ms = 800        # serve: 編集後、選び直すまでの待ち時間

[models.jev]                       # TypeSafe に直接
enable = true                      # false: エントリを残したまま無効にする
api = "systemone"
model = "jev-1.14.0"
api_key_env = "TYPESAFE_API_KEY"   # 環境変数の「名前」。キーそのものは書かない

# OpenCode 経由（以下のモデルは 1 つのキーで使える）
[models.gpt-luna]
api = "systemone"
model = "gpt-6-luna-decision"
base_url = "https://opencode.ai/inference/systemone"
api_key_env = "OPENCODE_API_KEY"

[models.gemini]
api = "systemone"
model = "gemini-decision-1"      # OpenCode には無料の "jev-1.13-free" もある
base_url = "https://opencode.ai/inference/systemone"
api_key_env = "OPENCODE_API_KEY"

[models.haiku]
api = "anthropic"
model = "claude-haiku-5-5"
base_url = "https://opencode.ai/inference/anthropic"
api_key_env = "OPENCODE_API_KEY"
auth = "bearer"

[models.glm]
api = "openai_chat"
model = "glm-5.3"
base_url = "https://opencode.ai/inference/openai"
api_key_env = "OPENCODE_API_KEY"

# Azure Foundry の Microsoft Decision（`model` にはデプロイ名）
[models.microsoft]
api = "systemone"
model = "<デプロイ名>"
base_url = "https://<リソース名>.services.ai.azure.com/providers/microsoft"
api_key_env = "AZURE_API_KEY"
auth = "api-key"

[cache]
enabled = true
dir = "~/.cache/mdpeek"  # 既定: $XDG_CACHE_HOME/mdpeek、未設定なら ~/.cache/mdpeek
max_size_mb = 100        # これを超えたら古いものから削除
ttl_days = 30            # これより古い判断は使わない
memory_entries = 256     # ディスクの前に置くプロセス内キャッシュの件数
```

`[models]` がない場合は、`jev`（TypeSafe、`TYPESAFE_API_KEY`）だけを使います。各エントリには `effort`（`low` 〜 `max`。Anthropic の `output_config.effort`、OpenAI の `reasoning.effort` になります）と `kind`（`decision` / `llm`。`api` と矛盾しないかを確認します）も書けます。
`enable = false` を書くと、エントリを消さずに無効にできます。モデルメニューに出なくなり、呼び出されることもありません。`[model] use` が無効なエントリを指している場合は最初の有効なエントリを使い、そのことを画面に表示します。すべて無効にすると、どのモデルも呼び出しません。

**キー。** 各エントリには、キーを入れた環境変数の名前（`api_key_env`）を書きます。キーはリクエストを送るときにその環境変数から読み、表示・ログ出力・キャッシュ・ブラウザへの返却は一切しません。設定ファイルにキーそのものを書くと（`api_key = …`）、そのエントリは使えなくなります。`http://` は `localhost` 以外では使えません。

**送る内容。** 見出し、セクションごとの抜粋（`excerpt_chars` 以内）、数値の特徴量です。ファイル全体は送りません。

**キャッシュ。** 判断はセクションごとに（メモリ、次にディスクへ）保存されます。キーは、セクションの内容のハッシュ、モデル（api、エンドポイント、id、effort）、候補の一覧です。変わっていないセクションを 2 回問い合わせることはなく、中止した実行は止めたところから再開できます。ディスクに書くのは候補の id と数値だけで（ファイル名は SHA-256 のダイジェスト）、文書の文章・見出し・キーは書きません。`[cache] enabled = false` にすると、メモリだけに保存します。

**失敗したとき。** 使えないモデル（設定の誤り、キーの環境変数が未設定、401 / 402 / 404、CLI がない）では GenUI を表示せず、理由と設定ファイルの場所を UI が通知します。タイムアウト、429 / 529（`retry-after` に従って 1 回だけ再試行）、5xx、不正な回答、ネットワーク不通では、実行がエラーで終わり、再試行できます。

```sh
mdpeek select README.md                     # デフォルトのモデル（[model] use）
mdpeek select README.md --model gemini      # ほかのエントリ
mdpeek serve README.md --model haiku        # このセッションだけほかのモデルを使う
mdpeek select-eval tests/fixtures/select --model jev --model haiku   # 正解率 / none / p50-p95
```

`mdpeek select` は判断をメモリにだけ保存するので、実行するたびにモデルへ問い合わせます。

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
