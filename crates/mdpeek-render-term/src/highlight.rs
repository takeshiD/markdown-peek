//! Syntax highlighting for code blocks using syntect via the `two-face` crate.
//!
//! Provides a single public function [`highlight`] that takes raw code and a language token,
//! applies ANSI 24-bit terminal colour escapes using the bundled syntect syntax/theme sets,
//! and returns the highlighted string.  When the language is unknown or empty the input is
//! returned verbatim so the caller never sees empty output.

use std::sync::OnceLock;
use two_face::re_exports::syntect::{
    easy::HighlightLines,
    highlighting::Theme,
    parsing::SyntaxSet,
    util::{LinesWithEndings, as_24_bit_terminal_escaped},
};
use two_face::theme::EmbeddedThemeName;

/// Cached (SyntaxSet, Theme) pair — built once on first call.
struct State {
    syntax_set: SyntaxSet,
    theme: Theme,
}

static STATE: OnceLock<State> = OnceLock::new();

fn state() -> &'static State {
    STATE.get_or_init(|| {
        let syntax_set = two_face::syntax::extra_newlines();
        let theme_set = two_face::theme::extra();
        // GruvboxDark: warm dark palette that reads well on most dark terminals.
        let theme = theme_set.get(EmbeddedThemeName::GruvboxDark).clone();
        State { syntax_set, theme }
    })
}

/// Honour the `NO_COLOR` convention (https://no-color.org): when the variable
/// is present and non-empty, suppress syntax highlighting.
fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
}

/// Return `code` with ANSI 24-bit syntax highlighting applied for `lang`.
///
/// * `lang` is matched case-insensitively as a file-extension / language token
///   (e.g. `"rs"`, `"python"`, `"js"`).
/// * When `lang` is empty or unrecognised, when `NO_COLOR` is set, or when
///   highlighting fails for any reason, `code` is returned unchanged.
/// * Trailing-newline presence is preserved: if `code` ends with `'\n'` the
///   output does too, and vice-versa.
/// * Each highlighted line is terminated with `\x1b[0m` so the terminal's
///   background colour is never "leaked" between lines.
pub fn highlight(code: &str, lang: &str) -> String {
    // Fast-path: nothing to highlight, or colour explicitly disabled.
    if lang.is_empty() || no_color() {
        return code.to_owned();
    }

    let st = state();

    // Resolve syntax by token (covers both names and extensions).
    let syntax = st.syntax_set.find_syntax_by_token(lang);

    // If no syntax found, or it is plain text, skip highlighting entirely.
    let syntax = match syntax {
        Some(s) if s.name != "Plain Text" => s,
        _ => return code.to_owned(),
    };

    let mut highlighter = HighlightLines::new(syntax, &st.theme);
    let mut out = String::with_capacity(code.len() * 2);

    for line in LinesWithEndings::from(code) {
        // highlight_line can fail only on internal parser bugs; fall back to raw.
        let ranges = match highlighter.highlight_line(line, &st.syntax_set) {
            Ok(r) => r,
            Err(_) => return code.to_owned(),
        };

        // bg=false → foreground colour only, no background escape.
        let escaped = as_24_bit_terminal_escaped(&ranges, false);

        // Strip the trailing newline from the escaped output if present,
        // append an explicit reset, then re-add the newline so the cursor
        // moves to the next line *after* the colours are cleared.
        if escaped.ends_with('\n') {
            out.push_str(escaped.trim_end_matches('\n'));
            out.push_str("\x1b[0m\n");
        } else {
            out.push_str(&escaped);
            out.push_str("\x1b[0m");
        }
    }

    // Preserve the absence of a trailing newline.
    if !code.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_lang_returns_verbatim() {
        let code = "let x = 1;\n";
        assert_eq!(highlight(code, ""), code);
    }

    #[test]
    fn unknown_lang_returns_verbatim() {
        let code = "hello world\n";
        assert_eq!(highlight(code, "nonexistentlang9999"), code);
    }

    #[test]
    fn rust_code_contains_ansi_escapes() {
        let code = "fn main() {\n    println!(\"hello\");\n}\n";
        let result = highlight(code, "rs");
        assert!(result.contains("\x1b["), "Expected ANSI escapes in output");
    }

    #[test]
    fn rust_code_ends_with_reset() {
        let code = "fn main() {}\n";
        let result = highlight(code, "rust");
        let trimmed = result.trim_end_matches('\n');
        assert!(
            trimmed.ends_with("\x1b[0m"),
            "Output should end with reset: {:?}",
            &trimmed[trimmed.len().saturating_sub(20)..]
        );
    }

    #[test]
    fn no_background_escapes() {
        let code = "let x: u32 = 42;\n";
        let result = highlight(code, "rs");
        assert!(
            !result.contains("\x1b[48;"),
            "Output must not contain background colour escapes"
        );
    }

    #[test]
    fn trailing_newline_preserved() {
        let with_nl = "let x = 1;\n";
        let without_nl = "let x = 1;";
        let r_with = highlight(with_nl, "rs");
        let r_without = highlight(without_nl, "rs");
        assert!(r_with.ends_with('\n'), "trailing newline should be kept");
        assert!(
            !r_without.ends_with('\n'),
            "no trailing newline should not be added"
        );
    }

    #[test]
    fn python_highlight_works() {
        let code = "def hello():\n    return 42\n";
        let result = highlight(code, "py");
        assert!(result.contains("\x1b["), "Python code should be highlighted");
    }

    #[test]
    fn javascript_highlight_works() {
        let code = "const x = () => 42;\n";
        let result = highlight(code, "js");
        assert!(result.contains("\x1b["), "JS code should be highlighted");
    }
}
