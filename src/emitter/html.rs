use pulldown_cmark::{Alignment, BlockQuoteKind, CodeBlockKind, Event, LinkType, Tag, TagEnd};
use std::collections::HashMap;

enum TableState {
    Head,
    Body,
}

pub struct HtmlEmitter<I> {
    iter: I,
    end_newline: bool,
    in_non_writing_block: bool,
    table_state: TableState,
    table_alignments: Vec<Alignment>,
    table_cell_index: usize,
    numbers: HashMap<String, usize>,
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
                        escape_html_body(&mut html_body, &text);
                        self.end_newline = text.ends_with('\n');
                    }
                }
                Event::Code(text) => {
                    html_body.push_str("<code>");
                    escape_html_body(&mut html_body, &text);
                    html_body.push_str("</code>");
                }
                Event::InlineMath(text) => {
                    html_body.push_str(r#"<span class="math math-inline">"#);
                    escape_html_body(&mut html_body, &text);
                    html_body.push_str(r#"</span>"#);
                }
                Event::DisplayMath(text) => {
                    html_body.push_str(r#"<span class="math math-display">"#);
                    escape_html_body(&mut html_body, &text);
                    html_body.push_str(r#"</span>"#);
                }
                Event::Html(html) | Event::InlineHtml(html) => {
                    html_body.push_str(&html);
                }
                Event::SoftBreak => {
                    self.end_newline = true;
                    html_body.push('\n');
                }
                Event::HardBreak => {
                    html_body.push_str("<br />\n");
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
                    html_body.push_str("<sup class=\"footnote-reference\"><a href=\"#");
                    escape_html(&mut html_body, &name);
                    html_body.push_str("\">");
                    let number = *self.numbers.entry(name.to_string()).or_insert(len);
                    html_body.push_str(&format!("{number}"));
                    html_body.push_str("</a></sup>");
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
            Tag::Heading {
                level,
                id,
                classes,
                attrs,
            } => {
                if self.end_newline {
                    buf.push('<');
                } else {
                    buf.push_str("\n<");
                }
                buf.push_str(&format!("{level}"));
                if let Some(id) = id {
                    buf.push_str(" id=\"");
                    escape_html(buf, &id);
                    buf.push('\"');
                }
                let mut classes = classes.iter();
                if let Some(class) = classes.next() {
                    buf.push_str(" class=\"");
                    escape_html(buf, class);
                    for class in classes {
                        buf.push(' ');
                        escape_html(buf, class);
                    }
                    buf.push('\"');
                }
                for (attr, value) in attrs {
                    buf.push(' ');
                    escape_html(buf, &attr);
                    if let Some(val) = value {
                        buf.push_str("=\"");
                        escape_html(buf, &val);
                        buf.push('\"');
                    } else {
                        buf.push_str("=\"\"");
                    }
                }
                buf.push('>')
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
            Tag::Subscript => buf.push_str("<sub>"),
            Tag::Superscript => buf.push_str("<sup>"),
            Tag::Emphasis => buf.push_str("<em>"),
            Tag::Strong => buf.push_str("<strong>"),
            Tag::Strikethrough => buf.push_str("<del>"),
            Tag::Link {
                link_type: LinkType::Email,
                dest_url,
                title,
                id: _,
            } => {
                buf.push_str("<a href=\"mailto:");
                escape_href(buf, &dest_url);
                if !title.is_empty() {
                    buf.push_str("\" title=\"");
                    escape_html(buf, &title);
                }
                buf.push_str("\">")
            }
            Tag::Link {
                link_type: _,
                dest_url,
                title,
                id: _,
            } => {
                buf.push_str("<a href=\"");
                escape_href(buf, &dest_url);
                if !title.is_empty() {
                    buf.push_str("\" title=\"");
                    escape_html(buf, &title);
                }
                buf.push_str("\">")
            }
            Tag::Image {
                link_type: _,
                dest_url,
                title,
                id: _,
            } => {
                buf.push_str("<img src=\"");
                escape_href(buf, &dest_url);
                buf.push_str("\" alt=\"");
                self.raw_text(buf);
                if !title.is_empty() {
                    buf.push_str("\" title=\"");
                    escape_html(buf, &title);
                }
                buf.push_str("\" />")
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
                buf.push_str("</");
                buf.push_str(&format!("{level}"));
                buf.push_str(">\n");
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
                buf.push_str("</em>");
            }
            TagEnd::Superscript => {
                buf.push_str("</sup>");
            }
            TagEnd::Subscript => {
                buf.push_str("</sub>");
            }
            TagEnd::Strong => {
                buf.push_str("</strong>");
            }
            TagEnd::Strikethrough => {
                buf.push_str("</del>");
            }
            TagEnd::Link => {
                buf.push_str("</a>");
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
        let escape = HTML_ESCAPE_TABLE_BODY[c as usize];
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
