//! GFM stream-transformation adapters layered on top of the `pulldown_cmark`
//! event stream.
//!
//! Two text-level GFM extensions that `pulldown_cmark` does not perform on its
//! own are implemented here:
//!
//! * Emoji shortcodes (`:smile:` -> 😄) resolved via the `emojis` crate.
//! * GFM extended autolinks (bare `https://`, `http://` and `www.` URLs in
//!   plain text) split into `Link` start / text / end events.
//!
//! Both transformations only act on [`Event::Text`]. Inline code
//! ([`Event::Code`]) and raw HTML ([`Event::Html`] / [`Event::InlineHtml`])
//! arrive as distinct events and are therefore left untouched automatically.

use pulldown_cmark::{CowStr, Event, LinkType, Tag, TagEnd};
use regex::Regex;
use std::collections::VecDeque;
use std::sync::LazyLock;

/// Matches an emoji shortcode like `:smile:` or `:+1:`.
///
/// The shortcode body allows letters, digits, `_`, `+` and `-`, mirroring the
/// character set used by GitHub / the `emojis` crate.
static EMOJI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":([a-zA-Z0-9_+-]+):").expect("valid emoji regex"));

/// Matches a bare URL candidate (`https://`, `http://` or `www.`).
///
/// The match is intentionally greedy on the trailing characters; precise
/// trimming of trailing punctuation and unbalanced parentheses is handled
/// afterwards in [`trim_autolink`].
static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(https?://|www\.)[^\s<]+").expect("valid url regex"));

/// Wraps an event iterator and applies the GFM text transformations
/// (emoji shortcodes first, then extended autolinks).
pub fn transform<'a, I>(iter: I) -> GfmTransform<'a, I>
where
    I: Iterator<Item = Event<'a>>,
{
    GfmTransform {
        iter,
        queue: VecDeque::new(),
        link_depth: 0,
    }
}

/// Iterator adapter produced by [`transform`].
pub struct GfmTransform<'a, I>
where
    I: Iterator<Item = Event<'a>>,
{
    iter: I,
    /// Buffered events produced from a single source `Text` event. A bare URL
    /// expands into multiple events (link start / text / link end), so we need
    /// somewhere to stage the overflow.
    queue: VecDeque<Event<'a>>,
    /// Nesting depth of `Link`/`Image` tags. Autolink expansion is suppressed
    /// inside them: linkifying text that already belongs to a link would
    /// produce nested links (invalid `<a>` nesting / duplicated URLs).
    link_depth: usize,
}

impl<'a, I> Iterator for GfmTransform<'a, I>
where
    I: Iterator<Item = Event<'a>>,
{
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(ev) = self.queue.pop_front() {
            return Some(ev);
        }
        match self.iter.next()? {
            Event::Text(text) => {
                // Resolve emoji shortcodes first, then detect autolinks within
                // the (possibly emoji-substituted) text.
                let replaced = replace_emoji(&text);
                if self.link_depth > 0 {
                    Some(Event::Text(CowStr::from(replaced)))
                } else {
                    expand_autolinks(&replaced, &mut self.queue);
                    self.queue.pop_front()
                }
            }
            ev @ Event::Start(Tag::Link { .. } | Tag::Image { .. }) => {
                self.link_depth += 1;
                Some(ev)
            }
            ev @ Event::End(TagEnd::Link | TagEnd::Image) => {
                self.link_depth = self.link_depth.saturating_sub(1);
                Some(ev)
            }
            other => Some(other),
        }
    }
}

/// Replaces every resolvable `:shortcode:` in `text` with its emoji character.
///
/// Unknown shortcodes (e.g. GitHub-only `:shipit:`) are left verbatim,
/// colons included. When no substitution happens the borrowed slice is reused.
fn replace_emoji(text: &str) -> String {
    if !text.contains(':') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for caps in EMOJI_RE.captures_iter(text) {
        let whole = caps.get(0).unwrap();
        let name = caps.get(1).unwrap().as_str();
        if let Some(emoji) = emojis::get_by_shortcode(name) {
            out.push_str(&text[last..whole.start()]);
            out.push_str(emoji.as_str());
            last = whole.end();
        }
        // Unknown shortcode: leave it untouched (the `last` cursor is not
        // advanced, so the original text including colons is copied later).
    }
    out.push_str(&text[last..]);
    out
}

/// Scans `text` for bare URLs and pushes the resulting event sequence onto
/// `queue`. Plain text segments become `Text` events; each detected URL becomes
/// a `Start(Link)` / `Text(url)` / `End(Link)` triple.
fn expand_autolinks<'a>(text: &str, queue: &mut VecDeque<Event<'a>>) {
    let mut last = 0;
    for m in URL_RE.find_iter(text) {
        let raw = m.as_str();
        let (url, trim_back) = trim_autolink(raw);
        if url.is_empty() {
            // Nothing usable once trimmed; treat as ordinary text.
            continue;
        }

        // Leading plain text before the URL.
        if m.start() > last {
            push_text(queue, &text[last..m.start()]);
        }

        let dest = if url.len() >= 4 && url[..4].eq_ignore_ascii_case("www.") {
            format!("http://{url}")
        } else {
            url.to_string()
        };
        queue.push_back(Event::Start(Tag::Link {
            link_type: LinkType::Autolink,
            dest_url: CowStr::from(dest),
            title: CowStr::from(""),
            id: CowStr::from(""),
        }));
        push_text(queue, url);
        queue.push_back(Event::End(TagEnd::Link));

        // The trailing punctuation we trimmed off stays as plain text; advance
        // `last` only up to the end of the kept URL.
        last = m.end() - trim_back;
    }
    if last < text.len() {
        push_text(queue, &text[last..]);
    }
    if queue.is_empty() {
        // Defensive: ensure an empty source text still yields one event so the
        // iterator does not silently drop it.
        queue.push_back(Event::Text(CowStr::from(text.to_string())));
    }
}

fn push_text<'a>(queue: &mut VecDeque<Event<'a>>, s: &str) {
    if !s.is_empty() {
        queue.push_back(Event::Text(CowStr::from(s.to_string())));
    }
}

/// Trims a raw URL match according to the GFM extended-autolink rules.
///
/// Returns the kept URL slice and the number of trailing bytes that were
/// trimmed off (so the caller can leave them in the plain-text stream).
///
/// Rules implemented:
/// * Trailing `?`, `!`, `.`, `,`, `:`, `*`, `_`, `~` are stripped.
/// * A trailing `)` is stripped only while the `)` count exceeds the `(` count
///   within the URL (parenthesis balancing).
/// * A trailing `&entity;`-style reference is stripped.
fn trim_autolink(raw: &str) -> (&str, usize) {
    let mut end = raw.len();
    loop {
        let slice = &raw[..end];
        let trimmed = slice.trim_end_matches(['?', '!', '.', ',', ':', '*', '_', '~']);
        if trimmed.len() != end {
            end = trimmed.len();
            continue;
        }

        // Parenthesis balancing for a trailing ')'.
        if slice.ends_with(')') {
            let opens = slice.bytes().filter(|&b| b == b'(').count();
            let closes = slice.bytes().filter(|&b| b == b')').count();
            if closes > opens {
                end -= 1;
                continue;
            }
        }

        // Trailing HTML entity reference such as `&amp;`.
        if slice.ends_with(';')
            && let Some(amp) = slice.rfind('&')
        {
            let entity = &slice[amp..];
            if entity.len() >= 3
                && entity[1..entity.len() - 1]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '#')
            {
                end = amp;
                continue;
            }
        }

        break;
    }
    (&raw[..end], raw.len() - end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(events: &[Event<'_>]) -> Vec<String> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Text(t) => Some(t.to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn emoji_known_shortcode_is_replaced() {
        let out = replace_emoji("a :+1: b :smile: c");
        assert!(out.contains('👍'), "got: {out}");
        assert!(out.contains('😄'), "got: {out}");
        assert!(!out.contains(":+1:"));
    }

    #[test]
    fn emoji_unknown_shortcode_is_preserved() {
        let out = replace_emoji("ship it :shipit: now");
        assert_eq!(out, "ship it :shipit: now");
    }

    #[test]
    fn emoji_no_colons_passthrough() {
        assert_eq!(replace_emoji("plain text"), "plain text");
    }

    #[test]
    fn autolink_trailing_period_excluded() {
        let (url, trimmed) = trim_autolink("www.commonmark.org.");
        assert_eq!(url, "www.commonmark.org");
        assert_eq!(trimmed, 1);
    }

    #[test]
    fn autolink_multi_dot_tail() {
        let (url, _) = trim_autolink("www.commonmark.org/a.b.");
        assert_eq!(url, "www.commonmark.org/a.b");
    }

    #[test]
    fn autolink_balanced_parens_kept() {
        let (url, trimmed) = trim_autolink("www.google.com/search?q=Markup+(business)");
        assert_eq!(url, "www.google.com/search?q=Markup+(business)");
        assert_eq!(trimmed, 0);
    }

    #[test]
    fn autolink_extra_closing_parens_excluded() {
        let (url, _) = trim_autolink("www.google.com/search?q=Markup+(business)))");
        assert_eq!(url, "www.google.com/search?q=Markup+(business)");
    }

    #[test]
    fn autolink_entity_excluded() {
        let (url, _) = trim_autolink("http://example.com?a=1&amp;");
        assert_eq!(url, "http://example.com?a=1");
    }

    #[test]
    fn expand_emits_link_with_www_prefix() {
        let mut q = VecDeque::new();
        expand_autolinks("Visit www.commonmark.org/help for info.", &mut q);
        let events: Vec<_> = q.into_iter().collect();
        // Expect a link start carrying an http:// prefixed destination.
        let has_link = events.iter().any(|e| matches!(
            e,
            Event::Start(Tag::Link { dest_url, .. }) if dest_url.as_ref() == "http://www.commonmark.org/help"
        ));
        assert!(has_link, "events: {events:?}");
        let joined = texts(&events).join("");
        assert!(joined.contains("Visit "));
        assert!(joined.contains("www.commonmark.org/help"));
        assert!(joined.contains(" for info."));
    }

    #[test]
    fn expand_plain_text_single_event() {
        let mut q = VecDeque::new();
        expand_autolinks("no links here", &mut q);
        assert_eq!(q.len(), 1);
        assert!(matches!(q.front(), Some(Event::Text(_))));
    }

    #[test]
    fn no_autolink_inside_existing_link() {
        use pulldown_cmark::Parser;
        // `<...>` autolinks already arrive as Link events; the URL text inside
        // must not be linkified a second time.
        let events: Vec<_> = transform(Parser::new("See <https://example.com>")).collect();
        let link_starts = events
            .iter()
            .filter(|e| matches!(e, Event::Start(Tag::Link { .. })))
            .count();
        assert_eq!(link_starts, 1, "nested link created: {events:?}");
    }

    #[test]
    fn no_autolink_inside_image_alt() {
        use pulldown_cmark::Parser;
        let events: Vec<_> =
            transform(Parser::new("![see https://example.com](img.png)")).collect();
        let link_starts = events
            .iter()
            .filter(|e| matches!(e, Event::Start(Tag::Link { .. })))
            .count();
        assert_eq!(link_starts, 0, "link created inside alt: {events:?}");
    }

    #[test]
    fn autolink_after_link_still_works() {
        use pulldown_cmark::Parser;
        let events: Vec<_> =
            transform(Parser::new("[a](http://a.com) then www.b.com here")).collect();
        let autolinked = events.iter().any(|e| {
            matches!(
                e,
                Event::Start(Tag::Link { dest_url, .. }) if dest_url.as_ref() == "http://www.b.com"
            )
        });
        assert!(autolinked, "events: {events:?}");
    }
}
