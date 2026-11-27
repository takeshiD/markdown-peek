//! `mdpeek select` with decision models and LLMs against scripted mock
//! servers (R2–R5). No real key / network is used: keys are dummies and every
//! `base_url` points at 127.0.0.1.

mod common;

use assert_cmd::Command;
use common::*;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const MARKER: &str = "ZZBODYMARKERZZ";

fn doc() -> String {
    let filler = "これは長い本文です。".repeat(200); // 2000 chars
    format!(
        "# Tool\n\nIntro paragraph.\n\n## Install\n\n```toml\n[install]\npath = \".\"\nlocked = true\n```\n\n\
## Steps\n\n1. one\n2. two\n3. three\n4. four\n5. five\n\n| a | status |\n|---|---|\n| 1 | ok |\n| 2 | ok |\n| 3 | ng |\n\n\
## Notes\n\n{filler}{MARKER}\n\n- [ ] a\n- [ ] b\n- [ ] c\n\n## License\n\nMIT\n"
    )
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
    json: Value,
    elapsed: Duration,
}

fn run(
    dir: &tempfile::TempDir,
    cfg: &std::path::Path,
    key: Option<&str>,
    extra_env: &[(&str, &str)],
) -> Run {
    let file = write(dir.path(), "README.md", &doc());
    let mut cmd = Command::cargo_bin("mdpeek").unwrap();
    cmd.arg("--config").arg(cfg).arg("select").arg(&file);
    cmd.env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path())
        .env("XDG_CACHE_HOME", dir.path().join("cache"))
        .env_remove("TYPESAFE_API_KEY")
        .env_remove("RUST_LOG");
    if let Some(k) = key {
        cmd.env("TYPESAFE_API_KEY", k);
    }
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let t = Instant::now();
    let out = cmd.output().unwrap();
    let elapsed = t.elapsed();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let json = serde_json::from_str(&stdout).unwrap_or(Value::Null);
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout,
        stderr,
        json,
        elapsed,
    }
}

fn placement<'a>(j: &'a Value, heading: &str) -> Option<&'a Value> {
    j["placements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["scope"] == "section" && p["anchor"]["heading"] == heading)
}
fn skipped<'a>(j: &'a Value, heading: &str) -> Option<&'a Value> {
    j["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["scope"] == "section" && p["anchor"]["heading"] == heading)
}

fn good_answers() -> Value {
    json!({ "model": "jev-1.14.0", "answers": {
        "doc_type": { "choice": "readme", "confidence": 0.93, "probabilities": { "readme": 0.93 } },
        "doc_helps": 0.2,
        "doc_widget": { "choice": "SemanticOutline", "confidence": 0.9 },
        "s1_helps": 0.9,
        "s2_helps": 0.8,
        "s2_widget": { "choice": "StepNavigator", "confidence": 0.9, "probabilities": { "StepNavigator": 0.9, "DataTable:status": 0.08 } },
        "s3_helps": 0.3
    }, "usage": { "input_tokens": 100 } })
}

fn assert_no_secrets(r: &Run) {
    assert!(!r.stdout.contains(TEST_KEY), "key leaked to stdout");
    assert!(!r.stderr.contains(TEST_KEY), "key leaked to stderr");
    assert!(!r.stderr.contains(MARKER), "body leaked to stderr");
    assert!(
        !r.stderr.contains("これは長い本文"),
        "body leaked to stderr"
    );
}

#[test]
fn jev_sends_one_request_with_prefiltered_candidates_and_excerpts_only() {
    let mock = Mock::start(vec![Reply::json(200, good_answers())]);
    let dir = tempfile::tempdir().unwrap();
    let cfg = jev_config(dir.path(), &mock.base_url(), "");
    let r = run(&dir, &cfg, Some(TEST_KEY), &[("RUST_LOG", "trace")]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_no_secrets(&r);

    // Exactly one POST /v1/systemone with the bearer key.
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    let q = &reqs[0];
    assert_eq!(q.method, "POST");
    assert_eq!(q.path, "/v1/systemone");
    assert_eq!(
        q.header("authorization"),
        Some(format!("Bearer {TEST_KEY}").as_str())
    );
    let body = q.json();
    assert_eq!(body["model"], "jev-1.14.0");
    let qs = body["questions"].as_object().unwrap();
    // Companion noul per asked section; choice only with >1 candidate.
    for k in ["s1_helps", "s2_helps", "s3_helps", "doc_type", "doc_helps"] {
        assert!(qs.contains_key(k), "missing {k}");
    }
    assert_eq!(qs["s1_helps"]["type"], "noul");
    assert!(!qs.contains_key("s1_widget"));
    assert!(
        !qs.contains_key("s0_helps"),
        "section without candidates must not be asked"
    );
    assert!(!qs.contains_key("s4_helps"));
    let crit: Vec<&String> = qs["s2_widget"]["criteria"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert_eq!(qs["s2_widget"]["type"], "choice");
    assert!(
        crit.iter().all(
            |c| ["StepNavigator", "DataTable:status", "DataTable:sortable"].contains(&c.as_str())
        ),
        "{crit:?}"
    );
    // Excerpts only: ≤600 chars, long body tail never sent.
    assert!(!q.body.contains(MARKER), "full body was sent");
    for s in body["state"]["sections"].as_array().unwrap() {
        if let Some(e) = s["excerpt"].as_str() {
            assert!(e.chars().count() <= 600);
        }
        assert!(s["features"].is_object());
    }

    // Output: jev decisions + companion-noul based skips.
    let j = &r.json;
    assert_eq!(j["selector"]["name"], "jev");
    assert_eq!(j["selector"]["kind"], "decision");
    assert_eq!(j["selector"]["error"], Value::Null);
    assert_eq!(j["progress"]["state"], "done");
    assert_eq!(j["doc_type"]["value"], "readme");
    assert_eq!(j["doc_type"]["by"], "jev");
    let p = placement(j, "Install").unwrap();
    assert_eq!(p["why"]["by"], "jev");
    assert_eq!(p["why"]["reason_code"], "chosen");
    assert_eq!(p["node"]["kind"], "ConfigViewer");
    assert!(p["node"]["sourceRange"].is_object());
    assert_eq!(
        placement(j, "Steps").unwrap()["why"]["candidate"],
        "StepNavigator"
    );
    let s = skipped(j, "Notes").unwrap();
    assert_eq!(s["reason_code"], "low_helps");
    assert_eq!(s["by"], "jev");
    assert!(
        j["skipped"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["scope"] == "document" && s["reason_code"] == "low_helps")
    );
}

#[test]
fn low_confidence_choice_is_not_shown() {
    let mut a = good_answers();
    a["answers"]["s2_widget"]["confidence"] = json!(0.4);
    let mock = Mock::start(vec![Reply::json(200, a)]);
    let dir = tempfile::tempdir().unwrap();
    let cfg = jev_config(dir.path(), &mock.base_url(), "");
    let r = run(&dir, &cfg, Some(TEST_KEY), &[]);
    assert_eq!(r.code, 0);
    assert!(placement(&r.json, "Steps").is_none());
    assert_eq!(
        skipped(&r.json, "Steps").unwrap()["reason_code"],
        "low_confidence"
    );
}

#[test]
fn thresholds_are_configurable() {
    // helps 0.8 < 0.85 → skipped; default would have shown it.
    let mock = Mock::start(vec![Reply::json(200, good_answers())]);
    let dir = tempfile::tempdir().unwrap();
    let cfg = jev_config(dir.path(), &mock.base_url(), "helps_threshold = 0.85");
    let r = run(&dir, &cfg, Some(TEST_KEY), &[]);
    assert_eq!(
        skipped(&r.json, "Steps").unwrap()["reason_code"],
        "low_helps"
    );
    assert!(placement(&r.json, "Install").is_some());
}

#[test]
fn invalid_answers_show_nothing_for_that_section_only() {
    let mut a = good_answers();
    a["answers"]["s1_helps"] = json!("yes"); // wrong type
    a["answers"]["s2_widget"] = json!({ "choice": "EvilWidget", "confidence": 0.99 }); // unknown option
    a["answers"]["s3_helps"] = json!(0.95);
    let mock = Mock::start(vec![Reply::json(200, a)]);
    let dir = tempfile::tempdir().unwrap();
    let cfg = jev_config(dir.path(), &mock.base_url(), "");
    let r = run(&dir, &cfg, Some(TEST_KEY), &[]);
    assert_eq!(r.code, 0);
    let j = &r.json;
    assert_eq!(j["selector"]["error"], Value::Null);
    for h in ["Install", "Steps"] {
        assert!(placement(j, h).is_none(), "{h}");
        assert_eq!(
            skipped(j, h).unwrap()["reason_code"],
            "invalid_answer",
            "{h}"
        );
    }
    let n = placement(j, "Notes").unwrap();
    assert_eq!(n["why"]["by"], "jev");
    assert_eq!(n["node"]["kind"], "Checklist");
    assert!(
        !r.stdout.contains("EvilWidget\""),
        "unknown option must not become a node"
    );
}

#[test]
fn missing_answers_show_nothing_for_that_section() {
    let mut a = good_answers();
    a["answers"].as_object_mut().unwrap().remove("s1_helps");
    let mock = Mock::start(vec![Reply::json(200, a)]);
    let dir = tempfile::tempdir().unwrap();
    let cfg = jev_config(dir.path(), &mock.base_url(), "");
    let r = run(&dir, &cfg, Some(TEST_KEY), &[]);
    assert_eq!(
        skipped(&r.json, "Install").unwrap()["reason_code"],
        "invalid_answer"
    );
    assert_eq!(placement(&r.json, "Steps").unwrap()["why"]["by"], "jev");
}

/// Every failure mode: exit 0, no aids, `selector.error` set, state `error`.
fn expect_fallback(
    replies: Vec<Reply>,
    extra_cfg: &str,
    key: Option<&str>,
    code: &str,
    max_requests: usize,
) -> (Run, usize) {
    let mock = Mock::start(replies);
    let dir = tempfile::tempdir().unwrap();
    let cfg = jev_config(dir.path(), &mock.base_url(), extra_cfg);
    let r = run(&dir, &cfg, key, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_no_secrets(&r);
    assert_eq!(r.json["selector"]["name"], "jev", "{}", r.stdout);
    assert_eq!(r.json["selector"]["error"], code, "{}", r.stdout);
    assert!(r.json["selector"]["message"].is_string(), "{}", r.stdout);
    assert_eq!(r.json["progress"]["state"], "error", "{}", r.stdout);
    assert!(
        r.json["placements"].as_array().unwrap().is_empty(),
        "no aids on failure"
    );
    assert!(
        mock.count() <= max_requests,
        "sent {} requests",
        mock.count()
    );
    (r, mock.count())
}

#[test]
fn fallback_unauthorized() {
    expect_fallback(
        vec![Reply::raw(401, "{\"error\":\"unauthorized\"}")],
        "",
        Some(TEST_KEY),
        "unauthorized",
        1,
    );
}

#[test]
fn payment_required_is_explained() {
    let (r, _) = expect_fallback(vec![Reply::raw(402, "{}")], "", Some(TEST_KEY), "payment_required", 1);
    assert!(r.json["selector"]["message"].as_str().unwrap().contains("credit"));
}

#[test]
fn fallback_server_error() {
    expect_fallback(
        vec![Reply::raw(503, "oops")],
        "",
        Some(TEST_KEY),
        "server_error",
        1,
    );
}

#[test]
fn fallback_broken_json() {
    expect_fallback(
        vec![Reply::raw(200, "{not json")],
        "",
        Some(TEST_KEY),
        "invalid_response",
        1,
    );
}

#[test]
fn fallback_timeout_finishes_within_limit() {
    let (r, _) = expect_fallback(
        vec![Reply::json(200, good_answers()).delay(15_000)],
        "timeout_ms = 1500",
        Some(TEST_KEY),
        "timeout",
        1,
    );
    assert!(
        r.elapsed < Duration::from_millis(5000),
        "took {:?}",
        r.elapsed
    );
}

#[test]
fn rate_limited_retries_once_honouring_retry_after_then_succeeds() {
    let mock = Mock::start(vec![
        Reply::raw(429, "{}").header("retry-after", "1"),
        Reply::json(200, good_answers()),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let cfg = jev_config(dir.path(), &mock.base_url(), "");
    let r = run(&dir, &cfg, Some(TEST_KEY), &[]);
    assert_eq!(r.code, 0);
    assert_eq!(mock.count(), 2);
    assert!(
        r.elapsed >= Duration::from_millis(1000),
        "retry-after not honoured: {:?}",
        r.elapsed
    );
    assert_eq!(r.json["selector"]["error"], Value::Null);
    assert_eq!(r.json["selector"]["requests"], 2);
}

#[test]
fn rate_limited_twice_falls_back_after_one_retry() {
    let (r, n) = expect_fallback(
        vec![Reply::raw(429, "{}").header("retry-after", "0")],
        "",
        Some(TEST_KEY),
        "rate_limited",
        2,
    );
    assert_eq!(n, 2, "exactly one retry");
    assert_eq!(
        r.json["selector"]["requests"], 2,
        "selector.requests must match what was sent"
    );
}

#[test]
fn overloaded_with_retry_after_beyond_deadline_does_not_retry() {
    let (r, n) = expect_fallback(
        vec![Reply::raw(529, "{}").header("retry-after", "30")],
        "",
        Some(TEST_KEY),
        "overloaded",
        1,
    );
    assert_eq!(n, 1);
    assert!(r.elapsed < Duration::from_millis(3000));
}

#[test]
fn fallback_connection_refused() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = jev_config(
        dir.path(),
        &format!("http://127.0.0.1:{}", closed_port()),
        "",
    );
    let r = run(&dir, &cfg, Some(TEST_KEY), &[]);
    assert_eq!(r.code, 0);
    assert_eq!(r.json["selector"]["error"], "offline");
    assert!(r.json["placements"].as_array().unwrap().is_empty());
}

#[test]
fn no_key_means_no_request() {
    let (_, n) = expect_fallback(
        vec![Reply::json(200, good_answers())],
        "",
        None,
        "missing_key",
        0,
    );
    assert_eq!(n, 0);
}

#[test]
fn default_config_without_a_key_sends_nothing_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = write(dir.path(), "config.toml", "");
    let r = run(&dir, &cfg, None, &[]);
    assert_eq!(r.code, 0);
    assert_eq!(r.json["selector"]["name"], "jev");
    assert_eq!(r.json["selector"]["error"], "missing_key");
    assert_eq!(
        r.json["selector"]["message"],
        "environment variable TYPESAFE_API_KEY is not set"
    );
    assert_eq!(r.json["selector"]["requests"], 0);
}

#[test]
fn key_in_config_file_is_rejected() {
    // `api_key` in a model entry makes that entry invalid; the value is never
    // used nor echoed.
    let mock = Mock::start(vec![Reply::json(200, good_answers())]);
    let dir = tempfile::tempdir().unwrap();
    let cfg = write(
        dir.path(),
        "config.toml",
        &format!(
            "[models.jev]\napi = \"systemone\"\nmodel = \"jev-1.14.0\"\nbase_url = \"{}\"\napi_key = \"{TEST_KEY}\"\n",
            mock.base_url()
        ),
    );
    let r = run(&dir, &cfg, None, &[]);
    assert_eq!(r.code, 0);
    assert_eq!(r.json["selector"]["error"], "config_invalid");
    assert!(
        r.json["selector"]["message"]
            .as_str()
            .unwrap()
            .contains("api_key is not allowed")
    );
    assert_eq!(mock.count(), 0);
    assert_no_secrets(&r);
}

#[test]
fn batches_split_the_sections() {
    // batch_sections = 1 → one request per asked section; the document
    // questions travel with the first one.
    let replies = vec![Reply::json(200, good_answers()); 3];
    let mock = Mock::start(replies);
    let dir = tempfile::tempdir().unwrap();
    let cfg = jev_config(dir.path(), &mock.base_url(), "batch_sections = 1");
    let r = run(&dir, &cfg, Some(TEST_KEY), &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 3);
    assert!(reqs[0].json()["questions"].get("doc_type").is_some());
    assert!(reqs[1].json()["questions"].get("doc_type").is_none());
    assert_eq!(r.json["selector"]["requests"], 3);
    assert_eq!(r.json["progress"]["state"], "done");
}

#[test]
fn microsoft_decision_uses_the_api_key_header() {
    let mock = Mock::start(vec![Reply::json(200, good_answers())]);
    let dir = tempfile::tempdir().unwrap();
    let cfg = write(
        dir.path(),
        "config.toml",
        &format!(
            "[model]\nuse = \"microsoft\"\n\n[models.microsoft]\napi = \"systemone\"\nmodel = \"my-deployment\"\nbase_url = \"{}/providers/microsoft\"\napi_key_env = \"MDPEEK_TEST_AZURE_KEY\"\nauth = \"api-key\"\n",
            mock.base_url()
        ),
    );
    let r = run(&dir, &cfg, None, &[("MDPEEK_TEST_AZURE_KEY", TEST_KEY)]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let q = &mock.requests()[0];
    assert_eq!(q.path, "/providers/microsoft/v1/systemone");
    assert_eq!(q.header("api-key"), Some(TEST_KEY));
    assert_eq!(q.header("authorization"), None);
    assert_eq!(q.json()["model"], "my-deployment");
    assert_eq!(r.json["selector"]["name"], "microsoft");
    assert!(placement(&r.json, "Install").is_some());
}

fn llm_config(dir: &std::path::Path, api: &str, base_url: &str, model: &str) -> std::path::PathBuf {
    write(
        dir,
        "config.toml",
        &format!(
            "[model]\nuse = \"m\"\n\n[models.m]\napi = \"{api}\"\nmodel = \"{model}\"\nbase_url = \"{base_url}\"\napi_key_env = \"MDPEEK_TEST_LLM_KEY\"\n"
        ),
    )
}

fn answers_text() -> String {
    format!(
        "Here you go:\n```json\n{}\n```",
        json!({ "answers": good_answers()["answers"] })
    )
}

#[test]
fn anthropic_llm_answers_in_the_same_shape() {
    let reply = json!({ "content": [{ "type": "thinking", "thinking": "" }, { "type": "text", "text": answers_text() }] });
    let mock = Mock::start(vec![Reply::json(200, reply)]);
    let dir = tempfile::tempdir().unwrap();
    let cfg = llm_config(
        dir.path(),
        "anthropic",
        &mock.base_url(),
        "claude-haiku-5-5",
    );
    let r = run(&dir, &cfg, None, &[("MDPEEK_TEST_LLM_KEY", TEST_KEY)]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_no_secrets(&r);
    let q = &mock.requests()[0];
    assert_eq!(q.path, "/v1/messages");
    assert_eq!(q.header("x-api-key"), Some(TEST_KEY));
    assert!(q.header("anthropic-version").is_some());
    let body = q.json();
    assert_eq!(body["model"], "claude-haiku-5-5");
    assert!(!q.body.contains(MARKER), "full body was sent");
    assert!(
        body["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("s2_widget")
    );
    assert_eq!(r.json["selector"]["kind"], "llm");
    assert_eq!(
        placement(&r.json, "Steps").unwrap()["why"]["candidate"],
        "StepNavigator"
    );
}

#[test]
fn openai_compatible_chat_llm() {
    let reply =
        json!({ "choices": [{ "message": { "role": "assistant", "content": answers_text() } }] });
    let mock = Mock::start(vec![Reply::json(200, reply)]);
    let dir = tempfile::tempdir().unwrap();
    let cfg = llm_config(
        dir.path(),
        "openai_chat",
        &format!("{}/inference/openai", mock.base_url()),
        "glm-5.3",
    );
    let r = run(&dir, &cfg, None, &[("MDPEEK_TEST_LLM_KEY", TEST_KEY)]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let q = &mock.requests()[0];
    assert_eq!(q.path, "/inference/openai/v1/chat/completions");
    assert_eq!(
        q.header("authorization"),
        Some(format!("Bearer {TEST_KEY}").as_str())
    );
    assert!(placement(&r.json, "Install").is_some());
}

#[test]
fn llm_prose_without_json_is_an_invalid_response() {
    let reply = json!({ "choices": [{ "message": { "content": "I think Steps would benefit from a navigator." } }] });
    let mock = Mock::start(vec![Reply::json(200, reply)]);
    let dir = tempfile::tempdir().unwrap();
    let cfg = llm_config(
        dir.path(),
        "openai_chat",
        &mock.base_url(),
        "deepseek-v4-pro",
    );
    let r = run(&dir, &cfg, None, &[("MDPEEK_TEST_LLM_KEY", TEST_KEY)]);
    assert_eq!(r.json["selector"]["error"], "invalid_response");
    assert!(r.json["placements"].as_array().unwrap().is_empty());
}

// --- Regression tests for the independent review (2026-11-27) ---

#[test]
fn model_supplied_alternative_keys_never_reach_output() {
    let evil = "<img src=x onerror=alert(1)> IGNORE PREVIOUS";
    let mut a = good_answers();
    a["answers"]["s2_widget"]["probabilities"][evil] = json!(0.5);
    a["answers"]["doc_helps"] = json!(0.9);
    a["answers"]["doc_widget"]["probabilities"] = json!({ evil: 0.5, "SemanticOutline": 0.4 });
    let mock = Mock::start(vec![Reply::json(200, a)]);
    let dir = tempfile::tempdir().unwrap();
    let cfg = jev_config(dir.path(), &mock.base_url(), "");
    let r = run(&dir, &cfg, Some(TEST_KEY), &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.json["selector"]["error"], Value::Null);
    assert!(
        !r.stdout.contains("onerror"),
        "untrusted probabilities key echoed: {}",
        r.stdout
    );
    let steps = placement(&r.json, "Steps").expect("Steps placement");
    for alt in steps["why"]["alternatives"].as_array().unwrap() {
        assert_eq!(alt[0], "DataTable:status");
    }
}

#[test]
fn hostile_retry_after_does_not_panic() {
    for v in ["1e300", "1e18", "NaN", "-5", "inf"] {
        let (r, _) = expect_fallback(
            vec![Reply::raw(429, "{}").header("retry-after", v)],
            "",
            Some(TEST_KEY),
            "rate_limited",
            2,
        );
        assert_eq!(r.code, 0, "retry-after {v}: {}", r.stderr);
    }
}

#[test]
fn long_headings_count_toward_the_state_cap() {
    let mock = Mock::start(vec![Reply::json(200, good_answers())]);
    let dir = tempfile::tempdir().unwrap();
    let long = "H".repeat(5_000);
    let mut md = String::from("# Tool\n\n");
    for i in 0..60 {
        md.push_str(&format!("## {long}{i}\n\n1. a\n2. b\n3. c\n\n"));
    }
    let file = write(dir.path(), "README.md", &md);
    let cfg = jev_config(dir.path(), &mock.base_url(), "max_state_chars = 3000");
    let out = Command::cargo_bin("mdpeek")
        .unwrap()
        .arg("--config")
        .arg(&cfg)
        .arg("select")
        .arg(&file)
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path())
        .env("TYPESAFE_API_KEY", TEST_KEY)
        .output()
        .unwrap();
    assert!(out.status.success());
    let req = mock.requests().pop().expect("one request").json();
    let state = req["state"].to_string();
    // Clipped headings (120 chars each) + features: far below the raw 300k chars.
    assert!(state.len() < 40_000, "state is {} bytes", state.len());
    for h in req["state"]["document"]["headings"].as_array().unwrap() {
        assert!(h.as_str().unwrap().chars().count() <= 120);
    }
}

#[test]
fn insecure_base_url_is_refused_before_sending_the_key() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = jev_config(dir.path(), "http://example.invalid", "");
    let r = run(&dir, &cfg, Some(TEST_KEY), &[]);
    assert_eq!(r.code, 0);
    assert_eq!(
        r.json["selector"]["error"], "config_invalid",
        "{}",
        r.stdout
    );
    assert!(
        r.json["selector"]["message"]
            .as_str()
            .unwrap()
            .contains("https")
    );
    assert_eq!(r.json["selector"]["requests"], 0);
}

#[test]
fn config_parse_errors_do_not_echo_values() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = write(
        dir.path(),
        "config.toml",
        "[model]\ntimeout_ms = \"sk-SECRETXYZ\"\n",
    );
    let file = write(dir.path(), "README.md", "# t\n");
    let out = Command::cargo_bin("mdpeek")
        .unwrap()
        .arg("--config")
        .arg(&cfg)
        .arg("select")
        .arg(&file)
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path())
        .env_remove("TYPESAFE_API_KEY")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("line 2"), "{stderr}");
    assert!(!stderr.contains("sk-SECRETXYZ"), "value echoed: {stderr}");
}
