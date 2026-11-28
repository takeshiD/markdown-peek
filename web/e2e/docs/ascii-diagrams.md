# Generative UI 設計図

図は等幅の罫線で描かれている。読書補助はこれをノードとリンクのグラフに描き直す。

## 全体アーキテクチャ

核心は重い処理を Rust core に集約し、UI IR を web/TUI 共通の契約にすること。

```
        ┌──────────────────────── Rust core (mdpeek-core) ───────────────────────┐
        │                                                                        │
document.md ─▶ parser ─▶ model ─▶ analyzer ─▶ planner ─▶ generator ─▶ ir(validate)
        │      (pulldown  (block   (rules +   (semantic   (rules/     (schema +              │
        │       -cmark     tree +   LLM)       model →     LLM →       sourceRange            │
        │       OffsetIter offsets)            UI plan)    UI IR)      検証。永続 cache なし E)│
        └──────────────────────────────────────────┬───────────────────────────┘
                                                    │  UI IR (JSON, serde)
                        ┌───────────────────────────┼───────────────────────────┐
                        ▼                                                         ▼
              web renderer (Preact)                                    tui renderer (ratatui)
              registry[kind] → component                               registry[kind] → widget
              (全 UiNode 対応)                                          (部分集合を段階対応)
```

## 依存関係(クリティカルパス)

```
SourceRange parser ──┬─▶ Layer2 analyzer ──▶ Layer3 IR/generator ──▶ Layer3 renderer
                     └─▶ #16 差分 (BlockId 安定化)
#14 worktree ────────────────────────────────────────────────────▶ Layer4
```

## ディレクトリ

```
src/
├── main.rs
├── server.rs
└── watcher.rs
```
