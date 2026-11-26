//! `mdpeek select` / `mdpeek select-eval` without network: rules default (R1),
//! scripted fake provider (R7), evaluation report (R9), provenance (R10).

mod common;

use assert_cmd::Command;
use common::*;
use serde_json::{Value, json};

const FIXTURES: &str = "tests/fixtures/select";

fn mdpeek(home: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("mdpeek").unwrap();
    c.env("HOME", home).env("XDG_CONFIG_HOME", home).env_remove("TYPESAFE_API_KEY");
    c
}

fn select_json(home: &std::path::Path, args: &[&str]) -> Value {
    let out = mdpeek(home).arg("select").args(args).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn default_select_is_rules_offline_with_full_wire_format() {
    let home = tempfile::tempdir().unwrap();
    let j = select_json(home.path(), &[&format!("{FIXTURES}/runbook-deploy.md")]);
    assert_eq!(j["version"], 1);
    assert!(j["revision"].is_u64());
    assert_eq!(j["selector"]["provider"], "rules");
    assert_eq!(j["selector"]["requested"], "rules");
    assert_eq!(j["selector"]["fallback"], Value::Null);
    assert_eq!(j["selector"]["requests"], 0);
    assert!(j["selector"]["model"].is_string());
    assert!(j["doc_type"]["value"].is_string());
    let ps = j["placements"].as_array().unwrap();
    assert!(!ps.is_empty());
    for p in ps {
        assert!(p["scope"] == "section" || p["scope"] == "document");
        for k in ["block_id", "heading", "range"] {
            assert!(!p["anchor"][k].is_null(), "anchor.{k}");
        }
        assert!(p["node"]["kind"].is_string());
        for k in ["by", "confidence", "helps", "reason_code", "alternatives"] {
            assert!(!p["why"][k].is_null(), "why.{k}");
        }
    }
    let sk = j["skipped"].as_array().unwrap();
    assert!(sk.iter().any(|s| s["anchor"]["heading"] == "Contacts" && s["reason_code"].is_string()));
}

#[test]
fn select_missing_file_fails_cleanly() {
    let home = tempfile::tempdir().unwrap();
    mdpeek(home.path())
        .args(["select", "does-not-exist.md"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("is not a file"));
}

fn strip_timing(mut v: Value) -> Value {
    v["timing_ms"] = Value::Null;
    v
}

#[test]
fn fake_provider_is_scripted_and_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let script = write(
        dir.path(),
        "fake.json",
        &json!({
            "model": "fake-e2e",
            "doc_type": { "value": "runbook", "confidence": 0.95 },
            "document": { "widget": "ActionItems", "confidence": 0.9, "helps": 0.9 },
            "sections": {
                "Steps": { "widget": "StepNavigator", "confidence": 0.9, "helps": 0.9 },
                "Commands": { "helps": 0.1 },
                "Rollback": { "widget": "NotAWidget", "helps": 0.9 },
                "Prerequisites": { "confidence": 0.3, "helps": 0.9 }
            },
            "default": "rules"
        })
        .to_string(),
    );
    let file = format!("{FIXTURES}/runbook-deploy.md");
    let args = ["--provider", "fake", "--fake-script", script.to_str().unwrap(), &file];
    let a = select_json(dir.path(), &args);
    let b = select_json(dir.path(), &args);
    assert_eq!(strip_timing(a.clone()), strip_timing(b));
    assert_eq!(a["selector"]["provider"], "fake");
    assert_eq!(a["selector"]["model"], "fake-e2e");
    assert_eq!(a["doc_type"]["value"], "runbook");
    let find = |h: &str| a["placements"].as_array().unwrap().iter().find(|p| p["anchor"]["heading"] == h && p["scope"] == "section").cloned();
    let skip = |h: &str| a["skipped"].as_array().unwrap().iter().find(|p| p["anchor"]["heading"] == h).cloned();
    assert_eq!(find("Steps").unwrap()["why"]["by"], "fake");
    assert_eq!(skip("Commands").unwrap()["reason_code"], "low_helps");
    assert_eq!(skip("Prerequisites").unwrap()["reason_code"], "low_confidence");
    // Rollback's only content is a GFM alert, already highlighted in the body,
    // so it has no candidate (invalid answers are covered in select_jev.rs).
    assert_eq!(skip("Rollback").unwrap()["reason_code"], "no_candidates");
    let docs: Vec<_> = a["placements"].as_array().unwrap().iter().filter(|p| p["scope"] == "document").collect();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0]["node"]["kind"], "ActionItems");
}

#[test]
fn fake_provider_from_config_and_scripted_failure() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "fake.json", r#"{ "fail": "timeout" }"#);
    let cfg = write(dir.path(), "config.toml", &format!("[select]\nprovider = \"fake\"\nfake_script = \"{}\"\n", dir.path().join("fake.json").display()));
    let j = select_json(dir.path(), &["--config", cfg.to_str().unwrap(), &format!("{FIXTURES}/README.md")]);
    assert_eq!(j["selector"]["requested"], "fake");
    assert_eq!(j["selector"]["provider"], "rules");
    assert_eq!(j["selector"]["fallback"], "timeout");
    assert!(!j["placements"].as_array().unwrap().is_empty());
}

#[test]
fn every_widget_is_extracted_verbatim_with_valid_source_range() {
    let home = tempfile::tempdir().unwrap();
    for e in std::fs::read_dir(FIXTURES).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "md") {
            continue;
        }
        let md = std::fs::read_to_string(&p).unwrap();
        let lines = md.split('\n').count() as u64;
        let j = select_json(home.path(), &[p.to_str().unwrap()]);
        for pl in j["placements"].as_array().unwrap() {
            let n = &pl["node"];
            let r = &n["sourceRange"];
            assert!(r.is_object(), "{} {} lacks sourceRange", p.display(), n["kind"]);
            let (s, e) = (r["start_line"].as_u64().unwrap(), r["end_line"].as_u64().unwrap());
            assert!(s >= 1 && s <= e && e <= lines, "{} range {r}", p.display());
            match n["kind"].as_str().unwrap() {
                "ConfigViewer" => {
                    for l in n["content"].as_str().unwrap().lines() {
                        assert!(md.contains(l), "not verbatim: {l}");
                    }
                }
                "StepNavigator" => {
                    for st in n["steps"].as_array().unwrap() {
                        assert!(md.contains(st["title"].as_str().unwrap()));
                    }
                }
                "Checklist" => {
                    for it in n["items"].as_array().unwrap() {
                        assert!(md.contains(it["title"].as_str().unwrap()));
                    }
                }
                "DataTable" => {
                    for row in n["rows"].as_array().unwrap() {
                        for v in row.as_object().unwrap().values() {
                            assert!(md.contains(v.as_str().unwrap()));
                        }
                    }
                }
                "Callout" => {
                    let body = n["body"].as_str().unwrap();
                    assert!(!body.trim().is_empty(), "{} empty Callout body", p.display());
                    for l in body.lines() {
                        assert!(md.contains(l), "Callout not verbatim: {l}");
                    }
                    // The range points at the quoted lines.
                    let src: Vec<&str> = md.split('\n').collect();
                    let first_body = body.lines().next().unwrap();
                    let covered = src[(s - 1) as usize..e as usize].join("\n");
                    assert!(covered.contains(first_body), "range does not cover body");
                }
                "RiskPanel" => {
                    let risks = n["risks"].as_array().unwrap();
                    assert!(!risks.is_empty());
                    for rk in risks {
                        let t = rk["title"].as_str().unwrap();
                        assert!(!t.trim().is_empty(), "empty risk");
                        assert!(md.contains(t), "RiskPanel not verbatim: {t}");
                    }
                }
                "Timeline" => {
                    for ev in n["events"].as_array().unwrap() {
                        assert!(md.contains(ev["title"].as_str().unwrap()));
                    }
                }
                _ => {}
            }
        }
    }
}

#[test]
fn eval_reports_rules_metrics_and_marks_jev_not_run_without_key() {
    let home = tempfile::tempdir().unwrap();
    let out = mdpeek(home.path()).args(["select-eval", FIXTURES, "--runs", "2", "--json", "--allow-network"]).output().unwrap();
    assert!(out.status.success());
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(j["fixtures"].as_array().unwrap().len() >= 6);
    let rules = &j["providers"][0];
    assert_eq!(rules["provider"], "rules");
    for k in ["accuracy", "none_appropriate", "widget_recall", "p50_ms", "p95_ms"] {
        assert!(rules[k].is_number(), "{k}");
    }
    let jev = &j["providers"][1];
    assert_eq!(jev["provider"], "jev");
    assert_eq!(jev["status"], "not_run");
    assert!(jev["reason"].as_str().unwrap().contains("TYPESAFE_API_KEY"));
    assert!(jev["accuracy"].is_null() && jev["p50_ms"].is_null(), "no fabricated numbers");
}

#[test]
fn eval_runs_jev_when_opted_in_with_key() {
    let mock = Mock::start(vec![Reply::json(200, json!({ "answers": { "doc_type": { "choice": "readme", "confidence": 0.9 } } }))]);
    let dir = tempfile::tempdir().unwrap();
    let cfg = jev_config(dir.path(), &mock.base_url(), "");
    let out = mdpeek(dir.path())
        .env("TYPESAFE_API_KEY", TEST_KEY)
        .args(["--config", cfg.to_str().unwrap(), "select-eval", FIXTURES, "--runs", "1", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    let jev = &j["providers"][1];
    assert_eq!(jev["provider"], "jev");
    assert_ne!(jev["status"], "not_run");
    assert!(jev["accuracy"].is_number());
    assert!(mock.count() >= 6, "one request per fixture");
}

#[test]
fn documented_example_fake_script_works() {
    let home = tempfile::tempdir().unwrap();
    let j = select_json(home.path(), &["--provider", "fake", "--fake-script", "tests/fake-select.json", &format!("{FIXTURES}/runbook-deploy.md")]);
    assert_eq!(j["selector"]["provider"], "fake");
    assert_eq!(j["selector"]["fallback"], Value::Null);
    assert_eq!(j["doc_type"]["value"], "runbook");
    let skipped: Vec<_> = j["skipped"].as_array().unwrap().iter().map(|s| (s["anchor"]["heading"].as_str().unwrap().to_string(), s["reason_code"].as_str().unwrap().to_string())).collect();
    assert!(skipped.contains(&("Commands".into(), "low_helps".into())), "{skipped:?}");
    assert!(skipped.contains(&("Prerequisites".into(), "low_confidence".into())), "{skipped:?}");
}

#[test]
fn malformed_fake_script_reports_reason_on_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let bad = write(dir.path(), "bad.json", r#"{ "sections": { "x": { "helps": "high" } } }"#);
    let out = mdpeek(dir.path()).args(["select", "--provider", "fake", "--fake-script", bad.to_str().unwrap(), &format!("{FIXTURES}/README.md")]).output().unwrap();
    assert!(out.status.success());
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["selector"]["fallback"], "fake_script_error");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("line 1 column"), "{err}");
}

#[test]
fn crlf_documents_yield_clean_widget_text() {
    let dir = tempfile::tempdir().unwrap();
    let md = "# T\r\n\r\n## Install\r\n\r\n```sh\r\ncargo build\r\ncargo test\r\ncp a b\r\n```\r\n";
    let file = dir.path().join("README.md");
    std::fs::write(&file, md).unwrap();
    let out = assert_cmd::Command::cargo_bin("mdpeek").unwrap()
        .arg("select").arg(&file)
        .env("HOME", dir.path()).env("XDG_CONFIG_HOME", dir.path()).env_remove("TYPESAFE_API_KEY")
        .output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(stdout.contains("cargo build"), "{stdout}");
    assert!(!stdout.contains("\\r"), "carriage return leaked into widget text: {stdout}");
}

#[test]
fn empty_warning_alert_is_not_a_callout() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("README.md");
    std::fs::write(&file, "# T\n\n## Danger\n\n> [!WARNING]\n\n## Next\n\ntext\n").unwrap();
    let out = assert_cmd::Command::cargo_bin("mdpeek").unwrap()
        .arg("select").arg(&file)
        .env("HOME", dir.path()).env("XDG_CONFIG_HOME", dir.path()).env_remove("TYPESAFE_API_KEY")
        .output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("\"body\": \"\""), "empty callout emitted: {stdout}");
}

#[test]
fn document_aid_covered_by_a_section_aid_is_not_shown_twice() {
    // In the E2E sample runbook the only task list lives in one section, so
    // document-level ActionItems and that section's Checklist would list the
    // same lines: only the section aid (beside its text) is kept.
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/e2e/docs/deploy-guide.md");
    let dir = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("mdpeek").unwrap()
        .arg("select").arg(&file)
        .env("HOME", dir.path()).env("XDG_CONFIG_HOME", dir.path()).env_remove("TYPESAFE_API_KEY")
        .output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let kinds: Vec<String> = v["placements"].as_array().unwrap().iter().map(|p| format!("{}:{}", p["scope"].as_str().unwrap(), p["node"]["kind"].as_str().unwrap())).collect();
    assert!(kinds.contains(&"section:Checklist".to_string()), "{kinds:?}");
    assert!(!kinds.contains(&"document:ActionItems".to_string()), "duplicate shown: {kinds:?}");
    assert!(v["skipped"].as_array().unwrap().iter().any(|s| s["reason_code"] == "covered_by_section"), "{}", v["skipped"]);
}
