# 納品・受入記録: markdown-peek: Tailwind + shadcn/ui による Adaptive Markdown viewer UI

- 受入日時: 2026-11-26T20:52:27.545Z
- 要求版: 45442db605dedd3f
- 成果物 fingerprint: 46f186df071110f98e51fcb9
- 基準版: 9a19b23d15c42740
- 検収回数: 5

## 使い始め方

準備:

    npm ci --prefix web --no-audit --no-fund
    sh scripts/cargo-isolated-home.sh build --bin mdpeek

- **serve** (cli): `target/debug/mdpeek serve web/e2e/docs/deploy-guide.md` — Starts the browser UI; open the printed http://127.0.0.1:PORT in a browser. Add `-c config.toml` with [select] provider = "fake" / "jev" to choose the selector.
- **e2e** (cli): `npm --prefix web run test:e2e` — One command for the whole Playwright E2E suite: builds web/dist and the binary, runs 42 tests with the fake selector (no key, no network) on the system Chrome, and refreshes docs/design/screenshots/.

## 検収判定

合格（accepted）と判定する。前回差し戻した R12 の不適合（レールの Outline/Files tabpanel でフォーカスが見えない）は、tabs.tsx の focus-visible ring で修正されている。変更された C7 は、ページ読み込みから Tab して tabpanel まで通るので、この欠陥を検出できる。C7 と C12 の変更理由はどちらも妥当。オーケストレータの再実行では C1〜C8 と C10〜C14 がすべて pass した。C9 のデザインレビューも §6.2 に沿っている。私の利用者試行 S1〜S14 では、R1〜R15 を実際の操作で確認した。ライブ更新と古い revision の破棄、diff、axe（light/dark）、dist の再現、1 コマンドでの E2E も含む。軽微な残件は follow_ups に回す。

- R1: satisfied — C1 と C13 は再実行で pass。試行 S1 では、選択を 2.5 秒遅らせても本文が約 330ms で表示され、上部バー・左レール・本文・widget 領域がそろっていた。
- R2: satisfied — C1 は pass。試行 S2 で 3 つのモードを、キー・画面上の切替・パレットのどれでも切り替えられ、再読み込み後もモードが保たれた。Adaptive ではセクションの横に、390px 幅ではセクションの直後に widget が並んだ。
- R3: satisfied — C1 は pass。試行 S3 でマウスのドラッグと矢印キーで幅が変わり、[ と ] で折り畳め、幅と折り畳み状態が再読み込み後も保たれた。
- R4: satisfied — C2 は pass。elementFromPoint で他の要素に覆われていないことと、要素同士が重ならないことも検査している。試行 S4 では横スクロールが無く、シートで開閉でき、シートとメニューの操作対象は 44px 以上あった。44px 未満だったのは本文中のリンクなどだけ。
- R5: satisfied — C3 は pass。部分入力の検査も含む。試行 S5 で、ファイル切替・見出しへのジャンプ・モード切替・テーマ切替・widget の再選択を文字入力で絞り込み、Enter で実行できた。
- R6: satisfied — C4 は pass。試行 S6 で、行範囲を押すと本文の該当ブロックへスクロールして一時的に強調され、スクロールに合わせて Outline と widget に aria-current が付いた。
- R7: satisfied — C4 は pass。エラー状態の 500 と Try again による回復も検査している。試行 S7 で、『なぜこの widget か』に provider/model・confidence・helps・他の候補・fallback が表示され、上部の状態表示は通常・updating・fallback の各状態を正しく示した。
- R8: satisfied — C4 は pass。別の文書の選択が影響しないことも検査している。試行 S8 で、pin と hide が再読み込み後も保たれ、Hidden から戻せた。
- R9: satisfied — C5 は pass。試行 S9 で、変更したブロックだけがその場で更新・強調され、scrollTop は保たれた。選択中は前回の widget が表示され続け、連続編集でも途中の版A は表示されず、最新の版B だけが出た。
- R10: satisfied — C6 は pass。試行 S10 で explorer・breadcrumb・diff のすべての操作と再 diff、見出しの絞り込み、front matter、mermaid、コードハイライト、GFM の表示、Generated UI が動いた。Guided reading もパネルが開いた。数式の描画は既存の MathJax CDN に依存しており、CDN の撤去は対象外。
- R11: satisfied — C10 と C11 は再実行で pass。試行 S13 で成果物の複製に npm ci とビルドを行い、web/dist に差分が出なかった。app.css を読み、デザイントークンが :root と .dark にまとまっていることを確認した。配信物に外部 URL やフォントの読み込みは無かった。
- R12: satisfied — 前回の不適合は修正されている。tabs.tsx:84 の TabsContent に focus-visible:ring-2 ring-inset が付いた。新しい C7 の 7 本目のテストは、ページ読み込みから skip link を使わずに 120 回 Tab し、Outline と Files の両方でレールの tabpanel まで通ることを確かめている。C7 は再実行で 7 passed。試行 S11 でも Tab の全停止点でフォーカスが見え、axe は light/dark/mobile で 0 件、reduced-motion では動きが 0 だった。
- R13: satisfied — C8 は pass で、22 枚のスクリーンショットが決まった場所にある。C9 は検収側でレビューした。ニュートラルな paper/ink と cobalt 1 色の配色、左罫線の欄外注のような widget、カードの羅列・グラデーション・全大文字ラベルが無いこと、light/dark の可読性、モバイルが単一カラムとシートであることを確認した。
- R14: satisfied — C12 は再実行で 42 passed。試行 S13 でも成果物の複製から 1 コマンドで、キーなしで 42 passed だった。
- R15: satisfied — C14 は pass。試行 S14 で README を読み、モード・ショートカット・Jev の opt-in が書かれていて、ショートカットは画面の ? 一覧と一致した。

## 後続事項（要求外・受入を妨げない）

- diff ツールバーのファイル名が省略される: 左右とも『− repo …』としか表示されず、どちらのファイルか分かりにくい。受注側も未解決として申告している。
- 区切りのキー操作 1 回の幅変化が大きい: 矢印キー 1 回で約 72px 動き、細かく調整しにくい。
- fallback の文言が provider に合っていない: provider が fake のときも『Jev did not answer in time』と表示される。
- / を押した直後の 1 文字目が欠ける: / の直後に数 ms 以内で打った文字が見出しの絞り込み欄に入らない。
- test:e2e が成果物内のスクリーンショットを書き換える: 実行するたびに docs/design/screenshots が更新され、作業ツリーに差分が出る。
- 受注側が申告した未解決事項: 約 800px 幅で上部バーの表示が重なる。Generated UI が閲覧中のリポジトリに .cache を書き込む。1280px でレールを開くと Adaptive が狭い幅のレイアウトになる。ほかにも申告された項目がある。

