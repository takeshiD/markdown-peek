# 見本

## 設定値

| 項目 | 値 | 備考 |
|------|----|------|
| replicas | 6 | ピーク時は 10 |
| timeout | 30s | 上流の API に合わせる |
| retries | 2 | 冪等な操作のみ |
| region | ap-northeast-1 | 東京 |
| log_level | info | 障害時は debug |
| cache_ttl | 300s | 5 分 |
