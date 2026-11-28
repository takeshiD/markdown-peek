//! Verdict lists: list items written `R1: satisfied — C1 and C3 passed …`, as
//! in acceptance records, review results and requirement traces.
//!
//! Everything is read from the item's own text. The outcome comes from the
//! verdict word, the cited ids (`C3`, `S4`, `REG1`, `C6〜C8`) are the ones
//! written in the text, and the "re-checked" / "untested" notes are sentences
//! of the text picked out by fixed phrases.

use std::sync::OnceLock;

use regex::Regex;

use crate::ir::SourceRange;
use crate::ir::node::{VerdictItem, VerdictOutcome};

/// `ID: verdict — text`. The verdict is one to three words before a dash.
fn line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?:\*\*|__)?([A-Z]{1,5}-?\d{1,4})(?:\*\*|__)?\s*[:：]\s*(?:\*\*|__|`)?([^\s—–:：。.,、`*][^—–:：。.,、`*]{0,30}?)(?:\*\*|__|`)?\s*(?:—|–|--|\s-\s)\s*(.+)$",
        )
        .unwrap()
    })
}

/// An id another item or check is known by: `C3`, `S4`, `REG1`, a range
/// `C6〜C8` / `C1–C33`.
fn cite_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"([A-Z]{1,3})\d{1,4}(?:\s*[〜~–]\s*(?:[A-Z]{1,3})?\d{1,4})?").unwrap())
}

/// Prefixes that are units or formats, not ids (`UTF8`, `SHA256`, `MD5`).
const NOT_IDS: &[&str] = &["UTF", "SHA", "MD", "ES", "CP", "AES", "RSA", "X", "H", "V"];

/// The item was failed (or sent back) before and checked again.
const RECHECK: &[&str] = &[
    "前回", "差し戻", "再検証", "再検収", "修正後", "直って", "直った", "rework", "sent back", "previously failed",
    "previously rejected", "regression",
];
/// Part of the item was not tried.
const UNTESTED: &[&str] = &[
    "試していない", "試せていない", "未確認", "確認していない", "確認できていない", "未検証", "not tested",
    "untested", "not tried", "not verified", "unverified", "could not be checked",
];

/// Parse one list item (its text without the list marker).
pub fn verdict_item(text: &str, range: SourceRange) -> Option<VerdictItem> {
    let c = line_re().captures(text.trim())?;
    let id = c[1].to_string();
    let verdict = c[2].trim().to_string();
    if verdict.split_whitespace().count() > 3 {
        return None;
    }
    let body = c[3].trim().to_string();
    let prefix: String = id.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let sentences = sentences(&body);
    let find = |words: &[&str], reg: bool| {
        sentences
            .iter()
            .find(|s| {
                let l = s.to_lowercase();
                words.iter().any(|w| l.contains(w)) || (reg && s.contains("REG"))
            })
            .cloned()
    };
    Some(VerdictItem {
        outcome: outcome(&verdict),
        cites: cites(&body, &prefix),
        rechecked: find(RECHECK, true),
        untested: find(UNTESTED, false),
        id,
        verdict,
        text: body,
        source_range: Some(range),
    })
}

/// What a verdict word means. Unknown words stay [`VerdictOutcome::Unknown`].
pub fn outcome(verdict: &str) -> VerdictOutcome {
    let l = verdict.to_lowercase();
    let has = |ws: &[&str]| ws.iter().any(|w| l.contains(w));
    if has(&["partial", "partly", "一部"]) {
        VerdictOutcome::Partial
    } else if has(&[
        "not ", "not_", "not-", "unsatisf", "fail", "reject", "rework", "unmet", "不合格", "未達", "不可", "差し戻",
        "満たさない", "満たしていない", "✗", "×", "❌",
    ]) || l == "ng"
    {
        VerdictOutcome::Fail
    } else if has(&["satisf", "pass", "accept", "done", "合格", "満た", "済", "✓", "✅"]) || ["ok", "met", "yes"].contains(&l.as_str())
    {
        VerdictOutcome::Pass
    } else {
        VerdictOutcome::Unknown
    }
}

/// Ids cited in `text`, in order and once each, as written. Ids with the
/// item's own prefix (other requirements) are left out.
fn cites(text: &str, own_prefix: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in cite_re().captures_iter(text) {
        let whole = m.get(0).unwrap();
        let before = text[..whole.start()].chars().next_back();
        let after = text[whole.end()..].chars().next();
        let glued = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
        if glued(before) || glued(after) || &m[1] == own_prefix || NOT_IDS.contains(&&m[1]) {
            continue;
        }
        let id: String = whole.as_str().split_whitespace().collect();
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// Split at `。` / `！` / `？`, and at `.` / `!` / `?` followed by a space.
fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut it = text.chars().peekable();
    while let Some(ch) = it.next() {
        cur.push(ch);
        let end = matches!(ch, '。' | '！' | '？')
            || (matches!(ch, '.' | '!' | '?') && it.peek().is_none_or(|n| n.is_whitespace()));
        if end {
            let s = cur.trim().to_string();
            if !s.is_empty() {
                out.push(s);
            }
            cur.clear();
        }
    }
    let s = cur.trim().to_string();
    if !s.is_empty() {
        out.push(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(line: u32) -> SourceRange {
        SourceRange {
            start_line: line,
            start_column: 1,
            end_line: line,
            end_column: 80,
        }
    }

    #[test]
    fn reads_id_verdict_text_and_cites() {
        let v = verdict_item(
            "R10: satisfied — 前回差し戻した点(文字列 'esc' 'enter' などがキー操作として処理される)は直っていた。REG1、C19、C18 がすべて pass。基本操作も C6〜C8 と利用者試行 S4/S10 で確認した。",
            r(31),
        )
        .unwrap();
        assert_eq!(v.id, "R10");
        assert_eq!(v.verdict, "satisfied");
        assert_eq!(v.outcome, VerdictOutcome::Pass);
        assert_eq!(v.cites, ["REG1", "C19", "C18", "C6〜C8", "S4", "S10"]);
        assert!(v.rechecked.as_deref().unwrap().starts_with("前回差し戻した点"));
        assert_eq!(v.untested, None);
    }

    #[test]
    fn untested_sentence_and_english_text() {
        let v = verdict_item(
            "R9: satisfied — C9: the URL arrived as one argument. Opening the default browser was not tested, since no real browser may be opened.",
            r(1),
        )
        .unwrap();
        assert_eq!(v.cites, ["C9"]);
        assert!(v.untested.as_deref().unwrap().starts_with("Opening the default browser"));
        assert_eq!(v.rechecked, None);
        // "previous accepted version" is not a re-check.
        let w = verdict_item("R2: satisfied — C14: unchanged from the previous accepted version.", r(2)).unwrap();
        assert_eq!(w.rechecked, None);
    }

    #[test]
    fn outcomes() {
        for (w, o) in [
            ("satisfied", VerdictOutcome::Pass),
            ("not satisfied", VerdictOutcome::Fail),
            ("unsatisfied", VerdictOutcome::Fail),
            ("partially satisfied", VerdictOutcome::Partial),
            ("FAIL", VerdictOutcome::Fail),
            ("pass", VerdictOutcome::Pass),
            ("合格", VerdictOutcome::Pass),
            ("不合格", VerdictOutcome::Fail),
            ("ok", VerdictOutcome::Pass),
            ("pending", VerdictOutcome::Unknown),
            ("未確認", VerdictOutcome::Unknown),
        ] {
            assert_eq!(outcome(w), o, "{w}");
        }
    }

    #[test]
    fn not_a_verdict() {
        // A sentence, not a verdict word, before the dash.
        assert!(verdict_item("Note: this is a long explanation of things — and more", r(1)).is_none());
        assert!(verdict_item("plain item", r(1)).is_none());
        assert!(verdict_item("R1 satisfied — no colon", r(1)).is_none());
        // Units are not cites; glued tokens are not either.
        let v = verdict_item("C2: pass — UTF8 and SHA256 ok, see REG2 and build X11 on amd64C4.", r(1)).unwrap();
        assert_eq!(v.cites, ["REG2"]);
    }
}
