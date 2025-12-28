use owo_colors::{OwoColorize, Style};
use pulldown_cmark::{BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, LinkType, Tag, TagEnd};
use std::collections::HashMap;

enum ListState {
    Ordered { index: usize },
    Unordered,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub heading: Style,
    pub block_quote: Style,
    pub quote_bar: Style,
    pub code: Style,
    pub link: Style,
    pub list_marker: Style,
    pub rule: Style,
    pub table_header: Style,
    pub footnote: Style,
}

impl Theme {
    pub fn glow() -> Self {
        Self {
            heading: Style::new().bright_cyan().bold(),
            block_quote: Style::new().bright_magenta(),
            quote_bar: Style::new().bright_black(),
            code: Style::new().bright_yellow(),
            link: Style::new().bright_blue(),
            list_marker: Style::new().bright_green().bold(),
            rule: Style::new().bright_black(),
            table_header: Style::new().bright_white().bold(),
            footnote: Style::new().bright_black(),
        }
    }

    pub fn mono() -> Self {
        Self {
            heading: Style::new(),
            block_quote: Style::new(),
            quote_bar: Style::new(),
            code: Style::new(),
            link: Style::new(),
            list_marker: Style::new(),
            rule: Style::new(),
            table_header: Style::new(),
            footnote: Style::new(),
        }
    }

    pub fn catputtin() -> Self {
        Self {
            heading: Style::new().bright_yellow().bold(),
            block_quote: Style::new().bright_magenta(),
            quote_bar: Style::new().bright_black(),
            code: Style::new().bright_cyan(),
            link: Style::new().bright_blue(),
            list_marker: Style::new().bright_green().bold(),
            rule: Style::new().bright_black(),
            table_header: Style::new().bright_white().bold(),
            footnote: Style::new().bright_black(),
        }
    }

    pub fn dracura() -> Self {
        Self {
            heading: Style::new().bright_magenta().bold(),
            block_quote: Style::new().bright_cyan(),
            quote_bar: Style::new().bright_black(),
            code: Style::new().bright_yellow(),
            link: Style::new().bright_blue(),
            list_marker: Style::new().bright_green().bold(),
            rule: Style::new().bright_black(),
            table_header: Style::new().bright_white().bold(),
            footnote: Style::new().bright_black(),
        }
    }

    pub fn solarized() -> Self {
        Self {
            heading: Style::new().bright_cyan().bold(),
            block_quote: Style::new().bright_green(),
            quote_bar: Style::new().bright_black(),
            code: Style::new().bright_yellow(),
            link: Style::new().bright_blue(),
            list_marker: Style::new().bright_magenta().bold(),
            rule: Style::new().bright_black(),
            table_header: Style::new().bright_white().bold(),
            footnote: Style::new().bright_black(),
        }
    }

    pub fn nord() -> Self {
        Self {
            heading: Style::new().bright_blue().bold(),
            block_quote: Style::new().bright_cyan(),
            quote_bar: Style::new().bright_black(),
            code: Style::new().bright_white(),
            link: Style::new().bright_blue(),
            list_marker: Style::new().bright_green().bold(),
            rule: Style::new().bright_black(),
            table_header: Style::new().bright_white().bold(),
            footnote: Style::new().bright_black(),
        }
    }

    pub fn ayu() -> Self {
        Self {
            heading: Style::new().bright_yellow().bold(),
            block_quote: Style::new().bright_magenta(),
            quote_bar: Style::new().bright_black(),
            code: Style::new().bright_cyan(),
            link: Style::new().bright_blue(),
            list_marker: Style::new().bright_green().bold(),
            rule: Style::new().bright_black(),
            table_header: Style::new().bright_white().bold(),
            footnote: Style::new().bright_black(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::glow()
    }
}

pub struct TerminalEmitter<I> {
    iter: I,
    theme: Theme,
    end_newline: bool,
    in_non_writing_block: bool,
    in_heading: bool,
    heading_level: Option<HeadingLevel>,
    h1_started: bool,
    in_block_quote: bool,
    in_code_block: bool,
    in_link: bool,
    in_table_head: bool,
    in_table: bool,
    in_table_cell: bool,
    list_stack: Vec<ListState>,
    table_cell_index: usize,
    table_header_row: Option<Vec<String>>,
    table_rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_cell: String,
    pending_list_marker: Option<String>,
    numbers: HashMap<String, usize>,
    link_stack: Vec<String>,
}

impl<'a, I> TerminalEmitter<I>
where
    I: Iterator<Item = Event<'a>>,
{
    pub fn new(iter: I, theme: Theme) -> Self {
        Self {
            iter,
            theme,
            end_newline: false,
            in_non_writing_block: false,
            in_heading: false,
            heading_level: None,
            h1_started: false,
            in_block_quote: false,
            in_code_block: false,
            in_link: false,
            in_table_head: false,
            in_table: false,
            in_table_cell: false,
            list_stack: Vec::new(),
            table_cell_index: 0,
            table_header_row: None,
            table_rows: Vec::new(),
            current_row: Vec::new(),
            current_cell: String::new(),
            pending_list_marker: None,
            numbers: HashMap::new(),
            link_stack: Vec::new(),
        }
    }

    pub fn run(&mut self) -> String {
        let mut out = String::new();
        while let Some(event) = self.iter.next() {
            match event {
                Event::Start(tag) => self.start_tag(&mut out, tag),
                Event::End(tag) => self.end_tag(&mut out, tag),
                Event::Text(text) => {
                    if !self.in_non_writing_block {
                        if self.in_table_cell {
                            self.push_table_text(&text);
                        } else {
                            self.flush_pending_marker(&mut out);
                            self.push_text(&mut out, &text);
                        }
                        self.end_newline = text.ends_with('\n');
                    }
                }
                Event::Code(text) => {
                    if self.in_table_cell {
                        self.push_table_text(&text);
                    } else {
                        self.flush_pending_marker(&mut out);
                        let styled = format!("{}", text.style(self.theme.code));
                        out.push('`');
                        out.push_str(&styled);
                        out.push('`');
                    }
                }
                Event::InlineMath(text) => {
                    if self.in_table_cell {
                        self.push_table_text("$");
                        self.push_table_text(&text);
                        self.push_table_text("$");
                    } else {
                        self.flush_pending_marker(&mut out);
                        out.push('$');
                        self.push_text(&mut out, &text);
                        out.push('$');
                    }
                }
                Event::DisplayMath(text) => {
                    if self.in_table_cell {
                        self.push_table_text("$$");
                        self.push_table_text(&text);
                        self.push_table_text("$$");
                    } else {
                        self.flush_pending_marker(&mut out);
                        out.push_str("$$");
                        self.push_text(&mut out, &text);
                        out.push_str("$$");
                    }
                }
                Event::Html(_) | Event::InlineHtml(_) => {
                    // Skip raw HTML for terminal output.
                }
                Event::SoftBreak => {
                    if self.in_table_cell {
                        self.push_table_text(" ");
                    } else {
                        self.flush_pending_marker(&mut out);
                        out.push(' ');
                    }
                }
                Event::HardBreak => {
                    if self.in_table_cell {
                        self.push_table_text(" ");
                    } else {
                        self.flush_pending_marker(&mut out);
                        out.push('\n');
                        self.end_newline = true;
                    }
                }
                Event::Rule => {
                    if self.in_table_cell {
                        self.push_table_text("—");
                    } else {
                        if !self.end_newline {
                            out.push('\n');
                        }
                        let rule = "----------------------------------------\n";
                        out.push_str(&format!("{}", rule.style(self.theme.rule)));
                        self.end_newline = true;
                    }
                }
                Event::FootnoteReference(name) => {
                    let len = self.numbers.len() + 1;
                    let number = *self.numbers.entry(name.to_string()).or_insert(len);
                    if self.in_table_cell {
                        self.push_table_text(&format!("[^{number}]"));
                    } else {
                        self.flush_pending_marker(&mut out);
                        out.push_str(&format!("[^{number}]"));
                    }
                }
                Event::TaskListMarker(true) => {
                    if self.in_table_cell {
                        self.push_table_text("[x] ");
                    } else {
                        self.render_task_marker(&mut out, true);
                    }
                }
                Event::TaskListMarker(false) => {
                    if self.in_table_cell {
                        self.push_table_text("[ ] ");
                    } else {
                        self.render_task_marker(&mut out, false);
                    }
                }
            }
        }
        out
    }

    fn start_tag(&mut self, out: &mut String, tag: Tag) {
        match tag {
            Tag::HtmlBlock => (),
            Tag::Paragraph => {
                if !self.end_newline {
                    out.push('\n');
                }
            }
            Tag::Heading { level, .. } => {
                if !self.end_newline {
                    out.push('\n');
                }
                self.in_heading = true;
                self.heading_level = Some(level);
                self.h1_started = false;
                if level != HeadingLevel::H1 {
                    out.push_str(&"#".repeat(level as usize));
                    out.push(' ');
                }
            }
            Tag::Table(_) => {
                self.table_cell_index = 0;
                if !self.end_newline {
                    out.push('\n');
                }
                self.in_table = true;
                self.table_header_row = None;
                self.table_rows.clear();
            }
            Tag::TableHead => {
                self.table_cell_index = 0;
                self.in_table_head = true;
            }
            Tag::TableRow => {
                self.table_cell_index = 0;
                self.current_row.clear();
                if !self.end_newline {
                    out.push('\n');
                }
            }
            Tag::TableCell => {
                self.in_table_cell = true;
                self.current_cell.clear();
            }
            Tag::BlockQuote(kind) => {
                let label = match kind {
                    None => None,
                    Some(BlockQuoteKind::Note) => Some("[!NOTE]"),
                    Some(BlockQuoteKind::Tip) => Some("[!TIP]"),
                    Some(BlockQuoteKind::Important) => Some("[!IMPORTANT]"),
                    Some(BlockQuoteKind::Warning) => Some("[!WARNING]"),
                    Some(BlockQuoteKind::Caution) => Some("[!CAUTION]"),
                };
                if out.ends_with("\n\n") {
                    out.pop();
                }
                if !self.end_newline {
                    out.push('\n');
                }
                self.in_block_quote = true;
                out.push_str(&format!("{}", "│".style(self.theme.quote_bar)));
                if let Some(label) = label {
                    out.push(' ');
                    out.push_str(&format!("{}", label.style(self.theme.block_quote)));
                    out.push('\n');
                    out.push_str(&format!("{}", "│".style(self.theme.quote_bar)));
                    out.push(' ');
                } else {
                    out.push(' ');
                }
            }
            Tag::CodeBlock(info) => {
                if !self.end_newline {
                    out.push('\n');
                }
                self.in_code_block = true;
                match info {
                    CodeBlockKind::Fenced(lang) => {
                        out.push_str("```");
                        out.push_str(lang.split(' ').next().unwrap_or(""));
                        out.push('\n');
                    }
                    CodeBlockKind::Indented => {
                        out.push_str("```\n");
                    }
                }
            }
            Tag::List(Some(start)) => {
                self.list_stack.push(ListState::Ordered {
                    index: start as usize,
                });
            }
            Tag::List(None) => {
                self.list_stack.push(ListState::Unordered);
            }
            Tag::Item => {
                if !self.end_newline {
                    out.push('\n');
                }
                match self.list_stack.last() {
                    Some(ListState::Ordered { index }) => {
                        let marker = format!("{}. ", index);
                        self.pending_list_marker = Some(format!("{}", marker.style(self.theme.list_marker)));
                    }
                    Some(ListState::Unordered) | None => {
                        self.pending_list_marker = Some(format!("{}", "• ".style(self.theme.list_marker)));
                    }
                }
            }
            Tag::DefinitionList => {
                if !self.end_newline {
                    out.push('\n');
                }
            }
            Tag::DefinitionListTitle => {
                if !self.end_newline {
                    out.push('\n');
                }
            }
            Tag::DefinitionListDefinition => {
                if !self.end_newline {
                    out.push('\n');
                }
                out.push_str(": ");
            }
            Tag::Subscript => {
                out.push('~');
            }
            Tag::Superscript => {
                out.push('^');
            }
            Tag::Emphasis => {
                out.push_str(&format!("{}", "*".style(self.theme.code)));
            }
            Tag::Strong => {
                out.push_str(&format!("{}", "**".style(self.theme.code)));
            }
            Tag::Strikethrough => {
                out.push_str(&format!("{}", "~~".style(self.theme.code)));
            }
            Tag::Link {
                link_type: LinkType::Email,
                dest_url,
                ..
            } => {
                self.in_link = true;
                self.link_stack.push(format!("mailto:{dest_url}"));
            }
            Tag::Link { dest_url, .. } => {
                self.in_link = true;
                self.link_stack.push(dest_url.to_string());
            }
            Tag::Image { dest_url, .. } => {
                out.push_str(&format!("{}", "[image: ".style(self.theme.code)));
                self.raw_text(out);
                out.push_str(&format!("{}", "]".style(self.theme.code)));
                if !dest_url.is_empty() {
                    out.push_str(" (");
                    out.push_str(&format!("{}", dest_url.style(self.theme.link)));
                    out.push(')');
                }
            }
            Tag::FootnoteDefinition(name) => {
                if !self.end_newline {
                    out.push('\n');
                }
                let len = self.numbers.len() + 1;
                let number = *self.numbers.entry(name.to_string()).or_insert(len);
                let label = format!("[^{number}]: ");
                out.push_str(&format!("{}", label.style(self.theme.footnote)));
            }
            Tag::MetadataBlock(_) => {
                self.in_non_writing_block = true;
            }
        }
    }

    fn end_tag(&mut self, out: &mut String, tag: TagEnd) {
        match tag {
            TagEnd::HtmlBlock => {}
            TagEnd::Paragraph => {
                out.push('\n');
                out.push('\n');
                self.end_newline = true;
            }
            TagEnd::Heading(level) => {
                if level == HeadingLevel::H1 {
                    let style = self.theme.heading.bold().on_bright_black();
                    out.push_str(&format!("{}", " ".style(style)));
                }
                out.push('\n');
                out.push('\n');
                self.in_heading = false;
                self.heading_level = None;
                self.h1_started = false;
                self.end_newline = true;
            }
            TagEnd::Table => {
                if self.in_table {
                    if let Some(header) = self.table_header_row.take() {
                        self.table_rows.insert(0, header);
                    }
                    self.render_table(out);
                }
                out.push('\n');
                self.in_table = false;
                self.end_newline = true;
            }
            TagEnd::TableHead => {
                self.in_table_head = false;
                self.end_newline = true;
            }
            TagEnd::TableRow => {
                if self.in_table_cell {
                    self.current_row.push(self.current_cell.trim().to_string());
                    self.in_table_cell = false;
                }
                if self.in_table_head {
                    self.table_header_row = Some(self.current_row.clone());
                } else {
                    self.table_rows.push(self.current_row.clone());
                }
                self.end_newline = true;
            }
            TagEnd::TableCell => {
                self.table_cell_index += 1;
                if self.in_table_cell {
                    self.current_row.push(self.current_cell.trim().to_string());
                    self.in_table_cell = false;
                }
            }
            TagEnd::BlockQuote(_) => {
                out.push('\n');
                self.in_block_quote = false;
                self.end_newline = true;
            }
            TagEnd::CodeBlock => {
                out.push_str("```\n\n");
                self.in_code_block = false;
                self.end_newline = true;
            }
            TagEnd::List(true) | TagEnd::List(false) => {
                let _ = self.list_stack.pop();
                out.push('\n');
                self.end_newline = true;
            }
            TagEnd::Item => {
                out.push('\n');
                self.pending_list_marker = None;
                if let Some(ListState::Ordered { index }) = self.list_stack.last_mut() {
                    *index += 1;
                }
                self.end_newline = true;
            }
            TagEnd::DefinitionList => {
                out.push('\n');
                self.end_newline = true;
            }
            TagEnd::DefinitionListTitle => {
                out.push('\n');
                self.end_newline = true;
            }
            TagEnd::DefinitionListDefinition => {
                out.push('\n');
                self.end_newline = true;
            }
            TagEnd::Emphasis => {
                out.push_str(&format!("{}", "*".style(self.theme.code)));
            }
            TagEnd::Superscript => {
                out.push('^');
            }
            TagEnd::Subscript => {
                out.push('~');
            }
            TagEnd::Strong => {
                out.push_str(&format!("{}", "**".style(self.theme.code)));
            }
            TagEnd::Strikethrough => {
                out.push_str(&format!("{}", "~~".style(self.theme.code)));
            }
            TagEnd::Link => {
                if let Some(dest) = self.link_stack.pop()
                    && !dest.is_empty()
                {
                    out.push_str(" (");
                    out.push_str(&format!(
                        "{}",
                        dest.style(self.theme.link.underline().dimmed())
                    ));
                    out.push(')');
                }
                self.in_link = false;
            }
            TagEnd::Image => {}
            TagEnd::FootnoteDefinition => {
                out.push('\n');
                self.end_newline = true;
            }
            TagEnd::MetadataBlock(_) => {
                self.in_non_writing_block = false;
            }
        }
    }

    fn push_text(&mut self, out: &mut String, text: &str) {
        let style = if self.in_code_block {
            Some(self.theme.code)
        } else if self.in_heading {
            if self.heading_level == Some(HeadingLevel::H1) {
                Some(self.theme.heading.bold().on_bright_black())
            } else {
                Some(self.theme.heading.bold())
            }
        } else if self.in_block_quote {
            Some(self.theme.block_quote)
        } else if self.in_table_head {
            Some(self.theme.table_header)
        } else if self.in_link {
            Some(self.theme.link.bold())
        } else {
            None
        };
        if let Some(style) = style {
            if self.heading_level == Some(HeadingLevel::H1) && !self.h1_started {
                out.push_str(&format!("{}", " ".style(style)));
                self.h1_started = true;
            }
            out.push_str(&format!("{}", text.style(style)));
        } else {
            out.push_str(text);
        }
    }

    fn push_table_text(&mut self, text: &str) {
        self.current_cell.push_str(text);
    }

    fn flush_pending_marker(&mut self, out: &mut String) {
        if let Some(marker) = self.pending_list_marker.take() {
            out.push_str(&marker);
        }
    }

    fn render_task_marker(&mut self, out: &mut String, checked: bool) {
        if let Some(marker) = self.pending_list_marker.take() {
            if marker.contains('•') {
                out.push_str("  ");
            } else {
                out.push_str(&marker);
            }
        }
        if checked {
            out.push_str("[x] ");
        } else {
            out.push_str("[ ] ");
        }
    }

    fn render_table(&self, out: &mut String) {
        if self.table_rows.is_empty() {
            return;
        }
        let mut widths = Vec::new();
        for row in &self.table_rows {
            for (i, cell) in row.iter().enumerate() {
                if widths.len() <= i {
                    widths.push(0usize);
                }
                let len = cell.chars().count();
                if len > widths[i] {
                    widths[i] = len;
                }
            }
        }
        let indent = " ";
        let header = &self.table_rows[0];
        out.push_str(indent);
        for (i, cell) in header.iter().enumerate() {
            if i > 0 {
                out.push_str(" │ ");
            }
            out.push_str(&pad(cell, widths[i]));
        }
        out.push('\n');
        out.push_str(indent);
        for (i, width) in widths.iter().enumerate() {
            if i > 0 {
                out.push('┼');
            }
            out.push_str(&"─".repeat(*width + if i > 0 { 2 } else { 0 }));
        }
        out.push('\n');
        for row in self.table_rows.iter().skip(1) {
            out.push_str(indent);
            for (i, cell) in row.iter().enumerate() {
                if i > 0 {
                    out.push_str(" │ ");
                }
                out.push_str(&pad(cell, widths[i]));
            }
            out.push('\n');
        }
    }

    #[allow(clippy::while_let_on_iterator)]
    fn raw_text(&mut self, out: &mut String) {
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
                Event::Html(_) | Event::InlineHtml(_) => {}
                Event::InlineMath(text)
                | Event::DisplayMath(text)
                | Event::Code(text)
                | Event::Text(text) => {
                    out.push_str(&text);
                    self.end_newline = text.ends_with('\n');
                }
                Event::SoftBreak | Event::HardBreak | Event::Rule => {
                    out.push(' ');
                }
                Event::FootnoteReference(name) => {
                    let len = self.numbers.len() + 1;
                    let number = *self.numbers.entry(name.to_string()).or_insert(len);
                    out.push_str(&format!("[^{number}]"));
                }
                Event::TaskListMarker(true) => out.push_str("[x]"),
                Event::TaskListMarker(false) => out.push_str("[ ]"),
            }
        }
    }
}

fn pad(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        text.to_string()
    } else {
        format!("{}{}", text, " ".repeat(width - len))
    }
}
