//! GitHub-style `:shortcode:` emoji substitution for terminal output.

use std::borrow::Cow;

/// テキスト中の :shortcode: を Unicode 絵文字に置換して返す。
/// 対応する絵文字が無い :foo: はそのまま残す。
///
/// 置換が一切発生しない場合は [`Cow::Borrowed`] を返しアロケーションを避ける。
pub fn replace_shortcodes(text: &str) -> Cow<'_, str> {
    // Fast path: no colon at all → borrow with no allocation.
    if !text.contains(':') {
        return Cow::Borrowed(text);
    }

    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut out = String::new();
    let mut replaced = false;
    // index up to which `text` has already been copied into `out`.
    let mut copied = 0usize;
    let mut i = 0usize;

    while i < len {
        if bytes[i] != b':' {
            i += 1;
            continue;
        }

        // bytes[i] == b':'  →  scan ahead for a closing ':'
        let start = i; // position of the opening ':'
        let mut j = i + 1;

        // Collect valid shortcode characters: [a-zA-Z0-9_+\-]
        while j < len && is_shortcode_char(bytes[j]) {
            j += 1;
        }

        // A valid token requires at least one shortcode char and a closing ':'.
        if j > i + 1 && j < len && bytes[j] == b':' {
            let candidate = &text[i + 1..j];
            if let Some(emoji) = emojis::get_by_shortcode(candidate) {
                // Copy the untouched gap (UTF-8 safe: slices on `:` boundaries).
                out.push_str(&text[copied..start]);
                out.push_str(emoji.as_str());
                replaced = true;
                i = j + 1; // skip past the closing ':'
                copied = i;
                continue;
            }
        }

        // No match: leave the ':' in place and advance by one byte.
        i += 1;
    }

    if replaced {
        out.push_str(&text[copied..]);
        Cow::Owned(out)
    } else {
        Cow::Borrowed(text)
    }
}

/// Returns `true` for characters that may appear inside a GitHub shortcode.
#[inline]
fn is_shortcode_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'+' || b == b'-'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smile() {
        let result = replace_shortcodes(":smile:");
        let expected = emojis::get_by_shortcode("smile").unwrap().as_str();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_unknown_shortcode_preserved() {
        let input = ":unknown_xyz:";
        let result = replace_shortcodes(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_http_url_untouched() {
        let input = "see http://example.com for details";
        let result = replace_shortcodes(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_https_url_untouched() {
        let input = "https://example.com/path?a=1";
        let result = replace_shortcodes(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_no_replacement_returns_borrowed() {
        let input = "hello world";
        match replace_shortcodes(input) {
            Cow::Borrowed(_) => {}
            Cow::Owned(_) => panic!("Expected Cow::Borrowed for no-replacement case"),
        }
    }

    #[test]
    fn test_no_colon_returns_borrowed() {
        let input = "plain text without colon";
        match replace_shortcodes(input) {
            Cow::Borrowed(_) => {}
            Cow::Owned(_) => panic!("Expected Cow::Borrowed when no colon present"),
        }
    }

    #[test]
    fn test_unknown_code_returns_borrowed() {
        let input = ":unknown_xyz:";
        match replace_shortcodes(input) {
            Cow::Borrowed(_) => {}
            Cow::Owned(_) => panic!("Expected Cow::Borrowed for all-unknown shortcodes"),
        }
    }

    #[test]
    fn test_adjacent_shortcodes() {
        let smile = emojis::get_by_shortcode("smile").unwrap().as_str();
        let plus1 = emojis::get_by_shortcode("+1").unwrap().as_str();
        let result = replace_shortcodes(":smile::+1:");
        assert_eq!(result, format!("{smile}{plus1}"));
    }

    #[test]
    fn test_mixed_known_and_unknown() {
        let smile = emojis::get_by_shortcode("smile").unwrap().as_str();
        let input = ":smile: and :nope_xyz:";
        let result = replace_shortcodes(input);
        assert_eq!(result, format!("{smile} and :nope_xyz:"));
    }

    #[test]
    fn test_trailing_lone_colon() {
        let input = "hello:";
        let result = replace_shortcodes(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_empty_colons() {
        let input = "a::b";
        let result = replace_shortcodes(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_shortcode_with_spaces_invalid() {
        let input = ":hello world:";
        let result = replace_shortcodes(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_multibyte_preserved_after_replacement() {
        // 置換後に続くマルチバイト文字(日本語)が壊れないこと。
        let smile = emojis::get_by_shortcode("smile").unwrap().as_str();
        let input = ":smile: 日本語のテキスト";
        let result = replace_shortcodes(input);
        assert_eq!(result, format!("{smile} 日本語のテキスト"));
    }

    #[test]
    fn test_multibyte_between_shortcodes() {
        let smile = emojis::get_by_shortcode("smile").unwrap().as_str();
        let input = "あ:smile:い";
        let result = replace_shortcodes(input);
        assert_eq!(result, format!("あ{smile}い"));
    }
}
