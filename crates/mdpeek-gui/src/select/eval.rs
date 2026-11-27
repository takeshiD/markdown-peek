//! Selection evaluation (design §7, R9): compare models against hand labels.
//!
//! Fixtures: `<dir>/*.md` plus `<dir>/labels.json`:
//!
//! ```json
//! { "readme.md": { "Installation": "ConfigViewer:commands", "License": "none" } }
//! ```
//!
//! Metrics per provider: section-level agreement, *none appropriateness*
//! (share of `none`-labelled sections where nothing was shown), widget recall
//! (share of widget-labelled sections where the exact widget was shown) and
//! select latency p50/p95 over `runs` cold-cache runs per document. A model
//! that cannot run (bad entry, key not set) is reported as `not_run` with the
//! reason — never with numbers.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use serde::Serialize;

use super::{ModelSpec, SelectConfig, SelectionCache, adaptive_layout};

#[derive(Debug, Clone, Serialize)]
pub struct Mismatch {
    pub doc: String,
    pub heading: String,
    pub expected: String,
    pub got: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderReport {
    pub provider: String,
    /// `ok` | `degraded` (some runs fell back to rules) | `not_run`
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labelled_sections: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accuracy: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub none_appropriate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widget_recall: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p50_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p95_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallbacks: Option<BTreeMap<String, usize>>,
    pub mismatches: Vec<Mismatch>,
}

impl ProviderReport {
    fn not_run(provider: &str, reason: String) -> Self {
        ProviderReport {
            provider: provider.into(),
            status: "not_run".into(),
            reason: Some(reason),
            docs: None,
            labelled_sections: None,
            accuracy: None,
            none_appropriate: None,
            widget_recall: None,
            p50_ms: None,
            p95_ms: None,
            fallbacks: None,
            mismatches: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalReport {
    pub fixtures: Vec<String>,
    pub doc_types: Vec<String>,
    pub runs: usize,
    pub providers: Vec<ProviderReport>,
}

type Labels = BTreeMap<String, BTreeMap<String, String>>;

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((p / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn run_provider(
    docs: &[(String, String)],
    labels: &Labels,
    cfg: &SelectConfig,
    spec: &ModelSpec,
    runs: usize,
) -> ProviderReport {
    let mut times = Vec::new();
    let mut total = 0usize;
    let mut agree = 0usize;
    let mut none_total = 0usize;
    let mut none_ok = 0usize;
    let mut w_total = 0usize;
    let mut w_ok = 0usize;
    let mut fallbacks: BTreeMap<String, usize> = BTreeMap::new();
    let mut mismatches = Vec::new();
    for (name, md) in docs {
        let mut last = None;
        for _ in 0..runs.max(1) {
            let cache = SelectionCache::new(256);
            let t = Instant::now();
            let l = adaptive_layout(md, Some(name), cfg, spec, &cache, 0);
            times.push(t.elapsed().as_secs_f64() * 1000.0);
            if let Some(f) = &l.selector.error {
                *fallbacks.entry(f.clone()).or_default() += 1;
            }
            last = Some(l);
        }
        let l = last.unwrap();
        let Some(lab) = labels.get(name) else {
            continue;
        };
        for (heading, expected) in lab {
            let got = l
                .placements
                .iter()
                .find(|p| p.scope == "section" && &p.anchor.heading == heading)
                .map(|p| p.why.candidate.clone())
                .unwrap_or_else(|| "none".into());
            total += 1;
            if expected == "none" {
                none_total += 1;
                if got == "none" {
                    none_ok += 1;
                }
            } else {
                w_total += 1;
                if &got == expected {
                    w_ok += 1;
                }
            }
            if &got == expected {
                agree += 1;
            } else {
                mismatches.push(Mismatch {
                    doc: name.clone(),
                    heading: heading.clone(),
                    expected: expected.clone(),
                    got,
                });
            }
        }
    }
    times.sort_by(|a, b| a.total_cmp(b));
    let ratio =
        |a: usize, b: usize| (b > 0).then(|| (a as f64 / b as f64 * 1000.0).round() / 1000.0);
    ProviderReport {
        provider: spec.name.clone(),
        status: if fallbacks.is_empty() {
            "ok"
        } else {
            "degraded"
        }
        .into(),
        reason: (!fallbacks.is_empty())
            .then(|| "some runs failed (no aids shown); numbers include those runs".into()),
        docs: Some(docs.len()),
        labelled_sections: Some(total),
        accuracy: ratio(agree, total),
        none_appropriate: ratio(none_ok, none_total),
        widget_recall: ratio(w_ok, w_total),
        p50_ms: Some((percentile(&times, 50.0) * 100.0).round() / 100.0),
        p95_ms: Some((percentile(&times, 95.0) * 100.0).round() / 100.0),
        fallbacks: Some(fallbacks),
        mismatches,
    }
}

/// Evaluate each model that can run; the others are reported as `not_run`.
pub fn evaluate(
    dir: &Path,
    base: &SelectConfig,
    models: &[ModelSpec],
    runs: usize,
) -> anyhow::Result<EvalReport> {
    let labels_text = std::fs::read_to_string(dir.join("labels.json"))
        .map_err(|e| anyhow::anyhow!("reading {}/labels.json: {e}", dir.display()))?;
    let labels: Labels = serde_json::from_str(&labels_text)?;
    let mut docs = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        docs.push((name, std::fs::read_to_string(&p)?));
    }
    let mut doc_types: Vec<String> = docs
        .iter()
        .map(|(n, md)| {
            let a = mdpeek_analyzer::analyze(md, Some(n));
            serde_json::to_value(a.model.doc_type.value)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default()
        })
        .collect();
    doc_types.sort();
    doc_types.dedup();

    let mut providers = Vec::new();
    for spec in models {
        match spec.problem() {
            Some(p) => providers.push(ProviderReport::not_run(
                &spec.name,
                format!("未実行 (not run): {p}"),
            )),
            None => providers.push(run_provider(&docs, &labels, base, spec, runs)),
        }
    }

    Ok(EvalReport {
        fixtures: docs.iter().map(|d| d.0.clone()).collect(),
        doc_types,
        runs,
        providers,
    })
}

/// Human-readable summary table.
pub fn render_text(r: &EvalReport) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "fixtures: {} (analyzer doc types {}: {}), runs/doc: {}\n",
        r.fixtures.len(),
        r.doc_types.len(),
        r.doc_types.join(", "),
        r.runs
    ));
    s.push_str("provider  status    accuracy  none_ok  widget_recall  p50_ms   p95_ms\n");
    for p in &r.providers {
        if p.status == "not_run" {
            s.push_str(&format!(
                "{:<9} not_run   {}\n",
                p.provider,
                p.reason.clone().unwrap_or_default()
            ));
            continue;
        }
        let f = |v: Option<f64>| v.map(|x| format!("{x:.3}")).unwrap_or_else(|| "-".into());
        s.push_str(&format!(
            "{:<9} {:<9} {:<9} {:<8} {:<14} {:<8} {}\n",
            p.provider,
            p.status,
            f(p.accuracy),
            f(p.none_appropriate),
            f(p.widget_recall),
            f(p.p50_ms),
            f(p.p95_ms)
        ));
        if let Some(fb) = &p.fallbacks
            && !fb.is_empty()
        {
            s.push_str(&format!("          fallbacks: {fb:?}\n"));
        }
        for m in &p.mismatches {
            s.push_str(&format!(
                "          mismatch {} / {}: expected {} got {}\n",
                m.doc, m.heading, m.expected, m.got
            ));
        }
    }
    s
}
