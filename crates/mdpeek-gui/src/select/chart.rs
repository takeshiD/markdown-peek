//! Chart candidates from Markdown tables (deterministic, no invented data).
//!
//! A table becomes a chart only when it has a label column and at least one
//! *measure* column: most non-empty cells parse as numbers that share one unit
//! (`12 ms`, `$4.20`, `35%`, `1,200`). Mixed units, identifiers (`#`, `ID`,
//! `1..n` rank columns) and years are never plotted as magnitudes. Values keep
//! their original cell text next to the parsed number.

use std::sync::OnceLock;

use regex::Regex;

use crate::ir::node::{ChartNode, ChartSeries, ChartType, NodeMeta};

/// Most bars a horizontal bar chart shows before it stops being readable.
const MAX_BAR_ROWS: usize = 30;
/// Most points on a line.
const MAX_LINE_ROWS: usize = 200;
/// Most measure columns offered (the reader switches between them).
const MAX_SERIES: usize = 6;

fn number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?P<pre>[$¥€£￥~≈約]?)\s*(?P<num>[-+−]?(?:\d{1,3}(?:,\d{3})+|\d+)(?:\.\d+)?)\s*(?P<suf>[^\d\s][^\d]{0,7})?$",
        )
        .unwrap()
    })
}

fn id_header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(#|no\.?|id|index|idx|番号|順位|rank|ランク|issue|pr|ticket|row)$")
            .unwrap()
    })
}

fn temporal_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?ix)^(
                (19|20)\d{2}([-/.](0?[1-9]|1[0-2])([-/.](0?[1-9]|[12]\d|3[01]))?)? |
                (19|20)\d{2}\s*年(\s*\d{1,2}\s*月(\s*\d{1,2}\s*日)?)? |
                \d{1,2}\s*月(\s*\d{1,2}\s*日)? |
                (19|20)?\d{2}\s*-?\s*q[1-4] | q[1-4](\s*(19|20)?\d{2})? |
                h[12](\s*(19|20)?\d{2})? | (19|20)\d{2}\s*h[12] |
                (jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\.?(\s+(19|20)?\d{2})? |
                v?\d+\.\d+(\.\d+)?(-[0-9a-z.]+)? |
                (week|wk|w|sprint|day|第)\s*\d+\s*(週|日|回)?
            )$",
        )
        .unwrap()
    })
}

/// Strip inline Markdown that wraps a whole cell (`**12**`, `` `3` ``).
pub(crate) fn plain(cell: &str) -> String {
    let mut t = cell.trim();
    loop {
        let before = t;
        for w in ["**", "__", "`", "*", "_", "~~"] {
            if t.len() > 2 * w.len()
                && let Some(x) = t.strip_prefix(w).and_then(|x| x.strip_suffix(w))
            {
                t = x.trim();
            }
        }
        if t == before {
            break;
        }
    }
    t.to_string()
}

pub(crate) fn is_blank(cell: &str) -> bool {
    matches!(
        cell,
        "" | "-" | "—" | "–" | "n/a" | "N/A" | "NA" | "?" | "TBD" | "tbd"
    )
}

/// A parsed numeric cell: value plus its affix (`"$"` prefix or `"ms"` suffix).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Num {
    pub(crate) value: f64,
    pub(crate) unit: Option<String>,
    pub(crate) prefix: bool,
}

pub(crate) fn parse_cell(cell: &str) -> Option<Num> {
    let c = number_re().captures(cell)?;
    let raw = c["num"].replace(',', "").replace('−', "-");
    let value: f64 = raw.parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    let pre = c.name("pre").map(|m| m.as_str()).unwrap_or("");
    let suf = c.name("suf").map(|m| m.as_str().trim()).unwrap_or("");
    // `~` / `≈` / `約` mean "about"; the number is still the stated one.
    let pre = if matches!(pre, "~" | "≈" | "約") {
        ""
    } else {
        pre
    };
    if !pre.is_empty() && !suf.is_empty() && suf != "%" {
        return None;
    }
    // A trailing word ("12 items left") is prose, not a unit.
    if suf.chars().count() > 5 || suf.contains(' ') {
        return None;
    }
    let (unit, prefix) = if !pre.is_empty() {
        (Some(pre.to_string()), true)
    } else if !suf.is_empty() {
        (Some(suf.to_string()), false)
    } else {
        (None, false)
    };
    Some(Num {
        value,
        unit,
        prefix,
    })
}

/// A column that can be plotted: every non-blank cell parses with one unit.
fn measure(cells: &[String], header: &str) -> Option<(Option<String>, bool, Vec<Option<f64>>)> {
    if id_header_re().is_match(header.trim()) {
        return None;
    }
    let mut unit: Option<(Option<String>, bool)> = None;
    let mut values = Vec::with_capacity(cells.len());
    let mut filled = 0usize;
    for c in cells {
        if is_blank(c) {
            values.push(None);
            continue;
        }
        let n = parse_cell(c)?;
        let key = (n.unit.as_ref().map(|u| u.to_lowercase()), n.prefix);
        match &unit {
            None => unit = Some((n.unit.clone(), n.prefix)),
            Some((u, p)) if (u.as_ref().map(|x| x.to_lowercase()), *p) != key => return None,
            _ => {}
        }
        values.push(Some(n.value));
        filled += 1;
    }
    // Every written cell parsed; at least half the rows must have a value.
    if filled < 2 || filled * 2 < cells.len() {
        return None;
    }
    let (u, p) = unit.unwrap_or((None, false));
    let ints: Vec<f64> = values.iter().flatten().copied().collect();
    if u.is_none() && ints.iter().all(|v| v.fract() == 0.0) {
        // 1, 2, 3, … is a row number; 1990–2100 throughout is a year axis.
        let seq = ints.windows(2).all(|w| w[1] - w[0] == 1.0) && ints.first() == Some(&1.0);
        let years = ints.iter().all(|v| (1900.0..=2100.0).contains(v));
        if seq || years {
            return None;
        }
    }
    Some((u, p, values))
}

/// Build a chart from one table, or `None` when the table is not chartable.
pub fn chart_from_table(
    header: &[String],
    rows: &[Vec<String>],
    title: Option<String>,
    meta: NodeMeta,
) -> Option<ChartNode> {
    if rows.len() < 3 || header.len() < 2 {
        return None;
    }
    let col = |i: usize| -> Vec<String> {
        rows.iter()
            .map(|r| plain(r.get(i).map(String::as_str).unwrap_or("")))
            .collect()
    };
    let mut label_col: Option<usize> = None;
    let mut series = Vec::new();
    for (i, h) in header.iter().enumerate() {
        let cells = col(i);
        match measure(&cells, h) {
            Some((unit, unit_prefix, values)) if label_col.is_some() || i > 0 => {
                if series.len() < MAX_SERIES {
                    series.push(ChartSeries {
                        name: plain(h),
                        unit,
                        unit_prefix,
                        values,
                        cells: rows
                            .iter()
                            .map(|r| r.get(i).cloned().unwrap_or_default())
                            .collect(),
                    });
                }
            }
            _ if label_col.is_none() => label_col = Some(i),
            _ => {}
        }
    }
    let label_col = label_col?;
    if series.is_empty() {
        return None;
    }
    let labels = col(label_col);
    if labels.iter().all(|l| l.is_empty()) {
        return None;
    }
    let temporal = labels.iter().all(|l| temporal_re().is_match(l.trim()));
    let chart = if temporal && rows.len() >= 4 {
        ChartType::Line
    } else {
        ChartType::Bar
    };
    let cap = match chart {
        ChartType::Bar => MAX_BAR_ROWS,
        ChartType::Line => MAX_LINE_ROWS,
    };
    if rows.len() > cap {
        return None;
    }
    Some(ChartNode {
        meta,
        chart,
        title,
        label_header: plain(&header[label_col]),
        labels,
        series,
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
    fn parses_units_and_rejects_prose() {
        assert_eq!(parse_cell("1,200").unwrap().value, 1200.0);
        let ms = parse_cell("140 ms").unwrap();
        assert_eq!(
            (ms.value, ms.unit.as_deref(), ms.prefix),
            (140.0, Some("ms"), false)
        );
        let usd = parse_cell("$4.20").unwrap();
        assert_eq!(
            (usd.value, usd.unit.as_deref(), usd.prefix),
            (4.2, Some("$"), true)
        );
        assert_eq!(parse_cell("−3").unwrap().value, -3.0);
        assert_eq!(parse_cell("35%").unwrap().unit.as_deref(), Some("%"));
        assert!(parse_cell("12 items were left").is_none());
        assert!(parse_cell("2024-01-02").is_none());
        assert!(parse_cell("v1.2.3").is_none());
        assert!(parse_cell("ok").is_none());
    }

    #[test]
    fn bar_chart_from_category_table() {
        let (h, r) = t(&[
            &["Service", "p95 latency", "Errors"],
            &["auth", "120 ms", "3"],
            &["billing", "340 ms", "12"],
            &["search", "**95 ms**", "-"],
        ]);
        let c = chart_from_table(&h, &r, Some("Latency".into()), NodeMeta::default()).unwrap();
        assert_eq!(c.chart, ChartType::Bar);
        assert_eq!(c.label_header, "Service");
        assert_eq!(c.labels, ["auth", "billing", "search"]);
        assert_eq!(c.series.len(), 2);
        assert_eq!(c.series[0].unit.as_deref(), Some("ms"));
        assert_eq!(c.series[0].values, [Some(120.0), Some(340.0), Some(95.0)]);
        // The original text rides along for tooltips.
        assert_eq!(c.series[0].cells[2], "**95 ms**");
        assert_eq!(c.series[1].values[2], None);
    }

    #[test]
    fn line_chart_over_time_and_years_are_labels() {
        let (h, r) = t(&[
            &["Year", "Users"],
            &["2021", "1,000"],
            &["2022", "2,400"],
            &["2023", "3,100"],
            &["2024", "5,000"],
        ]);
        let c = chart_from_table(&h, &r, None, NodeMeta::default()).unwrap();
        assert_eq!(c.chart, ChartType::Line);
        assert_eq!(c.label_header, "Year");
        assert_eq!(c.series.len(), 1);
        assert_eq!(c.series[0].name, "Users");
    }

    #[test]
    fn not_chartable() {
        // Mixed units in a column.
        let (h, r) = t(&[
            &["Item", "Size"],
            &["a", "3 MB"],
            &["b", "120 ms"],
            &["c", "4 MB"],
        ]);
        assert!(chart_from_table(&h, &r, None, NodeMeta::default()).is_none());
        // Only identifiers / row numbers.
        let (h, r) = t(&[&["#", "Task"], &["1", "x"], &["2", "y"], &["3", "z"]]);
        assert!(chart_from_table(&h, &r, None, NodeMeta::default()).is_none());
        let (h, r) = t(&[&["Step", "Order"], &["a", "1"], &["b", "2"], &["c", "3"]]);
        assert!(chart_from_table(&h, &r, None, NodeMeta::default()).is_none());
        // Text-only table.
        let (h, r) = t(&[
            &["Name", "Status"],
            &["a", "done"],
            &["b", "open"],
            &["c", "open"],
        ]);
        assert!(chart_from_table(&h, &r, None, NodeMeta::default()).is_none());
        // Too few rows.
        let (h, r) = t(&[&["k", "v"], &["a", "1"], &["b", "2"]]);
        assert!(chart_from_table(&h, &r, None, NodeMeta::default()).is_none());
    }
}
