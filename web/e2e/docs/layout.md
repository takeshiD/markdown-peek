# リポジトリ構成

このメモはリポジトリの構成をまとめたものです。

## ディレクトリ構成

```
markdown-peek/
├── Cargo.toml                      # [workspace]
├── crates/
│   ├── mdpeek-core/                # ★ 解析の心臓部
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   └── parser/             # markdown -> block tree
│   │   └── Cargo.toml
│   └── mdpeek-cli/                 # バイナリ `mdpeek`
├── web/                            # フロントエンド
│   ├── package.json
│   └── src/
│       └── main.tsx
└── docs/
    └── architecture.md             # 本書
```

## 補足

ビルド成果物はコミットしません。
