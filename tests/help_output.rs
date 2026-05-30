// ヘルプ出力の統合テスト
// `mdpeek --help` と `mdpeek term --help` が exit code 0 で終了し、
// 使い方文字列を含むことを確認する。

use assert_cmd::Command;
use predicates::prelude::*;

/// `mdpeek --help` が正常終了し、ツール名を含むこと
#[test]
fn help_flag_exits_zero() {
    Command::cargo_bin("mdpeek")
        .expect("mdpeek バイナリが見つからない")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("mdpeek"));
}

/// `mdpeek --help` の出力に "Usage" または "Usage:" が含まれること
#[test]
fn help_flag_contains_usage() {
    Command::cargo_bin("mdpeek")
        .expect("mdpeek バイナリが見つからない")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::is_match("(?i)usage").expect("正規表現が不正"));
}

/// `mdpeek term --help` が正常終了し、"term" サブコマンドの説明を含むこと
#[test]
fn term_help_flag_exits_zero() {
    Command::cargo_bin("mdpeek")
        .expect("mdpeek バイナリが見つからない")
        .arg("term")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("terminal"));
}

/// `mdpeek term --help` の出力に "--theme" オプションが含まれること
#[test]
fn term_help_contains_theme_option() {
    Command::cargo_bin("mdpeek")
        .expect("mdpeek バイナリが見つからない")
        .arg("term")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("theme"));
}
