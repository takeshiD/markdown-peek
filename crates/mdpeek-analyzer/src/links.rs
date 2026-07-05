//! Link extraction.
//!
//! `mdpeek-parser`'s `BlockTree` folds inline formatting into plain text and
//! does not surface hyperlinks, so Layer 2 re-parses the source once to collect
//! links with their source ranges (for the model's `links` list and, later,
//! link-aware UI). Uses the same GFM options as the block parser so autolinks
//! and reference links are seen identically.

use mdpeek_parser::{LineIndex, SourceRange};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::Serialize;

/// A hyperlink discovered in the source, with its span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Link {
    pub text: String,
    pub url: String,
    pub range: SourceRange,
}

/// Extract every link (`[text](url)`, autolinks, reference links) in document
/// order.
pub fn extract(markdown: &str) -> Vec<Link> {
    let line_index = LineIndex::new(markdown);
    let mut links = Vec::new();
    // Stack of in-progress links: (url, accumulated text, byte range).
    let mut open: Vec<(String, String, std::ops::Range<usize>)> = Vec::new();

    for (event, range) in Parser::new_ext(markdown, mdpeek_gfm::parser_options()).into_offset_iter()
    {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                open.push((dest_url.to_string(), String::new(), range));
            }
            Event::End(TagEnd::Link) => {
                if let Some((url, text, byte_range)) = open.pop() {
                    links.push(Link {
                        text: text.trim().to_string(),
                        url,
                        range: line_index.source_range(byte_range),
                    });
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some(top) = open.last_mut() {
                    top.1.push_str(&t);
                }
            }
            _ => {}
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_inline_link() {
        let links = extract("See [the docs](https://example.com/docs) here.\n");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://example.com/docs");
        assert_eq!(links[0].text, "the docs");
        assert_eq!(links[0].range.start_line, 1);
    }

    #[test]
    fn extracts_multiple_links_in_order() {
        let links = extract("[a](http://a.test) then [b](http://b.test)\n");
        let urls: Vec<&str> = links.iter().map(|l| l.url.as_str()).collect();
        assert_eq!(urls, vec!["http://a.test", "http://b.test"]);
    }

    #[test]
    fn no_links_yields_empty() {
        assert!(extract("plain text only\n").is_empty());
    }
}
