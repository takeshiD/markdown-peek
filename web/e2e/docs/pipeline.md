# Viewer pipeline

## パイプライン各段の責務

文書は次の段を順に通る。

### 3.1 parser — `markdown -> BlockTree`

pulldown-cmark の offset iterator で各ブロックの行範囲を取る。

### 3.2 analyzer — 分類・抽出

rules で確定できるものは全部 rules。

### 3.3 planner — semantic model → UI plan

文書タイプ別に出すノードを決める。

### 3.4 generator — UI plan → UI IR

決定的に埋められるノードを作る。

### 3.5 validate — schema + sourceRange 検証

未知の kind と捏造レンジを落とす。

### 3.6 生成物の非永続方針

生成物はディスクに保存しない。
