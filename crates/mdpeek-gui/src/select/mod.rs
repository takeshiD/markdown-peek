//! Adaptive reading-aid selection (design `docs/design/jev-adaptive-ui.md` §3).
//!
//! ```text
//! markdown → sections + features + candidates (deterministic, `sections`)
//!          → Selector (rules | jev | fake) picks *ids only*
//!          → composer (thresholds, limits, "show nothing")
//!          → validate_nodes → AdaptiveLayout (wire format v1)
//! ```
//!
//! The selector never produces content: every widget node is built by
//! [`sections::plan_document`] from the document itself and carries a
//! `sourceRange`. Any selector failure degrades to the rules selector and is
//! reported in `selector.fallback`; the pipeline itself never fails.

pub mod cache;
pub mod eval;
pub mod fake;
pub mod jev;
pub mod sections;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::ir::node::UiNode;
use crate::ir::{SourceRange, validate_nodes};

pub use cache::SelectionCache;
pub use sections::{Candidate, DocPlan, REGISTRY_VERSION, Section};

/// Bumped when composer thresholds/limits semantics change (cache key part).
pub const POLICY_VERSION: u32 = 1;
/// Wire-format version of [`AdaptiveLayout`].
pub const LAYOUT_VERSION: u32 = 1;
/// Environment variable holding the Jev API key (the **only** key source).
pub const API_KEY_ENV: &str = "TYPESAFE_API_KEY";

const MAX_DOC_WIDGETS: usize = 3;

/// Which selector to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(clap::ValueEnum)]
pub enum Provider {
    #[default]
    Rules,
    Jev,
    Fake,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Rules => "rules",
            Provider::Jev => "jev",
            Provider::Fake => "fake",
        }
    }
}

/// Resolved `[select]` settings (design §5). The API key is *not* here: it is
/// read from [`API_KEY_ENV`] at call time only.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectConfig {
    pub provider: Provider,
    pub allow_network: bool,
    pub model: String,
    pub base_url: String,
    pub timeout_ms: u64,
    pub confidence_threshold: f32,
    pub helps_threshold: f32,
    pub max_questions: usize,
    pub excerpt_chars: usize,
    pub max_state_chars: usize,
    pub debounce_ms: u64,
    pub cache_entries: usize,
    pub fake_script: Option<std::path::PathBuf>,
}

impl Default for SelectConfig {
    fn default() -> Self {
        SelectConfig {
            provider: Provider::Rules,
            allow_network: false,
            model: "jev-1.14.0".into(),
            base_url: "https://api.typesafe.ai".into(),
            timeout_ms: 4000,
            confidence_threshold: 0.6,
            helps_threshold: 0.5,
            max_questions: 40,
            excerpt_chars: 600,
            max_state_chars: 24_000,
            debounce_ms: 800,
            cache_entries: 256,
            fake_script: None,
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
    pub doc_type: DocTypeInfo,
    pub placements: Vec<Placement>,
    pub skipped: Vec<Skipped>,
    pub cache: CacheStats,
    pub timing_ms: Timing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectorInfo {
    /// Who actually produced the decisions: `rules` | `jev` | `fake`.
    pub provider: String,
    /// What the configuration asked for.
    pub requested: String,
    pub model: String,
    /// Why the requested selector was not (fully) used, e.g. `timeout`.
    pub fallback: Option<String>,
    /// Number of HTTP requests sent to the external selector for this layout.
    pub requests: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocTypeInfo {
    pub value: String,
    pub confidence: f32,
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
    /// `rules` | `jev` | `fake`
    pub by: String,
    /// The chosen option id (e.g. `ConfigViewer:commands`).
    pub candidate: String,
    pub confidence: f32,
    pub helps: f32,
    /// Enumerated only (no free text): `chosen`, `rules_score`,
    /// `fallback_invalid_answer`, `fallback_question_limit`, `fallback_<error>`.
    pub reason_code: String,
    pub alternatives: Vec<(String, f32)>,
    /// True when this decision was reused from the in-memory cache.
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
    /// `no_candidates` | `low_helps` | `low_confidence` | `validation_failed`
    pub reason_code: String,
    pub by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub helps: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Set when the section fell back to rules (e.g. `invalid_answer`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
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

/// A raw selector answer for one section (or the document). Thresholds are
/// applied by the composer, so raw answers are what the cache stores.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Raw {
        candidate: String,
        confidence: f32,
        helps: f32,
        alternatives: Vec<(String, f32)>,
    },
    /// The answer was unknown / missing / ill-typed → rules for that section.
    Invalid(String),
}

#[derive(Debug, Clone, Default)]
pub struct SelectionDecisions {
    pub doc_type: Option<(String, f32)>,
    pub sections: HashMap<String, Answer>,
    pub document: Option<Answer>,
    pub requests: u32,
}

/// What a selector is asked about. Only uncached sections with ≥1 candidate.
pub struct SelectionRequest<'a> {
    pub plan: &'a DocPlan,
    pub filename: Option<&'a str>,
    pub sections: Vec<&'a Section>,
    pub ask_document: bool,
    pub cfg: &'a SelectConfig,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SelectError {
    #[error("external selection disabled (allow_network = false)")]
    Disabled,
    #[error("{API_KEY_ENV} is not set")]
    MissingKey,
    #[error("jev support not built (built without the `llm` feature)")]
    NotBuilt,
    #[error("base_url must use https (plain http is allowed only for loopback)")]
    InsecureBaseUrl,
    #[error("timed out")]
    Timeout,
    #[error("unauthorized")]
    Unauthorized,
    #[error("rate limited")]
    RateLimited,
    #[error("overloaded")]
    Overloaded,
    #[error("server error {0}")]
    Server(u16),
    #[error("http status {0}")]
    Http(u16),
    #[error("invalid response")]
    InvalidResponse,
    #[error("network unreachable")]
    Network,
    #[error("fake script error")]
    FakeScript,
    #[error("scripted failure: {0}")]
    Scripted(String),
}

impl SelectError {
    /// Enumerated fallback code surfaced in `selector.fallback`.
    pub fn code(&self) -> String {
        match self {
            SelectError::Disabled => "disabled".into(),
            SelectError::MissingKey => "missing_key".into(),
            SelectError::NotBuilt => "not_built".into(),
            SelectError::InsecureBaseUrl => "insecure_base_url".into(),
            SelectError::Timeout => "timeout".into(),
            SelectError::Unauthorized => "unauthorized".into(),
            SelectError::RateLimited => "rate_limited".into(),
            SelectError::Overloaded => "overloaded".into(),
            SelectError::Server(_) => "server_error".into(),
            SelectError::Http(_) => "http_error".into(),
            SelectError::InvalidResponse => "invalid_response".into(),
            SelectError::Network => "offline".into(),
            SelectError::FakeScript => "fake_script_error".into(),
            SelectError::Scripted(c) => c.clone(),
        }
    }
}

pub trait Selector: Send + Sync {
    /// `(provider, model)` — part of the cache key.
    fn id(&self) -> (&'static str, String);
    fn select(
        &self,
        req: &SelectionRequest<'_>,
        deadline: Instant,
    ) -> Result<SelectionDecisions, SelectError>;
    /// Requests actually sent during the last `select` (for `selector.requests`
    /// on failure, e.g. a retried 429). `None` = not a networked selector.
    fn last_requests(&self) -> Option<u32> {
        None
    }
}

/// Build the configured selector, or the reason it cannot be used.
pub fn build_selector(cfg: &SelectConfig) -> Result<Box<dyn Selector>, SelectError> {
    match cfg.provider {
        Provider::Rules => Ok(Box::new(RulesSelector)),
        Provider::Fake => {
            let path = cfg.fake_script.as_ref().ok_or(SelectError::FakeScript)?;
            Ok(Box::new(fake::FakeSelector::load(path)?))
        }
        Provider::Jev => {
            if !cfg.allow_network {
                return Err(SelectError::Disabled);
            }
            let key = std::env::var(API_KEY_ENV).ok().filter(|k| !k.trim().is_empty());
            if !base_url_is_safe(&cfg.base_url) {
                return Err(SelectError::InsecureBaseUrl);
            }
            let Some(key) = key else {
                return Err(SelectError::MissingKey);
            };
            jev::JevSelector::new(cfg, key).map(|s| Box::new(s) as Box<dyn Selector>)
        }
    }
}

/// Deterministic rules selector: highest-scoring candidate, `helps = score`.
pub struct RulesSelector;

/// The Bearer key must not travel in cleartext: https, or http to loopback
/// (local mock servers in tests).
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

pub fn rules_answer(cands: &[Candidate]) -> Option<Answer> {
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

impl Selector for RulesSelector {
    fn id(&self) -> (&'static str, String) {
        ("rules", format!("rules-v{REGISTRY_VERSION}"))
    }
    fn select(
        &self,
        req: &SelectionRequest<'_>,
        _deadline: Instant,
    ) -> Result<SelectionDecisions, SelectError> {
        let mut d = SelectionDecisions::default();
        for s in &req.sections {
            if let Some(a) = rules_answer(&s.candidates) {
                d.sections.insert(s.id.clone(), a);
            }
        }
        Ok(d)
    }
}

// ---------------------------------------------------------------------------
// Pipeline + composer (§3.4)
// ---------------------------------------------------------------------------

/// Build the adaptive layout for `markdown`. Never fails: any selector problem
/// degrades to rules and is recorded in `selector.fallback`.
pub fn adaptive_layout(
    markdown: &str,
    filename: Option<&str>,
    cfg: &SelectConfig,
    cache: &SelectionCache,
    revision: u64,
) -> AdaptiveLayout {
    let selector = build_selector(cfg);
    adaptive_layout_with(markdown, filename, cfg, selector, cache, revision)
}

/// Same as [`adaptive_layout`] with an explicit (possibly failed) selector.
pub fn adaptive_layout_with(
    markdown: &str,
    filename: Option<&str>,
    cfg: &SelectConfig,
    selector: Result<Box<dyn Selector>, SelectError>,
    cache: &SelectionCache,
    revision: u64,
) -> AdaptiveLayout {
    let started = Instant::now();
    let analysis = mdpeek_analyzer::analyze(markdown, filename);
    let plan = sections::plan_document(markdown, filename, &analysis, cfg.excerpt_chars);

    let requested = cfg.provider.as_str().to_string();
    let mut fallback: Option<String> = None;
    let mut stats = CacheStats::default();
    let mut requests = 0u32;
    let mut select_ms = 0u64;

    // section id -> (answer, by, cached, fallback_code)
    let mut answers: HashMap<String, (Answer, String, bool, Option<String>)> = HashMap::new();
    let mut doc_answer: Option<(Answer, String, bool, Option<String>)> = None;
    let mut doc_type: Option<(String, f32, String)> = None;

    let (provider_used, model) = match &selector {
        Ok(sel) => {
            let (pid, model) = sel.id();
            let sel_key = format!("{pid}/{model}");
            // Cache lookup.
            let mut to_ask: Vec<&Section> = Vec::new();
            for s in plan.sections.iter().filter(|s| !s.candidates.is_empty()) {
                let key = cache::section_key(s, &sel_key);
                match cache.get(&key) {
                    Some(a) => {
                        stats.hits += 1;
                        answers.insert(s.id.clone(), (a, pid.to_string(), true, None));
                    }
                    None => {
                        stats.misses += 1;
                        to_ask.push(s);
                    }
                }
            }
            let doc_key = cache::doc_key(&plan, &sel_key);
            let mut ask_document = false;
            if pid != "rules" {
                match cache.get_doc(&doc_key) {
                    Some((a, dt)) => {
                        stats.hits += 1;
                        if let Some(a) = a {
                            doc_answer = Some((a, pid.to_string(), true, None));
                        }
                        if let Some((v, c)) = dt {
                            doc_type = Some((v, c, pid.to_string()));
                        }
                    }
                    None => {
                        stats.misses += 1;
                        ask_document = true;
                    }
                }
            }

            // Question cap: doc_type (1) + doc questions + per-section questions.
            let mut budget = cfg.max_questions;
            if ask_document {
                let doc_q = 1 + if plan.doc_candidates.is_empty() {
                    0
                } else {
                    q_count(plan.doc_candidates.len())
                };
                if doc_q <= budget {
                    budget -= doc_q;
                } else {
                    // Not enough room: document-level stays on rules.
                    ask_document = false;
                }
            }
            to_ask.sort_by(|a, b| b.strength().total_cmp(&a.strength()));
            let mut over_limit = Vec::new();
            let mut asked = Vec::new();
            for s in to_ask {
                let q = q_count(s.candidates.len());
                if q <= budget {
                    budget -= q;
                    asked.push(s);
                } else {
                    over_limit.push(s);
                }
            }
            for s in over_limit {
                if let Some(a) = rules_answer(&s.candidates) {
                    answers.insert(
                        s.id.clone(),
                        (a, "rules".into(), false, Some("question_limit".into())),
                    );
                }
            }

            if !asked.is_empty() || ask_document {
                asked.sort_by_key(|s| s.range.start_line);
                let req = SelectionRequest {
                    plan: &plan,
                    filename,
                    sections: asked.clone(),
                    ask_document,
                    cfg,
                };
                let t0 = Instant::now();
                let deadline = t0 + Duration::from_millis(cfg.timeout_ms);
                let res = sel.select(&req, deadline);
                select_ms = t0.elapsed().as_millis() as u64;
                match res {
                    Ok(dec) => {
                        requests = dec.requests;
                        for s in &asked {
                            let a = dec
                                .sections
                                .get(&s.id)
                                .cloned()
                                .unwrap_or_else(|| Answer::Invalid("missing".into()));
                            if !matches!(a, Answer::Invalid(_)) {
                                cache.put(cache::section_key(s, &sel_key), a.clone());
                            }
                            answers.insert(s.id.clone(), (a, pid.to_string(), false, None));
                        }
                        if ask_document {
                            let doc_ok = dec
                                .document
                                .clone()
                                .filter(|a| !matches!(a, Answer::Invalid(_)));
                            let doc_cacheable =
                                plan.doc_candidates.is_empty() || doc_ok.is_some();
                            if doc_cacheable {
                                cache.put_doc(doc_key, doc_ok.clone(), dec.doc_type.clone());
                            }
                            if let Some(a) = dec.document.clone() {
                                doc_answer = Some((a, pid.to_string(), false, None));
                            } else if !plan.doc_candidates.is_empty() {
                                doc_answer = Some((
                                    Answer::Invalid("missing".into()),
                                    pid.to_string(),
                                    false,
                                    None,
                                ));
                            }
                            if let Some((v, c)) = dec.doc_type {
                                doc_type = Some((v, c, pid.to_string()));
                            }
                        }
                        (pid.to_string(), model)
                    }
                    Err(e) => {
                        requests = sel.last_requests().unwrap_or_else(|| e_requests(&e));
                        fallback = Some(e.code());
                        // Only the sections that were asked fall back to rules;
                        // decisions already cached for unchanged sections stay
                        // (design §3.7), so one timeout does not reshuffle the
                        // whole layout. Over-limit sections are rules already.
                        for s in &asked {
                            answers.remove(&s.id);
                        }
                        if ask_document {
                            doc_answer = None;
                            doc_type = None;
                        }
                        let kept_external = answers.values().any(|(_, by, _, _)| by != "rules")
                            || doc_answer.is_some();
                        if kept_external {
                            (pid.to_string(), model)
                        } else {
                            ("rules".to_string(), format!("rules-v{REGISTRY_VERSION}"))
                        }
                    }
                }
            } else {
                (pid.to_string(), model)
            }
        }
        Err(e) => {
            fallback = Some(e.code());
            ("rules".to_string(), format!("rules-v{REGISTRY_VERSION}"))
        }
    };

    // --- compose ---
    let mut placements = Vec::new();
    let mut skipped = Vec::new();
    let ht = cfg.helps_threshold;
    let ct = cfg.confidence_threshold;
    let whole_fallback = fallback.clone();

    // Document-level first.
    let doc_anchor = Anchor {
        block_id: plan.doc_anchor.0.clone(),
        heading: plan.title.clone(),
        range: plan.doc_anchor.1,
    };
    if !plan.doc_candidates.is_empty() {
        match doc_answer {
            Some((Answer::Raw { candidate, confidence, helps, alternatives }, by, cached, _))
                if by != "rules" =>
            {
                match plan.doc_candidates.iter().find(|c| c.id == candidate) {
                    None => push_doc_rules(
                        &plan,
                        &doc_anchor,
                        Some("invalid_answer"),
                        &mut placements,
                        &mut skipped,
                        ht,
                    ),
                    Some(_) if helps < ht => skipped.push(Skipped {
                        scope: "document".into(),
                        anchor: doc_anchor.clone(),
                        reason_code: "low_helps".into(),
                        by,
                        helps: Some(helps),
                        confidence: Some(confidence),
                        fallback: None,
                    }),
                    Some(_) if confidence < ct => skipped.push(Skipped {
                        scope: "document".into(),
                        anchor: doc_anchor.clone(),
                        reason_code: "low_confidence".into(),
                        by,
                        helps: Some(helps),
                        confidence: Some(confidence),
                        fallback: None,
                    }),
                    Some(c) => placements.push(Placement {
                        scope: "document".into(),
                        anchor: doc_anchor.clone(),
                        node: c.node.clone(),
                        why: Why {
                            by,
                            candidate,
                            confidence,
                            helps,
                            reason_code: "chosen".into(),
                            alternatives: only_candidates(alternatives, &plan.doc_candidates),
                            cached,
                        },
                    }),
                }
            }
            Some((Answer::Invalid(_), by, _, _)) if by != "rules" => push_doc_rules(
                &plan,
                &doc_anchor,
                Some("invalid_answer"),
                &mut placements,
                &mut skipped,
                ht,
            ),
            _ => push_doc_rules(
                &plan,
                &doc_anchor,
                whole_fallback.as_deref(),
                &mut placements,
                &mut skipped,
                ht,
            ),
        }
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
                fallback: None,
            });
            continue;
        }
        let (mut ans, mut by, cached, mut fb) = answers
            .get(&s.id)
            .cloned()
            .unwrap_or_else(|| {
                (
                    rules_answer(&s.candidates).unwrap(),
                    "rules".into(),
                    false,
                    whole_fallback.clone(),
                )
            });
        // Unknown option → invalid.
        if let Answer::Raw { candidate, .. } = &ans
            && !s.candidates.iter().any(|c| c.id == candidate)
        {
            ans = Answer::Invalid("unknown_option".into());
        }
        if let Answer::Invalid(_) = ans {
            ans = rules_answer(&s.candidates).unwrap();
            by = "rules".into();
            fb = Some("invalid_answer".into());
        }
        let Answer::Raw { candidate, confidence, helps, alternatives } = ans else {
            unreachable!()
        };
        // Defence in depth: only enumerated candidate ids leave the server.
        let alternatives = only_candidates(alternatives, &s.candidates);
        if helps < ht || confidence < ct {
            skipped.push(Skipped {
                scope: "section".into(),
                anchor,
                reason_code: if helps < ht { "low_helps" } else { "low_confidence" }.into(),
                by,
                helps: Some(helps),
                confidence: Some(confidence),
                fallback: fb,
            });
            continue;
        }
        let node = s
            .candidates
            .iter()
            .find(|c| c.id == candidate)
            .unwrap()
            .node
            .clone();
        let reason_code = match (&fb, by.as_str()) {
            (Some(f), _) => format!("fallback_{f}"),
            (None, "rules") => "rules_score".into(),
            _ => "chosen".into(),
        };
        placements.push(Placement {
            scope: "section".into(),
            anchor,
            node,
            why: Why {
                by,
                candidate,
                confidence,
                helps,
                reason_code,
                alternatives,
                cached,
            },
        });
    }

    // Validate every node (allowlist + sourceRange bounds); drop failures.
    let mut kept = Vec::new();
    for mut p in placements {
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
                fallback: None,
            }),
        }
    }

    let doc_type = match doc_type {
        Some((v, c, by)) if c >= ct => DocTypeInfo { value: v, confidence: c, by },
        _ => DocTypeInfo {
            value: plan.rules_doc_type.clone(),
            confidence: plan.rules_doc_type_confidence,
            by: "rules".into(),
        },
    };

    AdaptiveLayout {
        version: LAYOUT_VERSION,
        revision,
        doc_hash: plan.doc_hash.clone(),
        selector: SelectorInfo {
            provider: provider_used,
            requested,
            model,
            fallback,
            requests,
        },
        doc_type,
        placements: kept,
        skipped,
        cache: stats,
        timing_ms: Timing {
            select: select_ms,
            total: started.elapsed().as_millis() as u64,
        },
    }
}

fn e_requests(e: &SelectError) -> u32 {
    match e {
        SelectError::Disabled
        | SelectError::MissingKey
        | SelectError::NotBuilt
        | SelectError::InsecureBaseUrl
        | SelectError::FakeScript
        | SelectError::Scripted(_) => 0,
        _ => 1,
    }
}

fn q_count(n_candidates: usize) -> usize {
    if n_candidates > 1 { 2 } else { 1 }
}

fn push_doc_rules(
    plan: &DocPlan,
    anchor: &Anchor,
    fallback: Option<&str>,
    placements: &mut Vec<Placement>,
    skipped: &mut Vec<Skipped>,
    ht: f32,
) {
    let mut n = 0;
    for c in &plan.doc_candidates {
        if n >= MAX_DOC_WIDGETS {
            break;
        }
        if c.score < ht {
            continue;
        }
        n += 1;
        placements.push(Placement {
            scope: "document".into(),
            anchor: anchor.clone(),
            node: c.node.clone(),
            why: Why {
                by: "rules".into(),
                candidate: c.id.to_string(),
                confidence: 1.0,
                helps: c.score,
                reason_code: match fallback {
                    Some(f) => format!("fallback_{f}"),
                    None => "rules_score".into(),
                },
                alternatives: Vec::new(),
                cached: false,
            },
        });
    }
    if n == 0 {
        skipped.push(Skipped {
            scope: "document".into(),
            anchor: anchor.clone(),
            reason_code: "low_helps".into(),
            by: "rules".into(),
            helps: None,
            confidence: None,
            fallback: fallback.map(str::to_string),
        });
    }
}

/// Pretty JSON for the `mdpeek select` command.
pub fn layout_json(layout: &AdaptiveLayout) -> String {
    serde_json::to_string_pretty(layout).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MD: &str = "# Tool\n\nIntro.\n\n## Install\n\n```sh\ncargo build\ncargo install --path .\ncp a b\n```\n\n## Steps\n\n1. one\n2. two\n3. three\n4. four\n5. five\n\n## License\n\nMIT\n";

    #[test]
    fn rules_layout_is_offline_and_valid() {
        let cfg = SelectConfig::default();
        let cache = SelectionCache::new(16);
        let l = adaptive_layout(MD, Some("README.md"), &cfg, &cache, 3);
        assert_eq!(l.version, 1);
        assert_eq!(l.revision, 3);
        assert_eq!(l.selector.provider, "rules");
        assert!(l.selector.fallback.is_none());
        assert_eq!(l.selector.requests, 0);
        let kinds: Vec<_> = l.placements.iter().map(|p| p.why.candidate.as_str()).collect();
        assert!(kinds.contains(&"ConfigViewer:commands"), "{kinds:?}");
        assert!(kinds.contains(&"StepNavigator"));
        assert!(l.skipped.iter().any(|s| s.anchor.heading == "License"));
        for p in &l.placements {
            assert!(p.node.meta().source_range.is_some());
        }
    }

    #[test]
    fn jev_without_permission_falls_back() {
        let cfg = SelectConfig {
            provider: Provider::Jev,
            ..Default::default()
        };
        let cache = SelectionCache::new(16);
        let l = adaptive_layout(MD, None, &cfg, &cache, 1);
        assert_eq!(l.selector.provider, "rules");
        assert_eq!(l.selector.fallback.as_deref(), Some("disabled"));
        assert!(!l.placements.is_empty());
    }

    #[test]
    fn cache_hits_on_second_run() {
        let cfg = SelectConfig::default();
        let cache = SelectionCache::new(16);
        let _ = adaptive_layout(MD, None, &cfg, &cache, 1);
        let l = adaptive_layout(MD, None, &cfg, &cache, 2);
        assert!(l.cache.hits >= 2, "{:?}", l.cache);
        assert_eq!(l.cache.misses, 0);
    }
}
