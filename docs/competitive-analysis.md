# Markdownプレビュー/レンダリングツール市場 — 競合分析（mdpeek視点）

> 調査日: 2026-05-30 ／ 手法: deep-research ワークフロー（6観点に分解 → 並列Web検索 → 19ソース取得 → 94主張抽出 → 25主張を3票の敵対的検証 → 24件確定/1件棄却）。証拠の大半は一次情報（公式README・GitHub API・lib.rs）。

## 1. 市場構造 — 2つのカテゴリ

市場は明確に2分されており、mdpeek は両方にまたがる稀な位置にいる。

| カテゴリ | 主要プレイヤー | 特徴 |
|---|---|---|
| **ターミナル/CLIレンダラー** | Glow, mdcat, Frogmouth, rich-cli | 端末内に直接描画。ブラウザは使わない |
| **ブラウザ/ネイティブ ライブプレビュー** | grip, go-grip, markserv, vmd, md-preview, mdr, markdown-viewer | ファイル監視→ブラウザ/ウィンドウ表示。ライブリロード中心 |

**mdpeek は「ブラウザ・ライブプレビュー系」に属しつつ、`term` サブコマンドでターミナル系にも足を伸ばしている** — この両刀がポジショニング上の鍵。

## 2. 競合マッピング

### ターミナル/CLI系

| ツール | 言語 | スター | 状態 | 機能 | メモ |
|---|---|---|---|---|---|
| **Glow** | Go | **~25.5k** | 活発 | 端末描画のみ。**ライブリロード・ブラウザなし** | カテゴリの絶対王者。だが mdpeek と直接競合しない領域 |
| **mdcat** | Rust | ~2.4k | **アーカイブ済(2025-01-10、メンテ終了)** | CommonMark+syntect+インライン画像+クリック可能リンク | 蓄積された採用実績はあるが「生きた競合」ではない |
| **Frogmouth** | Python(Textual) | — | 活発 | TUIブラウザ。履歴/ブックマーク/TOC | 端末内ナビゲーション型 |
| **rich-cli** | Python | — | 活発 | 汎用ツールボックス(JSON/CSV/Jupyter/code等)。**ライブリロード・数式・Mermaidなし** | Markdownは多機能の一部にすぎない |

### ブラウザ/ネイティブ ライブプレビュー系

| ツール | 言語/基盤 | 出力先 | 描画方式 | 弱点 |
|---|---|---|---|---|
| **grip** | Python/Flask | ブラウザ | **GitHub API経由**(GFM完全一致、自動リフレッシュ) | **ネットワーク必須・APIレート制限** |
| **go-grip** | Go | ブラウザ(localhost:6419) | オフライン再実装、GFM+LaTeX数式+Mermaid、`--no-reload` | gripの正統な後継 |
| **markserv** | Node.js | ブラウザ | WebSocketホットリロード、ディレクトリ索引、プラグイン不要 | Node依存 |
| **vmd** | JS/**Electron** | 独立ウィンドウ | GitHub風描画、ファイル監視ライブプレビュー | Electronで重い |
| **md-preview** | **Rust/wry(システムWebView)** | ネイティブウィンドウ | CommonMark+GFM(pulldown-cmark)、オフラインKaTeX/Mermaid/40+言語ハイライト、~5MB、Chromium非同梱 | **強力な競合**だが出力はネイティブウィンドウ |
| **mdr** | **Rust** | egui/WebView/TUIの3バックエンド | フルGFM(表/タスクリスト/取り消し線/脚注/autolink)+syntect+Mermaid+自動TOC、300msデバウンスのライブリロード | **mdpeekの最も近い直接競合**。ただし出力は**ネイティブウィンドウ/TUIでブラウザタブではない**。v0.3.0(2026-05-20)、~448スター、自称"vibe coded"・pre-1.0 |
| **markdown-viewer** | ブラウザ拡張(Chrome/FF/Edge他) | ブラウザ | GFM/MathJax/Mermaid/Prism、1秒ポーリング自動リロード | CLIではない |

## 3. mdpeek の差別化余地（ギャップ）

検証で確定した事実から、mdpeek が狙える隙間は3つ:

1. **GLFM（GitLab Flavored Markdown）対応** — 調査した競合のうち**これを謳うものは1つも無い**。全ブラウザプレビュー系はGitHub風描画に偏っている。最も明確な独自性。
   - ⚠️ ただしこれは「調査対象内での不在」であり、網羅的市場スキャンではない（GitLab公式ツールやVS Code拡張は未調査）。

2. **「ブラウザライブプレビュー＋ターミナル描画」を1つのRustバイナリで提供** — この組み合わせを部分的にでも持つのは**mdrのみ**。しかしmdrの出力はネイティブウィンドウであり、**ブラウザタブで開けるのはmdpeekの独自性**。ブラウザ系ニッチには「単一バイナリで配れるRust製の決定版」がまだ存在しない。

3. **GitHub API非依存のオフラインGFM描画＋モダン機能（数式/Mermaid）** — 市場原典であるgripの最大の弱点（ネットワーク必須・レート制限）を解消できる。汎用ツールボックスのrich-cliはライブリロード・数式・Mermaidを全て欠いており、ここも明確な空白。

## 4. 戦略的示唆

- **Glow（25.5k）とは正面戦争しない** — 端末専用王者とは土俵が違う。mdpeekの`term`モードは「補完機能」であり、主戦場はブラウザライブプレビュー。
- **真の対抗馬は mdr と md-preview**（ともにRust製・軽量・ライブリロード）。差別化軸は「**ブラウザ出力 × GLFM × オフライン**」の3点セットに集約すべき。
- **ブラウザ系はNode/Electron/Pythonで断片化**しており、Rust単一バイナリのリーダーが不在 → mdpeekの参入余地は実在する。

## 5. 留意点

- **スター数は時点値**（Glow 25.5k / mdcat 2.4k / mdr ~448、2026-05-29〜30検証）で変動する。採用度の指標が**GitHubスターのみ**で、実DL数（cargo/npm/brew）は未取得 → 古い・露出の高いプロジェクトに有利なバイアスあり。
- 機能は各プロジェクトの**自己申告（README）**であり、独立ベンチマークではない。mdrは自称"vibe coded"・pre-1.0で品質は不確実。
- mdpeek自身の配布チャネルは現状 **cargo + プリビルドバイナリのみ**（Nix/NPM/brewはREADME上"Planned"）。

## 6. 未解決の問い（追加調査候補）

1. 実DL/インストール数（cargo downloads, npm downloads, Homebrew installs）— スターより実使用に近い指標。
2. 調査対象外（GitLab公式ツール・VS Code拡張等）のGLFM対応有無 — mdpeekの差別化根拠を揺るがしうる。
3. オフライン描画各社(go-grip/md-preview/mdr/mdpeek = pulldown-cmark系)とgrip(GitHub API)の描画忠実度の実差（alert/脚注/emoji等のエッジケース）。
4. mdpeek自身の現在スター数・リリース頻度・現実的な配布ロードマップ → 競合マップ上に定量配置するため。

## 付録: 主要ソース

- Glow: https://github.com/charmbracelet/glow
- mdcat: https://github.com/swsnr/mdcat
- Frogmouth: https://github.com/Textualize/frogmouth
- rich-cli: https://github.com/Textualize/rich-cli
- grip: https://github.com/joeyespo/grip
- go-grip: https://github.com/chrishrb/go-grip
- markserv: https://github.com/markserv/markserv
- vmd: https://github.com/yoshuawuyts/vmd
- md-preview: https://github.com/vorojar/md-preview
- mdr: https://lib.rs/crates/mdr ／ https://github.com/CleverCloud/mdr
- markdown-viewer: https://github.com/simov/markdown-viewer
