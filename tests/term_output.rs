// ターミナル出力の統合テスト
// `mdpeek term <FILE>` を実際に実行し、stdout の内容を検証する。
// ビルド済みバイナリを使うブラックボックステストであるため、
// src/ を直接参照しない。

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;
use tempfile::NamedTempFile;

/// テスト用の Markdown コンテンツ（見出し・リスト・コードブロック・テーブルを含む）
fn sample_markdown() -> &'static str {
    "# Hello World\n\
     \n\
     ## Section Two\n\
     \n\
     A paragraph of text.\n\
     \n\
     - item alpha\n\
     - item beta\n\
     - item gamma\n\
     \n\
     ```rust\n\
     fn main() { println!(\"hello\"); }\n\
     ```\n\
     \n\
     | Name  | Score |\n\
     |-------|-------|\n\
     | Alice | 100   |\n\
     | Bob   | 80    |\n\
     \n\
     - [x] done task\n\
     - [ ] pending task\n"
}

/// `mdpeek term <file>` を実行し一時ファイルを返す共通ヘルパー
fn run_term(md: &str) -> (assert_cmd::assert::Assert, NamedTempFile) {
    let mut tmp = NamedTempFile::new().expect("一時ファイルの作成に失敗");
    write!(tmp, "{}", md).expect("一時ファイルへの書き込みに失敗");
    let path = tmp.path().to_owned();
    let assert = Command::cargo_bin("mdpeek")
        .expect("mdpeek バイナリが見つからない")
        .arg("term")
        .arg(&path)
        // NO_COLOR=1 を渡すことで owo-colors が ANSI エスケープを出力しなくなる場合がある。
        // owo-colors v4 は NO_COLOR を尊重しない場合もあるため、
        // テストでは「部分文字列が含まれること」だけを検証する。
        .env("NO_COLOR", "1")
        .assert();
    (assert, tmp)
}

/// 見出しテキストが stdout に含まれること
#[test]
fn term_heading_is_present() {
    let (assert, _tmp) = run_term(sample_markdown());
    assert
        .success()
        .stdout(predicate::str::contains("Hello World"));
}

/// H2 見出しテキストが含まれること
#[test]
fn term_h2_heading_is_present() {
    let (assert, _tmp) = run_term(sample_markdown());
    assert
        .success()
        .stdout(predicate::str::contains("Section Two"));
}

/// リスト項目が stdout に含まれること
#[test]
fn term_list_items_present() {
    let (assert, _tmp) = run_term(sample_markdown());
    assert
        .success()
        .stdout(predicate::str::contains("item alpha"))
        .stdout(predicate::str::contains("item beta"))
        .stdout(predicate::str::contains("item gamma"));
}

/// コードブロックの内容が stdout に含まれること
#[test]
fn term_code_block_present() {
    let (assert, _tmp) = run_term(sample_markdown());
    assert.success().stdout(predicate::str::contains("fn main"));
}

/// テーブルのセル内容（Alice, Bob）が stdout に含まれること
#[test]
fn term_table_cells_present() {
    let (assert, _tmp) = run_term(sample_markdown());
    assert
        .success()
        .stdout(predicate::str::contains("Alice"))
        .stdout(predicate::str::contains("Bob"));
}

/// タスクリストのチェック済みマーカーが出力に含まれること
#[test]
fn term_tasklist_marker_present() {
    let (assert, _tmp) = run_term(sample_markdown());
    // チェック済みは [✓] または [x] として出力される
    assert
        .success()
        .stdout(predicate::str::contains("[✓]").or(predicate::str::contains("[x]")));
}

/// 段落テキストが出力に含まれること
#[test]
fn term_paragraph_present() {
    let (assert, _tmp) = run_term(sample_markdown());
    assert
        .success()
        .stdout(predicate::str::contains("A paragraph of text."));
}
