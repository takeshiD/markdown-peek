//! Scripted, deterministic selector for tests / web E2E without a network
//! (R7). Not documented for users: `[models.x] api = "fake"`, with an optional
//! `script = "…"` (JSON file):
//!
//! ```json
//! {
//!   "model": "fake-1",
//!   "delay_ms": 0,
//!   "fail": null,
//!   "doc_type": { "value": "readme", "confidence": 0.9 },
//!   "document": { "widget": "SemanticOutline", "confidence": 0.9, "helps": 0.8 },
//!   "sections": {
//!     "Steps": { "widget": "StepNavigator", "confidence": 0.95, "helps": 0.9 },
//!     "License": { "helps": 0.1 }
//!   },
//!   "default": "rules"
//! }
//! ```
//!
//! Sections are matched by heading text, then by positional id (`s3`).
//! `fail` makes every call fail with that fallback code (e.g. `"timeout"`),
//! `delay_ms` simulates latency (and times out if it exceeds the deadline).
//! Unmatched sections use `default`: `"rules"` (default: the highest-scoring
//! candidate), `"missing"` (treated as an invalid answer), or an entry object.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::{
    Answer, Candidate, SelectError, SelectionDecisions, SelectionRequest, Selector, score_answer,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub widget: Option<String>,
    pub confidence: Option<f32>,
    pub helps: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum DefaultMode {
    Mode(String),
    Entry(Entry),
}

#[derive(Debug, Clone, Deserialize)]
pub struct DocType {
    pub value: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub delay_ms: u64,
    #[serde(default)]
    pub fail: Option<String>,
    #[serde(default)]
    pub doc_type: Option<DocType>,
    #[serde(default)]
    pub document: Option<Entry>,
    #[serde(default)]
    pub sections: HashMap<String, Entry>,
    #[serde(default)]
    pub default: Option<DefaultMode>,
}

pub struct FakeSelector {
    script: Script,
}

impl FakeSelector {
    pub fn load(path: &Path) -> Result<Self, SelectError> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            tracing::warn!(target: "mdpeek::select", "fake script {}: cannot read ({})", path.display(), e.kind());
            SelectError::FakeScript
        })?;
        let script: Script = serde_json::from_str(&text).map_err(|e| {
            // serde_json messages name the field / position, not the input text.
            tracing::warn!(target: "mdpeek::select", "fake script {}: invalid ({e})", path.display());
            SelectError::FakeScript
        })?;
        Ok(FakeSelector { script })
    }

    /// No script: every section gets its highest-scoring candidate.
    pub fn unscripted() -> Self {
        FakeSelector {
            script: Script {
                model: None,
                delay_ms: 0,
                fail: None,
                doc_type: None,
                document: None,
                sections: HashMap::new(),
                default: None,
            },
        }
    }

    pub fn from_script(script: Script) -> Self {
        FakeSelector { script }
    }
}

fn entry_answer(e: &Entry, cands: &[Candidate]) -> Answer {
    let rules = score_answer(cands);
    let candidate = match &e.widget {
        Some(w) => w.clone(),
        None => match &rules {
            Some(Answer::Raw { candidate, .. }) => candidate.clone(),
            _ => return Answer::Invalid("missing".into()),
        },
    };
    Answer::Raw {
        candidate,
        confidence: e.confidence.unwrap_or(1.0),
        helps: e.helps.unwrap_or(1.0),
        alternatives: Vec::new(),
    }
}

impl Selector for FakeSelector {
    fn select(
        &self,
        req: &SelectionRequest<'_>,
        deadline: Instant,
    ) -> Result<SelectionDecisions, SelectError> {
        if self.script.delay_ms > 0 {
            let want = Instant::now() + Duration::from_millis(self.script.delay_ms);
            let until = want.min(deadline);
            while Instant::now() < until {
                if req.cancel.load(std::sync::atomic::Ordering::SeqCst) {
                    return Err(SelectError::Cancelled);
                }
                std::thread::sleep(
                    Duration::from_millis(20).min(until.saturating_duration_since(Instant::now())),
                );
            }
            if want > deadline {
                return Err(SelectError::Timeout);
            }
        }
        if let Some(code) = &self.script.fail {
            return Err(SelectError::Scripted(code.clone()));
        }
        let mut d = SelectionDecisions {
            requests: 1,
            ..Default::default()
        };
        for s in &req.sections {
            let entry = self
                .script
                .sections
                .get(&s.heading)
                .or_else(|| self.script.sections.get(&s.id));
            let a = match (entry, &self.script.default) {
                (Some(e), _) => entry_answer(e, &s.candidates),
                (None, Some(DefaultMode::Entry(e))) => entry_answer(e, &s.candidates),
                (None, Some(DefaultMode::Mode(m))) if m == "missing" => {
                    Answer::Invalid("missing".into())
                }
                _ => match score_answer(&s.candidates) {
                    Some(a) => a,
                    None => continue,
                },
            };
            d.sections.insert(s.id.clone(), a);
        }
        if req.ask_document {
            // Unscripted: the highest-scoring document candidate.
            d.document = match &self.script.document {
                Some(e) => Some(entry_answer(e, &req.plan.doc_candidates)),
                None => score_answer(&req.plan.doc_candidates),
            };
            d.doc_type = self
                .script
                .doc_type
                .as_ref()
                .map(|t| (t.value.clone(), t.confidence));
        }
        Ok(d)
    }
}
