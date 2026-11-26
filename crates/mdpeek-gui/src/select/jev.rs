//! TypeSafe AI **Jev** selector (design §0 / §3.3).
//!
//! One `POST {base_url}/v1/systemone` per document. The state holds only the
//! headings, a bounded excerpt (default 600 chars) and deterministic features
//! per section — never the full text. Questions:
//!
//! - `doc_type` — `choice` over document kinds,
//! - `doc_helps` (`noul`) + `doc_widget` (`choice`, omitted with 1 candidate),
//! - per section `sN_helps` (`noul`, companion — Jev has no "none" type) and
//!   `sN_widget` (`choice` over the *prefiltered* candidates only).
//!
//! The answer space is enumerated option ids; nothing Jev returns becomes
//! content. Malformed answers become [`Answer::Invalid`] for that section only.
//!
//! Logging: status, latency, counts and fallback codes only — never document
//! text, excerpts, question text or the API key.

use std::time::Instant;

use serde_json::{Map, Value, json};

use super::{
    Answer, Candidate, SelectConfig, SelectError, SelectionDecisions, SelectionRequest, Selector,
};

pub const DOC_TYPES: &[(&str, &str)] = &[
    ("readme", "Project README: what it is, install, usage"),
    (
        "design_doc",
        "Design document: architecture, trade-offs, risks",
    ),
    (
        "adr",
        "Architecture decision record: context, decision, consequences",
    ),
    ("runbook", "Operational runbook / procedure for operators"),
    (
        "minutes",
        "Meeting minutes: attendees, discussion, decisions, action items",
    ),
    ("changelog", "Changelog / release notes ordered by version"),
    ("procedure", "Step-by-step work instructions / manual"),
    ("novel", "Narrative fiction"),
    ("generic", "None of the above"),
];

/// Max characters per heading sent to Jev (document list and per section).
pub const HEADING_CHARS: usize = 120;

const INSTRUCTIONS: &str = "You choose reading aids for a Markdown viewer. \
Each section lists candidate widgets that were pre-validated against its content. \
Prefer plain text unless a widget clearly helps a reader. \
Never assume facts beyond the given excerpts and features.";

pub struct JevSelector {
    model: String,
    url: String,
    key: String,
    /// HTTP attempts actually sent by the last `select` (incl. a retry).
    sent: std::sync::atomic::AtomicU32,
}

impl JevSelector {
    pub fn new(cfg: &SelectConfig, key: String) -> Result<Self, SelectError> {
        if !cfg!(feature = "llm") {
            return Err(SelectError::NotBuilt);
        }
        Ok(JevSelector {
            model: cfg.model.clone(),
            url: format!("{}/v1/systemone", cfg.base_url.trim_end_matches('/')),
            key,
            sent: std::sync::atomic::AtomicU32::new(0),
        })
    }
}

fn criteria(cands: &[Candidate]) -> Value {
    let mut m = Map::new();
    for c in cands {
        m.insert(c.id.to_string(), Value::String(c.description.to_string()));
    }
    Value::Object(m)
}

/// Build the System One request body (pure; unit-tested).
pub fn build_request(req: &SelectionRequest<'_>, model: &str) -> Value {
    let cfg = req.cfg;
    let mut used = 0usize;
    let mut sections = Vec::new();
    let mut questions = Map::new();
    if req.ask_document {
        let mut c = Map::new();
        for (k, v) in DOC_TYPES {
            c.insert((*k).into(), Value::String((*v).into()));
        }
        questions.insert(
            "doc_type".into(),
            json!({ "type": "choice", "instructions": "Which kind of document is this?", "criteria": c }),
        );
        let dc = &req.plan.doc_candidates;
        if !dc.is_empty() {
            questions.insert(
                "doc_helps".into(),
                json!({ "type": "noul", "instructions": "Would a document-level overview panel help a reader more than the plain document?" }),
            );
            if dc.len() > 1 {
                questions.insert(
                    "doc_widget".into(),
                    json!({ "type": "choice", "instructions": "Which document-level panel fits best?", "criteria": criteria(dc) }),
                );
            }
        }
    }
    // Headings count toward the same state budget as excerpts and are clipped,
    // so long or numerous headings cannot carry body text past the cap.
    let clip = |s: &str| -> String { s.chars().take(HEADING_CHARS).collect() };
    let mut doc_headings = Vec::new();
    for h in req.plan.headings.iter().take(80) {
        let h = clip(h);
        let n = h.chars().count();
        if used + n > cfg.max_state_chars {
            break;
        }
        used += n;
        doc_headings.push(h);
    }
    for s in &req.sections {
        let heading = clip(&s.heading);
        used += heading.chars().count();
        let excerpt = if used + s.excerpt.chars().count() <= cfg.max_state_chars {
            used += s.excerpt.chars().count();
            Some(s.excerpt.clone())
        } else {
            None
        };
        let mut sec = json!({
            "id": s.id,
            "heading": heading,
            "level": s.level,
            "features": s.features,
            "candidates": s.candidates.iter().map(|c| c.id).collect::<Vec<_>>(),
        });
        if let Some(e) = excerpt {
            sec["excerpt"] = Value::String(e);
        }
        sections.push(sec);
        questions.insert(
            format!("{}_helps", s.id),
            json!({ "type": "noul", "instructions": format!("Section {}: would one of its candidate widgets help a reader more than the plain text?", s.id) }),
        );
        if s.candidates.len() > 1 {
            questions.insert(
                format!("{}_widget", s.id),
                json!({ "type": "choice", "instructions": format!("Section {}: which candidate widget fits best?", s.id), "criteria": criteria(&s.candidates) }),
            );
        }
    }
    json!({
        "model": model,
        "state": {
            "document": {
                "filename_hint": req.filename.unwrap_or(""),
                "headings": doc_headings,
                "rules_doc_type": req.plan.rules_doc_type,
            },
            "sections": sections,
            // The System One request has no top-level `instructions` (OpenAPI
            // 0.2.0: model/state/questions/assets only — the live API answers
            // 400 otherwise), so the viewer-wide guidance travels in the state.
            "guidance": INSTRUCTIONS,
        },
        "questions": questions,
    })
}

fn unit(v: &Value) -> Option<f32> {
    let f = v.as_f64()?;
    (0.0..=1.0).contains(&f).then_some(f as f32)
}

/// Accept a `noul` answer as a bare probability or `{noul|probability|yes: p}`.
fn noul(v: Option<&Value>) -> Result<f32, String> {
    let v = v.ok_or("missing")?;
    if let Some(p) = unit(v) {
        return Ok(p);
    }
    for k in ["noul", "probability", "yes", "p"] {
        if let Some(p) = v.get(k).and_then(unit) {
            return Ok(p);
        }
    }
    Err("type".into())
}

type Choice = (String, f32, Vec<(String, f32)>);

fn choice(v: Option<&Value>) -> Result<Choice, String> {
    let v = v.ok_or("missing")?;
    let c = v.get("choice").and_then(Value::as_str).ok_or("type")?;
    let conf = v.get("confidence").and_then(unit).ok_or("type")?;
    let mut alts: Vec<(String, f32)> = v
        .get("probabilities")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter(|(k, _)| k.as_str() != c)
                .filter_map(|(k, p)| unit(p).map(|p| (k.clone(), p)))
                .collect()
        })
        .unwrap_or_default();
    alts.sort_by(|a, b| b.1.total_cmp(&a.1));
    alts.truncate(3);
    Ok((c.to_string(), conf, alts))
}

fn answer(
    answers: &Map<String, Value>,
    helps_q: &str,
    widget_q: &str,
    cands: &[Candidate],
) -> Answer {
    let helps = match noul(answers.get(helps_q)) {
        Ok(h) => h,
        Err(e) => return Answer::Invalid(e),
    };
    if cands.len() == 1 {
        return Answer::Raw {
            candidate: cands[0].id.to_string(),
            confidence: 1.0,
            helps,
            alternatives: Vec::new(),
        };
    }
    match choice(answers.get(widget_q)) {
        Ok((c, conf, mut alts)) if cands.iter().any(|x| x.id == c) => {
            // Only enumerated candidate ids may reach the client: any other
            // `probabilities` key is untrusted model text and is dropped.
            alts.retain(|(k, _)| cands.iter().any(|x| x.id == k));
            Answer::Raw {
                candidate: c,
                confidence: conf,
                helps,
                alternatives: alts,
            }
        }
        Ok(_) => Answer::Invalid("unknown_option".into()),
        Err(e) => Answer::Invalid(e),
    }
}

/// Map a System One response to decisions (pure; unit-tested).
pub fn parse_response(
    body: &Value,
    req: &SelectionRequest<'_>,
) -> Result<SelectionDecisions, SelectError> {
    let answers = body
        .get("answers")
        .and_then(Value::as_object)
        .ok_or(SelectError::InvalidResponse)?;
    let mut d = SelectionDecisions {
        requests: 1,
        ..Default::default()
    };
    for s in &req.sections {
        let a = answer(
            answers,
            &format!("{}_helps", s.id),
            &format!("{}_widget", s.id),
            &s.candidates,
        );
        d.sections.insert(s.id.clone(), a);
    }
    if req.ask_document {
        if let Ok((v, c, _)) = choice(answers.get("doc_type"))
            && DOC_TYPES.iter().any(|(k, _)| *k == v)
        {
            d.doc_type = Some((v, c));
        }
        if !req.plan.doc_candidates.is_empty() {
            d.document = Some(answer(
                answers,
                "doc_helps",
                "doc_widget",
                &req.plan.doc_candidates,
            ));
        }
    }
    Ok(d)
}

impl Selector for JevSelector {
    fn id(&self) -> (&'static str, String) {
        ("jev", self.model.clone())
    }

    fn last_requests(&self) -> Option<u32> {
        Some(self.sent.load(std::sync::atomic::Ordering::SeqCst))
    }

    fn select(
        &self,
        req: &SelectionRequest<'_>,
        deadline: Instant,
    ) -> Result<SelectionDecisions, SelectError> {
        let body = build_request(req, &self.model);
        let n_questions = body["questions"].as_object().map(|m| m.len()).unwrap_or(0);
        let started = Instant::now();
        self.sent.store(0, std::sync::atomic::Ordering::SeqCst);
        let res = http::post(&self.url, &self.key, &body, deadline, &self.sent);
        let ms = started.elapsed().as_millis();
        match res {
            Ok((status, attempts, value)) => {
                let parsed = parse_response(&value, req);
                tracing::info!(
                    target: "mdpeek::select",
                    "jev: status {status} in {ms}ms, attempts {attempts}, sections {}, questions {n_questions}{}",
                    req.sections.len(),
                    if parsed.is_err() { ", invalid response" } else { "" }
                );
                parsed.map(|mut d| {
                    d.requests = attempts;
                    d
                })
            }
            Err(e) => {
                tracing::warn!(
                    target: "mdpeek::select",
                    "jev: failed after {ms}ms ({}{}); falling back to rules",
                    e.code(),
                    match e {
                        SelectError::Http(s) | SelectError::Server(s) => format!(", HTTP {s}"),
                        _ => String::new(),
                    }
                );
                Err(e)
            }
        }
    }
}

#[cfg(feature = "llm")]
mod http {
    use std::time::{Duration, Instant};

    use serde_json::Value;

    use super::SelectError;

    /// POST with an overall deadline. 429/529 are retried at most once,
    /// honouring `retry-after` only when it fits inside the deadline.
    pub fn post(
        url: &str,
        key: &str,
        body: &Value,
        deadline: Instant,
        sent: &std::sync::atomic::AtomicU32,
    ) -> Result<(u16, u32, Value), SelectError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| SelectError::Network)?;
        rt.block_on(async {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, post_async(url, key, body, deadline, sent)).await
            {
                Ok(r) => r,
                Err(_) => Err(SelectError::Timeout),
            }
        })
    }

    async fn post_async(
        url: &str,
        key: &str,
        body: &Value,
        deadline: Instant,
        sent: &std::sync::atomic::AtomicU32,
    ) -> Result<(u16, u32, Value), SelectError> {
        let client = reqwest::Client::builder()
            .user_agent("mdpeek-select")
            .build()
            .map_err(|_| SelectError::Network)?;
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            sent.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let resp = client
                .post(url)
                .bearer_auth(key)
                .json(body)
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        SelectError::Timeout
                    } else {
                        SelectError::Network
                    }
                })?;
            let status = resp.status().as_u16();
            match status {
                200..=299 => {
                    let bytes = resp.bytes().await.map_err(|e| {
                        if e.is_timeout() {
                            SelectError::Timeout
                        } else {
                            SelectError::Network
                        }
                    })?;
                    let v: Value =
                        serde_json::from_slice(&bytes).map_err(|_| SelectError::InvalidResponse)?;
                    return Ok((status, attempt, v));
                }
                401 | 403 => return Err(SelectError::Unauthorized),
                429 | 529 => {
                    let err = if status == 429 {
                        SelectError::RateLimited
                    } else {
                        SelectError::Overloaded
                    };
                    if attempt >= 2 {
                        return Err(err);
                    }
                    let secs = resp
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.trim().parse::<f64>().ok())
                        .filter(|s| s.is_finite() && *s >= 0.0)
                        .unwrap_or(0.5);
                    // Compare in f64 *before* building a Duration, so a malformed
                    // or hostile header (e.g. 1e300) can neither panic nor
                    // overflow, and we never wait past our own deadline.
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if secs >= remaining.as_secs_f64() {
                        return Err(err);
                    }
                    let Ok(wait) = Duration::try_from_secs_f64(secs) else {
                        return Err(err);
                    };
                    tokio::time::sleep(wait).await;
                }
                500..=599 => return Err(SelectError::Server(status)),
                _ => return Err(SelectError::Http(status)),
            }
        }
    }
}

#[cfg(not(feature = "llm"))]
mod http {
    use std::time::Instant;

    use serde_json::Value;

    use super::SelectError;

    pub fn post(
        _url: &str,
        _key: &str,
        _body: &Value,
        _deadline: Instant,
        _sent: &std::sync::atomic::AtomicU32,
    ) -> Result<(u16, u32, Value), SelectError> {
        Err(SelectError::NotBuilt)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{SelectConfig, sections};
    use super::*;

    const MD: &str = "# Tool\n\nIntro.\n\n## Install\n\n```sh\ncargo build\ncargo install --path .\n```\n\n## Steps\n\n1. one\n2. two\n3. three\n\n| a | status |\n|---|---|\n| 1 | ok |\n| 2 | ok |\n| 3 | ng |\n\n## License\n\nMIT\n";

    fn with_req<R>(f: impl FnOnce(&SelectionRequest<'_>) -> R) -> R {
        let a = mdpeek_analyzer::analyze(MD, Some("README.md"));
        let plan = sections::plan_document(MD, Some("README.md"), &a, 600);
        let cfg = SelectConfig::default();
        let secs = plan
            .sections
            .iter()
            .filter(|s| !s.candidates.is_empty())
            .collect();
        let req = SelectionRequest {
            plan: &plan,
            filename: Some("README.md"),
            sections: secs,
            ask_document: true,
            cfg: &cfg,
        };
        f(&req)
    }

    #[test]
    fn request_has_only_candidates_and_companion_noul() {
        with_req(|req| {
            let body = build_request(req, "jev-1.14.0");
            let q = body["questions"].as_object().unwrap();
            // Install (1 candidate) → helps only; Steps (3 candidates) → helps + widget.
            assert!(q.contains_key("s1_helps"));
            assert!(!q.contains_key("s1_widget"));
            assert!(q.contains_key("s2_helps"));
            let crit = q["s2_widget"]["criteria"].as_object().unwrap();
            assert!(crit.contains_key("StepNavigator"));
            assert!(crit.contains_key("DataTable:status"));
            assert!(!crit.contains_key("Checklist"));
            // License has no candidates → not asked.
            assert!(!q.contains_key("s3_helps"));
            assert!(body["state"]["sections"][0].get("excerpt").is_some());
        });
    }

    #[test]
    fn request_matches_system_one_schema_shape() {
        // OpenAPI 0.2.0 SystemOneRequest = {model, state, questions, assets?};
        // questions are {type, instructions?, criteria?}. Anything else is a
        // live-API 400 that a lenient mock would not catch.
        with_req(|req| {
            let body = build_request(req, "jev-1.14.0");
            let top: Vec<&str> = body
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            for k in &top {
                assert!(
                    ["model", "state", "questions", "assets"].contains(k),
                    "unexpected top-level key {k}"
                );
            }
            for (name, q) in body["questions"].as_object().unwrap() {
                for k in q.as_object().unwrap().keys() {
                    assert!(
                        ["type", "instructions", "criteria"].contains(&k.as_str()),
                        "{name}: unexpected key {k}"
                    );
                }
                assert!(["choice", "noul", "score"].contains(&q["type"].as_str().unwrap()));
            }
        });
    }

    #[test]
    fn parses_thresholdable_answers_and_flags_invalid() {
        with_req(|req| {
            let resp = json!({ "answers": {
                "doc_type": { "choice": "readme", "confidence": 0.9 },
                "s1_helps": 0.8,
                "s2_helps": { "noul": 0.7 },
                "s2_widget": { "choice": "EvilWidget", "confidence": 0.99 },
            }});
            let d = parse_response(&resp, req).unwrap();
            assert!(
                matches!(d.sections["s1"], Answer::Raw { helps, .. } if (helps - 0.8).abs() < 1e-6)
            );
            assert_eq!(d.sections["s2"], Answer::Invalid("unknown_option".into()));
            assert_eq!(d.doc_type.as_ref().unwrap().0, "readme");
        });
    }

    #[test]
    fn rejects_body_without_answers() {
        with_req(|req| {
            assert_eq!(
                parse_response(&json!({"oops": 1}), req).unwrap_err(),
                SelectError::InvalidResponse
            );
        });
    }
}
