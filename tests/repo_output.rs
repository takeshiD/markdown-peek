// Layer 4 `repo` サブコマンドの統合テスト。
// リポジトリ内のドキュメントを解析し、壊れた参照・マニフェスト・JSON 出力を
// 正しく報告することを確認する。

use assert_cmd::Command;
use predicates::prelude::*;
use std::process::Command as StdCommand;

/// テスト用の一時 git リポジトリを作り、README と壊れた参照を用意する。
fn setup_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("一時ディレクトリ作成失敗");
    let root = dir.path();
    let git = |args: &[&str]| {
        StdCommand::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .expect("git 実行失敗");
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@example.com"]);
    git(&["config", "user.name", "tester"]);

    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(root.join("real.md"), "# Real\n").unwrap();
    std::fs::write(
        root.join("README.md"),
        "# Demo\n\nSee [ok](real.md) and [broken](gone.md).\n",
    )
    .unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "init"]);
    dir
}

/// `mdpeek repo README.md` が正常終了し、壊れた参照を報告すること。
#[test]
fn repo_reports_broken_reference() {
    let dir = setup_repo();
    Command::cargo_bin("mdpeek")
        .expect("mdpeek バイナリが見つからない")
        .current_dir(dir.path())
        .args(["repo", "README.md"])
        .assert()
        .success()
        .stdout(predicate::str::contains("gone.md"))
        .stdout(predicate::str::contains("broken"))
        .stdout(predicate::str::contains("Cargo.toml"));
}

/// `mdpeek repo --json` が有効な JSON を出力すること。
#[test]
fn repo_json_output_is_valid() {
    let dir = setup_repo();
    let output = Command::cargo_bin("mdpeek")
        .expect("mdpeek バイナリが見つからない")
        .current_dir(dir.path())
        .args(["repo", "--json", "README.md"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value =
        serde_json::from_slice(&output).expect("出力が有効な JSON でない");
    assert_eq!(value["document"], "README.md");
    assert_eq!(value["in_git_repo"], true);
    // gone.md は解決できないので broken として記録される。
    let refs = value["doc_refs"]["refs"].as_array().unwrap();
    assert!(
        refs.iter()
            .any(|r| r["target"] == "gone.md" && r["exists"] == false)
    );
}

/// 存在しないファイルを渡してもクラッシュせず終了すること。
#[test]
fn repo_missing_file_does_not_crash() {
    let dir = setup_repo();
    Command::cargo_bin("mdpeek")
        .expect("mdpeek バイナリが見つからない")
        .current_dir(dir.path())
        .args(["repo", "does-not-exist.md"])
        .assert()
        .success();
}
