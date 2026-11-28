//! Adaptive reading-aid selection (design `docs/design/jev-adaptive-ui.md` §3).
//!
//! ```text
//! markdown → sections + features + candidates (deterministic, `sections`)
//!          → a model (decision model or LLM, `models`) picks *ids only*,
//!            a batch of sections at a time (stoppable, resumable)
//!          → composer (thresholds, limits, "show nothing")
//!          → validate_nodes → AdaptiveLayout (wire format v2)
//! ```
//!
//! The model never produces content: every widget node is built by
//! [`sections::plan_document`] from the document itself and carries a
//! `sourceRange`. When the model cannot be used (bad config, missing key,
//! network error) no aid is shown and the layout says why
//! (`selector.error` / `selector.message`); the pipeline itself never fails.
//!
//! Decisions are cached per section ([`cache`]: memory, then disk), so a
//! stopped run resumes where it stopped and unchanged sections never ask again.

pub mod ascii;
pub mod cache;
pub mod chart;
pub mod compare;
pub mod eval;
pub mod fake;
pub mod graph;
mod http;
pub mod jev;
pub mod llm;
pub mod models;
pub mod sections;
pub mod stages;
pub mod tree;
pub mod verdicts;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::ir::node::UiNode;
use crate::ir::{SourceRange, validate_nodes};

pub use cache::{DiskCache, SelectionCache, default_cache_dir};
pub use models::{Api, ModelKind, ModelSpec, Models};
pub use sections::{Candidate, DocPlan, REGISTRY_VERSION, Section};

/// Bumped when composer thresholds/limits semantics change (cache key part).
pub const POLICY_VERSION: u32 = 1;
/// Wire-format version of [`AdaptiveLayout`].
pub const LAYOUT_VERSION: u32 = 2;

/// Resolved `[model]` settings (shared by every model).
#[derive(Debug, Clone, PartialEq)]
pub struct SelectConfig {
    /// Per request (one batch).
    pub timeout_ms: u64,
    pub confidence_threshold: f32,
    pub helps_threshold: f32,
    pub excerpt_chars: usize,
    pub max_state_chars: usize,
    /// Sections per request; also the unit of stop / resume.
    pub batch_sections: usize,
    pub debounce_ms: u64,
}

impl Default for SelectConfig {
    fn default() -> Self {
        SelectConfig {
            timeout_ms: 30_000,
            confidence_threshold: 0.6,
            helps_threshold: 0.5,
            excerpt_chars: 600,
            max_state_chars: 24_000,
            batch_sections: 8,
            debounce_ms: 800,
        }
    }
}

// ---------------------------------------------------------------------------
// Wire format (§3.6)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdaptiveLayout {
    pub version: u32,
    pub revision: u64,
    pub doc_hash: String,
    pub selector: SelectorInfo,
    pub progress: Progress,
    pub doc_type: DocTypeInfo,
    pub placements: Vec<Placement>,
    pub skipped: Vec<Skipped>,
    pub cache: CacheStats,
    pub timing_ms: Timing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectorInfo {
    /// The `[models.<name>]` entry that answers.
    pub name: String,
    /// `decision` | `llm`
    pub kind: ModelKind,
    /// Model id (or the api name for the CLIs).
    pub model: String,
    /// Enumerated code when the model could not (fully) answer, e.g.
    /// `config_invalid`, `missing_key`, `timeout`, `unauthorized`.
    pub error: Option<String>,
    /// Human-readable explanation of `error` (never contains a key or text
    /// from the document).
    pub message: Option<String>,
    /// HTTP requests (or CLI runs) sent for this layout.
    pub requests: u32,
}

/// Where the selection run stands.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    /// Nothing asked yet (aids off, or waiting for the reader to start).
    Idle,
    Running,
    /// The reader stopped it; resume asks only what is still missing.
    Stopped,
    Done,
    /// The model could not answer; see `selector.error`.
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Progress {
    pub state: JobState,
    /// Questions answered (sections with candidates, plus the document).
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocTypeInfo {
    pub value: String,
    pub confidence: f32,
    /// Model name, or `heuristic` when the model has not answered.
    pub by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Anchor {
    pub block_id: String,
    pub heading: String,
    pub range: SourceRange,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Why {
    /// The model entry name that chose it.
    pub by: String,
    /// The chosen option id (e.g. `ConfigViewer:commands`).
    pub candidate: String,
    pub confidence: f32,
    pub helps: f32,
    /// Enumerated only (no free text): `chosen`.
    pub reason_code: String,
    pub alternatives: Vec<(String, f32)>,
    /// True when this decision was reused from the cache.
    pub cached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Placement {
    /// `section` | `document`
    pub scope: String,
    pub anchor: Anchor,
    pub node: UiNode,
    pub why: Why,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Skipped {
    pub scope: String,
    pub anchor: Anchor,
    /// `no_candidates` | `pending` | `invalid_answer` | `low_helps` |
    /// `low_confidence` | `over_limit` | `covered_by_section` | `validation_failed`
    pub reason_code: String,
    pub by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub helps: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CacheStats {
    pub hits: u32,
    pub misses: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Timing {
    pub select: u64,
    pub total: u64,
}

// ---------------------------------------------------------------------------
// Selector trait (§3.5)
// ---------------------------------------------------------------------------

/// A raw model answer for one section (or the document). Thresholds are
/// applied by the composer, so raw answers are what the cache stores.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Raw {
        candidate: String,
        confidence: f32,
        helps: f32,
        alternatives: Vec<(String, f32)>,
    },
    /// The answer was unknown / missing / ill-typed: nothing is shown.
    Invalid(String),
}

#[derive(Debug, Clone, Default)]
pub struct SelectionDecisions {
    pub doc_type: Option<(String, f32)>,
    pub sections: HashMap<String, Answer>,
    pub document: Option<Answer>,
    pub requests: u32,
}

/// What a model is asked about: one batch of sections with ≥1 candidate.
pub struct SelectionRequest<'a> {
    pub plan: &'a DocPlan,
    pub filename: Option<&'a str>,
    pub sections: Vec<&'a Section>,
    pub ask_document: bool,
    pub cfg: &'a SelectConfig,
    /// Set by "stop": an in-flight request is abandoned.
    pub cancel: &'a AtomicBool,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SelectError {
    #[error("{0}")]
    Config(String),
    #[error("environment variable {0} is not set")]
    MissingKey(String),
    #[error("this build has no network support (built without the `llm` feature)")]
    NotBuilt,
    #[error("no answer within the time limit ([model] timeout_ms)")]
    Timeout,
    #[error("the API rejected the key (HTTP 401/403)")]
    Unauthorized,
    #[error("the endpoint or model was not found (HTTP 404); check base_url and model")]
    NotFound,
    #[error("the account has no credit or plan for this model (HTTP 402)")]
    PaymentRequired,
    #[error("rate limited by the API (HTTP 429)")]
    RateLimited,
    #[error("the API is overloaded (HTTP 529)")]
    Overloaded,
    #[error("the API failed (HTTP {0})")]
    Server(u16),
    #[error("the API answered HTTP {0}")]
    Http(u16),
    #[error("the model's answer could not be read")]
    InvalidResponse,
    #[error("could not reach the API")]
    Network,
    #[error("the claude / codex command was not found")]
    CliMissing,
    #[error("the claude / codex command failed")]
    CliFailed,
    #[error("stopped")]
    Cancelled,
    #[error("the fake script could not be read")]
    FakeScript,
    #[error("scripted failure: {0}")]
    Scripted(String),
}

impl SelectError {
    /// Enumerated code surfaced in `selector.error`.
    pub fn code(&self) -> String {
        match self {
            SelectError::Config(_) => "config_invalid".into(),
            SelectError::MissingKey(_) => "missing_key".into(),
            SelectError::NotBuilt => "not_built".into(),
            SelectError::Timeout => "timeout".into(),
            SelectError::Unauthorized => "unauthorized".into(),
            SelectError::NotFound => "not_found".into(),
            SelectError::PaymentRequired => "payment_required".into(),
            SelectError::RateLimited => "rate_limited".into(),
            SelectError::Overloaded => "overloaded".into(),
            SelectError::Server(_) => "server_error".into(),
            SelectError::Http(_) => "http_error".into(),
            SelectError::InvalidResponse => "invalid_response".into(),
            SelectError::Network => "offline".into(),
            SelectError::CliMissing => "cli_missing".into(),
            SelectError::CliFailed => "cli_failed".into(),
            SelectError::Cancelled => "cancelled".into(),
            SelectError::FakeScript => "fake_script_error".into(),
            SelectError::Scripted(c) => c.clone(),
        }
    }

    /// Problems in the settings (shown as "settings are invalid").
    pub fn is_config(&self) -> bool {
        matches!(
            self,
            SelectError::Config(_)
                | SelectError::MissingKey(_)
                | SelectError::NotBuilt
                | SelectError::Unauthorized
                | SelectError::NotFound
                | SelectError::PaymentRequired
                | SelectError::CliMissing
                | SelectError::FakeScript
        )
    }
}

pub trait Selector: Send + Sync {
    fn select(
        &self,
        req: &SelectionRequest<'_>,
        deadline: Instant,
    ) -> Result<SelectionDecisions, SelectError>;
    /// Requests actually sent during the last `select` (for `selector.requests`
    /// on failure, e.g. a retried 429).
    fn last_requests(&self) -> Option<u32> {
        None
    }
}

/// Build the selector for a model entry, or the reason it cannot be used.
pub fn build_selector(spec: &ModelSpec) -> Result<Box<dyn Selector>, SelectError> {
    if let Some(p) = &spec.config_problem {
        return Err(SelectError::Config(p.clone()));
    }
    let key = || {
        let var = spec.api_key_env.as_deref().unwrap_or("");
        models::read_key(var).ok_or_else(|| SelectError::MissingKey(var.to_string()))
    };
    match spec.api {
        Api::Fake => match &spec.script {
            Some(p) => Ok(Box::new(fake::FakeSelector::load(p)?)),
            None => Ok(Box::new(fake::FakeSelector::unscripted())),
        },
        Api::Systemone => {
            let k = key()?;
            jev::SystemOneSelector::new(spec, k).map(|s| Box::new(s) as Box<dyn Selector>)
        }
        Api::Anthropic | Api::Openai | Api::OpenaiChat => {
            let k = key()?;
            llm::LlmSelector::new(spec, Some(k)).map(|s| Box::new(s) as Box<dyn Selector>)
        }
        Api::ClaudeCode | Api::Codex => {
            llm::LlmSelector::new(spec, None).map(|s| Box::new(s) as Box<dyn Selector>)
        }
    }
}

/// The key must not travel in cleartext: https, or http to loopback (local
/// mock servers in tests).
pub fn base_url_is_safe(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    if u.starts_with("https://") {
        return true;
    }
    let Some(rest) = u.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or("");
    let host = host.rsplit_once('@').map(|(_, h)| h).unwrap_or(host);
    let host = if host.starts_with('[') {
        host.split(']').next().map(|h| &h[1..]).unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// Keep only alternatives that name one of `cands` (model output is untrusted).
fn only_candidates(alts: Vec<(String, f32)>, cands: &[Candidate]) -> Vec<(String, f32)> {
    alts.into_iter()
        .filter(|(k, _)| cands.iter().any(|c| c.id == k))
        .collect()
}

/// At most this many document-level aids are shown.
pub const MAX_DOCUMENT_AIDS: usize = 2;
/// At most this many aids in total (the rest are skipped as `over_limit`).
pub const MAX_AIDS: usize = 8;

/// The highest-scoring candidate as an answer (used by the fake selector).
pub(crate) fn score_answer(cands: &[Candidate]) -> Option<Answer> {
    let best = cands
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.score.total_cmp(&b.1.score).then(b.0.cmp(&a.0)))?
        .1;
    let alternatives = cands
        .iter()
        .filter(|c| c.id != best.id)
        .map(|c| (c.id.to_string(), c.score))
        .collect();
    Some(Answer::Raw {
        candidate: best.id.to_string(),
        confidence: 1.0,
        helps: best.score,
        alternatives,
    })
}

// ---------------------------------------------------------------------------
// Pipeline (§3.4)
// ---------------------------------------------------------------------------

/// Answers known so far: `(answer, cached)`.
#[derive(Debug, Clone, Default)]
pub struct Answers {
    pub sections: HashMap<String, (Answer, bool)>,
    pub document: Option<(Answer, bool)>,
    pub doc_type: Option<(String, f32)>,
    /// The document questions were answered (or came from the cache).
    pub doc_done: bool,
}

/// Plan the document (deterministic, no network).
pub fn plan(markdown: &str, filename: Option<&str>, cfg: &SelectConfig) -> DocPlan {
    let analysis = mdpeek_analyzer::analyze(markdown, filename);
    sections::plan_document(markdown, filename, &analysis, cfg.excerpt_chars)
}

/// What the cache already knows for `spec`, and what is still to ask.
pub fn from_cache<'p>(
    plan: &'p DocPlan,
    spec: &ModelSpec,
    cache: &SelectionCache,
    stats: &mut CacheStats,
) -> (Answers, Vec<&'p Section>) {
    let id = spec.cache_identity();
    let mut a = Answers::default();
    let mut to_ask = Vec::new();
    for s in plan.sections.iter().filter(|s| !s.candidates.is_empty()) {
        match cache.get(&cache::section_key(s, &id)) {
            Some(ans) => {
                stats.hits += 1;
                a.sections.insert(s.id.clone(), (ans, true));
            }
            None => {
                stats.misses += 1;
                to_ask.push(s);
            }
        }
    }
    match cache.get_doc(&cache::doc_key(plan, &id)) {
        Some((d, t)) => {
            stats.hits += 1;
            a.document = d.map(|x| (x, true));
            a.doc_type = t;
            a.doc_done = true;
        }
        None => stats.misses += 1,
    }
    to_ask.sort_by_key(|s| s.range.start_line);
    (a, to_ask)
}

fn info(spec: &ModelSpec) -> SelectorInfo {
    SelectorInfo {
        name: spec.name.clone(),
        kind: spec.kind,
        model: spec.label(),
        error: None,
        message: None,
        requests: 0,
    }
}

/// The layout from cached decisions only (no network): what a reader sees
/// before starting, or while aids are off.
pub fn snapshot(
    markdown: &str,
    filename: Option<&str>,
    cfg: &SelectConfig,
    spec: &ModelSpec,
    cache: &SelectionCache,
    revision: u64,
    state: JobState,
) -> AdaptiveLayout {
    let started = Instant::now();
    let p = plan(markdown, filename, cfg);
    let mut stats = CacheStats::default();
    let (answers, to_ask) = from_cache(&p, spec, cache, &mut stats);
    let state = if to_ask.is_empty() && answers.doc_done && state != JobState::Running {
        JobState::Done
    } else {
        state
    };
    let mut l = compose(&p, &answers, cfg, info(spec), state, stats, revision);
    l.timing_ms.total = started.elapsed().as_millis() as u64;
    l
}

/// Ask `selector` about everything the cache does not know, one batch at a
/// time; `on_progress` gets the layout after every batch but the last. Stops
/// early when `cancel` is set (state `stopped`) or the model fails (`error`).
#[allow(clippy::too_many_arguments)]
pub fn run_selection(
    markdown: &str,
    filename: Option<&str>,
    cfg: &SelectConfig,
    spec: &ModelSpec,
    selector: Result<Box<dyn Selector>, SelectError>,
    cache: &SelectionCache,
    revision: u64,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(&AdaptiveLayout),
) -> AdaptiveLayout {
    let started = Instant::now();
    let p = plan(markdown, filename, cfg);
    let mut stats = CacheStats::default();
    let (mut answers, to_ask) = from_cache(&p, spec, cache, &mut stats);
    let mut inf = info(spec);
    let id = spec.cache_identity();
    let mut select_ms = 0u64;

    let finish = |answers: &Answers,
                  inf: SelectorInfo,
                  state: JobState,
                  stats: CacheStats,
                  select_ms: u64| {
        let mut l = compose(&p, answers, cfg, inf, state, stats, revision);
        l.timing_ms.select = select_ms;
        l.timing_ms.total = started.elapsed().as_millis() as u64;
        l
    };

    if to_ask.is_empty() && answers.doc_done {
        return finish(&answers, inf, JobState::Done, stats, 0);
    }
    let sel = match selector {
        Ok(s) => s,
        Err(e) => {
            inf.error = Some(e.code());
            inf.message = Some(e.to_string());
            return finish(&answers, inf, JobState::Error, stats, 0);
        }
    };

    let size = cfg.batch_sections.max(1);
    let mut batches: Vec<Vec<&Section>> = to_ask.chunks(size).map(|c| c.to_vec()).collect();
    if batches.is_empty() {
        batches.push(Vec::new()); // document questions only
    }
    let n = batches.len();
    let mut state = JobState::Done;
    for (i, batch) in batches.into_iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            state = JobState::Stopped;
            break;
        }
        let ask_document = !answers.doc_done;
        let req = SelectionRequest {
            plan: &p,
            filename,
            sections: batch.clone(),
            ask_document,
            cfg,
            cancel,
        };
        let t0 = Instant::now();
        let res = sel.select(&req, t0 + Duration::from_millis(cfg.timeout_ms));
        select_ms += t0.elapsed().as_millis() as u64;
        match res {
            Ok(dec) => {
                inf.requests += dec.requests;
                for s in &batch {
                    let a = dec
                        .sections
                        .get(&s.id)
                        .cloned()
                        .unwrap_or_else(|| Answer::Invalid("missing".into()));
                    if matches!(a, Answer::Raw { .. }) {
                        cache.put(cache::section_key(s, &id), a.clone());
                    }
                    answers.sections.insert(s.id.clone(), (a, false));
                }
                if ask_document {
                    let doc = dec.document.clone();
                    let doc_ok = doc.clone().filter(|a| matches!(a, Answer::Raw { .. }));
                    if p.doc_candidates.is_empty() || doc_ok.is_some() {
                        cache.put_doc(cache::doc_key(&p, &id), doc_ok, dec.doc_type.clone());
                    }
                    answers.document = match doc {
                        Some(a) => Some((a, false)),
                        None if !p.doc_candidates.is_empty() => {
                            Some((Answer::Invalid("missing".into()), false))
                        }
                        None => None,
                    };
                    answers.doc_type = dec.doc_type;
                    answers.doc_done = true;
                }
                if i + 1 < n {
                    on_progress(&finish(
                        &answers,
                        inf.clone(),
                        JobState::Running,
                        stats.clone(),
                        select_ms,
                    ));
                }
            }
            Err(SelectError::Cancelled) => {
                inf.requests += sel.last_requests().unwrap_or(0);
                state = JobState::Stopped;
                break;
            }
            Err(e) => {
                inf.requests += sel.last_requests().unwrap_or(1);
                inf.error = Some(e.code());
                inf.message = Some(e.to_string());
                state = JobState::Error;
                break;
            }
        }
    }
    finish(&answers, inf, state, stats, select_ms)
}

/// Turn answers into placements (thresholds, limits, validation).
pub fn compose(
    plan: &DocPlan,
    ans: &Answers,
    cfg: &SelectConfig,
    selector: SelectorInfo,
    state: JobState,
    stats: CacheStats,
    revision: u64,
) -> AdaptiveLayout {
    let mut placements = Vec::new();
    let mut skipped = Vec::new();
    let ht = cfg.helps_threshold;
    let ct = cfg.confidence_threshold;
    let by = selector.name.clone();
    let mut done = 0u32;
    let mut total = 0u32;

    // One decision → a placement or a skip.
    let mut decide = |scope: &str,
                      anchor: Anchor,
                      answer: Option<&(Answer, bool)>,
                      cands: &[Candidate],
                      placements: &mut Vec<Placement>,
                      skipped: &mut Vec<Skipped>| {
        let skip = |reason: &str, helps: Option<f32>, confidence: Option<f32>| Skipped {
            scope: scope.into(),
            anchor: anchor.clone(),
            reason_code: reason.into(),
            by: by.clone(),
            helps,
            confidence,
        };
        total += 1;
        let Some((a, cached)) = answer else {
            skipped.push(skip("pending", None, None));
            return;
        };
        done += 1;
        let Answer::Raw {
            candidate,
            confidence,
            helps,
            alternatives,
        } = a
        else {
            skipped.push(skip("invalid_answer", None, None));
            return;
        };
        let Some(c) = cands.iter().find(|c| c.id == candidate) else {
            skipped.push(skip("invalid_answer", None, None));
            return;
        };
        if *helps < ht {
            skipped.push(skip("low_helps", Some(*helps), Some(*confidence)));
            return;
        }
        if *confidence < ct {
            skipped.push(skip("low_confidence", Some(*helps), Some(*confidence)));
            return;
        }
        placements.push(Placement {
            scope: scope.into(),
            anchor,
            node: c.node.clone(),
            why: Why {
                by: by.clone(),
                candidate: candidate.clone(),
                confidence: *confidence,
                helps: *helps,
                reason_code: "chosen".into(),
                alternatives: only_candidates(alternatives.clone(), cands),
                cached: *cached,
            },
        });
    };

    if !plan.doc_candidates.is_empty() {
        let anchor = Anchor {
            block_id: plan.doc_anchor.0.clone(),
            heading: plan.title.clone(),
            range: plan.doc_anchor.1,
        };
        decide(
            "document",
            anchor,
            ans.document.as_ref(),
            &plan.doc_candidates,
            &mut placements,
            &mut skipped,
        );
    }
    for s in &plan.sections {
        let anchor = Anchor {
            block_id: s.block_id.clone(),
            heading: s.heading.clone(),
            range: s.range,
        };
        if s.candidates.is_empty() {
            skipped.push(Skipped {
                scope: "section".into(),
                anchor,
                reason_code: "no_candidates".into(),
                by: "prefilter".into(),
                helps: None,
                confidence: None,
            });
            continue;
        }
        decide(
            "section",
            anchor,
            ans.sections.get(&s.id),
            &s.candidates,
            &mut placements,
            &mut skipped,
        );
    }

    // A document-level aid whose items all come from one section that already
    // has its own aid would show the same lines twice: keep the section one.
    let section_ranges: Vec<_> = placements
        .iter()
        .filter(|p| p.scope == "section")
        .filter_map(|p| p.node.meta().source_range)
        .collect();
    let (dupes, rest): (Vec<_>, Vec<_>) = placements.into_iter().partition(|p| {
        p.scope == "document"
            && p.node.meta().source_range.is_some_and(|d| {
                section_ranges
                    .iter()
                    .any(|s| s.start_line <= d.start_line && d.end_line <= s.end_line)
            })
    });
    let skip_placed = |p: Placement, reason: &str, skipped: &mut Vec<Skipped>| {
        skipped.push(Skipped {
            scope: p.scope,
            anchor: p.anchor,
            reason_code: reason.into(),
            by: p.why.by,
            helps: Some(p.why.helps),
            confidence: Some(p.why.confidence),
        })
    };
    for p in dupes {
        skip_placed(p, "covered_by_section", &mut skipped);
    }

    // Attention budget: keep the most helpful ones, in reading order.
    let (mut docs, mut secs): (Vec<Placement>, Vec<Placement>) =
        rest.into_iter().partition(|p| p.scope == "document");
    let mut over: Vec<Placement> = Vec::new();
    docs.sort_by(|a, b| b.why.helps.total_cmp(&a.why.helps));
    if docs.len() > MAX_DOCUMENT_AIDS {
        over.extend(docs.split_off(MAX_DOCUMENT_AIDS));
    }
    let room = MAX_AIDS.saturating_sub(docs.len());
    if secs.len() > room {
        let mut ranked: Vec<usize> = (0..secs.len()).collect();
        ranked.sort_by(|&a, &b| {
            secs[b]
                .why
                .helps
                .total_cmp(&secs[a].why.helps)
                .then(a.cmp(&b))
        });
        let keep: std::collections::HashSet<usize> = ranked.into_iter().take(room).collect();
        let (k, o): (Vec<_>, Vec<_>) = secs
            .into_iter()
            .enumerate()
            .partition(|(i, _)| keep.contains(i));
        secs = k.into_iter().map(|(_, p)| p).collect();
        over.extend(o.into_iter().map(|(_, p)| p));
    }
    for p in over {
        skip_placed(p, "over_limit", &mut skipped);
    }
    let mut ordered = docs;
    ordered.extend(secs);

    // Validate every node (allowlist + sourceRange bounds); drop failures.
    let mut kept = Vec::new();
    for mut p in ordered {
        let mut one = [p.node.clone()];
        match validate_nodes(&mut one, plan.total_lines) {
            Ok(()) => {
                let [n] = one;
                p.node = n;
                kept.push(p);
            }
            Err(_) => skipped.push(Skipped {
                scope: p.scope,
                anchor: p.anchor,
                reason_code: "validation_failed".into(),
                by: p.why.by,
                helps: None,
                confidence: None,
            }),
        }
    }

    let doc_type = match &ans.doc_type {
        Some((v, c)) if *c >= ct => DocTypeInfo {
            value: v.clone(),
            confidence: *c,
            by: by.clone(),
        },
        _ => DocTypeInfo {
            value: plan.rules_doc_type.clone(),
            confidence: plan.rules_doc_type_confidence,
            by: "heuristic".into(),
        },
    };

    AdaptiveLayout {
        version: LAYOUT_VERSION,
        revision,
        doc_hash: plan.doc_hash.clone(),
        selector,
        progress: Progress { state, done, total },
        doc_type,
        placements: kept,
        skipped,
        cache: stats,
        timing_ms: Timing::default(),
    }
}

/// Run to completion without stopping (CLI, eval).
pub fn adaptive_layout(
    markdown: &str,
    filename: Option<&str>,
    cfg: &SelectConfig,
    spec: &ModelSpec,
    cache: &SelectionCache,
    revision: u64,
) -> AdaptiveLayout {
    let cancel = AtomicBool::new(false);
    run_selection(
        markdown,
        filename,
        cfg,
        spec,
        build_selector(spec),
        cache,
        revision,
        &cancel,
        &mut |_| {},
    )
}

/// Pretty JSON for the `mdpeek select` command.
pub fn layout_json(layout: &AdaptiveLayout) -> String {
    serde_json::to_string_pretty(layout).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::select::models::ModelEntry;

    const MD: &str = "# Tool\n\nIntro.\n\n## Install\n\n- [ ] get a key\n- [x] install the binary\n\n## Steps\n\n1. one\n2. two\n3. three\n4. four\n5. five\n\n## License\n\nMIT\n";

    fn fake() -> ModelSpec {
        ModelSpec::from_entry(
            "fake",
            ModelEntry {
                api: Some(Api::Fake),
                ..Default::default()
            },
        )
    }

    #[test]
    fn fake_layout_is_offline_and_valid() {
        let cfg = SelectConfig::default();
        let cache = SelectionCache::new(16);
        let l = adaptive_layout(MD, Some("README.md"), &cfg, &fake(), &cache, 3);
        assert_eq!(l.version, LAYOUT_VERSION);
        assert_eq!(l.revision, 3);
        assert_eq!(l.selector.name, "fake");
        assert!(l.selector.error.is_none());
        assert_eq!(l.progress.state, JobState::Done);
        assert_eq!(l.progress.done, l.progress.total);
        let kinds: Vec<_> = l
            .placements
            .iter()
            .map(|p| p.why.candidate.as_str())
            .collect();
        // Removed: commands (#67) and the config viewer (the toml block above).
        assert!(!kinds.iter().any(|k| k.starts_with("ConfigViewer")), "{kinds:?}");
        assert!(kinds.contains(&"StepNavigator"));
        assert!(l.skipped.iter().any(|s| s.anchor.heading == "License"));
        for p in &l.placements {
            assert!(p.node.meta().source_range.is_some());
        }
    }

    #[test]
    fn unusable_model_shows_nothing_and_says_why() {
        let spec = ModelSpec::from_entry(
            "jev",
            ModelEntry {
                api: Some(Api::Systemone),
                model: Some("jev-1.14".into()),
                api_key_env: Some("MDPEEK_TEST_SURELY_UNSET_KEY".into()),
                ..Default::default()
            },
        );
        let cache = SelectionCache::new(16);
        let l = adaptive_layout(MD, None, &SelectConfig::default(), &spec, &cache, 1);
        assert_eq!(l.progress.state, JobState::Error);
        assert_eq!(l.selector.error.as_deref(), Some("missing_key"));
        assert_eq!(
            l.selector.message.as_deref(),
            Some("environment variable MDPEEK_TEST_SURELY_UNSET_KEY is not set")
        );
        assert!(l.placements.is_empty());
        assert!(l.skipped.iter().any(|s| s.reason_code == "pending"));
    }

    #[test]
    fn cache_hits_on_second_run() {
        let cfg = SelectConfig::default();
        let cache = SelectionCache::new(16);
        let _ = adaptive_layout(MD, None, &cfg, &fake(), &cache, 1);
        let l = adaptive_layout(MD, None, &cfg, &fake(), &cache, 2);
        assert!(l.cache.hits >= 2, "{:?}", l.cache);
        assert_eq!(l.cache.misses, 0);
        assert!(l.placements.iter().all(|p| p.why.cached));
    }

    #[test]
    fn batches_report_progress_and_stop_resumes() {
        let cfg = SelectConfig {
            batch_sections: 1,
            ..Default::default()
        };
        let cache = SelectionCache::new(16);
        let cancel = AtomicBool::new(false);
        let mut seen = Vec::new();
        let l = run_selection(
            MD,
            None,
            &cfg,
            &fake(),
            build_selector(&fake()),
            &cache,
            1,
            &cancel,
            &mut |l| {
                seen.push((l.progress.done, l.progress.state));
                // Stop after the first batch.
                cancel.store(true, Ordering::SeqCst);
            },
        );
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].1, JobState::Running);
        assert_eq!(l.progress.state, JobState::Stopped);
        assert!(l.progress.done < l.progress.total);
        // Before resuming, the snapshot shows the decisions so far.
        let s = snapshot(MD, None, &cfg, &fake(), &cache, 1, JobState::Stopped);
        assert_eq!(s.progress.state, JobState::Stopped);
        assert_eq!(s.progress.done, l.progress.done);
        // Resume: only what is missing is asked.
        let l2 = adaptive_layout(MD, None, &cfg, &fake(), &cache, 1);
        assert_eq!(l2.progress.state, JobState::Done);
        assert!(l2.cache.hits >= 1);
    }

    #[test]
    fn invalid_answers_show_nothing() {
        let script: fake::Script =
            serde_json::from_str(r#"{"sections": {"Install": {"widget": "EvilWidget"}}}"#).unwrap();
        let cache = SelectionCache::new(16);
        let cancel = AtomicBool::new(false);
        let l = run_selection(
            MD,
            None,
            &SelectConfig::default(),
            &fake(),
            Ok(Box::new(fake::FakeSelector::from_script(script))),
            &cache,
            1,
            &cancel,
            &mut |_| {},
        );
        let s = l
            .skipped
            .iter()
            .find(|s| s.anchor.heading == "Install")
            .unwrap();
        assert_eq!(s.reason_code, "invalid_answer");
        assert!(!l.placements.iter().any(|p| p.anchor.heading == "Install"));
    }
}
