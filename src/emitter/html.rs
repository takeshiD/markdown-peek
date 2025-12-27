use super::Emitter;
use pulldown_cmark::{Event, Tag, TagEnd};

pub struct HtmlEmitter {}

impl HtmlEmitter {
    pub fn run<'a>(events: impl Iterator<Item = Event<'a>>) -> String {
        let mut html_body = String::new();
        let mut end_newline = false;
        for event in events {
            match event {
                Event::Start(tag) => Self::start_tag(&mut html_body, &mut end_newline, tag),
                Event::End(tag) => Self::end_tag(&mut html_body, &mut end_newline, tag),
                Event::Text(text) => escape_html_body(&mut html_body, &text),
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
                    end_newline = true;
                    html_body.push('\n');
                }
                Event::HardBreak => {
                    html_body.push_str("<br />\n");
                }
                Event::Rule => {
                    if end_newline {
                        html_body.push_str("<hr />\n");
                    } else {
                        html_body.push_str("\n<hr />\n");
                    }
                }
                Event::FootnoteReference(name) => {}
                Event::TaskListMarker(true) => {
                    html_body.push_str(r#"<input disabled="" type="checkbox" checked="">\n"#);
                }
                Event::TaskListMarker(false) => {
                    html_body.push_str(r#"<input disabled="" type="checkbox">\n"#);
                }
            }
        }
        html_body
    }
    fn start_tag(buf: &mut String, end_newline: &mut bool, tag: Tag) {
        match tag {
            Tag::HtmlBlock => (),
            Tag::Paragraph => {
                if *end_newline {
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
                if *end_newline {
                    buf.push_str("<");
                } else {
                    buf.push_str("\n<");
                }
                // write!(&buf, "{}", level);
                buf.push_str(&format!("{level}"));
                if let Some(id) = id {
                    buf.push_str(" id=\"");
                    escape_html(&mut buf, &id);
                    buf.push_str("\"");
                }
                let mut classes = classes.iter();
                if let Some(class) = classes.next() {
                    buf.push_str(" class=\"");
                    escape_html(&mut buf, class);
                    for class in classes {
                        buf.push_str(" ");
                        escape_html(&mut buf, class);
                    }
                    buf.push_str("\"");
                }
                for (attr, value) in attrs {
                    buf.push_str(" ");
                    escape_html(&mut buf, &attr);
                    if let Some(val) = value {
                        buf.push_str("=\"");
                        escape_html(&mut buf, &val);
                        buf.push_str("\"");
                    } else {
                        buf.push_str("=\"\"");
                    }
                }
                buf.push_str(">")
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
                    _ => buf.push_str(">"),
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
                if end_newline {
                    buf.push_str(&format!("<blockquote{}>\n", class_str))
                } else {
                    buf.push_str(&format!("\n<blockquote{}>\n", class_str))
                }
            }
            Tag::CodeBlock(info) => {
                if !end_newline {
                    buf.push_str_newline();
                }
                match info {
                    CodeBlockKind::Fenced(info) => {
                        let lang = info.split(' ').next().unwrap();
                        if lang.is_empty() {
                            buf.push_str("<pre><code>")
                        } else {
                            buf.push_str("<pre><code class=\"language-");
                            escape_html(&mut buf.push_strr, lang);
                            buf.push_str("\">")
                        }
                    }
                    CodeBlockKind::Indented => buf.push_str("<pre><code>"),
                }
            }
            Tag::ContainerBlock(Default, kind) => {
                if !end_newline {
                    buf.push_str_newline();
                }
                buf.push_str("<div class=\"");
                escape_html(&mut buf.push_strr, &kind);
                buf.push_str("\">")
            }
            Tag::ContainerBlock(Spoiler, summary) => {
                if !end_newline {
                    buf.push_str_newline();
                }
                if summary.is_empty() {
                    buf.push_str("<details>")
                } else {
                    buf.push_str("<details><summary>");
                    escape_html(&mut buf.push_strr, summary.as_ref());
                    buf.push_str("</summary>")
                }
            }
            Tag::List(Some(1)) => {
                if end_newline {
                    buf.push_str("<ol>\n")
                } else {
                    buf.push_str("\n<ol>\n")
                }
            }
            Tag::List(Some(start)) => {
                if end_newline {
                    buf.push_str("<ol start=\"");
                } else {
                    buf.push_str("\n<ol start=\"");
                }
                write!(&mut buf.push_strr, "{}", start);
                buf.push_str("\">\n")
            }
            Tag::List(None) => {
                if end_newline {
                    buf.push_str("<ul>\n")
                } else {
                    buf.push_str("\n<ul>\n")
                }
            }
            Tag::Item => {
                if end_newline {
                    buf.push_str("<li>")
                } else {
                    buf.push_str("\n<li>")
                }
            }
            Tag::DefinitionList => {
                if end_newline {
                    buf.push_str("<dl>\n")
                } else {
                    buf.push_str("\n<dl>\n")
                }
            }
            Tag::DefinitionListTitle => {
                if end_newline {
                    buf.push_str("<dt>")
                } else {
                    buf.push_str("\n<dt>")
                }
            }
            Tag::DefinitionListDefinition => {
                if end_newline {
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
                escape_href(&mut buf.push_strr, &dest_url);
                if !title.is_empty() {
                    buf.push_str("\" title=\"");
                    escape_html(&mut buf.push_strr, &title);
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
                escape_href(&mut buf.push_strr, &dest_url);
                if !title.is_empty() {
                    buf.push_str("\" title=\"");
                    escape_html(&mut buf.push_strr, &title);
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
                escape_href(&mut buf.push_strr, &dest_url);
                buf.push_str("\" alt=\"");
                self.raw_text();
                if !title.is_empty() {
                    buf.push_str("\" title=\"");
                    escape_html(&mut buf.push_strr, &title);
                }
                buf.push_str("\" />")
            }
            Tag::FootnoteDefinition(name) => {
                if end_newline {
                    buf.push_str("<div class=\"footnote-definition\" id=\"");
                } else {
                    buf.push_str("\n<div class=\"footnote-definition\" id=\"");
                }
                escape_html(&mut buf.push_strr, &name);
                buf.push_str("\"><sup class=\"footnote-definition-label\">");
                let len = self.numbers.len() + 1;
                let number = *self.numbers.entry(name).or_insert(len);
                write!(&mut buf.push_strr, "{}", number);
                buf.push_str("</sup>")
            }
            Tag::MetadataBlock(_) => {
                self.in_non_writing_block = true;
                Ok(())
            }
        }
    }
    fn end_tag(buf: &mut String, end_newline: &mut bool, tag: pulldown_cmark::TagEnd) {}
}
impl Emitter for HtmlEmitter {
    fn emit<'a>(&mut self, events: impl Iterator<Item = pulldown_cmark::Event<'a>>) -> String {
        unimplemented!()
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
