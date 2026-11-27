//! `mdpeek serve`: GET /api/adaptive, run / stop, model switching, and the
//! WebSocket `adaptive` pushes (R6), with debounce, stale-revision discard and
//! cache reuse (memory + disk, ids only).

mod common;

use common::*;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const DOC: &str = "# Tool\n\nIntro paragraph.\n\n## Install\n\n```sh\ncargo build\ncargo install --path .\ncp a b\n```\n\n\
## Steps\n\n1. one\n2. two\n3. three\n4. four\n5. five\n\n## Notes\n\n- [ ] a\n- [ ] b\n- [ ] c\n\n## License\n\nMIT\n";

fn answers() -> Value {
    json!({ "answers": {
        "doc_type": { "choice": "readme", "confidence": 0.9 },
        "doc_helps": 0.9,
        "doc_widget": { "choice": "SemanticOutline", "confidence": 0.9 },
        "s1_helps": 0.9, "s2_helps": 0.9, "s3_helps": 0.9
    }})
}

fn get_layout(port: u16) -> Value {
    let (status, body) = http_get(port, "/api/adaptive");
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

fn fake_config(
    dir: &std::path::Path,
    extra_model: &str,
    script: Option<&str>,
) -> std::path::PathBuf {
    let script = script
        .map(|s| format!("script = \"{s}\"\n"))
        .unwrap_or_default();
    write(
        dir,
        "config.toml",
        &format!(
            "[model]\nuse = \"fake\"\n{extra_model}\n\n[models.fake]\napi = \"fake\"\nmodel = \"fake-1\"\n{script}\n[models.other]\napi = \"fake\"\nmodel = \"fake-2\"\n"
        ),
    )
}

fn run(port: u16) {
    let (status, _) = http_post(port, "/api/adaptive/run", "");
    assert_eq!(status, 202);
}

#[test]
fn get_never_asks_a_model_and_run_fills_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "README.md", DOC);
    let cfg = fake_config(dir.path(), "debounce_ms = 100", None);
    let srv = spawn_serve(dir.path(), &file, &cfg, &[]);
    let mut ws = Ws::connect(srv.port);
    let a = get_layout(srv.port);
    assert_eq!(a["version"], 2);
    assert_eq!(a["selector"]["name"], "fake");
    assert_eq!(a["progress"]["state"], "idle");
    assert!(
        a["placements"].as_array().unwrap().is_empty(),
        "nothing before a run"
    );

    run(srv.port);
    let done = ws
        .wait_settled(Duration::from_secs(10))
        .expect("run pushed");
    assert_eq!(done["layout"]["progress"]["state"], "done");
    assert!(
        done["layout"]["placements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["anchor"]["heading"] == "Install")
    );

    let b = get_layout(srv.port);
    assert_eq!(b["progress"]["state"], "done");
    assert_eq!(b, done["layout"]);
    // Existing endpoints still work.
    assert_eq!(http_get(srv.port, "/").0, 200);
    assert_eq!(http_get(srv.port, "/api/tree").0, 200);
    assert_eq!(http_get(srv.port, "/api/gui").0, 404, "removed with mdpeek gen");
}

#[test]
fn decisions_survive_a_restart_on_disk_without_document_text() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "README.md", DOC);
    let cfg = fake_config(dir.path(), "", None);
    {
        let srv = spawn_serve(dir.path(), &file, &cfg, &[]);
        let mut ws = Ws::connect(srv.port);
        run(srv.port);
        ws.wait_settled(Duration::from_secs(10))
            .expect("run pushed");
    }
    // HOME = dir → $HOME/.cache/mdpeek
    let cache = dir.path().join(".cache/mdpeek/select");
    let mut all = String::new();
    for d in std::fs::read_dir(&cache).expect("cache dir").flatten() {
        for f in std::fs::read_dir(d.path()).unwrap().flatten() {
            all.push_str(&std::fs::read_to_string(f.path()).unwrap());
        }
    }
    assert!(all.contains("ConfigViewer"), "{all}");
    for text in ["cargo", "Install", "Steps", "one", "MIT"] {
        assert!(!all.contains(text), "document text {text:?} on disk: {all}");
    }
    // A new process shows the cached aids before any run.
    let srv = spawn_serve(dir.path(), &file, &cfg, &[]);
    let l = get_layout(srv.port);
    assert_eq!(l["progress"]["state"], "done");
    assert!(!l["placements"].as_array().unwrap().is_empty());
    assert!(
        l["placements"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["why"]["cached"] == true)
    );
}

#[test]
fn jev_serve_reuses_unchanged_sections_and_pushes_over_ws() {
    let mock = Mock::start(vec![Reply::json(200, answers())]);
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "README.md", DOC);
    let cfg = jev_config(dir.path(), &mock.base_url(), "debounce_ms = 200");
    let srv = spawn_serve(dir.path(), &file, &cfg, &[("TYPESAFE_API_KEY", TEST_KEY)]);

    let mut ws = Ws::connect(srv.port);
    let first = get_layout(srv.port);
    assert!(!first.to_string().contains(TEST_KEY));
    assert_eq!(mock.count(), 0, "GET must not ask the model");
    run(srv.port);
    let pushed = ws
        .wait_settled(Duration::from_secs(10))
        .expect("jev layout pushed");
    assert_eq!(pushed["layout"]["selector"]["name"], "jev");
    assert_eq!(pushed["layout"]["progress"]["state"], "done");
    let second = get_layout(srv.port);
    let third = get_layout(srv.port);
    assert_eq!(mock.count(), 1, "GETs must not re-ask Jev");
    assert_eq!(second, third);

    // Edit only the Install section → exactly one new request with one section.
    // The poll watcher compares mtimes at 1 s granularity.
    std::thread::sleep(Duration::from_millis(1100));
    std::fs::write(&file, DOC.replace("cp a b", "cp a b\nmake check")).unwrap();
    let rev0 = pushed["revision"].as_u64().unwrap();
    let msg = loop {
        let m = ws
            .wait_settled(Duration::from_secs(15))
            .unwrap_or_else(|| panic!("no adaptive push; log:\n{}", srv.log.lock().unwrap()));
        if m["revision"].as_u64().unwrap() > rev0 {
            break m;
        }
    };
    let rev = msg["revision"].as_u64().unwrap();
    assert_eq!(msg["layout"]["revision"], rev);
    let layout = &msg["layout"];
    assert_eq!(mock.count(), 2, "only the changed section is re-asked");
    let last = mock.requests().pop().unwrap().json();
    let secs = last["state"]["sections"].as_array().unwrap();
    assert_eq!(secs.len(), 1, "{secs:?}");
    assert_eq!(secs[0]["heading"], "Install");
    assert!(layout["cache"]["hits"].as_u64().unwrap() >= 2);
    let install = layout["placements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["anchor"]["heading"] == "Install")
        .unwrap();
    assert!(
        install["node"]["content"]
            .as_str()
            .unwrap()
            .contains("make check")
    );
    assert!(!msg.to_string().contains(TEST_KEY));
    let log = srv.log.lock().unwrap().clone();
    assert!(!log.contains(TEST_KEY));
    assert!(
        !log.contains("cargo install"),
        "document text leaked to log"
    );
}

#[test]
fn stop_cancels_and_resume_asks_only_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "README.md", DOC);
    write(dir.path(), "slow.json", r#"{ "delay_ms": 1500 }"#);
    let cfg = fake_config(
        dir.path(),
        "batch_sections = 1\ndebounce_ms = 100",
        Some("slow.json"),
    );
    let srv = spawn_serve(dir.path(), &file, &cfg, &[]);
    let mut ws = Ws::connect(srv.port);
    run(srv.port);
    // First batch lands (running), then stop while the second is in flight.
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let v = ws
            .wait_adaptive(end.saturating_duration_since(Instant::now()))
            .expect("progress push");
        if v["layout"]["progress"]["state"] == "running"
            && v["layout"]["progress"]["done"].as_u64().unwrap() >= 1
        {
            break;
        }
    }
    let t = Instant::now();
    assert_eq!(http_post(srv.port, "/api/adaptive/stop", "").0, 202);
    let stopped = ws
        .wait_settled(Duration::from_secs(5))
        .expect("stopped push");
    assert!(
        t.elapsed() < Duration::from_millis(1000),
        "in-flight request was not abandoned: {:?}",
        t.elapsed()
    );
    let l = &stopped["layout"];
    assert_eq!(l["progress"]["state"], "stopped");
    let done = l["progress"]["done"].as_u64().unwrap();
    let total = l["progress"]["total"].as_u64().unwrap();
    assert!(done < total, "{done}/{total}");
    assert_eq!(get_layout(srv.port)["progress"]["state"], "stopped");

    // Resume: finishes, and the sections answered before the stop are cached.
    run(srv.port);
    let fin = ws
        .wait_settled(Duration::from_secs(20))
        .expect("resumed run");
    assert_eq!(fin["layout"]["progress"]["state"], "done");
    assert!(fin["layout"]["cache"]["hits"].as_u64().unwrap() >= 1);
}

#[test]
fn models_endpoint_switch_and_default() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "README.md", DOC);
    let cfg = write(
        dir.path(),
        "config.toml",
        "# keep me\n[model]\nuse = \"fake\"\ndebounce_ms = 100\n\n[models.fake]\napi = \"fake\"\n\n[models.other]\napi = \"fake\"\nmodel = \"fake-2\"\n\n[models.broken]\napi = \"systemone\"\nmodel = \"x\"\napi_key_env = \"MDPEEK_TEST_UNSET_KEY\"\n",
    );
    let srv = spawn_serve(dir.path(), &file, &cfg, &[]);
    let mut ws = Ws::connect(srv.port);
    let (st, body) = http_get(srv.port, "/api/models");
    assert_eq!(st, 200);
    let m: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(m["default"], "fake");
    assert_eq!(m["active"], "fake");
    let names: Vec<_> = m["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["fake", "other", "broken"]);
    assert_eq!(
        m["models"][2]["problem"],
        "environment variable MDPEEK_TEST_UNSET_KEY is not set"
    );
    assert!(m["models"][0]["problem"].is_null());

    // Switch for this session: the default and the file stay as they are.
    let (st, body) = http_post(srv.port, "/api/models/active", r#"{"name":"other"}"#);
    assert_eq!(st, 200, "{body}");
    let m: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(m["active"], "other");
    assert_eq!(m["default"], "fake");
    assert!(
        std::fs::read_to_string(&cfg)
            .unwrap()
            .contains("use = \"fake\"")
    );
    run(srv.port);
    let l = ws
        .wait_settled(Duration::from_secs(10))
        .expect("run with other");
    assert_eq!(l["layout"]["selector"]["name"], "other");

    // An invalid model: the run ends with the reason.
    http_post(srv.port, "/api/models/active", r#"{"name":"broken"}"#);
    let l = ws
        .wait_settled(Duration::from_secs(10))
        .expect("run with broken");
    assert_eq!(l["layout"]["progress"]["state"], "error");
    assert_eq!(l["layout"]["selector"]["error"], "missing_key");

    assert_eq!(
        http_post(srv.port, "/api/models/active", r#"{"name":"nope"}"#).0,
        400
    );

    // Make "other" the default: written to the config, comments kept.
    let (st, body) = http_post(srv.port, "/api/models/default", r#"{"name":"other"}"#);
    assert_eq!(st, 200, "{body}");
    let m: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(m["default"], "other");
    let text = std::fs::read_to_string(&cfg).unwrap();
    assert!(text.starts_with("# keep me\n"), "{text}");
    assert!(text.contains("use = \"other\""), "{text}");
    assert!(text.contains("[models.broken]"), "{text}");
}

#[test]
fn edits_wait_for_the_debounce() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "README.md", DOC);
    let cfg = fake_config(dir.path(), "", None); // default debounce 800 ms
    let srv = spawn_serve(dir.path(), &file, &cfg, &[]);
    let mut ws = Ws::connect(srv.port);
    run(srv.port);
    ws.wait_settled(Duration::from_secs(10)).expect("first run");
    std::thread::sleep(Duration::from_millis(1100));
    std::fs::write(&file, format!("{DOC}\n## More\n\ntext\n")).unwrap();
    // First the immediate body update, then the adaptive push ≥ 800 ms later.
    let end = Instant::now() + Duration::from_secs(10);
    let mut t_update = None;
    while Instant::now() < end {
        let t = ws
            .next_text(Duration::from_secs(10))
            .expect("no ws message");
        let v: Value = serde_json::from_str(&t).unwrap();
        if v["type"] == "update" && t_update.is_none() {
            t_update = Some(Instant::now());
        }
        if v["type"] == "adaptive"
            && let Some(tu) = t_update
        {
            let dt = tu.elapsed();
            assert!(dt >= Duration::from_millis(750), "pushed after {dt:?}");
            if v["layout"]["progress"]["state"] == "done" {
                assert!(
                    v["layout"]["skipped"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|s| s["anchor"]["heading"] == "More")
                );
                return;
            }
        }
    }
    panic!("no adaptive push");
}

#[test]
fn stale_revision_results_are_discarded() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "README.md", DOC);
    write(
        dir.path(),
        "fake.json",
        r#"{ "model": "fake-slow", "delay_ms": 4000 }"#,
    );
    let cfg = fake_config(
        dir.path(),
        "timeout_ms = 10000\ndebounce_ms = 100",
        Some("fake.json"),
    );
    let srv = spawn_serve(dir.path(), &file, &cfg, &[]);
    let mut ws = Ws::connect(srv.port);
    run(srv.port);
    let start = ws.wait_settled(Duration::from_secs(15)).expect("first run");
    let rev0 = start["revision"].as_u64().unwrap();

    // Edit A; once the server has seen it (body update), edit B while A's
    // 4-second selection is still running.
    std::thread::sleep(Duration::from_millis(1100));
    std::fs::write(&file, DOC.replace("cp a b", "cp a b\necho edit-A")).unwrap();
    loop {
        let t = ws
            .next_text(Duration::from_secs(10))
            .expect("no update for edit A");
        if t.contains("\"update\"") {
            break;
        }
    }
    std::thread::sleep(Duration::from_millis(1100));
    std::fs::write(
        &file,
        DOC.replace("cp a b", "cp a b\necho edit-A\necho edit-B-longer"),
    )
    .unwrap();

    let pushes = ws.adaptive_for(Duration::from_secs(12));
    let settled: Vec<_> = pushes
        .iter()
        .filter(|p| p["layout"]["progress"]["state"] == "done")
        .collect();
    assert_eq!(
        settled.len(),
        1,
        "stale revision was pushed: {:?}",
        pushes
            .iter()
            .map(|p| (&p["revision"], &p["layout"]["progress"]["state"]))
            .collect::<Vec<_>>()
    );
    let p = settled[0];
    assert_eq!(p["revision"].as_u64().unwrap(), rev0 + 2);
    assert!(p.to_string().contains("edit-B-longer"));
    assert!(
        pushes
            .iter()
            .all(|p| p["revision"].as_u64().unwrap() != rev0 + 1
                || p["layout"]["progress"]["state"] != "done")
    );
}
