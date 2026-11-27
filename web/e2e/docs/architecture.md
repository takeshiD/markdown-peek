# Software Factory 設計メモ

AI オーケストレーションの実行フローと、実行方式の比較をまとめる。

## 実行フロー

- Planning (Orchestrator) → Frontend, Backend
- Frontend, Backend → Verification
- Verification --PASS--> Release
- Verification --RETRY--> Backend

各ノードの役割:

- **Frontend**: Agent 01。UI を実装する
- **Backend**: Agent 02。API を実装する
- **Verification**: Quality Gate。テストと検収基準で判定する
- **Release**: Completed

## 実行方式の比較

| 観点 | Single Agent | Agent Swarm |
|------|-------------:|------------:|
| 実行速度 | 45 | 95 |
| コスト効率 | 95 | 45 |
| 品質 | 70 | 95 |
| 想定実行時間 | 28 min | 12 min |
| 相対実行コスト | 1.0x | 2.4x |

品質と速度を重視するため、Agent Swarm を推奨する。
