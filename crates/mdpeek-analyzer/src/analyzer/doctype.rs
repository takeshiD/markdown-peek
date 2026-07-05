//! Document-type inference by rules (AGENTS.md §3.2).
//!
//! Priority: explicit `type:` frontmatter → filename convention → heading-set
//! heuristics → content sniffing → `Generic`. Each rule reports a confidence so
//! Layer 3 can decide whether to escalate a low-confidence guess to the LLM.

use crate::model::{Classified, DocumentType, OutlineEntry};
use mdpeek_parser::BlockTree;

/// Infer the document type from filename, frontmatter and structure.
pub fn classify(
    filename: Option<&str>,
    tree: &BlockTree,
    outline: &[OutlineEntry],
) -> Classified<DocumentType> {
    // 1. Explicit frontmatter `type:` wins outright.
    if let Some(t) = tree.frontmatter().and_then(frontmatter_type) {
        return Classified::rules(t, 0.95);
    }

    // 2. Filename conventions.
    if let Some(name) = filename.map(base_name)
        && let Some(t) = filename_type(&name)
    {
        return Classified::rules(t, 0.9);
    }

    // 3. Heading-set heuristics.
    let titles: Vec<String> = outline.iter().map(|e| e.title.to_lowercase()).collect();
    let has = |kw: &str| titles.iter().any(|t| t.contains(kw));
    let has_any = |kws: &[&str]| kws.iter().any(|kw| has(kw));

    // ADR: status + context + (decision | consequences).
    if has("status") && has("context") && (has("decision") || has("consequence")) {
        return Classified::rules(DocumentType::Adr, 0.85);
    }
    // Recipe: ingredients + a preparation section.
    if has_any(&["ingredient", "材料"]) && has_any(&["instruction", "作り方", "手順", "steps"]) {
        return Classified::rules(DocumentType::Recipe, 0.8);
    }
    // Meeting minutes.
    if has_any(&["agenda", "attendee", "action item", "議題", "出席", "決定事項"]) {
        return Classified::rules(DocumentType::Minutes, 0.8);
    }
    // Changelog.
    if has_any(&["changelog", "unreleased", "変更履歴"]) {
        return Classified::rules(DocumentType::Changelog, 0.8);
    }
    // Runbook / procedure.
    if has_any(&["prerequisite", "前提", "手順", "procedure", "runbook", "rollback"]) {
        return Classified::rules(DocumentType::Runbook, 0.7);
    }
    // Design doc.
    if has_any(&["architecture", "アーキテクチャ", "設計", "data model", "データモデル"])
        && has_any(&["overview", "概要", "design", "risk", "リスク"])
    {
        return Classified::rules(DocumentType::DesignDoc, 0.7);
    }
    // FAQ: a majority of headings are questions.
    if !titles.is_empty() {
        let questions = titles
            .iter()
            .filter(|t| t.trim_end().ends_with('?') || t.contains('？'))
            .count();
        if questions * 2 >= titles.len() && questions >= 2 {
            return Classified::rules(DocumentType::Faq, 0.7);
        }
    }

    // 4. Content sniffing: git log.
    if looks_like_git_log(tree) {
        return Classified::rules(DocumentType::GitLog, 0.65);
    }

    // 5. Fallback.
    Classified::rules(DocumentType::Generic, 0.3)
}

/// Strip directory components from a path, keeping the final segment.
fn base_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

fn filename_type(name: &str) -> Option<DocumentType> {
    let lower = name.to_lowercase();
    let stem = lower.strip_suffix(".md").unwrap_or(&lower);
    if stem.starts_with("readme") {
        return Some(DocumentType::Readme);
    }
    if stem.starts_with("changelog") {
        return Some(DocumentType::Changelog);
    }
    if stem.starts_with("contributing") {
        return Some(DocumentType::Readme);
    }
    // ADR files: `adr-0001-*`, `0001-title`, or anything containing `adr`.
    if stem.starts_with("adr") || stem.contains("adr-") || stem.contains("-adr") {
        return Some(DocumentType::Adr);
    }
    None
}

/// Parse a `type:` (or `doc_type:`) value out of raw YAML/TOML frontmatter.
fn frontmatter_type(frontmatter: &str) -> Option<DocumentType> {
    for line in frontmatter.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once([':', '=']) else {
            continue;
        };
        let key = key.trim().trim_matches('"').to_lowercase();
        if key != "type" && key != "doc_type" && key != "kind" {
            continue;
        }
        let value = value.trim().trim_matches(['"', '\'']).to_lowercase();
        return map_type_name(&value);
    }
    None
}

/// Map a free-form type name (frontmatter value) to a [`DocumentType`].
fn map_type_name(value: &str) -> Option<DocumentType> {
    Some(match value {
        "designdoc" | "design_doc" | "design" | "設計書" => DocumentType::DesignDoc,
        "readme" => DocumentType::Readme,
        "adr" | "decision" | "decision_record" => DocumentType::Adr,
        "minutes" | "meeting" | "議事録" => DocumentType::Minutes,
        "runbook" => DocumentType::Runbook,
        "investigation" | "postmortem" | "調査" => DocumentType::Investigation,
        "changelog" => DocumentType::Changelog,
        "gitlog" | "git_log" => DocumentType::GitLog,
        "novel" | "story" | "小説" => DocumentType::Novel,
        "production_order" | "production" | "生産指示書" => DocumentType::ProductionOrder,
        "procedure" | "sop" | "手順書" => DocumentType::Procedure,
        "recipe" | "レシピ" => DocumentType::Recipe,
        "contract" | "契約書" => DocumentType::Contract,
        "paper" | "論文" => DocumentType::Paper,
        "faq" => DocumentType::Faq,
        "generic" => DocumentType::Generic,
        _ => return None,
    })
}

/// Heuristic: many top-level lines start with a 7–40 hex commit-ish token.
fn looks_like_git_log(tree: &BlockTree) -> bool {
    let mut hits = 0;
    let mut total = 0;
    for block in tree.iter() {
        for line in block.text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            total += 1;
            let token = line.split_whitespace().next().unwrap_or("");
            if (7..=40).contains(&token.len()) && token.chars().all(|c| c.is_ascii_hexdigit()) {
                hits += 1;
            }
        }
    }
    total > 0 && hits >= 2 && hits * 2 >= total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::outline;
    use mdpeek_parser::BlockTree;

    fn classify_md(md: &str, filename: Option<&str>) -> DocumentType {
        let tree = BlockTree::parse(md);
        let ol = outline(&tree);
        classify(filename, &tree, &ol).value
    }

    #[test]
    fn frontmatter_type_takes_priority() {
        let md = "---\ntype: adr\n---\n\n# Anything\n";
        // Filename says readme, but frontmatter wins.
        assert_eq!(classify_md(md, Some("README.md")), DocumentType::Adr);
    }

    #[test]
    fn readme_by_filename() {
        assert_eq!(classify_md("# Hi\n", Some("docs/README.md")), DocumentType::Readme);
    }

    #[test]
    fn changelog_by_filename() {
        assert_eq!(classify_md("# 1.0\n", Some("CHANGELOG.md")), DocumentType::Changelog);
    }

    #[test]
    fn adr_by_heading_set() {
        let md = "# Use Postgres\n\n## Status\nAccepted\n\n## Context\n...\n\n## Decision\n...\n\n## Consequences\n...\n";
        assert_eq!(classify_md(md, None), DocumentType::Adr);
    }

    #[test]
    fn recipe_by_headings() {
        let md = "# Pancakes\n\n## Ingredients\n- flour\n\n## Instructions\n1. mix\n";
        assert_eq!(classify_md(md, None), DocumentType::Recipe);
    }

    #[test]
    fn minutes_by_headings() {
        let md = "# Weekly Sync\n\n## Attendees\n- A\n\n## Agenda\n- x\n\n## Action Items\n- y\n";
        assert_eq!(classify_md(md, None), DocumentType::Minutes);
    }

    #[test]
    fn design_doc_by_headings() {
        let md = "# System\n\n## Overview\n...\n\n## Architecture\n...\n\n## Risks\n...\n";
        assert_eq!(classify_md(md, None), DocumentType::DesignDoc);
    }

    #[test]
    fn faq_by_question_headings() {
        let md = "# Help\n\n## How do I install?\n...\n\n## Where is config?\n...\n";
        assert_eq!(classify_md(md, None), DocumentType::Faq);
    }

    #[test]
    fn git_log_by_content() {
        let md = "```\ndeadbeef1 fix bug\ncafebabe2 add feature\n0badf00d3 tidy up\n```\n";
        assert_eq!(classify_md(md, None), DocumentType::GitLog);
    }

    #[test]
    fn unknown_falls_back_to_generic() {
        assert_eq!(classify_md("just some prose\n", None), DocumentType::Generic);
    }
}
