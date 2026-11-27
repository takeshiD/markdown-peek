//! Local image URL rewriting (#55, #56).
//!
//! The browser preview is served from `/`, so a document-relative image such
//! as `![](img/a.png)` or `<img src="../b.png">` would otherwise resolve
//! against the server root (and `..` would be normalised away). The emitter
//! rewrites such URLs to `{prefix}{percent-encoded path}`, where the prefix is
//! a server route that resolves the path against the open document's
//! directory. The path travels in the query string, which browsers do not
//! normalise, so `..` segments reach the server intact and are checked there.
//!
//! External URLs (`https:`, `data:`, …), protocol-relative (`//host`),
//! root-absolute (`/x`) and fragment/query-only references are left untouched.

/// True when `url` is a document-relative reference (no scheme, no leading
/// `/`, `\`, `#` or `?`).
pub fn is_local_relative(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty() || url.starts_with(['/', '\\', '#', '?']) {
        return false;
    }
    // RFC 3986 scheme: ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ) ":"
    if let Some(colon) = url.find(':') {
        let scheme = &url[..colon];
        let mut chars = scheme.chars();
        if chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        {
            return false;
        }
    }
    true
}

/// The served URL for a document-relative `url`, or `None` to keep it as is.
/// The query and fragment are dropped and the path is percent-decoded once
/// (URL semantics: `my%20img.png` names the file `my img.png`) before being
/// re-encoded as a single query component.
pub fn local_asset_url(prefix: &str, url: &str) -> Option<String> {
    if !is_local_relative(url) {
        return None;
    }
    let path = url.trim();
    let path = &path[..path.find(['?', '#']).unwrap_or(path.len())];
    if path.is_empty() {
        return None;
    }
    let decoded = percent_decode(path);
    let mut out = String::with_capacity(prefix.len() + decoded.len() * 3);
    out.push_str(prefix);
    percent_encode_component(&mut out, &decoded);
    Some(out)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(h << 4 | l);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Encode everything but RFC 3986 unreserved characters and `/`, so the
/// result is safe both as a query value and inside an HTML attribute.
fn percent_encode_component(out: &mut String, s: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &c in s.as_bytes() {
        if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(c as char);
        } else {
            out.push('%');
            out.push(HEX[(c >> 4) as usize] as char);
            out.push(HEX[(c & 0xF) as usize] as char);
        }
    }
}

/// Rewrite the `src` of every `<img …>` tag in a raw HTML fragment whose value
/// is document-relative. Everything else is copied verbatim.
pub fn rewrite_img_srcs(html: &str, prefix: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;
    while let Some(off) = lower[pos..].find("<img") {
        let start = pos + off;
        let after = start + 4;
        // `<img` must be followed by whitespace, `/` or `>` (not `<imgx`).
        if !lower[after..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_whitespace() || c == '/' || c == '>')
        {
            out.push_str(&html[pos..after]);
            pos = after;
            continue;
        }
        let end = tag_end(html, after);
        out.push_str(&html[pos..after]);
        out.push_str(&rewrite_tag_attrs(&html[after..end], prefix));
        pos = end;
    }
    out.push_str(&html[pos..]);
    out
}

/// Index just past the `>` closing the tag that starts before `from`,
/// honouring quoted attribute values; the end of input if unterminated.
fn tag_end(html: &str, from: usize) -> usize {
    let mut quote: Option<u8> = None;
    for (i, &c) in html.as_bytes()[from..].iter().enumerate() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == b'"' || c == b'\'' => quote = Some(c),
            None if c == b'>' => return from + i + 1,
            None => {}
        }
    }
    html.len()
}

/// Rewrite the `src` attribute inside one tag's attribute text.
fn rewrite_tag_attrs(attrs: &str, prefix: &str) -> String {
    let b = attrs.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // Skip to the next attribute name.
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        let name_start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'=' | b'>' | b'/') {
            i += 1;
        }
        if i == name_start {
            // `>` or stray character: nothing more to rewrite.
            break;
        }
        let name = &attrs[name_start..i];
        let mut j = i;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= b.len() || b[j] != b'=' {
            continue; // boolean attribute
        }
        j += 1;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        let (val_start, val_end, next) = match b.get(j) {
            Some(&q) if q == b'"' || q == b'\'' => {
                let close = attrs[j + 1..]
                    .find(q as char)
                    .map_or(b.len(), |k| j + 1 + k);
                (j + 1, close, (close + 1).min(b.len()))
            }
            _ => {
                let mut k = j;
                while k < b.len() && !b[k].is_ascii_whitespace() && b[k] != b'>' {
                    k += 1;
                }
                (j, k, k)
            }
        };
        if name.eq_ignore_ascii_case("src") {
            let value = attrs[val_start..val_end].replace("&amp;", "&");
            if let Some(url) = local_asset_url(prefix, &value) {
                let quoted = val_start != j;
                let mut out = String::with_capacity(attrs.len() + url.len());
                out.push_str(&attrs[..j]);
                out.push('"');
                out.push_str(&url);
                out.push('"');
                out.push_str(&attrs[if quoted { next } else { val_end }..]);
                return out;
            }
            return attrs.to_string();
        }
        i = next;
    }
    attrs.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &str = "/asset?path=";

    #[test]
    fn classifies_local_relative_urls() {
        for local in ["img/a.png", "./a.png", "../b.png", "a b.png", "日本.png"] {
            assert!(is_local_relative(local), "{local}");
        }
        for other in [
            "",
            "https://example.com/x.png",
            "HTTP://x/y.png",
            "data:image/png;base64,AAAA",
            "//cdn.example.com/x.png",
            "/abs/x.png",
            "#frag",
            "?q",
            "mailto:a@b",
            "C:\\x.png",
        ] {
            assert!(!is_local_relative(other), "{other}");
        }
    }

    #[test]
    fn local_asset_url_encodes_path_into_query() {
        assert_eq!(
            local_asset_url(P, "img/a.png").as_deref(),
            Some("/asset?path=img/a.png")
        );
        // `..` survives (query strings are not normalised by browsers).
        assert_eq!(
            local_asset_url(P, "../b.png").as_deref(),
            Some("/asset?path=../b.png")
        );
        // Percent-escapes are decoded once, then re-encoded; query/fragment dropped.
        assert_eq!(
            local_asset_url(P, "my%20img.png?v=2#x").as_deref(),
            Some("/asset?path=my%20img.png")
        );
        assert_eq!(
            local_asset_url(P, "a&b \"c\".png").as_deref(),
            Some("/asset?path=a%26b%20%22c%22.png")
        );
        assert_eq!(local_asset_url(P, "https://x/y.png"), None);
        assert_eq!(local_asset_url(P, "#only"), None);
    }

    #[test]
    fn rewrites_img_src_in_raw_html() {
        assert_eq!(
            rewrite_img_srcs(r#"<img src="path/to/img" />"#, P),
            r#"<img src="/asset?path=path/to/img" />"#
        );
        assert_eq!(
            rewrite_img_srcs(r#"<p><IMG alt='x > y' SRC='a.png' width=10></p>"#, P),
            r#"<p><IMG alt='x > y' SRC="/asset?path=a.png" width=10></p>"#
        );
        assert_eq!(
            rewrite_img_srcs("<img src=a.png>", P),
            r#"<img src="/asset?path=a.png">"#
        );
        // Multi-line tag inside an HTML block.
        assert_eq!(
            rewrite_img_srcs("<img\n  data-src=\"k\"\n  src=\"x.svg\"\n/>", P),
            "<img\n  data-src=\"k\"\n  src=\"/asset?path=x.svg\"\n/>"
        );
    }

    #[test]
    fn leaves_external_and_non_img_untouched() {
        for html in [
            r#"<img src="https://example.com/x.png">"#,
            r#"<img src="data:image/png;base64,AA">"#,
            r#"<img alt="no src">"#,
            r#"<imgx src="a.png">"#,
            r#"<script src="a.js"></script>"#,
            r#"<a href="a.png">a</a>"#,
            "<img",
        ] {
            assert_eq!(rewrite_img_srcs(html, P), html);
        }
    }
}
