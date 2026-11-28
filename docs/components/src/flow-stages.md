# 見本

## 処理の各段

段階ごとの入力と出力です。

### 1. parser — markdown -> BlockTree

Markdown を読み、ブロックの木にします。

### 2. analyzer — BlockTree -> semantic model

ブロックを分類します。

### 3. planner — semantic model -> UI plan

どの補助を出すかを決めます。

### 4. renderer — UI plan -> HTML

画面に描きます。
