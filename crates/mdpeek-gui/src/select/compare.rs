//! Comparison-card candidates from comparison tables (deterministic).
//!
//! A table compares 2–4 options on criteria. Options are the columns when the
//! top-left header names the criteria (`観点`, `Criteria`, empty …) and the rows
//! otherwise. A criterion row is *scored* when every written cell is a score on
//! one scale: plain numbers up to 5 / 10 / 100 (optionally `点` / `%`), stars
//! (`★★★☆☆`, out of 5) or ◎ ○ △ × (3 / 2 / 1 / 0, out of 3). Other rows
//! (durations, prices, notes) are kept verbatim as facts. A recommendation is
//! only shown when the document states one (推奨 / recommended / 採用 …).

use std::sync::OnceLock;

use regex::Regex;

use super::chart::{is_blank, parse_cell, plain};
use crate::ir::node::{
    ComparisonCriterion, ComparisonFact, ComparisonNode, ComparisonOption, NodeMeta,
};

fn criteria_header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(観点|項目|指標|評価軸|評価項目|評価|基準|比較項目|criteria|criterion|metric|metrics|aspect|dimension|feature|features)$")
            .unwrap()
    })
}
fn option_header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(案|候補|選択肢|方式|手法|構成|製品|ツール|プラン|option|options|alternative|approach|candidate|plan|product|tool|architecture|name)$")
            .unwrap()
    })
}
fn cue_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\s*[（(]\s*(推奨|おすすめ|採用|recommended)\s*[)）]|✅|★推奨|\s*\[(推奨|recommended)\]").unwrap())
}
fn recommend_word_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)推奨|おすすめ|採用|を選ぶ|に決定|recommend|we choose|we pick|go with")
            .unwrap()
    })
}
fn verdict_row_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(推奨|結論|採用|判定|総合判定|recommendation|recommended|verdict)$")
            .unwrap()
    })
}
/// Rows that are measurements, not scores, even when written as bare numbers.
fn fact_row_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)時間|日数|期間|価格|費用|料金|円|件数|人数|サイズ|容量|レイテンシ|latency|time|duration|days|price|fee|count|size|\$")
            .unwrap()
    })
}

/// Words that mark a section as a comparison (raises the rules score).
pub fn is_comparison_heading(h: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)比較|選定|検討|評価|違い|トレードオフ|comparison|compare|versus|\bvs\.?\b|trade-?offs?|evaluation").unwrap()
    })
    .is_match(h)
}

/// One cell as a score: (value, scale max, kind tag).
fn score(cell: &str) -> Option<(f64, f64, u8)> {
    let t = plain(cell);
    if !t.is_empty() && t.chars().all(|c| matches!(c, '★' | '☆' | ' ')) {
        let n = t.chars().filter(|c| *c == '★').count() as f64;
        return Some((n, 5.0, 1));
    }
    match t.as_str() {
        "◎" => return Some((3.0, 3.0, 2)),
        "○" | "〇" | "◯" => return Some((2.0, 3.0, 2)),
        "△" => return Some((1.0, 3.0, 2)),
        "×" | "✕" | "✗" => return Some((0.0, 3.0, 2)),
        _ => {}
    }
    let n = parse_cell(&t)?;
    let unit = n.unit.as_deref();
    if n.prefix
        || !matches!(
            unit,
            None | Some("点") | Some("%") | Some("/5") | Some("/10") | Some("/100")
        )
    {
        return None;
    }
    let max = match unit {
        Some("%") | Some("/100") => 100.0,
        Some("/5") => 5.0,
        Some("/10") => 10.0,
        _ => 0.0, // decided per row from the largest value
    };
    (n.value >= 0.0).then_some((n.value, max, 0))
}

/// A row of cells as a scored criterion, or `None` (then it is a fact).
fn criterion(name: &str, cells: &[String]) -> Option<ComparisonCriterion> {
    if fact_row_re().is_match(name) {
        return None;
    }
    let mut kind = None;
    let mut max: f64 = 0.0;
    let mut scores = Vec::with_capacity(cells.len());
    for c in cells {
        if is_blank(plain(c).as_str()) {
            scores.push(None);
            continue;
        }
        let (v, m, k) = score(c)?;
        if kind.is_some_and(|x| x != k) {
            return None;
        }
        kind = Some(k);
        max = max.max(m);
        scores.push(Some(v));
    }
    let vals: Vec<f64> = scores.iter().flatten().copied().collect();
    if vals.len() < 2 {
        return None;
    }
    let top = vals.iter().copied().fold(0.0, f64::max);
    if max == 0.0 {
        max = if top <= 5.0 {
            5.0
        } else if top <= 10.0 {
            10.0
        } else if top <= 100.0 {
            100.0
        } else {
            return None;
        };
    }
    if top > max {
        return None;
    }
    Some(ComparisonCriterion {
        name: name.to_string(),
        max,
        scores,
        cells: cells.to_vec(),
    })
}

/// Build comparison cards from one table; `section_text` is searched for a
/// sentence that recommends exactly one option.
pub fn comparison_from_table(
    header: &[String],
    rows: &[Vec<String>],
    section_text: &str,
    title: Option<String>,
    meta: NodeMeta,
) -> Option<ComparisonNode> {
    if header.len() < 3 && rows.len() < 3 {
        return None;
    }
    let h0 = plain(header.first()?);
    let cols_are_options = if h0.is_empty() || criteria_header_re().is_match(h0.trim()) {
        true
    } else if option_header_re().is_match(h0.trim()) {
        false
    } else {
        // Fewer options than criteria is the usual shape.
        header.len() - 1 <= rows.len()
    };
    // (option names, criterion rows as (name, cells per option))
    let (mut names, lines): (Vec<String>, Vec<(String, Vec<String>)>) = if cols_are_options {
        let n = header.len() - 1;
        (
            header[1..].iter().map(|h| h.trim().to_string()).collect(),
            rows.iter()
                .map(|r| {
                    let cells = (1..=n)
                        .map(|i| r.get(i).cloned().unwrap_or_default())
                        .collect();
                    (plain(r.first().map(String::as_str).unwrap_or("")), cells)
                })
                .collect(),
        )
    } else {
        (
            rows.iter()
                .map(|r| r.first().cloned().unwrap_or_default().trim().to_string())
                .collect(),
            (1..header.len())
                .map(|j| {
                    (
                        plain(&header[j]),
                        rows.iter()
                            .map(|r| r.get(j).cloned().unwrap_or_default())
                            .collect(),
                    )
                })
                .collect(),
        )
    };
    if !(2..=4).contains(&names.len()) || names.iter().any(|n| plain(n).is_empty()) {
        return None;
    }

    let mut recommended = None;
    let mut recommended_by = None;
    for (i, n) in names.iter_mut().enumerate() {
        if cue_re().is_match(n) {
            *n = cue_re().replace_all(n, "").trim().to_string();
            if recommended.is_none() {
                recommended = Some(i);
                recommended_by = Some("table header".to_string());
            }
        }
    }
    let names: Vec<String> = names.iter().map(|n| plain(n)).collect();

    let mut criteria = Vec::new();
    let mut facts = Vec::new();
    for (name, cells) in &lines {
        if name.is_empty() {
            continue;
        }
        if verdict_row_re().is_match(name) {
            let yes: Vec<usize> = cells
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    let t = plain(c).to_lowercase();
                    matches!(
                        t.as_str(),
                        "◎" | "○"
                            | "〇"
                            | "✓"
                            | "✔"
                            | "✅"
                            | "yes"
                            | "はい"
                            | "推奨"
                            | "採用"
                            | "recommended"
                    )
                })
                .map(|(i, _)| i)
                .collect();
            if yes.len() == 1 && recommended.is_none() {
                recommended = Some(yes[0]);
                recommended_by = Some(format!("row “{name}”"));
            }
            facts.push(ComparisonFact {
                name: name.clone(),
                cells: cells.clone(),
            });
            continue;
        }
        match criterion(name, cells) {
            Some(c) => criteria.push(c),
            None => facts.push(ComparisonFact {
                name: name.clone(),
                cells: cells.clone(),
            }),
        }
    }
    if criteria.len() < 2 {
        return None;
    }
    if recommended.is_none() {
        // A sentence in the section that recommends exactly one option.
        for sentence in section_text
            .split(['。', '\n', '!', '！'])
            .flat_map(|s| s.split(". "))
        {
            if !recommend_word_re().is_match(sentence) {
                continue;
            }
            let low = sentence.to_lowercase();
            let hits: Vec<usize> = names
                .iter()
                .enumerate()
                .filter(|(_, n)| low.contains(&n.to_lowercase()))
                .map(|(i, _)| i)
                .collect();
            if hits.len() == 1 {
                recommended = Some(hits[0]);
                let s: String = sentence
                    .trim()
                    .trim_start_matches(['-', '*', '>', ' '])
                    .chars()
                    .take(120)
                    .collect();
                recommended_by = Some(s);
                break;
            }
        }
    }
    Some(ComparisonNode {
        meta,
        title,
        options: names
            .into_iter()
            .map(|name| ComparisonOption { name, note: None })
            .collect(),
        criteria,
        facts,
        recommended,
        recommended_by,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(rows: &[&[&str]]) -> (Vec<String>, Vec<Vec<String>>) {
        let all: Vec<Vec<String>> = rows
            .iter()
            .map(|r| r.iter().map(|c| c.to_string()).collect())
            .collect();
        (all[0].clone(), all[1..].to_vec())
    }

    #[test]
    fn options_as_columns_with_scores_facts_and_a_stated_recommendation() {
        let (h, r) = t(&[
            &["観点", "Single Agent", "Agent Swarm"],
            &["実行速度", "45", "95"],
            &["コスト効率", "95", "45"],
            &["品質", "70", "95"],
            &["想定実行時間", "28 min", "12 min"],
        ]);
        let c = comparison_from_table(
            &h,
            &r,
            "品質を重視するため Agent Swarm を推奨する。",
            None,
            NodeMeta::default(),
        )
        .unwrap();
        assert_eq!(c.options.len(), 2);
        assert_eq!(c.criteria.len(), 3);
        assert_eq!(c.criteria[0].max, 100.0);
        assert_eq!(c.criteria[0].scores, [Some(45.0), Some(95.0)]);
        assert_eq!(c.facts.len(), 1);
        assert_eq!(c.facts[0].cells, ["28 min", "12 min"]);
        assert_eq!(c.recommended, Some(1));
        assert!(c.recommended_by.unwrap().contains("Agent Swarm"));
    }

    #[test]
    fn options_as_rows_symbols_and_stars() {
        let (h, r) = t(&[
            &["案", "速度", "保守性", "評判"],
            &["A 案 (推奨)", "◎", "○", "★★★★☆"],
            &["B 案", "△", "◎", "★★☆☆☆"],
        ]);
        let c = comparison_from_table(&h, &r, "", None, NodeMeta::default()).unwrap();
        assert_eq!(c.options[0].name, "A 案");
        assert_eq!(c.recommended, Some(0));
        assert_eq!(c.criteria.len(), 3);
        assert_eq!(c.criteria[0].max, 3.0);
        assert_eq!(c.criteria[2].scores, [Some(4.0), Some(2.0)]);
    }

    #[test]
    fn no_recommendation_unless_stated_and_non_comparisons_rejected() {
        let (h, r) = t(&[&["", "X", "Y"], &["速さ", "8", "6"], &["安さ", "5", "9"]]);
        let c = comparison_from_table(&h, &r, "どちらも妥当。", None, NodeMeta::default()).unwrap();
        assert_eq!(c.recommended, None);
        assert_eq!(c.criteria[0].max, 10.0);
        // Only one scored row.
        let (h, r) = t(&[
            &["観点", "X", "Y"],
            &["速さ", "8", "6"],
            &["メモ", "速い", "安い"],
        ]);
        assert!(comparison_from_table(&h, &r, "", None, NodeMeta::default()).is_none());
        // Five options.
        let (h, r) = t(&[
            &["観点", "a", "b", "c", "d", "e"],
            &["x", "1", "2", "3", "4", "5"],
            &["y", "1", "2", "3", "4", "5"],
        ]);
        assert!(comparison_from_table(&h, &r, "", None, NodeMeta::default()).is_none());
    }
}
