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

/// One prior conversation turn in the in-panel Q&A.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
}

/// A reader's question about a section (POST body for `/api/scrolly/ask`).
#[derive(Debug, Clone, Deserialize)]
pub struct AskRequest {
    /// Heading anchor of the section the reader is asking about.
    pub anchor: String,
    pub question: String,
    #[serde(default = "default_lang")]
    pub lang: String,
    #[serde(default)]
    pub history: Vec<ChatTurn>,
    /// The guide's whole-document overview (so Q&A can be about the guide's own
    /// explanation, not just the source text).
    #[serde(default)]
    pub guide_overview: String,
    /// The guide's commentary for the current section.
    #[serde(default)]
    pub guide_commentary: String,
}

fn default_lang() -> String {
    "auto".to_string()
}

/// The model's answer, returned to the panel.
#[derive(Debug, Clone, Serialize)]
pub struct AnswerResult {
    pub answer: String,
}

/// Map a `lang` selection to an explicit instruction for the model.
fn lang_instruction(lang: &str) -> &'static str {
    match lang {
        "ja" => "Respond in Japanese (必ず日本語で回答してください).",
        "en" => "Respond in English.",
        _ => "Respond in the SAME LANGUAGE as the document.",
    }
}

/// A heading + the raw markdown of its section (until the next H1–H3).
struct SectionMeta {
    index: usize,
    anchor: String,
    title: String,
    level: u8,
    body: String,
}

/// Build the guide for `markdown` via the LLM `backend`, caching the result under
/// `cache_root/.cache/mdpeek/<hash>.scrolly.json`. `lang` steers the commentary
/// language (`"auto"` = match the document; `"ja"`/`"en"` force). LLM-only: with
/// no backend, or on a generation/parse failure, this returns an error (the
/// mode is always model-generated — there is no offline fallback).
pub fn generate(
    markdown: &str,
    backend: Option<&LlmBackendConfig>,
    cache_root: &Path,
    lang: &str,
) -> anyhow::Result<ScrollyGuide> {
    let backend = backend
        .ok_or_else(|| anyhow::anyhow!("Guided Reading needs an LLM backend (none configured)"))?;

    let cache_path = cache_path(cache_root, markdown, &model_id(backend), lang);
    if let Some(hit) = read_cache(&cache_path) {
        return Ok(hit);
    }

    let metas = section_metas(markdown);
    let guide = generate_llm(markdown, &metas, backend, lang)?;
    write_cache(&cache_path, &guide);
    Ok(guide)
}

/// Answer a reader's question about a specific section (in-panel Q&A). Grounded
/// in that section's source text plus a whole-doc overview for context, honouring
/// `lang` and the prior `history` turns. Requires an LLM backend.
pub fn answer(
    markdown: &str,
    backend: Option<&LlmBackendConfig>,
    req: &AskRequest,
) -> AnswerResult {
    let backend = match backend {
        Some(b) => b,
        None => {
            return AnswerResult {
                answer: "質問応答にはLLMバックエンドが必要です（オフラインでは利用できません）。\
                         Q&A needs an LLM backend and is unavailable offline."
                    .to_string(),
            };
        }
    };

    let metas = section_metas(markdown);
    let section = metas.iter().find(|m| m.anchor == req.anchor);
    let (title, body) = match section {
        Some(m) => (m.title.as_str(), m.body.as_str()),
        None => ("(document)", markdown),
    };

    let system = format!(
        "You answer a reader's questions while they read a design or planning \
         document with a guided-reading assistant. Two things are in scope: (a) \
         the ORIGINAL document text, and (b) the GUIDE's own explanation (its \
         overview and per-section commentary) — the reader may be asking about \
         either. Be concise and grounded in the material provided; if it does not \
         answer the question, say so plainly rather than guessing. {}",
        lang_instruction(&req.lang)
    );

    let mut convo = String::new();
    for turn in &req.history {
        let who = if turn.role == "user" { "Reader" } else { "You" };
        convo.push_str(&format!("{who}: {}\n", turn.content));
    }

    // Only include guide blocks when the client supplied them (kept optional so
    // the endpoint still works for callers without a loaded guide).
    let guide_overview = if req.guide_overview.trim().is_empty() {
        String::new()
    } else {
        format!(
            "Guide's overview of the whole document:\n{}\n\n---\n",
            req.guide_overview.trim()
        )
    };
    let guide_commentary = if req.guide_commentary.trim().is_empty() {
        String::new()
    } else {
        format!(
            "Guide's commentary on this section:\n{}\n\n---\n",
            req.guide_commentary.trim()
        )
    };

    let user = format!(
        "Current section title: {title}\n\n\
         Original section text:\n{}\n\n---\n\
         {guide_commentary}{guide_overview}\
         Whole original document (for reference):\n{}\n\n---\n\
         {convo}Reader's question: {}\n\nAnswer:",
        truncate_chars(body, 6_000),
        truncate_chars(markdown, 8_000),
        req.question,
    );

    match mdpeek_gui::complete_text_blocking(backend, &system, &user) {
        Ok(text) => AnswerResult { answer: text },
        Err(e) => AnswerResult {
            answer: format!("回答の生成に失敗しました: {e}"),
        },
    }
}

// ---- LLM path -------------------------------------------------------------

fn generate_llm(
    markdown: &str,
    metas: &[SectionMeta],
    backend: &LlmBackendConfig,
    lang: &str,
) -> anyhow::Result<ScrollyGuide> {
    let system = system_prompt(lang);
    let user = user_prompt(markdown, metas);
    let raw = mdpeek_gui::complete_text_blocking(backend, &system, &user)?;

    // Marker-delimited plain text (not JSON): free-text commentary can contain
    // newlines and quotes that routinely break a single big JSON blob, and a
    // truncated response would lose *everything*. With markers, parsing is
    // newline-safe and a cut-off response still yields every section before the
    // cut. See [`parse_guide_output`].
    let (overview, mut by_index) = parse_guide_output(&raw);
    if overview.is_empty() && by_index.is_empty() {
        anyhow::bail!(
            "could not parse guide from model output ({} chars): {}",
            raw.len(),
            raw.chars().take(200).collect::<String>()
        );
    }

    let sections = metas
        .iter()
        .map(|m| ScrollySection {
            index: m.index,
            anchor: m.anchor.clone(),
            title: m.title.clone(),
            level: m.level,
            commentary: by_index.remove(&m.index).unwrap_or_default(),
        })
        .collect();

    Ok(ScrollyGuide {
        overview,
        sections,
        origin: "llm".to_string(),
    })
}

/// Parse the marker-delimited guide format:
///
/// ```text
/// @@OVERVIEW@@
/// <overview text, may span lines>
/// @@SECTION 0@@
/// <commentary>
/// @@SECTION 1@@
/// <commentary>
/// ```
///
/// Tolerant: ignores code fences, stray prose before the first marker, and a
/// truncated tail (everything parsed so far is kept).
fn parse_guide_output(raw: &str) -> (String, std::collections::HashMap<usize, String>) {
    #[derive(PartialEq)]
    enum Target {
        None,
        Overview,
        Section(usize),
    }

    let mut overview = String::new();
    let mut sections: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    let mut target = Target::None;
    let mut buf: Vec<&str> = Vec::new();

    fn commit(
        target: &Target,
        buf: &mut Vec<&str>,
        overview: &mut String,
        sections: &mut std::collections::HashMap<usize, String>,
    ) {
        let text = buf.join("\n").trim().to_string();
        buf.clear();
        if text.is_empty() {
            return;
        }
        match target {
            Target::Overview => *overview = text,
            Target::Section(i) => {
                sections.insert(*i, text);
            }
            Target::None => {}
        }
    }

    for line in raw.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            continue; // ignore code-fence lines if the model added them
        }
        if t.eq_ignore_ascii_case("@@overview@@") {
            commit(&target, &mut buf, &mut overview, &mut sections);
            target = Target::Overview;
        } else if let Some(idx) = parse_section_marker(t) {
            commit(&target, &mut buf, &mut overview, &mut sections);
            target = Target::Section(idx);
        } else {
            buf.push(line);
        }
    }
    commit(&target, &mut buf, &mut overview, &mut sections);

    (overview, sections)
}

/// Recognise `@@SECTION <n>@@` (case-insensitive) → `n`.
fn parse_section_marker(line: &str) -> Option<usize> {
    let inner = line.strip_prefix("@@").and_then(|s| s.strip_suffix("@@"))?;
    let rest = inner.trim();
    let n = rest.get(..7).filter(|p| p.eq_ignore_ascii_case("SECTION"))?;
    let _ = n;
    rest[7..].trim().parse::<usize>().ok()
}

fn system_prompt(lang: &str) -> String {
    format!(
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
         4. {}\n\
         5. Output PLAIN TEXT in EXACTLY this marker format and nothing else (no \
         JSON, no code fences, no extra prose):\n\
         @@OVERVIEW@@\n\
         <overview text>\n\
         @@SECTION 0@@\n\
         <commentary for section 0>\n\
         @@SECTION 1@@\n\
         <commentary for section 1>\n\
         ...one @@SECTION <index>@@ block per section index given, in order. Use \
         the exact indices provided.",
        lang_instruction(lang)
    )
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

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}\n\n[... document truncated for length ...]")
}

// ---- Cache ----------------------------------------------------------------

fn model_id(backend: &LlmBackendConfig) -> String {
    let model = backend.model.as_deref().unwrap_or("default");
    match backend.effort {
        Some(e) => format!("{:?}-{model}-{:?}", backend.provider, e),
        None => format!("{:?}-{model}", backend.provider),
    }
}

fn cache_path(root: &Path, markdown: &str, model_id: &str, lang: &str) -> PathBuf {
    let mut h = DefaultHasher::new();
    SCROLLY_SCHEMA.hash(&mut h);
    model_id.hash(&mut h);
    lang.hash(&mut h);
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
    fn generate_without_backend_errors() {
        // LLM-only: no backend must be a hard error, not a silent fallback.
        let dir = std::env::temp_dir();
        assert!(generate(DOC, None, &dir, "auto").is_err());
    }

    #[test]
    fn parses_marker_format_with_multiline_commentary() {
        let raw = "@@OVERVIEW@@\nA doc.\nSecond line.\n@@SECTION 0@@\nWhy it matters.\n\nMore.\n@@SECTION 1@@\nRisk note.";
        let (overview, map) = parse_guide_output(raw);
        assert_eq!(overview, "A doc.\nSecond line.");
        assert_eq!(map.get(&0).unwrap(), "Why it matters.\n\nMore.");
        assert_eq!(map.get(&1).unwrap(), "Risk note.");
    }

    #[test]
    fn truncated_tail_keeps_earlier_sections() {
        // Model cut off mid-section 2 — 0 and 1 must still survive.
        let raw = "@@OVERVIEW@@\nO.\n@@SECTION 0@@\nc0\n@@SECTION 1@@\nc1\n@@SECTION 2@@\nc2 unfin";
        let (overview, map) = parse_guide_output(raw);
        assert_eq!(overview, "O.");
        assert_eq!(map.get(&0).unwrap(), "c0");
        assert_eq!(map.get(&1).unwrap(), "c1");
        assert_eq!(map.get(&2).unwrap(), "c2 unfin");
    }

    #[test]
    fn ignores_code_fences_around_markers() {
        let raw = "```\n@@OVERVIEW@@\nO.\n@@SECTION 0@@\nc0\n```";
        let (overview, map) = parse_guide_output(raw);
        assert_eq!(overview, "O.");
        assert_eq!(map.get(&0).unwrap(), "c0");
    }

    #[test]
    fn parse_section_marker_matches_case_insensitively() {
        assert_eq!(parse_section_marker("@@SECTION 3@@"), Some(3));
        assert_eq!(parse_section_marker("@@section 12@@"), Some(12));
        assert_eq!(parse_section_marker("@@OVERVIEW@@"), None);
        assert_eq!(parse_section_marker("not a marker"), None);
    }
}
