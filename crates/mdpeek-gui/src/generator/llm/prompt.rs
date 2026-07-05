//! Prompt construction for the Claude generator (design doc §7).
//!
//! The contract is strict: the model may return **only UI IR JSON** — an array
//! of nodes whose `kind` is in the registry allowlist — and every node must
//! carry a `sourceRange` into the original document. No prose, no HTML, no code.
//! Validation (`ir::validate_json`) enforces this after the fact; the prompt
//! just makes compliance likely.

use crate::ir::registry;

/// System prompt: role, hard constraints, and the allowed component list.
pub fn system_prompt() -> String {
    let kinds = registry::all_kinds().collect::<Vec<_>>().join(", ");
    format!(
        "You are a reading assistant. Convert a Markdown document into \
**reading lenses** — UI that helps a human understand the document faster. \
Output ONLY a JSON array of UI nodes, nothing else (no prose, no markdown \
fences).\n\n\
Produce reading lenses, NOT a reprint of the body. Prefer these kinds:\n\
- SemanticOutline: sections grouped by meaning (Overview / Design / Decisions / \
Risks / Open Questions / Next Actions), each item with a short reason.\n\
- SummaryCards: per-section title + 1-2 sentence summary + keyPoints.\n\
- DecisionLog: decisions with decision/alternatives/reason/impact/status.\n\
- ActionItems: tasks with assignee/dueDate/status when stated.\n\
- OpenQuestions: unresolved items with severity.\n\
- RiskPanel: risks (severity/likelihood/mitigation) and assumptions.\n\
- Glossary: terms/acronyms with definition or inferredDefinition.\n\n\
Hard rules:\n\
1. `kind` MUST be one of: {kinds}. Never invent components.\n\
2. Do NOT emit body reprints (DataTable, ConfigViewer, Diagram, Callout) — those \
are already shown in the document body. Only emit derived reading lenses.\n\
3. Every node and list item MUST include a `sourceRange` {{startLine, \
startColumn, endLine, endColumn}} (1-based) pointing at the exact source lines. \
Do NOT fabricate ranges — they are verified and rejected if out of bounds.\n\
4. Ground everything in the text. If a claim is inferred, set `confidence` to \
\"low\" or \"medium\"; only stated-verbatim items are \"high\".\n\
5. Set `origin` to \"llm\" on every node."
    )
}

/// User prompt: the document (with line numbers) plus the node kinds the planner
/// asked us to fill.
pub fn user_prompt(markdown: &str, requested_kinds: &[&str]) -> String {
    let numbered = markdown
        .lines()
        .enumerate()
        .map(|(i, l)| format!("{:>4}  {l}", i + 1))
        .collect::<Vec<_>>()
        .join("\n");
    let asks = if requested_kinds.is_empty() {
        "any interpretive nodes that help the reader".to_string()
    } else {
        requested_kinds.join(", ")
    };
    format!(
        "Produce UI IR nodes of these kinds where the document supports them: \
{asks}.\n\nDocument (line-numbered):\n---\n{numbered}\n---"
    )
}

/// Strip an accidental ```json … ``` fence the model may wrap the array in.
pub fn strip_code_fence(text: &str) -> &str {
    let t = text.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    t.trim().strip_suffix("```").unwrap_or(t).trim()
}

/// Extract the JSON array from noisy CLI output (Claude Code / Codex may print
/// preamble, logs or a trailing summary around the payload). Strips a code fence
/// first, then narrows to the outermost `[` … `]`. Falls back to the fence-
/// stripped text so the validator produces a clear error if nothing matches.
pub fn extract_json_array(text: &str) -> &str {
    let t = strip_code_fence(text.trim());
    match (t.find('['), t.rfind(']')) {
        (Some(start), Some(end)) if end > start => &t[start..=end],
        _ => t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_fence() {
        assert_eq!(strip_code_fence("```json\n[]\n```"), "[]");
        assert_eq!(strip_code_fence("[]"), "[]");
    }

    #[test]
    fn extracts_array_from_noise() {
        let out = "Thinking...\nHere is the IR:\n```json\n[{\"kind\":\"Callout\"}]\n```\nDone.";
        assert_eq!(extract_json_array(out), "[{\"kind\":\"Callout\"}]");
        assert_eq!(extract_json_array("prefix [1,2] suffix"), "[1,2]");
    }

    #[test]
    fn system_prompt_lists_allowed_kinds() {
        let s = system_prompt();
        assert!(s.contains("Checklist"));
        assert!(s.contains("ObligationMatrix"));
    }
}
