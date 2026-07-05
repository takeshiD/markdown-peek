//! Integration tests for the `mdpeek gen` subcommand (Layer 3 Generative UI).

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;

fn write_md(dir: &tempfile::TempDir, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    path
}

#[test]
fn gen_emits_ir_for_tasks_and_tables() {
    let dir = tempfile::tempdir().unwrap();
    let md = "## Todo\n\n- [ ] first\n- [x] second\n\n| Name | Status |\n|------|--------|\n| a | ok |\n";
    let file = write_md(&dir, "doc.md", md);

    Command::cargo_bin("mdpeek")
        .unwrap()
        .arg("gen")
        .arg(&file)
        .arg("--no-cache")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"kind\": \"Checklist\""))
        .stdout(predicate::str::contains("\"kind\": \"DataTable\""))
        .stdout(predicate::str::contains("sourceRange"));
}

#[test]
fn gen_writes_cache_file() {
    let dir = tempfile::tempdir().unwrap();
    let md = "> [!WARNING]\n> danger\n";
    let file = write_md(&dir, "warn.md", md);

    // Run inside the temp dir so `.cache/mdpeek` is created there.
    Command::cargo_bin("mdpeek")
        .unwrap()
        .current_dir(dir.path())
        .arg("gen")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("Callout"));

    let cache_dir = dir.path().join(".cache").join("mdpeek");
    let entries: Vec<_> = std::fs::read_dir(&cache_dir).unwrap().collect();
    assert_eq!(entries.len(), 1, "expected one cached .gui.json file");
}

#[test]
fn gen_llm_falls_back_to_rules_when_backend_unavailable() {
    // In a default build the `anthropic_api` backend needs `--features llm`, so
    // `build()` errors and generation must fall back to deterministic rules
    // rather than failing the command.
    let dir = tempfile::tempdir().unwrap();
    let md = "- [ ] task a\n- [x] task b\n";
    let file = write_md(&dir, "tasks.md", md);

    Command::cargo_bin("mdpeek")
        .unwrap()
        .arg("gen")
        .arg(&file)
        .arg("--no-cache")
        .arg("--llm")
        .arg("--provider")
        .arg("anthropic_api")
        .assert()
        .success()
        .stdout(predicate::str::contains("Checklist"));
}

#[test]
fn gen_rejects_directory() {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("mdpeek")
        .unwrap()
        .arg("gen")
        .arg(dir.path())
        .assert()
        .failure();
}
