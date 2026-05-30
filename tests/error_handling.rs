// エラーハンドリングの統合テスト
// 存在しないファイルを渡したとき、バイナリがパニックせずに
// 正常（または非ゼロ）終了することを確認する。

use assert_cmd::Command;
use predicates::prelude::*;

/// 存在しないファイルを指定しても panic しないこと
/// （終了コードは問わないが、プロセス自体が落ちないことを確認）
#[test]
fn term_nonexistent_file_does_not_panic() {
    Command::cargo_bin("mdpeek")
        .expect("mdpeek バイナリが見つからない")
        .arg("term")
        .arg("/nonexistent/path/to/missing_file.md")
        .assert()
        // パニック時は stderr に "thread 'main' panicked" が出る。
        // そのような文字列が含まれないことで panic 非発生を確認。
        .stderr(predicates::str::contains("panicked").not());
}
