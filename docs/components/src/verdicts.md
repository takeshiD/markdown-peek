# 受入記録

## 検収判定

合格。前回 R3 で差し戻した点は直っており、REG1 が再実行で pass した。

- R1: satisfied — C1: make build でビルドできた。
- R2: satisfied — C14 で DB への参照が無いことを確認した。C3 と利用者試行 S2 でも見た。
- R3: satisfied — 前回差し戻した点(入力欄で 'esc' がキー操作になる)は直っていた。REG1、C3 が pass。基本操作は C6〜C8 で確認した。
- R4: satisfied — C9 で URL が1引数のまま渡ることを確認した。既定ブラウザでの起動は試していない。
- R5: not satisfied — C3 は pass したが C12 が fail。

## 後続事項

- 設定の警告を出す
