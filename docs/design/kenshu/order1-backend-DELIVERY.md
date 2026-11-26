# 納品・受入記録: markdown-peek: Jev による読書補助 widget 選択バックエンド

- 受入日時: 2026-11-26T18:35:46.063Z
- 要求版: 590baa63eabf3e0e
- 成果物 fingerprint: b17031d03a21476c62f7b5ee
- 基準版: c0ad375ea15dd5a0
- 検収回数: 2

## 使い始め方

準備:

    git init -q
    git add -A
    sh scripts/cargo-isolated-home.sh build --locked
    sh scripts/cargo-isolated-home.sh build --locked --no-default-features --target-dir target/nodef

- **select** (cli): `target/debug/mdpeek select tests/fixtures/select/runbook-deploy.md` — AdaptiveLayout JSON を標準出力に出す。既定は rules で、ネットワークに接続しない。--provider jev --allow-network（キーは環境変数 TYPESAFE_API_KEY）、または --provider fake --fake-script tests/fake-select.json で切り替える。
- **select-eval** (cli): `target/debug/mdpeek select-eval tests/fixtures/select --runs 5` — ラベル付き fixture を使い、rules（opt-in かつキーがあれば jev も）の一致率、none の適切率、widget recall、p50/p95 を表示する。
- **serve** (cli): `target/debug/mdpeek serve tests/fixtures/select/runbook-deploy.md` — GET /api/adaptive が現在のファイルのレイアウトを返す。ファイルを変更すると WS /ws に {type:"adaptive", revision, layout} を送る。

## 検収判定

第2回の検収は合格（accepted）。前回差し戻した Callout の body が空になる不具合は直っている。REG1 は pass で、C2 の抽出テストにも Callout と RiskPanel の検査が加わった。C1〜C11 はすべて新しい環境の再実行で pass。C12 は検収側でコードレビューし、Jev の回答から使うのは id と数値だけであること、全ノードが validate を通ること、ログに本文とキーが出ないことを確認した。利用者試行では、自作の模擬 Jev サーバを使って R1〜R11 をすべて観察で確かめた。確かめた内容は、opt-in・送信内容・抜粋の上限、閾値の扱い、すべての失敗モードでのフォールバック、serve の WS・キャッシュ・古い revision の破棄、fake provider、互換性の各点。実際の Jev サービスとの疎通は発注の対象外で、確認していない。

- R1: satisfied — C1 と C2 は pass。利用者試行 S1 では、設定なしの rules で wire format の全フィールドが出て、ネットワークを遮断した状態でも出力がバイト単位で一致した。
- R2: satisfied — C3 は pass。S2 では模擬サーバが受信した内容を見た。opt-in したときだけ POST /v1/systemone が1回送られる。各セクションに noul が付き、choice の選択肢は事前に絞り込んだ候補だけ。抜粋は 600 文字で切られ、後半のマーカーは送られていない。C12 のレビューでは、質問型が choice と noul だけで、criteria が候補から作られることを確認した（jev.rs:65-127）。
- R3: satisfied — C3 は pass。S3 で low_helps と low_confidence が skipped に入り、未知の値・型違い・欠損はそのセクションだけ fallback_invalid_answer になった。parse_response は候補 id と照合し、一致しなければ Invalid(unknown_option) にする（jev.rs:203-212）。
- R4: satisfied — C3 と C8 は pass。S4 で 401、429、529、5xx、壊れた JSON、timeout（約 4.0 秒で終了）、接続不可のすべてで exit 0 になり、rules の結果と fallback が返った。429 は retry-after を守って1回だけ再試行した。serve の API も 200 を返した。
- R5: satisfied — C3、C6、C8、C9 は pass。S5 では RUST_LOG=trace でもキーと本文が出なかった。C12 のレビューで jev.rs の info/warn を読み、出しているのは status、所要時間、件数、エラーコードだけと確認した。
- R6: satisfied — C4 と C11 は pass。S6 で次を観察した。debounce の後に WS で adaptive が1通だけ届く。セクションを1つ変えると模擬サーバの受信が1回増え、送られるのは変更したセクションだけ。古い revision は送られない。ディスクへの書き込みは無い。
- R7: satisfied — C2 は pass。S7 で fake 台本の出力が決定的であること、壊れた台本では fake_script_error になること、serve でも使えることを確認した。
- R8: satisfied — C5 と C6 は pass。S8 で gen、term、serve の既存 API が動き、no-default-features ビルドでは not_built に degrade した。未知キーの扱いは [llm] やトップレベルと同じ。
- R9: satisfied — C7 は pass。fixture は 8 件で 7 種類、ラベル付き。キーが無いときは jev を『未実行』と表示し、数値を出さない。模擬サーバ相手では jev も評価された。rules の 1.000 は楽観的な値であることが §11 に明記されている。
- R10: satisfied — REG1 は pass。Callout の body に原文の警告文が入り、sourceRange は 27–29 行を指す。C2 に Callout と RiskPanel の原文一致の検査が加わった（tests/select_cli.rs:160-177）。C12 のレビューでは、Jev の回答から取り出すのが id と数値だけであること、全ノードが validate_nodes を通ること（mod.rs:703）を確認した。S10 では全 widget に sourceRange があった。
- R11: satisfied — C10 は pass。S11 で README と config.example.toml を読み、opt-in と env だけでキーを読むことが書かれていると確認した。

## 後続事項（要求外・受入を妨げない）

- 評価ラベルを独立に作り直す: rules の一致率がすべて 1.000 なのは、ラベルと rules を同じ作業者が作ったため。実 Jev と比べる前に、別の人が付けたラベルを用意するのが望ましい。
- GET /api/adaptive の cached 表示: 再選択した revision でも、GET の応答では全 placement が cached:true になる。WS 側は正しい値を返す。Web UI で使うときは表示の意味を揃えるとよい。
- 実 API の noul 応答形を確認する: 実際の Jev サービスでの疎通と、noul の応答形が想定どおりかは、発注者が確認する必要がある。
- 未知の設定キーで設定全体が無視される: 従来と同じ挙動だが、[select] に typo があると、警告だけ出て設定全体が既定値（rules）になる。利用者が気づきにくい。

