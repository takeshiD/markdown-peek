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
        "You convert a Markdown document into a Generative UI intermediate \
representation (UI IR). Output ONLY a JSON array of UI nodes and nothing else — \
no explanation, no markdown fences, no prose.\n\n\
Hard rules:\n\
1. Each node MUST have a `kind` field, and `kind` MUST be one of: {kinds}.\n\
2. Never invent components outside that list.\n\
3. Every node MUST include a `sourceRange` object {{startLine, startColumn, \
endLine, endColumn}} (1-based) pointing at the exact lines it summarises. Do \
NOT fabricate ranges — they are verified against the source and rejected if out \
of bounds.\n\
4. Only add nodes that require interpretation (risk extraction, decision \
graphs, section classification). Do not restate tables/checklists that simple \
rules already handle.\n\
5. Set `origin` to \"llm\" and `confidence` (0.0-1.0) on every node you emit."
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
