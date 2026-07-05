use pulldown_cmark::{Alignment, BlockQuoteKind, CodeBlockKind, Event, LinkType, Tag, TagEnd};
use std::collections::HashMap;
use tracing::{debug, error};

enum TableState {
    Head,
    Body,
}

#[derive(Debug, Clone)]
struct HeadingState {
    /// 見出しタグ内に出力する表示用HTML（インライン要素タグを含む）
    html: String,
    /// アンカーid生成用のプレーンテキスト（タグを含まない）
    text: String,
    end_newline: bool,
}

impl HeadingState {
    fn new() -> Self {
        Self {
            end_newline: false,
            html: String::new(),
            text: String::new(),
        }
    }
    fn push_html(&mut self, html: &str) {
        self.html.push_str(html);
    }
    fn push_text_escaped(&mut self, text: &str) {
        // html側はエスケープ済み文字列を追加、text側はプレーンテキストを追加
        let mut escaped = String::new();
        escape_html_body(&mut escaped, text);
        self.html.push_str(&escaped);
        self.text.push_str(text);
    }
}

pub struct HtmlEmitter<I> {
    iter: I,
    end_newline: bool,
    in_non_writing_block: bool,
    table_state: TableState,
    table_alignments: Vec<Alignment>,
    table_cell_index: usize,
    numbers: HashMap<String, usize>,
    heading_state: Option<HeadingState>,
}

impl<'a, I> HtmlEmitter<I>
where
    I: Iterator<Item = Event<'a>>,
{
    pub fn new(iter: I) -> Self {
        Self {
            iter,
            end_newline: false,
            in_non_writing_block: false,
            table_state: TableState::Head,
            table_alignments: Vec::new(),
            table_cell_index: 0,
            numbers: HashMap::new(),
            heading_state: None,
        }
    }
    pub fn run(&mut self) -> String {
        let mut html_body = String::new();
        while let Some(event) = self.iter.next() {
            match event {
                Event::Start(tag) => self.start_tag(&mut html_body, tag),
                Event::End(tag) => self.end_tag(&mut html_body, tag),
                Event::Text(text) => {
                    if !self.in_non_writing_block {
                        if let Some(heading_state) = self.heading_state.as_mut() {
                            // html側はエスケープ済み、text側はプレーンテキストを蓄積
                            heading_state.push_text_escaped(&text);
                        } else {
                            escape_html_body(&mut html_body, &text);
                        }
                        self.end_newline = text.ends_with('\n');
                    }
                }
                Event::Code(text) => {
                    if let Some(heading_state) = self.heading_state.as_mut() {
                        // 見出し中: html側に<code>タグ付きで、text側にはプレーンテキストを蓄積
                        heading_state.push_html("<code>");
                        let mut escaped = String::new();
                        escape_html_body(&mut escaped, &text);
                        heading_state.push_html(&escaped);
                        heading_state.push_html("</code>");
                        heading_state.text.push_str(&text);
                    } else {
                        html_body.push_str("<code>");
                        escape_html_body(&mut html_body, &text);
                        html_body.push_str("</code>");
                        if let Some(color) = parse_css_color(text.trim()) {
                            html_body.push_str(
                                "<span class=\"color-swatch\" style=\"display:inline-block;width:0.85em;height:0.85em;border-radius:3px;border:1px solid #d0d7de;vertical-align:middle;margin-left:0.4em;background-color:",
                            );
                            escape_html(&mut html_body, &color);
                            html_body.push_str("\"></span>");
                        }
                    }
                }
                Event::InlineMath(text) => {
                    let mut span = String::from(r#"<span class="math math-inline">"#);
                    escape_html_body(&mut span, &text);
                    span.push_str(r#"</span>"#);
                    if let Some(heading_state) = self.heading_state.as_mut() {
                        heading_state.push_html(&span);
                        heading_state.text.push_str(&text);
                    } else {
                        html_body.push_str(&span);
                    }
                }
                Event::DisplayMath(text) => {
                    let mut span = String::from(r#"<span class="math math-display">"#);
                    escape_html_body(&mut span, &text);
                    span.push_str(r#"</span>"#);
                    if let Some(heading_state) = self.heading_state.as_mut() {
                        heading_state.push_html(&span);
                        heading_state.text.push_str(&text);
                    } else {
                        html_body.push_str(&span);
                    }
                }
                Event::Html(html) | Event::InlineHtml(html) => {
                    html_body.push_str(&html);
                }
                Event::SoftBreak => {
                    // Setext headings can span multiple source lines.
                    if let Some(heading_state) = self.heading_state.as_mut() {
                        heading_state.push_html("\n");
                        heading_state.text.push(' ');
                    } else {
                        self.end_newline = true;
                        html_body.push('\n');
                    }
                }
                Event::HardBreak => {
                    if let Some(heading_state) = self.heading_state.as_mut() {
                        heading_state.push_html("<br />\n");
                        heading_state.text.push(' ');
                    } else {
                        html_body.push_str("<br />\n");
                    }
                }
                Event::Rule => {
                    if self.end_newline {
                        html_body.push_str("<hr />\n");
                    } else {
                        html_body.push_str("\n<hr />\n");
                    }
                }
                Event::FootnoteReference(name) => {
                    let len = self.numbers.len() + 1;
                    let mut sup = String::from("<sup class=\"footnote-reference\"><a href=\"#");
                    escape_html(&mut sup, &name);
                    sup.push_str("\">");
                    let number = *self.numbers.entry(name.to_string()).or_insert(len);
                    sup.push_str(&format!("{number}"));
                    sup.push_str("</a></sup>");
                    if let Some(heading_state) = self.heading_state.as_mut() {
                        heading_state.push_html(&sup);
                    } else {
                        html_body.push_str(&sup);
                    }
                }
                Event::TaskListMarker(true) => {
                    html_body.push_str("<input disabled=\"\" type=\"checkbox\" checked=\"\">\n");
                }
                Event::TaskListMarker(false) => {
                    html_body.push_str("<input disabled=\"\" type=\"checkbox\">\n");
                }
            }
        }
        html_body
    }
    fn start_tag(&mut self, buf: &mut String, tag: Tag) {
        match tag {
            Tag::HtmlBlock => (),
            Tag::Paragraph => {
                if self.end_newline {
                    buf.push_str("<p>")
                } else {
                    buf.push_str("\n<p>")
                }
            }
            Tag::Heading { .. } => {
                match self.heading_state.as_ref() {
                    Some(_) => {
                        error!("Heading is nested. This case is not considered.");
                    }
                    None => {
                        self.heading_state = Some(HeadingState::new());
                    }
                }
                if let Some(heading_state) = self.heading_state.as_mut() {
                    heading_state.end_newline = self.end_newline;
                }
                // buf.push('<');
                // } else {
                //     buf.push_str("\n<");
                // }
                // buf.push_str(&format!("{level}"));
                // buf.push('>');
            }
            Tag::Table(alignments) => {
                self.table_alignments = alignments;
                buf.push_str("<table>")
            }
            Tag::TableHead => {
                self.table_state = TableState::Head;
                self.table_cell_index = 0;
                buf.push_str("<thead><tr>")
            }
            Tag::TableRow => {
                self.table_cell_index = 0;
                buf.push_str("<tr>")
            }
            Tag::TableCell => {
                match self.table_state {
                    TableState::Head => {
                        buf.push_str("<th");
                    }
                    TableState::Body => {
                        buf.push_str("<td");
                    }
                }
                match self.table_alignments.get(self.table_cell_index) {
                    Some(&Alignment::Left) => buf.push_str(" style=\"text-align: left\">"),
                    Some(&Alignment::Center) => buf.push_str(" style=\"text-align: center\">"),
                    Some(&Alignment::Right) => buf.push_str(" style=\"text-align: right\">"),
                    _ => buf.push('>'),
                }
            }
            Tag::BlockQuote(kind) => {
                let class_str = match kind {
                    None => "",
                    Some(kind) => match kind {
                        BlockQuoteKind::Note => " class=\"markdown-alert-note\"",
                        BlockQuoteKind::Tip => " class=\"markdown-alert-tip\"",
                        BlockQuoteKind::Important => " class=\"markdown-alert-important\"",
                        BlockQuoteKind::Warning => " class=\"markdown-alert-warning\"",
                        BlockQuoteKind::Caution => " class=\"markdown-alert-caution\"",
                    },
                };
                if self.end_newline {
                    buf.push_str(&format!("<blockquote{}>\n", class_str))
                } else {
                    buf.push_str(&format!("\n<blockquote{}>\n", class_str))
                }
            }
            Tag::CodeBlock(info) => {
                if !self.end_newline {
                    self.end_newline = true;
                    buf.push('\n');
                }
                match info {
                    CodeBlockKind::Fenced(info) => {
                        let lang = info.split(' ').next().unwrap();
                        debug!("CodeBlock: {lang}");
                        if lang.is_empty() {
                            buf.push_str("<pre><code>")
                        } else {
                            buf.push_str("<pre><code class=\"language-");
                            escape_html(buf, lang);
                            buf.push_str("\">")
                        }
                    }
                    CodeBlockKind::Indented => buf.push_str("<pre><code>"),
                }
            }
            Tag::List(Some(1)) => {
                if self.end_newline {
                    buf.push_str("<ol>\n")
                } else {
                    buf.push_str("\n<ol>\n")
                }
            }
            Tag::List(Some(start)) => {
                if self.end_newline {
                    buf.push_str("<ol start=\"");
                } else {
                    buf.push_str("\n<ol start=\"");
                }
                // write!(&mut buf, "{}", start);
                buf.push_str(&start.to_string());
                buf.push_str("\">\n")
            }
            Tag::List(None) => {
                if self.end_newline {
                    buf.push_str("<ul>\n")
                } else {
                    buf.push_str("\n<ul>\n")
                }
            }
            Tag::Item => {
                if self.end_newline {
                    buf.push_str("<li>")
                } else {
                    buf.push_str("\n<li>")
                }
            }
            Tag::DefinitionList => {
                if self.end_newline {
                    buf.push_str("<dl>\n")
                } else {
                    buf.push_str("\n<dl>\n")
                }
            }
            Tag::DefinitionListTitle => {
                if self.end_newline {
                    buf.push_str("<dt>")
                } else {
                    buf.push_str("\n<dt>")
                }
            }
            Tag::DefinitionListDefinition => {
                if self.end_newline {
                    buf.push_str("<dd>")
                } else {
                    buf.push_str("\n<dd>")
                }
            }
            Tag::Subscript => {
                if let Some(hs) = self.heading_state.as_mut() {
                    hs.push_html("<sub>");
                } else {
                    buf.push_str("<sub>");
                }
            }
            Tag::Superscript => {
                if let Some(hs) = self.heading_state.as_mut() {
                    hs.push_html("<sup>");
                } else {
                    buf.push_str("<sup>");
                }
            }
            Tag::Emphasis => {
                if let Some(hs) = self.heading_state.as_mut() {
                    hs.push_html("<em>");
                } else {
                    buf.push_str("<em>");
                }
            }
            Tag::Strong => {
                if let Some(hs) = self.heading_state.as_mut() {
                    hs.push_html("<strong>");
                } else {
                    buf.push_str("<strong>");
                }
            }
            Tag::Strikethrough => {
                if let Some(hs) = self.heading_state.as_mut() {
                    hs.push_html("<del>");
                } else {
                    buf.push_str("<del>");
                }
            }
            Tag::Link {
                link_type: LinkType::Email,
                dest_url,
                title,
                id: _,
            } => {
                let mut tmp = String::new();
                tmp.push_str("<a href=\"mailto:");
                escape_href(&mut tmp, &dest_url);
                if !title.is_empty() {
                    tmp.push_str("\" title=\"");
                    escape_html(&mut tmp, &title);
                }
                tmp.push_str("\">");
                if let Some(hs) = self.heading_state.as_mut() {
                    hs.push_html(&tmp);
                } else {
                    buf.push_str(&tmp);
                }
            }
            Tag::Link {
                link_type: _,
                dest_url,
                title,
                id: _,
            } => {
                let mut tmp = String::new();
                tmp.push_str("<a href=\"");
                escape_href(&mut tmp, &dest_url);
                if !title.is_empty() {
                    tmp.push_str("\" title=\"");
                    escape_html(&mut tmp, &title);
                }
                tmp.push_str("\">");
                if let Some(hs) = self.heading_state.as_mut() {
                    hs.push_html(&tmp);
                } else {
                    buf.push_str(&tmp);
                }
            }
            Tag::Image {
                link_type: _,
                dest_url,
                title,
                id: _,
            } => {
                let mut tmp = String::new();
                tmp.push_str("<img src=\"");
                escape_href(&mut tmp, &dest_url);
                tmp.push_str("\" alt=\"");
                self.raw_text(&mut tmp);
                if !title.is_empty() {
                    tmp.push_str("\" title=\"");
                    escape_html(&mut tmp, &title);
                }
                tmp.push_str("\" />");
                if let Some(hs) = self.heading_state.as_mut() {
                    hs.push_html(&tmp);
                } else {
                    buf.push_str(&tmp);
                }
            }
            Tag::FootnoteDefinition(name) => {
                if self.end_newline {
                    buf.push_str("<div class=\"footnote-definition\" id=\"");
                } else {
                    buf.push_str("\n<div class=\"footnote-definition\" id=\"");
                }
                escape_html(buf, &name);
                buf.push_str("\"><sup class=\"footnote-definition-label\">");
                let len = self.numbers.len() + 1;
                let number = *self.numbers.entry(name.to_string()).or_insert(len);
                buf.push_str(&number.to_string());
                buf.push_str("</sup>")
            }
            Tag::MetadataBlock(_) => {
                self.in_non_writing_block = true;
            }
        }
    }
    fn end_tag(&mut self, buf: &mut String, tag: pulldown_cmark::TagEnd) {
        match tag {
            TagEnd::HtmlBlock => {}
            TagEnd::Paragraph => {
                buf.push_str("</p>\n");
            }
            TagEnd::Heading(level) => {
                match self.heading_state.as_ref() {
                    Some(HeadingState {
                        html,
                        text,
                        end_newline,
                        ..
                    }) => {
                        if *end_newline {
                            buf.push('<');
                        } else {
                            buf.push_str("\n<");
                        }
                        buf.push_str(&format!("{level}"));
                        if !text.is_empty() {
                            // id はプレーンテキストから生成、表示は html バッファを使用
                            let anchor = convert_to_anochor_text(text.clone());
                            buf.push_str(&format!(" id=\"{anchor}\">{html}"));
                            buf.push_str(&format!(
                                "<a class=\"anchor\" aria-label=\"Permalink\" href=\"#{anchor}\">"
                            ));
                            buf.push_str("<svg class=\"octicon octicon-link\" viewBox=\"0 0 16 16\" version=\"16\" height=\"16\" aria-hidden=\"true\">");
                            buf.push_str("<path d=\"m7.775 3.275 1.25-1.25a3.5 3.5 0 1 1 4.95 4.95l-2.5 2.5a3.5 3.5 0 0 1-4.95 0 .751.751 0 0 1 .018-1.042.751.751 0 0 1 1.042-.018 1.998 1.998 0 0 0 2.83 0l2.5-2.5a2.002 2.002 0 0 0-2.83-2.83l-1.25 1.25a.751.751 0 0 1-1.042-.018.751.751 0 0 1-.018-1.042Zm-4.69 9.64a1.998 1.998 0 0 0 2.83 0l1.25-1.25a.751.751 0 0 1 1.042.018.751.751 0 0 1 .018 1.042l-1.25 1.25a3.5 3.5 0 1 1-4.95-4.95l2.5-2.5a3.5 3.5 0 0 1 4.95 0 .751.751 0 0 1-.018 1.042.751.751 0 0 1-1.042.018 1.998 1.998 0 0 0-2.83 0l-2.5 2.5a1.998 1.998 0 0 0 0 2.83Z\"></path>");
                            buf.push_str("</svg>");
                            buf.push_str("</a>");
                        } else {
                            buf.push_str(&format!(">{html}"));
                        }
                    }
                    None => {
                        error!("Heading is not set. This case is not considered.");
                    }
                };
                buf.push_str("</");
                buf.push_str(&format!("{level}"));
                buf.push_str(">\n");
                self.heading_state = None;
            }
            TagEnd::Table => {
                buf.push_str("</tbody></table>\n");
            }
            TagEnd::TableHead => {
                buf.push_str("</tr></thead><tbody>\n");
                self.table_state = TableState::Body;
            }
            TagEnd::TableRow => {
                buf.push_str("</tr>\n");
            }
            TagEnd::TableCell => {
                match self.table_state {
                    TableState::Head => {
                        buf.push_str("</th>");
                    }
                    TableState::Body => {
                        buf.push_str("</td>");
                    }
                }
                self.table_cell_index += 1;
            }
            TagEnd::BlockQuote(_) => {
                buf.push_str("</blockquote>\n");
            }
            TagEnd::CodeBlock => {
                buf.push_str("</code></pre>\n");
            }
            TagEnd::List(true) => {
                buf.push_str("</ol>\n");
            }
            TagEnd::List(false) => {
                buf.push_str("</ul>\n");
            }
            TagEnd::Item => {
                buf.push_str("</li>\n");
            }
            TagEnd::DefinitionList => {
                buf.push_str("</dl>\n");
            }
            TagEnd::DefinitionListTitle => {
                buf.push_str("</dt>\n");
            }
            TagEnd::DefinitionListDefinition => {
                buf.push_str("</dd>\n");
            }
            TagEnd::Emphasis => {
                if let Some(hs) = self.heading_state.as_mut() {
                    hs.push_html("</em>");
                } else {
                    buf.push_str("</em>");
                }
            }
            TagEnd::Superscript => {
                if let Some(hs) = self.heading_state.as_mut() {
                    hs.push_html("</sup>");
                } else {
                    buf.push_str("</sup>");
                }
            }
            TagEnd::Subscript => {
                if let Some(hs) = self.heading_state.as_mut() {
                    hs.push_html("</sub>");
                } else {
                    buf.push_str("</sub>");
                }
            }
            TagEnd::Strong => {
                if let Some(hs) = self.heading_state.as_mut() {
                    hs.push_html("</strong>");
                } else {
                    buf.push_str("</strong>");
                }
            }
            TagEnd::Strikethrough => {
                if let Some(hs) = self.heading_state.as_mut() {
                    hs.push_html("</del>");
                } else {
                    buf.push_str("</del>");
                }
            }
            TagEnd::Link => {
                if let Some(hs) = self.heading_state.as_mut() {
                    hs.push_html("</a>");
                } else {
                    buf.push_str("</a>");
                }
            }
            TagEnd::Image => (), // shouldn't happen, handled in start
            TagEnd::FootnoteDefinition => {
                buf.push_str("</div>\n");
            }
            TagEnd::MetadataBlock(_) => {
                self.in_non_writing_block = false;
            }
        }
    }
    #[allow(clippy::while_let_on_iterator)]
    fn raw_text(&mut self, buf: &mut String) {
        let mut nest = 0;
        while let Some(event) = self.iter.next() {
            match event {
                Event::Start(_) => nest += 1,
                Event::End(_) => {
                    if nest == 0 {
                        break;
                    }
                    nest -= 1;
                }
                Event::Html(_) => {}
                Event::InlineHtml(text) | Event::Code(text) | Event::Text(text) => {
                    // Don't use escape_html_body_text here.
                    // The output of this function is used in the `alt` attribute.
                    escape_html(buf, &text);
                    self.end_newline = text.ends_with('\n');
                }
                Event::InlineMath(text) => {
                    buf.push('$');
                    escape_html(buf, &text);
                    buf.push('$');
                }
                Event::DisplayMath(text) => {
                    buf.push_str("$$");
                    escape_html(buf, &text);
                    buf.push_str("$$");
                }
                Event::SoftBreak | Event::HardBreak | Event::Rule => {
                    buf.push(' ');
                }
                Event::FootnoteReference(name) => {
                    let len = self.numbers.len() + 1;
                    let number = *self.numbers.entry(name.to_string()).or_insert(len);
                    buf.push_str(&format!("[{number}]"));
                }
                Event::TaskListMarker(true) => buf.push_str("[x]"),
                Event::TaskListMarker(false) => buf.push_str("[ ]"),
            }
        }
    }
}

const fn create_html_escape_table(body: bool) -> [u8; 256] {
    let mut table = [0; 256];
    table[b'&' as usize] = 1;
    table[b'<' as usize] = 2;
    table[b'>' as usize] = 3;
    if !body {
        table[b'"' as usize] = 4;
        table[b'\'' as usize] = 5;
    }
    table
}

static HTML_ESCAPE_TABLE: [u8; 256] = create_html_escape_table(false);
static HTML_ESCAPE_TABLE_BODY: [u8; 256] = create_html_escape_table(true);
static HTML_ESCAPES: [&str; 6] = ["", "&amp;", "&lt;", "&gt;", "&quot;", "&#39;"];
#[rustfmt::skip]
static HREF_SAFE: [u8; 128] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 1, 0, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 0, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 1, 1,
    0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 1, 0,
];
static HEX_CHARS: &[u8] = b"0123456789ABCDEF";
static AMP_ESCAPE: &str = "&amp;";
static SINGLE_QUOTE_ESCAPE: &str = "&#x27;";

fn escape_html_body(buf: &mut String, text: &str) {
    let bytes = text.as_bytes();
    let mut mark = 0;
    let mut i = 0;
    while i < text.len() {
        match bytes[i..]
            .iter()
            .position(|&c| HTML_ESCAPE_TABLE_BODY[c as usize] != 0)
        {
            Some(pos) => {
                i += pos;
            }
            None => break,
        }
        let c = bytes[i];
        let escape = HTML_ESCAPE_TABLE_BODY[c as usize];
        let espace_seq = HTML_ESCAPES[escape as usize];
        buf.push_str(&text[mark..i]);
        buf.push_str(espace_seq);
        i += 1;
        mark = i;
    }
    buf.push_str(&text[mark..]);
}

fn escape_html(buf: &mut String, text: &str) {
    let bytes = text.as_bytes();
    let mut mark = 0;
    let mut i = 0;
    while i < text.len() {
        match bytes[i..]
            .iter()
            .position(|&c| HTML_ESCAPE_TABLE[c as usize] != 0)
        {
            Some(pos) => {
                i += pos;
            }
            None => break,
        }
        let c = bytes[i];
        let escape = HTML_ESCAPE_TABLE[c as usize];
        let espace_seq = HTML_ESCAPES[escape as usize];
        buf.push_str(&text[mark..i]);
        buf.push_str(espace_seq);
        i += 1;
        mark = i;
    }
    buf.push_str(&text[mark..]);
}

fn escape_href(buf: &mut String, text: &str) {
    let bytes = text.as_bytes();
    let mut mark = 0;
    for i in 0..bytes.len() {
        let c = bytes[i];
        if c >= 0x80 || HREF_SAFE[c as usize] == 0 {
            if mark < i {
                buf.push_str(&text[mark..i]);
            }
            match c {
                b'&' => {
                    buf.push_str(AMP_ESCAPE);
                }
                b'\'' => {
                    buf.push_str(SINGLE_QUOTE_ESCAPE);
                }
                _ => {
                    let mut b = [0u8; 3];
                    b[0] = b'%';
                    b[1] = HEX_CHARS[((c as usize) >> 4) & 0xF];
                    b[2] = HEX_CHARS[(c as usize) & 0xF];
                    let escaped = str::from_utf8(&b).unwrap();
                    buf.push_str(escaped);
                }
            }
            mark = i + 1; // all escaped characters are ASCII
        }
    }
    buf.push_str(&text[mark..])
}

/// Parse a string that is exactly a CSS color value (HEX / rgb() / rgba() /
/// hsl() / hsla()). Returns the color string usable in CSS, or `None`.
pub(crate) fn parse_css_color(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(hex) = s.strip_prefix('#') {
        let len = hex.len();
        if (len == 3 || len == 4 || len == 6 || len == 8)
            && hex.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Some(s.to_string());
        }
        return None;
    }
    let lower = s.to_ascii_lowercase();
    for prefix in ["rgba", "rgb", "hsla", "hsl"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let rest = rest.trim_start();
            if let Some(inner) = rest.strip_prefix('(').and_then(|r| r.strip_suffix(')'))
                && is_valid_color_args(inner)
            {
                return Some(s.to_string());
            }

            return None;
        }
    }
    None
}

/// Check that the inside of `rgb(...)`/`hsl(...)` is comma-separated tokens
/// that each look like a number or percentage.
fn is_valid_color_args(inner: &str) -> bool {
    let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
    if parts.len() < 3 || parts.len() > 4 {
        return false;
    }
    parts.iter().all(|p| is_number_or_percent(p))
}

fn is_number_or_percent(token: &str) -> bool {
    let token = token.strip_suffix('%').unwrap_or(token);
    if token.is_empty() {
        return false;
    }
    let mut seen_dot = false;
    let mut seen_digit = false;
    for (i, c) in token.chars().enumerate() {
        match c {
            '0'..='9' => seen_digit = true,
            '.' if !seen_dot => seen_dot = true,
            '+' | '-' if i == 0 => {}
            _ => return false,
        }
    }
    seen_digit
}

/// Slugify heading text into the same anchor id the HTML emitter assigns to
/// `<h*>` elements. Public so other crates (e.g. the server's scrollytelling
/// section splitter) can address the exact ids present in the rendered DOM.
pub fn heading_anchor(heading_text: &str) -> String {
    convert_to_anochor_text(heading_text.to_string())
}

fn convert_to_anochor_text(heading_text: String) -> String {
    let mut anchor = String::with_capacity(heading_text.len());
    let mut prev_hyphen = false;

    for ch in heading_text.trim().chars() {
        for lower in ch.to_lowercase() {
            if lower.is_alphanumeric() || lower == '_' {
                anchor.push(lower);
                prev_hyphen = false;
            } else if (lower.is_whitespace() || lower == '-') && !prev_hyphen && !anchor.is_empty()
            {
                anchor.push('-');
                prev_hyphen = true;
            }
        }
    }

    while anchor.ends_with('-') {
        anchor.pop();
    }

    anchor
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::Parser;

    fn render(md: &str) -> String {
        HtmlEmitter::new(Parser::new(md)).run()
    }

    #[test]
    fn image_in_heading_stays_inside_heading_tag() {
        let out = render("# ![alt](img.png)");
        let h_open = out.find("<h1").expect("h1 tag");
        let img = out.find("<img").expect("img tag");
        assert!(img > h_open, "img leaked before heading: {out}");
    }

    #[test]
    fn setext_heading_softbreak_stays_inside_heading() {
        let out = render("Line1\nLine2\n=====\n");
        assert!(
            out.contains("Line1\nLine2"),
            "lines should be separated inside the heading: {out}"
        );
        assert!(
            out.contains("id=\"line1-line2\""),
            "anchor should treat the soft break as a space: {out}"
        );
    }

    #[test]
    fn math_in_heading_stays_inside_heading_tag() {
        let mut options = pulldown_cmark::Options::empty();
        options.insert(pulldown_cmark::Options::ENABLE_MATH);
        let out = HtmlEmitter::new(Parser::new_ext("# $x$", options)).run();
        let h_open = out.find("<h1").expect("h1 tag");
        let math = out.find("math-inline").expect("math span");
        assert!(math > h_open, "math leaked before heading: {out}");
    }
}
