//! Code-block intent detection (AGENTS.md §3.2 "コードブロック意図").
//!
//! Rules-only: use the fence language when present, otherwise sniff the content.
//! This is an analyser building block consumed by later layers (e.g. an
//! `ApiExplorer` for HTTP, a `ConfigViewer` for JSON/YAML/TOML); Layer 2 exposes
//! it with tests rather than embedding the result in `DocumentModel`.

use mdpeek_parser::{BlockId, BlockKind, BlockTree};

/// What a fenced code block appears to contain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeIntent {
    Shell,
    Json,
    Yaml,
    Toml,
    Sql,
    /// An HTTP request/response snippet (has a method line or status line).
    Http,
    /// A Mermaid / graphviz-style diagram.
    Diagram,
    /// A unified diff / patch.
    Diff,
    /// A recognised programming language (normalised name).
    Language(String),
    /// Unclassified.
    Unknown,
}

/// Classify a single code block from its fence language and body.
pub fn intent(lang: Option<&str>, content: &str) -> CodeIntent {
    if let Some(lang) = lang
        && let Some(known) = from_lang(&lang.to_lowercase())
    {
        return known;
    }
    sniff(content)
}

/// Map a fence language token to an intent.
fn from_lang(lang: &str) -> Option<CodeIntent> {
    Some(match lang {
        "sh" | "bash" | "zsh" | "shell" | "console" | "shell-session" => CodeIntent::Shell,
        "json" | "jsonc" | "json5" => CodeIntent::Json,
        "yaml" | "yml" => CodeIntent::Yaml,
        "toml" => CodeIntent::Toml,
        "sql" | "postgresql" | "mysql" => CodeIntent::Sql,
        "http" => CodeIntent::Http,
        "mermaid" | "dot" | "graphviz" => CodeIntent::Diagram,
        "diff" | "patch" => CodeIntent::Diff,
        // A real language name we recognise but don't specialise.
        "rust" | "python" | "py" | "js" | "javascript" | "ts" | "typescript" | "go" | "c"
        | "cpp" | "java" | "ruby" | "php" | "kotlin" | "swift" => {
            CodeIntent::Language(normalise_lang(lang))
        }
        _ => return None,
    })
}

fn normalise_lang(lang: &str) -> String {
    match lang {
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        other => other,
    }
    .to_string()
}

/// Guess intent from body content when no fence language is given.
fn sniff(content: &str) -> CodeIntent {
    let trimmed = content.trim_start();
    let first_line = trimmed.lines().next().unwrap_or("").trim();

    // Diagram markers.
    if trimmed.starts_with("sequenceDiagram")
        || trimmed.starts_with("graph ")
        || trimmed.starts_with("flowchart ")
        || trimmed.starts_with("classDiagram")
        || trimmed.starts_with("digraph ")
    {
        return CodeIntent::Diagram;
    }
    // Unified diff.
    if trimmed.starts_with("diff ")
        || trimmed.starts_with("--- ")
        || trimmed.starts_with("@@ ")
        || trimmed.lines().any(|l| l.starts_with("@@"))
    {
        return CodeIntent::Diff;
    }
    // HTTP request line: METHOD SP path.
    if let Some(method) = first_line.split_whitespace().next()
        && matches!(
            method,
            "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS"
        )
    {
        return CodeIntent::Http;
    }
    // Shell prompts.
    if trimmed.starts_with("$ ") || trimmed.starts_with("#!") || first_line.starts_with("$ ") {
        return CodeIntent::Shell;
    }
    // JSON object/array.
    if (trimmed.starts_with('{') && trimmed.trim_end().ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.trim_end().ends_with(']'))
    {
        return CodeIntent::Json;
    }
    // SQL statement.
    let upper = first_line.to_uppercase();
    if ["SELECT ", "INSERT ", "UPDATE ", "DELETE ", "CREATE ", "ALTER "]
        .iter()
        .any(|kw| upper.starts_with(kw))
    {
        return CodeIntent::Sql;
    }
    // TOML table header.
    if first_line.starts_with('[') && first_line.ends_with(']') {
        return CodeIntent::Toml;
    }
    CodeIntent::Unknown
}

/// Classify every code block in the tree, pairing each with its intent.
pub fn classify(tree: &BlockTree) -> Vec<(BlockId, CodeIntent)> {
    tree.iter()
        .filter_map(|b| match &b.kind {
            BlockKind::CodeBlock { language } => Some((b.id, intent(language.as_deref(), &b.text))),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_language_wins() {
        assert_eq!(intent(Some("bash"), "echo hi"), CodeIntent::Shell);
        assert_eq!(intent(Some("JSON"), "{}"), CodeIntent::Json);
        assert_eq!(intent(Some("mermaid"), "graph TD"), CodeIntent::Diagram);
        assert_eq!(intent(Some("rust"), "fn a(){}"), CodeIntent::Language("rust".into()));
        assert_eq!(intent(Some("py"), "x=1"), CodeIntent::Language("python".into()));
    }

    #[test]
    fn sniff_http() {
        assert_eq!(intent(None, "GET /users/1 HTTP/1.1\nHost: x"), CodeIntent::Http);
    }

    #[test]
    fn sniff_json_and_toml() {
        assert_eq!(intent(None, "{\n  \"a\": 1\n}"), CodeIntent::Json);
        assert_eq!(intent(None, "[package]\nname = \"x\""), CodeIntent::Toml);
    }

    #[test]
    fn sniff_shell_and_sql() {
        assert_eq!(intent(None, "$ ls -la"), CodeIntent::Shell);
        assert_eq!(intent(None, "SELECT * FROM t;"), CodeIntent::Sql);
    }

    #[test]
    fn sniff_diff_and_diagram() {
        assert_eq!(intent(None, "@@ -1,2 +1,3 @@\n-old\n+new"), CodeIntent::Diff);
        assert_eq!(intent(None, "sequenceDiagram\nA->>B: hi"), CodeIntent::Diagram);
    }

    #[test]
    fn unknown_when_no_signal() {
        assert_eq!(intent(None, "lorem ipsum"), CodeIntent::Unknown);
    }

    #[test]
    fn classify_reads_fence_language_from_tree() {
        let tree = BlockTree::parse("```bash\nmake\n```\n");
        let intents = classify(&tree);
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].1, CodeIntent::Shell);
    }
}
