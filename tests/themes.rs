// テーマ切り替えの統合テスト
// `mdpeek term <FILE> --theme <THEME>` が全テーマで正常終了することを確認する。
// サポートされているテーマ: glow, mono, catputtin, dracura, solarized, nord, ayu

use assert_cmd::Command;
use std::io::Write;
use tempfile::NamedTempFile;

/// 与えられたテーマ名で `mdpeek term` が正常終了することを確認するヘルパー
fn assert_theme_exits_ok(theme: &str) {
    let mut tmp = NamedTempFile::new().expect("一時ファイルの作成に失敗");
    writeln!(tmp, "# Theme Test\n\nSome content for theme: {}", theme)
        .expect("一時ファイルへの書き込みに失敗");
    let path = tmp.path().to_owned();

    Command::cargo_bin("mdpeek")
        .expect("mdpeek バイナリが見つからない")
        .arg("term")
        .arg(&path)
        .arg("--theme")
        .arg(theme)
        .assert()
        .success();
}

#[test]
fn theme_glow_exits_ok() {
    assert_theme_exits_ok("glow");
}

#[test]
fn theme_mono_exits_ok() {
    assert_theme_exits_ok("mono");
}

#[test]
fn theme_catputtin_exits_ok() {
    assert_theme_exits_ok("catputtin");
}

#[test]
fn theme_dracura_exits_ok() {
    assert_theme_exits_ok("dracura");
}

#[test]
fn theme_solarized_exits_ok() {
    assert_theme_exits_ok("solarized");
}

#[test]
fn theme_nord_exits_ok() {
    assert_theme_exits_ok("nord");
}

#[test]
fn theme_ayu_exits_ok() {
    assert_theme_exits_ok("ayu");
}
