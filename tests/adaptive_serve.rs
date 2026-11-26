//! `mdpeek serve`: GET /api/adaptive + WebSocket `adaptive` pushes (R6), with
//! debounce, stale-revision discard, in-memory cache reuse and no disk writes.

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

#[test]
fn get_adaptive_returns_layout_and_reuses_cache() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "README.md", DOC);
    let cfg = write(dir.path(), "config.toml", "");
    let srv = spawn_serve(dir.path(), &file, &cfg, &[]);
    let a = get_layout(srv.port);
    assert_eq!(a["version"], 1);
    assert_eq!(a["selector"]["provider"], "rules");
    assert!(a["placements"].as_array().unwrap().iter().any(|p| p["anchor"]["heading"] == "Install"));
    std::thread::sleep(Duration::from_millis(1200)); // let the startup selection finish
    let b = get_layout(srv.port);
    assert_eq!(b["cache"]["misses"], 0, "{}", b["cache"]);
    assert!(b["cache"]["hits"].as_u64().unwrap() >= 3);
    assert!(b["placements"].as_array().unwrap().iter().filter(|p| p["scope"] == "section").all(|p| p["why"]["cached"] == true));
    // Existing endpoints still work.
    assert_eq!(http_get(srv.port, "/").0, 200);
    assert_eq!(http_get(srv.port, "/api/tree").0, 200);
    assert_eq!(http_get(srv.port, "/api/gui").0, 200);
}

#[test]
fn jev_serve_reuses_unchanged_sections_and_pushes_over_ws() {
    let mock = Mock::start(vec![Reply::json(200, answers())]);
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "README.md", DOC);
    let cfg = jev_config(dir.path(), &mock.base_url(), "debounce_ms = 200");
    let srv = spawn_serve(dir.path(), &file, &cfg, &[("TYPESAFE_API_KEY", TEST_KEY)]);

    // GET never waits on the network: until the Jev selection for the current
    // revision exists it answers with a rules layout marked `pending`, and the
    // real layout arrives over the WebSocket.
    let mut ws = Ws::connect(srv.port);
    let first = get_layout(srv.port);
    assert!(!first.to_string().contains(TEST_KEY));
    if first["selector"]["fallback"] == "pending" {
        assert_eq!(first["selector"]["provider"], "rules");
        assert_eq!(first["selector"]["requested"], "jev");
        let pushed = ws
            .wait_adaptive(Duration::from_secs(10))
            .expect("jev layout pushed after a pending GET");
        assert_eq!(pushed["layout"]["selector"]["provider"], "jev");
    } else {
        assert_eq!(first["selector"]["provider"], "jev");
    }
    let end = Instant::now() + Duration::from_secs(10);
    while mock.count() < 1 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(500));
    // Later GETs reuse the computed layout: no new external request.
    let second = get_layout(srv.port);
    let third = get_layout(srv.port);
    assert_eq!(mock.count(), 1, "GETs must not re-ask Jev; single-flight");
    assert_eq!(second["selector"]["provider"], "jev");
    assert_eq!(second, third);
    // Drain any revision-1 pushes (on-demand and/or watch) before editing.
    let _ = ws.adaptive_for(Duration::from_millis(1500));

    // Edit only the Install section → exactly one new request with one section.
    // The poll watcher compares mtimes at 1 s granularity.
    std::thread::sleep(Duration::from_millis(1100));
    std::fs::write(&file, DOC.replace("cp a b", "cp a b\nmake check")).unwrap();
    let msg = ws.wait_adaptive(Duration::from_secs(15)).unwrap_or_else(|| panic!("no adaptive push; log:\n{}", srv.log.lock().unwrap()));
    assert_eq!(msg["type"], "adaptive");
    let rev = msg["revision"].as_u64().unwrap();
    assert!(rev >= 2);
    assert_eq!(msg["layout"]["revision"], rev);
    let layout = &msg["layout"];
    assert_eq!(mock.count(), 2, "only the changed section is re-asked");
    let last = mock.requests().pop().unwrap().json();
    let secs = last["state"]["sections"].as_array().unwrap();
    assert_eq!(secs.len(), 1, "{secs:?}");
    assert_eq!(secs[0]["heading"], "Install");
    assert!(layout["cache"]["hits"].as_u64().unwrap() >= 2);
    let install = layout["placements"].as_array().unwrap().iter().find(|p| p["anchor"]["heading"] == "Install").unwrap();
    assert!(install["node"]["content"].as_str().unwrap().contains("make check"));
    assert!(!msg.to_string().contains(TEST_KEY));

    // Nothing persisted: only the files we wrote exist.
    let mut names: Vec<String> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    names.sort();
    assert_eq!(names, ["README.md", "config.toml"]);
    let log = srv.log.lock().unwrap().clone();
    assert!(!log.contains(TEST_KEY));
    assert!(!log.contains("cargo install"), "document text leaked to log");
}

#[test]
fn push_waits_for_debounce() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "README.md", DOC);
    let cfg = write(dir.path(), "config.toml", "");
    let srv = spawn_serve(dir.path(), &file, &cfg, &[]);
    let mut ws = Ws::connect(srv.port);
    let _ = ws.adaptive_for(Duration::from_millis(1500)); // drain startup push
    std::fs::write(&file, format!("{DOC}\n## More\n\ntext\n")).unwrap();
    // First the immediate body update, then the adaptive push ≥ 800 ms later.
    let end = Instant::now() + Duration::from_secs(10);
    let mut t_update = None;
    while Instant::now() < end {
        let t = ws.next_text(Duration::from_secs(10)).expect("no ws message");
        let v: Value = serde_json::from_str(&t).unwrap();
        if v["type"] == "update" && t_update.is_none() {
            t_update = Some(Instant::now());
        }
        if v["type"] == "adaptive" {
            let dt = t_update.expect("adaptive before update").elapsed();
            assert!(dt >= Duration::from_millis(750), "pushed after {dt:?}");
            assert!(v["layout"]["skipped"].as_array().unwrap().iter().any(|s| s["anchor"]["heading"] == "More"));
            return;
        }
    }
    panic!("no adaptive push");
}

#[test]
fn stale_revision_results_are_discarded() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "README.md", DOC);
    write(dir.path(), "fake.json", r#"{ "model": "fake-slow", "delay_ms": 4000, "default": "rules" }"#);
    let cfg = write(dir.path(), "config.toml", "[select]\nprovider = \"fake\"\nfake_script = \"fake.json\"\ntimeout_ms = 10000\ndebounce_ms = 100\n");
    let srv = spawn_serve(dir.path(), &file, &cfg, &[]);
    let mut ws = Ws::connect(srv.port);
    let start = ws.wait_adaptive(Duration::from_secs(15)).expect("startup push");
    let rev0 = start["revision"].as_u64().unwrap();
    assert_eq!(start["layout"]["selector"]["provider"], "fake");

    // Edit A; once the server has seen it (body update), edit B while A's
    // 4-second selection is still running.
    std::fs::write(&file, DOC.replace("cp a b", "cp a b\necho edit-A")).unwrap();
    loop {
        let t = ws.next_text(Duration::from_secs(10)).expect("no update for edit A");
        if t.contains("\"update\"") {
            break;
        }
    }
    std::thread::sleep(Duration::from_millis(1000));
    std::fs::write(&file, DOC.replace("cp a b", "cp a b\necho edit-A\necho edit-B-longer")).unwrap();

    let pushes = ws.adaptive_for(Duration::from_secs(12));
    assert_eq!(pushes.len(), 1, "stale revision was pushed: {:?}", pushes.iter().map(|p| &p["revision"]).collect::<Vec<_>>());
    let p = &pushes[0];
    assert_eq!(p["revision"].as_u64().unwrap(), rev0 + 2);
    assert!(p.to_string().contains("edit-B-longer"));
    let log = srv.log.lock().unwrap().clone();
    assert!(log.contains(&format!("discarded stale revision {}", rev0 + 1)), "{log}");
}
