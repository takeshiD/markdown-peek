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
fn gen_emits_reading_lenses_not_body_reprints() {
    let dir = tempfile::tempdir().unwrap();
    // Tasks → ActionItems lens; the table stays in the body (no DataTable lens).
    let md = "## Todo\n\n- [ ] first\n- [x] second\n\n| Name | Status |\n|------|--------|\n| a | ok |\n";
    let file = write_md(&dir, "doc.md", md);

    Command::cargo_bin("mdpeek")
        .unwrap()
        .arg("gen")
        .arg(&file)
        .arg("--no-cache")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"kind\": \"ActionItems\""))
        .stdout(predicate::str::contains("sourceRange"))
        .stdout(predicate::str::contains("DataTable").not());
}

#[test]
fn gen_writes_cache_file() {
    let dir = tempfile::tempdir().unwrap();
    // A risk section → RiskPanel lens.
    let md = "# Design\n\n## Risks\n\nThe cache may go stale.\n";
    let file = write_md(&dir, "design.md", md);

    // Run inside the temp dir so `.cache/mdpeek` is created there.
    Command::cargo_bin("mdpeek")
        .unwrap()
        .current_dir(dir.path())
        .arg("gen")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("RiskPanel"));

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
    let md = "## Tasks\n\n- [ ] task a\n- [x] task b\n";
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
        .stdout(predicate::str::contains("ActionItems"));
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
