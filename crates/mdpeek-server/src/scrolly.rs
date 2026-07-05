//! Generative Scrollytelling backend (reader-paced guided reading, design §5).
//!
//! Given the active markdown, produce a [`ScrollyGuide`]: a whole-document
//! *overview* plus concise, **additive** commentary for each top-level section
//! (H1–H3). The client enters a reader-paced mode that greys out the document,
//! highlights the section the reader has scrolled to, and reveals its commentary
//! — reducing cognitive load without driving the scroll for them.
//!
//! One LLM call per document (overview + all section commentary as JSON), keyed
//! to the DOM by the *same* heading anchor ids the HTML emitter assigns, so the
//! client can map commentary to sections by `getElementById`. Offline-safe: with
//! no LLM backend / API key (or on any failure) it falls back to a deterministic
//! rules commentary so the experience always works.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

use mdpeek_gui::LlmBackendConfig;
use mdpeek_render_html::heading_anchor;

/// Longest markdown prefix handed to the model (keeps the prompt bounded on very
/// large docs; the tail is elided with a note).
const MAX_DOC_CHARS: usize = 16_000;
/// Cap on sections to guide (keeps the response bounded).
const MAX_SECTIONS: usize = 60;
/// Cache schema tag — bump when the guide shape or prompt changes.
const SCROLLY_SCHEMA: &str = "scrolly-v1";

/// One guided section, aligned to a rendered `<h*>` by `anchor`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollySection {
    pub index: usize,
    pub anchor: String,
    pub title: String,
    pub level: u8,
    pub commentary: String,
}

/// The full guide the client renders in reader-paced mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollyGuide {
    pub overview: String,
    pub sections: Vec<ScrollySection>,
    /// `"llm"` or `"rules"` — surfaced in the UI so readers know when commentary
    /// is model-generated vs. the offline fallback.
    pub origin: String,
}

/// A heading + the raw markdown of its section (until the next H1–H3).
struct SectionMeta {
    index: usize,
    anchor: String,
    title: String,
    level: u8,
    body: String,
}

/// Build the guide for `markdown`, using `backend` when available and caching
/// the result under `cache_root/.cache/mdpeek/<hash>.scrolly.json`.
pub fn generate(
    markdown: &str,
    backend: Option<&LlmBackendConfig>,
    cache_root: &Path,
) -> ScrollyGuide {
    let metas = section_metas(markdown);

    let model_id = backend.map(model_id).unwrap_or_else(|| "rules".to_string());
    let cache_path = cache_path(cache_root, markdown, &model_id);
    if let Some(hit) = read_cache(&cache_path) {
        return hit;
    }

    let guide = match backend {
        Some(b) => generate_llm(markdown, &metas, b).unwrap_or_else(|_| fallback(markdown, &metas)),
        None => fallback(markdown, &metas),
    };

    write_cache(&cache_path, &guide);
    guide
}

// ---- LLM path -------------------------------------------------------------

#[derive(Deserialize)]
struct LlmGuide {
    overview: String,
    sections: Vec<LlmSection>,
}

#[derive(Deserialize)]
struct LlmSection {
    index: usize,
    commentary: String,
}

fn generate_llm(
    markdown: &str,
    metas: &[SectionMeta],
    backend: &LlmBackendConfig,
) -> anyhow::Result<ScrollyGuide> {
    let system = system_prompt();
    let user = user_prompt(markdown, metas);
    let raw = mdpeek_gui::complete_text_blocking(backend, &system, &user)?;
    let json = extract_json_object(&raw);
    let parsed: LlmGuide = serde_json::from_str(json)?;

    // Map model commentary (by index) back onto our section metadata; any
    // section the model skipped falls back to a rules commentary so the guide is
    // always complete.
    let mut by_index: std::collections::HashMap<usize, String> = parsed
        .sections
        .into_iter()
        .map(|s| (s.index, s.commentary))
        .collect();

    let sections = metas
        .iter()
        .map(|m| ScrollySection {
            index: m.index,
            anchor: m.anchor.clone(),
            title: m.title.clone(),
            level: m.level,
            commentary: by_index
                .remove(&m.index)
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .unwrap_or_else(|| rules_commentary(&m.body)),
        })
        .collect();

    let overview = {
        let o = parsed.overview.trim().to_string();
        if o.is_empty() {
            rules_overview(markdown)
        } else {
            o
        }
    };

    Ok(ScrollyGuide {
        overview,
        sections,
        origin: "llm".to_string(),
    })
}

fn system_prompt() -> String {
    "You are a reading guide for design and planning documents. Your job is to \
     help a reader understand the document with reduced cognitive load as they \
     scroll through it.\n\n\
     Rules:\n\
     1. Be concise: 2-4 sentences of commentary per section.\n\
     2. Do NOT restate or paraphrase the section's sentences. Add value instead: \
     explain why the section matters, how it connects to earlier decisions, \
     surface unstated assumptions or gaps, and translate jargon.\n\
     3. The overview is 2-4 sentences on the whole document's purpose and how it \
     is structured.\n\
     4. Respond in the SAME LANGUAGE as the document.\n\
     5. Output STRICT JSON ONLY (no markdown code fences, no prose around it) \
     matching exactly:\n\
     {\"overview\": string, \"sections\": [{\"index\": number, \"commentary\": string}]}"
        .to_string()
}

fn user_prompt(markdown: &str, metas: &[SectionMeta]) -> String {
    let doc = truncate_chars(markdown, MAX_DOC_CHARS);
    let mut list = String::new();
    for m in metas {
        list.push_str(&format!("{}. {}\n", m.index, m.title));
    }
    format!(
        "Document:\n\n{doc}\n\n---\nSections to comment on (by index):\n{list}\n\
         Produce the overview and one commentary object per section index above."
    )
}

// ---- Rules / offline fallback --------------------------------------------

fn fallback(markdown: &str, metas: &[SectionMeta]) -> ScrollyGuide {
    let sections = metas
        .iter()
        .map(|m| ScrollySection {
            index: m.index,
            anchor: m.anchor.clone(),
            title: m.title.clone(),
            level: m.level,
            commentary: rules_commentary(&m.body),
        })
        .collect();
    ScrollyGuide {
        overview: rules_overview(markdown),
        sections,
        origin: "rules".to_string(),
    }
}

/// Naive commentary: the first sentence or two of the section body (heading and
/// markdown syntax stripped). Never claims to be more than an excerpt.
fn rules_commentary(body: &str) -> String {
    let text = plain_text(body);
    let excerpt = first_sentences(&text, 2);
    if excerpt.is_empty() {
        "(No preview available for this section.)".to_string()
    } else {
        excerpt
    }
}

fn rules_overview(markdown: &str) -> String {
    let text = plain_text(markdown);
    let excerpt = first_sentences(&text, 3);
    if excerpt.is_empty() {
        "Overview unavailable offline.".to_string()
    } else {
        excerpt
    }
}

// ---- Section extraction ---------------------------------------------------

/// Split `markdown` into H1–H3 sections (heading + body up to the next such
/// heading). Anchors match the HTML emitter's heading ids.
fn section_metas(markdown: &str) -> Vec<SectionMeta> {
    // Pass 1: collect qualifying headings with byte offset + text.
    struct Head {
        level: u8,
        title: String,
        start: usize,
    }
    let mut heads: Vec<Head> = Vec::new();
    let mut cur: Option<(u8, String, usize)> = None; // (level, text, start)

    let parser = Parser::new_ext(markdown, mdpeek_gfm::parser_options());
    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let lvl = heading_level(level);
                if lvl <= 3 {
                    cur = Some((lvl, String::new(), range.start));
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, ref mut text, _)) = cur {
                    text.push_str(&t);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, title, start)) = cur.take() {
                    let title = title.trim().to_string();
                    if !title.is_empty() {
                        heads.push(Head { level, title, start });
                    }
                }
            }
            _ => {}
        }
    }

    // Pass 2: body = source between this heading and the next.
    let mut out = Vec::new();
    for (i, h) in heads.iter().enumerate() {
        if i >= MAX_SECTIONS {
            break;
        }
        let end = heads.get(i + 1).map(|n| n.start).unwrap_or(markdown.len());
        let body = markdown.get(h.start..end).unwrap_or("").to_string();
        out.push(SectionMeta {
            index: i,
            anchor: heading_anchor(&h.title),
            title: h.title.clone(),
            level: h.level,
            body,
        });
    }
    out
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

// ---- Text helpers ---------------------------------------------------------

/// Strip common markdown syntax to plain-ish text for the offline excerpts.
fn plain_text(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    for raw in md.lines() {
        let line = raw.trim();
        // Skip headings, fences, frontmatter delimiters, and list/table markup
        // noise so the excerpt reads as prose.
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("```")
            || line.starts_with("---")
            || line.starts_with("+++")
            || line.starts_with('|')
            || line.starts_with('>')
        {
            continue;
        }
        let cleaned = line
            .trim_start_matches(|c| c == '-' || c == '*' || c == '+' || c == ' ')
            .replace(['`', '*', '_', '#'], "");
        if !cleaned.trim().is_empty() {
            out.push_str(cleaned.trim());
            out.push(' ');
        }
    }
    out.trim().to_string()
}

/// Take up to `n` sentences (`.`/`。`/`!`/`?` terminated), capped in length.
fn first_sentences(text: &str, n: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    for ch in text.chars() {
        out.push(ch);
        if matches!(ch, '.' | '。' | '!' | '?' | '！' | '？') {
            count += 1;
            if count >= n {
                break;
            }
        }
        if out.chars().count() >= 320 {
            break;
        }
    }
    out.trim().to_string()
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}\n\n[... document truncated for length ...]")
}

/// Best-effort extraction of the outermost `{...}` JSON object from model output
/// (handles code fences and stray prose around it).
fn extract_json_object(raw: &str) -> &str {
    let start = raw.find('{');
    let end = raw.rfind('}');
    match (start, end) {
        (Some(s), Some(e)) if e >= s => &raw[s..=e],
        _ => raw,
    }
}

// ---- Cache ----------------------------------------------------------------

fn model_id(backend: &LlmBackendConfig) -> String {
    let model = backend.model.as_deref().unwrap_or("default");
    match backend.effort {
        Some(e) => format!("{:?}-{model}-{:?}", backend.provider, e),
        None => format!("{:?}-{model}", backend.provider),
    }
}

fn cache_path(root: &Path, markdown: &str, model_id: &str) -> PathBuf {
    let mut h = DefaultHasher::new();
    SCROLLY_SCHEMA.hash(&mut h);
    model_id.hash(&mut h);
    markdown.hash(&mut h);
    let hash = h.finish();
    root.join(".cache")
        .join("mdpeek")
        .join(format!("{hash:016x}.scrolly.json"))
}

fn read_cache(path: &Path) -> Option<ScrollyGuide> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_cache(path: &Path, guide: &ScrollyGuide) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(guide) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# Title\n\nIntro paragraph one. Intro two.\n\n\
        ## Design\n\nWe chose X because Y. It affects Z.\n\n\
        ### Data model\n\nTables live here.\n\n\
        ## Risks\n\nThings could break.\n";

    #[test]
    fn extracts_h1_h3_sections_with_anchors() {
        let metas = section_metas(DOC);
        let titles: Vec<_> = metas.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(titles, ["Title", "Design", "Data model", "Risks"]);
        assert_eq!(metas[2].anchor, heading_anchor("Data model"));
        assert_eq!(metas[2].level, 3);
    }

    #[test]
    fn section_body_stops_at_next_heading() {
        let metas = section_metas(DOC);
        assert!(metas[1].body.contains("We chose X"));
        assert!(!metas[1].body.contains("Things could break"));
    }

    #[test]
    fn fallback_is_complete_and_marked_rules() {
        let metas = section_metas(DOC);
        let guide = fallback(DOC, &metas);
        assert_eq!(guide.origin, "rules");
        assert_eq!(guide.sections.len(), metas.len());
        assert!(!guide.overview.is_empty());
        assert!(guide.sections.iter().all(|s| !s.commentary.is_empty()));
    }

    #[test]
    fn extract_json_object_trims_fences() {
        let raw = "```json\n{\"overview\":\"a\",\"sections\":[]}\n```";
        assert_eq!(extract_json_object(raw), "{\"overview\":\"a\",\"sections\":[]}");
    }
}
