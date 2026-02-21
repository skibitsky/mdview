use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};

use crate::highlight::highlight_code;

pub fn render_markdown(input: &str, width: u16) -> Text<'static> {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(input, opts);
    let mut renderer = Renderer::new(width);
    renderer.process(parser);
    Text::from(renderer.lines)
}

struct ListState {
    ordered: bool,
    counter: u64,
}

struct Renderer {
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    style_stack: Vec<Style>,
    list_stack: Vec<ListState>,
    blockquote_depth: usize,
    in_code_block: bool,
    code_lang: Option<String>,
    code_buf: String,
    in_table: bool,
    table_alignments: Vec<Alignment>,
    table_header: Vec<Vec<Span<'static>>>,
    table_rows: Vec<Vec<Vec<Span<'static>>>>,
    current_cell: Vec<Span<'static>>,
    in_table_header: bool,
    link_url: String,
    item_paragraph_count: usize,
    width: u16,
}

impl Renderer {
    fn new(width: u16) -> Self {
        Self {
            lines: Vec::new(),
            spans: Vec::new(),
            style_stack: vec![Style::default()],
            list_stack: Vec::new(),
            blockquote_depth: 0,
            in_code_block: false,
            code_lang: None,
            code_buf: String::new(),
            in_table: false,
            table_alignments: Vec::new(),
            table_header: Vec::new(),
            table_rows: Vec::new(),
            current_cell: Vec::new(),
            in_table_header: false,
            link_url: String::new(),
            item_paragraph_count: 0,
            width,
        }
    }

    fn current_style(&self) -> Style {
        self.style_stack.last().copied().unwrap_or_default()
    }

    fn push_style(&mut self, modifier: fn(Style) -> Style) {
        let new = modifier(self.current_style());
        self.style_stack.push(new);
    }

    fn pop_style(&mut self) {
        if self.style_stack.len() > 1 {
            self.style_stack.pop();
        }
    }

    fn flush_line(&mut self) {
        if !self.spans.is_empty() {
            let spans = std::mem::take(&mut self.spans);
            self.lines.push(Line::from(spans));
        }
    }

    fn push_blank(&mut self) {
        self.flush_line();
        self.lines.push(Line::default());
    }

    fn blockquote_prefix(&self) -> Vec<Span<'static>> {
        let mut prefix = Vec::new();
        for _ in 0..self.blockquote_depth {
            prefix.push(Span::styled(
                "│ ",
                Style::default().fg(Color::DarkGray),
            ));
        }
        prefix
    }

    fn list_indent(&self) -> String {
        "  ".repeat(self.list_stack.len().saturating_sub(1))
    }

    fn process(&mut self, parser: Parser) {
        for event in parser {
            match event {
                Event::Start(tag) => self.start_tag(tag),
                Event::End(tag) => self.end_tag(tag),
                Event::Text(text) => self.text(&text),
                Event::Code(code) => self.inline_code(&code),
                Event::SoftBreak => self.soft_break(),
                Event::HardBreak => self.hard_break(),
                Event::Rule => self.rule(),
                Event::TaskListMarker(checked) => self.task_marker(checked),
                Event::Html(html) => self.raw_html(&html),
                Event::InlineHtml(html) => self.inline_raw_html(&html),
                Event::FootnoteReference(label) => self.footnote_ref(&label),
                Event::InlineMath(math) => self.math(&math),
                Event::DisplayMath(math) => self.display_math(&math),
            }
        }
        self.flush_line();
    }

    fn start_tag(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush_line();
                let (color, prefix) = match level {
                    pulldown_cmark::HeadingLevel::H1 => (Color::Cyan, "# "),
                    pulldown_cmark::HeadingLevel::H2 => (Color::Green, "## "),
                    pulldown_cmark::HeadingLevel::H3 => (Color::Yellow, "### "),
                    _ => (Color::White, "#### "),
                };
                let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
                self.style_stack.push(style);
                self.spans.push(Span::styled(prefix.to_string(), style));
            }

            Tag::Paragraph => {
                if self.in_table {
                    return;
                }
                if !self.list_stack.is_empty() {
                    self.item_paragraph_count += 1;
                    if self.item_paragraph_count > 1 {
                        self.flush_line();
                    }
                } else {
                    self.flush_line();
                }
            }

            Tag::BlockQuote(_) => {
                self.flush_line();
                self.blockquote_depth += 1;
            }

            Tag::List(start) => {
                self.flush_line();
                self.list_stack.push(ListState {
                    ordered: start.is_some(),
                    counter: start.unwrap_or(1),
                });
            }

            Tag::Item => {
                self.flush_line();
                self.item_paragraph_count = 0;
                let indent = self.list_indent();
                let mut prefix_spans = self.blockquote_prefix();

                if let Some(list) = self.list_stack.last_mut() {
                    let bullet = if list.ordered {
                        let s = format!("{indent}{}. ", list.counter);
                        list.counter += 1;
                        s
                    } else {
                        let marker = match self.list_stack.len() {
                            1 => "•",
                            2 => "◦",
                            _ => "▪",
                        };
                        format!("{indent}{marker} ")
                    };
                    prefix_spans.push(Span::styled(
                        bullet,
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                self.spans = prefix_spans;
            }

            Tag::Emphasis => self.push_style(|s| s.add_modifier(Modifier::ITALIC)),
            Tag::Strong => self.push_style(|s| s.add_modifier(Modifier::BOLD)),
            Tag::Strikethrough => {
                self.push_style(|s| s.add_modifier(Modifier::CROSSED_OUT))
            }

            Tag::Link { dest_url, .. } => {
                self.push_style(|s| s.fg(Color::Blue).add_modifier(Modifier::UNDERLINED));
                self.link_url = dest_url.to_string();
            }

            Tag::CodeBlock(kind) => {
                self.flush_line();
                self.in_code_block = true;
                self.code_lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => {
                        let l = lang.split_whitespace().next().unwrap_or("").to_string();
                        if l.is_empty() { None } else { Some(l) }
                    }
                    _ => None,
                };
                self.code_buf.clear();
            }

            Tag::Table(alignments) => {
                self.flush_line();
                self.in_table = true;
                self.table_alignments = alignments;
                self.table_header.clear();
                self.table_rows.clear();
            }

            Tag::TableHead => {
                self.in_table_header = true;
            }

            Tag::TableRow => {
                if !self.in_table_header {
                    self.table_rows.push(Vec::new());
                }
            }

            Tag::TableCell => {
                self.current_cell.clear();
            }

            _ => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.pop_style();
                self.flush_line();
                self.push_blank();
            }

            TagEnd::Paragraph => {
                if self.in_table {
                    return;
                }
                self.flush_line();
                if self.list_stack.is_empty() {
                    self.push_blank();
                }
            }

            TagEnd::BlockQuote(_) => {
                self.blockquote_depth = self.blockquote_depth.saturating_sub(1);
                self.flush_line();
            }

            TagEnd::List(_) => {
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    self.flush_line();
                    self.push_blank();
                }
            }

            TagEnd::Item => {
                self.flush_line();
            }

            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.pop_style();
            }

            TagEnd::Link => {
                self.pop_style();
                let url = std::mem::take(&mut self.link_url);
                self.spans.push(Span::styled(
                    format!(" ({url})"),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            TagEnd::CodeBlock => {
                self.in_code_block = false;
                let code = std::mem::take(&mut self.code_buf);
                let lang = self.code_lang.take();

                let highlighted = highlight_code(&code, lang.as_deref());
                let prefix = self.blockquote_prefix();

                for line in highlighted {
                    let mut spans = prefix.clone();
                    spans.push(Span::styled("  ", Style::default()));
                    spans.extend(line.spans);
                    self.lines.push(Line::from(spans));
                }
                self.push_blank();
            }

            TagEnd::Table => {
                self.render_table();
                self.in_table = false;
                self.push_blank();
            }

            TagEnd::TableHead => {
                self.in_table_header = false;
            }

            TagEnd::TableRow => {}

            TagEnd::TableCell => {
                let cell = std::mem::take(&mut self.current_cell);
                if self.in_table_header {
                    self.table_header.push(cell);
                } else if let Some(row) = self.table_rows.last_mut() {
                    row.push(cell);
                }
            }

            _ => {}
        }
    }

    fn text(&mut self, text: &str) {
        if self.in_code_block {
            self.code_buf.push_str(text);
            return;
        }

        if self.in_table {
            self.current_cell
                .push(Span::styled(text.to_string(), self.current_style()));
            return;
        }

        if self.blockquote_depth > 0 && self.spans.is_empty() {
            self.spans = self.blockquote_prefix();
        }

        self.spans
            .push(Span::styled(text.to_string(), self.current_style()));
    }

    fn inline_code(&mut self, code: &str) {
        if self.in_table {
            self.current_cell.push(Span::styled(
                format!("`{code}`"),
                Style::default().bg(Color::Indexed(239)),
            ));
            return;
        }

        self.spans.push(Span::styled(
            format!("`{code}`"),
            Style::default().bg(Color::Indexed(239)),
        ));
    }

    fn soft_break(&mut self) {
        self.spans.push(Span::raw(" "));
    }

    fn hard_break(&mut self) {
        self.flush_line();
        if self.blockquote_depth > 0 {
            self.spans = self.blockquote_prefix();
        }
    }

    fn rule(&mut self) {
        self.flush_line();
        let w = self.width.saturating_sub(2) as usize;
        self.lines.push(Line::styled(
            "─".repeat(w),
            Style::default().fg(Color::DarkGray),
        ));
        self.push_blank();
    }

    fn task_marker(&mut self, checked: bool) {
        let marker = if checked { "[✓] " } else { "[ ] " };
        self.spans.push(Span::styled(
            marker.to_string(),
            Style::default().fg(if checked { Color::Green } else { Color::DarkGray }),
        ));
    }

    fn raw_html(&mut self, html: &str) {
        self.flush_line();
        for line in html.lines() {
            self.lines.push(Line::styled(
                line.to_string(),
                Style::default().add_modifier(Modifier::DIM),
            ));
        }
    }

    fn inline_raw_html(&mut self, html: &str) {
        self.spans.push(Span::styled(
            html.to_string(),
            Style::default().add_modifier(Modifier::DIM),
        ));
    }

    fn footnote_ref(&mut self, label: &str) {
        self.spans.push(Span::styled(
            format!("[{label}]"),
            Style::default().fg(Color::Cyan),
        ));
    }

    fn math(&mut self, math: &str) {
        self.spans.push(Span::styled(
            math.to_string(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC),
        ));
    }

    fn display_math(&mut self, math: &str) {
        self.flush_line();
        self.lines.push(Line::styled(
            math.to_string(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC),
        ));
        self.push_blank();
    }

    fn render_table(&mut self) {
        let num_cols = self.table_header.len();
        if num_cols == 0 {
            return;
        }

        let natural_widths: Vec<usize> = (0..num_cols)
            .map(|i| {
                let header_w = cell_text_width(&self.table_header[i]);
                let max_body = self
                    .table_rows
                    .iter()
                    .map(|row| row.get(i).map_or(0, |c| cell_text_width(c)))
                    .max()
                    .unwrap_or(0);
                header_w.max(max_body).max(3)
            })
            .collect();

        let col_widths = budget_columns(&natural_widths, self.width as usize);
        let border_style = Style::default().fg(Color::DarkGray);

        self.lines.push(build_border(&col_widths, '┌', '┬', '┐', border_style));

        let header_lines = build_wrapped_row(
            &self.table_header,
            &col_widths,
            &self.table_alignments,
            border_style,
            Style::default().add_modifier(Modifier::BOLD),
            None,
            5,
        );
        self.lines.extend(header_lines);

        self.lines.push(build_border(&col_widths, '├', '┼', '┤', border_style));

        let zebra_bg = Color::Indexed(235);
        for (row_idx, row) in self.table_rows.iter().enumerate() {
            let row_bg = if row_idx % 2 == 1 { Some(zebra_bg) } else { None };
            let row_lines = build_wrapped_row(
                row,
                &col_widths,
                &self.table_alignments,
                border_style,
                Style::default(),
                row_bg,
                5,
            );
            self.lines.extend(row_lines);
        }

        self.lines.push(build_border(&col_widths, '└', '┴', '┘', border_style));
    }
}

fn cell_text_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.width()).sum()
}

fn budget_columns(natural: &[usize], terminal_width: usize) -> Vec<usize> {
    let num_cols = natural.len();
    let chrome = num_cols * 3 + 1;
    let available = terminal_width.saturating_sub(chrome);

    let total_natural: usize = natural.iter().sum();
    if total_natural <= available {
        return natural.to_vec();
    }

    let min_col: usize = 5;
    let mut widths = vec![0usize; num_cols];
    let mut locked = vec![false; num_cols];
    let mut budget = available;

    for i in 0..num_cols {
        if natural[i] <= min_col {
            widths[i] = min_col.min(budget);
            budget = budget.saturating_sub(widths[i]);
            locked[i] = true;
        }
    }

    loop {
        let unlocked: Vec<usize> = (0..num_cols).filter(|i| !locked[*i]).collect();
        if unlocked.is_empty() {
            break;
        }

        let fair = budget / unlocked.len();
        let mut newly_locked = false;

        for &i in &unlocked {
            if natural[i] <= fair {
                widths[i] = natural[i];
                budget = budget.saturating_sub(natural[i]);
                locked[i] = true;
                newly_locked = true;
            }
        }

        if !newly_locked {
            let remaining: Vec<usize> = (0..num_cols).filter(|i| !locked[*i]).collect();
            let share = budget / remaining.len().max(1);
            let mut leftover = budget % remaining.len().max(1);
            for &i in &remaining {
                let extra = if leftover > 0 { leftover -= 1; 1 } else { 0 };
                widths[i] = share + extra;
            }
            break;
        }
    }

    widths
}

fn build_border(widths: &[usize], left: char, mid: char, right: char, style: Style) -> Line<'static> {
    let mut s = String::new();
    s.push(left);
    for (i, &w) in widths.iter().enumerate() {
        for _ in 0..w + 2 {
            s.push('─');
        }
        s.push(if i + 1 < widths.len() { mid } else { right });
    }
    Line::styled(s, style)
}

struct StyledWord {
    chars: Vec<(char, usize, Style)>,
    width: usize,
    trailing_space: bool,
}

fn wrap_cell_spans(
    spans: &[Span<'static>],
    max_width: usize,
    max_lines: usize,
    base_style: Style,
) -> Vec<Vec<Span<'static>>> {
    let flat = flatten_to_styled_chars(spans, base_style);
    let total_width: usize = flat.iter().map(|(_, w, _)| w).sum();

    if total_width <= max_width {
        let styled: Vec<Span<'static>> = spans
            .iter()
            .map(|s| Span::styled(s.content.clone().into_owned(), base_style.patch(s.style)))
            .collect();
        return vec![styled];
    }

    let words = split_into_words(&flat);
    let mut lines: Vec<Vec<Span<'static>>> = Vec::new();
    let mut cur_chars: Vec<(char, Style)> = Vec::new();
    let mut cur_width: usize = 0;

    for word in &words {
        if cur_width > 0 && cur_width + word.width > max_width {
            if lines.len() + 1 >= max_lines {
                return finish_truncated(lines, &cur_chars, max_width);
            }
            lines.push(coalesce_chars(&cur_chars));
            cur_chars.clear();
            cur_width = 0;
        }

        if word.width > max_width {
            for &(ch, cw, style) in &word.chars {
                if cur_width + cw > max_width {
                    if lines.len() + 1 >= max_lines {
                        return finish_truncated(lines, &cur_chars, max_width);
                    }
                    lines.push(coalesce_chars(&cur_chars));
                    cur_chars.clear();
                    cur_width = 0;
                }
                cur_chars.push((ch, style));
                cur_width += cw;
            }
            if word.trailing_space && cur_width < max_width {
                cur_chars.push((' ', word.chars.last().map(|c| c.2).unwrap_or_default()));
                cur_width += 1;
            }
            continue;
        }

        for &(ch, _, style) in &word.chars {
            cur_chars.push((ch, style));
        }
        cur_width += word.width;

        if word.trailing_space && cur_width < max_width {
            cur_chars.push((' ', word.chars.last().map(|c| c.2).unwrap_or_default()));
            cur_width += 1;
        }
    }

    if !cur_chars.is_empty() {
        lines.push(coalesce_chars(&cur_chars));
    }

    if lines.is_empty() {
        lines.push(Vec::new());
    }

    lines
}

fn finish_truncated(
    mut lines: Vec<Vec<Span<'static>>>,
    cur_chars: &[(char, Style)],
    max_width: usize,
) -> Vec<Vec<Span<'static>>> {
    let coalesced = coalesce_chars(cur_chars);
    let mut truncated = truncate_line_spans(&coalesced, max_width.saturating_sub(1));
    truncated.push(Span::styled("…", Style::default().fg(Color::DarkGray)));
    lines.push(truncated);
    lines
}

fn flatten_to_styled_chars(spans: &[Span<'static>], base_style: Style) -> Vec<(char, usize, Style)> {
    let mut out = Vec::new();
    for span in spans {
        let style = base_style.patch(span.style);
        for ch in span.content.chars() {
            let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            out.push((ch, w, style));
        }
    }
    out
}

fn split_into_words(chars: &[(char, usize, Style)]) -> Vec<StyledWord> {
    let mut words = Vec::new();
    let mut current: Vec<(char, usize, Style)> = Vec::new();
    let mut width = 0;

    for &(ch, cw, style) in chars {
        if ch == ' ' {
            if !current.is_empty() {
                words.push(StyledWord {
                    chars: std::mem::take(&mut current),
                    width,
                    trailing_space: true,
                });
                width = 0;
            }
        } else {
            current.push((ch, cw, style));
            width += cw;
        }
    }

    if !current.is_empty() {
        words.push(StyledWord {
            chars: current,
            width,
            trailing_space: false,
        });
    }

    words
}

fn coalesce_chars(chars: &[(char, Style)]) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let mut cur_style = Style::default();

    for &(ch, style) in chars {
        if !buf.is_empty() && style != cur_style {
            spans.push(Span::styled(std::mem::take(&mut buf), cur_style));
        }
        cur_style = style;
        buf.push(ch);
    }

    if !buf.is_empty() {
        spans.push(Span::styled(buf, cur_style));
    }

    spans
}

fn truncate_line_spans(spans: &[Span<'static>], budget: usize) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    let mut remaining = budget;

    for span in spans {
        if remaining == 0 {
            break;
        }
        let w = span.width();
        if w <= remaining {
            out.push(span.clone());
            remaining -= w;
        } else {
            let mut truncated = String::new();
            let mut used = 0;
            for ch in span.content.chars() {
                let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                if used + cw > remaining {
                    break;
                }
                truncated.push(ch);
                used += cw;
            }
            out.push(Span::styled(truncated, span.style));
            break;
        }
    }

    out
}

fn build_empty_row(
    widths: &[usize],
    border_style: Style,
    bg_style: Option<Style>,
) -> Line<'static> {
    let pad_style = bg_style.unwrap_or_default();
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled("│", border_style));
    for &w in widths {
        spans.push(Span::styled(" ".repeat(w + 2), pad_style));
        spans.push(Span::styled("│", border_style));
    }
    Line::from(spans)
}

fn build_wrapped_row(
    cells: &[Vec<Span<'static>>],
    widths: &[usize],
    alignments: &[Alignment],
    border_style: Style,
    cell_base_style: Style,
    row_bg: Option<Color>,
    max_lines: usize,
) -> Vec<Line<'static>> {
    let bg_style = row_bg.map(|c| Style::default().bg(c));

    let wrapped: Vec<Vec<Vec<Span<'static>>>> = (0..widths.len())
        .map(|i| {
            let cell = cells.get(i).map(|c| c.as_slice()).unwrap_or(&[]);
            wrap_cell_spans(cell, widths[i], max_lines, cell_base_style)
        })
        .collect();

    let num_visual_rows = wrapped.iter().map(|w| w.len()).max().unwrap_or(1);

    let mut output_lines = Vec::new();
    let multiline = num_visual_rows > 1;

    if multiline {
        output_lines.push(build_empty_row(widths, border_style, None));
    }

    for vrow in 0..num_visual_rows {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled("│", border_style));

        for (i, &max_w) in widths.iter().enumerate() {
            let cell_line = wrapped[i].get(vrow);
            let content_len = cell_line.map_or(0, |s| s.iter().map(|sp| sp.width()).sum());
            let padding = max_w.saturating_sub(content_len);

            let align = alignments.get(i).copied().unwrap_or(Alignment::None);
            let (pad_left, pad_right) = if vrow == 0 {
                match align {
                    Alignment::Center => (padding / 2, padding - padding / 2),
                    Alignment::Right => (padding, 0),
                    _ => (0, padding),
                }
            } else {
                (0, padding)
            };

            let pad_style = bg_style.unwrap_or_default();
            spans.push(Span::styled(" ", pad_style));

            if pad_left > 0 {
                spans.push(Span::styled(" ".repeat(pad_left), pad_style));
            }

            if let Some(cell_spans) = cell_line {
                for span in cell_spans {
                    let mut s = span.style;
                    if let Some(bg) = bg_style {
                        if let (None, Some(bg_color)) = (span.style.bg, bg.bg) {
                            s = s.bg(bg_color);
                        }
                    }
                    spans.push(Span::styled(span.content.clone().into_owned(), s));
                }
            }

            if pad_right > 0 {
                spans.push(Span::styled(" ".repeat(pad_right), pad_style));
            }

            spans.push(Span::styled(" ", pad_style));
            spans.push(Span::styled("│", border_style));
        }

        output_lines.push(Line::from(spans));
    }

    if multiline {
        output_lines.push(build_empty_row(widths, border_style, None));
    }

    output_lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_to_plain(text: &Text) -> String {
        text.lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn load_fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to load fixture {name}: {e}"))
    }

    // --- Headings ---

    #[test]
    fn test_heading_prefixes() {
        let md = load_fixture("headings.md");
        let text = render_markdown(&md, 80);
        let plain = text_to_plain(&text);

        assert!(plain.contains("# Heading 1"));
        assert!(plain.contains("## Heading 2"));
        assert!(plain.contains("### Heading 3"));
        assert!(plain.contains("#### Heading 4"));
        assert!(plain.contains("#### Heading 5"));
        assert!(plain.contains("#### Heading 6"));
    }

    // --- Inline ---

    #[test]
    fn test_inline_code_backticks() {
        let md = load_fixture("inline.md");
        let text = render_markdown(&md, 80);
        let plain = text_to_plain(&text);

        assert!(plain.contains("`inline code`"));
    }

    #[test]
    fn test_link_url_appended() {
        let md = load_fixture("inline.md");
        let text = render_markdown(&md, 80);
        let plain = text_to_plain(&text);

        assert!(plain.contains("link (https://example.com)"));
        assert!(plain.contains("another link (https://example.com/path?q=1)"));
    }

    // --- Lists ---

    #[test]
    fn test_tight_list_no_blank_lines() {
        let md = "- Apple\n- Banana\n- Cherry\n";
        let text = render_markdown(md, 80);
        let plain = text_to_plain(&text);

        let item_indices: Vec<usize> = plain
            .lines()
            .enumerate()
            .filter(|(_, l)| l.contains('•'))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(item_indices.len(), 3);

        for pair in item_indices.windows(2) {
            assert_eq!(pair[1] - pair[0], 1, "Tight list items should be consecutive");
        }
    }

    #[test]
    fn test_loose_list_renders_all_items() {
        let md = "- First item\n\n- Second item\n\n- Third item\n";
        let text = render_markdown(md, 80);
        let plain = text_to_plain(&text);

        let item_lines: Vec<&str> = plain.lines().filter(|l| l.contains('•')).collect();
        assert_eq!(item_lines.len(), 3);
        assert!(plain.contains("First item"));
        assert!(plain.contains("Second item"));
        assert!(plain.contains("Third item"));
    }

    #[test]
    fn test_ordered_list_numbering() {
        let md = "1. One\n2. Two\n3. Three\n";
        let text = render_markdown(md, 80);
        let plain = text_to_plain(&text);

        assert!(plain.contains("1. One"));
        assert!(plain.contains("2. Two"));
        assert!(plain.contains("3. Three"));
    }

    #[test]
    fn test_nested_list_indent() {
        let md = "- Parent\n  - Child A\n  - Child B\n";
        let text = render_markdown(md, 80);
        let plain = text_to_plain(&text);

        let parent_line = plain.lines().find(|l| l.contains("Parent")).unwrap();
        let child_line = plain.lines().find(|l| l.contains("Child A")).unwrap();

        let parent_indent = parent_line.len() - parent_line.trim_start().len();
        let child_indent = child_line.len() - child_line.trim_start().len();
        assert!(
            child_indent > parent_indent,
            "Child should be more indented than parent"
        );
    }

    // --- Tables ---

    #[test]
    fn test_table_border_chars() {
        let md = load_fixture("tables.md");
        let text = render_markdown(&md, 80);
        let plain = text_to_plain(&text);

        for ch in ['┌', '┬', '┐', '├', '┼', '┤', '└', '┴', '┘'] {
            assert!(plain.contains(ch), "Missing border char: {ch}");
        }
    }

    #[test]
    fn test_table_column_count() {
        let md = "| A | B | C |\n|---|---|---|\n| 1 | 2 | 3 |\n";
        let text = render_markdown(md, 80);

        let content_lines: Vec<String> = text
            .lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .filter(|l| l.contains('│') && !l.contains('┌') && !l.contains('├') && !l.contains('└'))
            .collect();

        for line in &content_lines {
            let pipe_count = line.chars().filter(|&c| c == '│').count();
            assert_eq!(pipe_count, 4, "3 columns should have 4 │ chars, got {pipe_count} in: {line}");
        }
    }

    #[test]
    fn test_table_fits_width() {
        let md = load_fixture("tables.md");
        let width: u16 = 60;
        let text = render_markdown(&md, width);

        for (i, line) in text.lines.iter().enumerate() {
            let line_width: usize = line.spans.iter().map(|s| s.width()).sum();
            assert!(
                line_width <= width as usize,
                "Line {i} exceeds width {width}: {line_width} chars"
            );
        }
    }

    #[test]
    fn test_table_long_word_wraps() {
        let md = "| Path |\n|------|\n| /very/long/path/to/some/deeply/nested/file.txt |\n";
        let width: u16 = 30;
        let text = render_markdown(md, width);

        for (i, line) in text.lines.iter().enumerate() {
            let line_width: usize = line.spans.iter().map(|s| s.width()).sum();
            assert!(
                line_width <= width as usize,
                "Line {i} overflows at width {width}: {line_width}"
            );
        }
    }

    // --- Code Blocks ---

    #[test]
    fn test_code_block_content() {
        let md = load_fixture("code-blocks.md");
        let text = render_markdown(&md, 80);
        let plain = text_to_plain(&text);

        assert!(plain.contains("println!"));
        assert!(plain.contains("Hello, world!"));
        assert!(plain.contains("def greet"));
        assert!(plain.contains("const x = 42"));
        assert!(plain.contains("Indented code block"));
    }

    // --- Blockquotes ---

    #[test]
    fn test_blockquote_prefix() {
        let md = load_fixture("blockquotes.md");
        let text = render_markdown(&md, 80);
        let plain = text_to_plain(&text);

        assert!(
            plain.lines().any(|l| l.contains("│ ")),
            "Blockquote lines should contain '│ ' prefix"
        );
    }

    // --- Edge Cases ---

    #[test]
    fn test_empty_input() {
        let text = render_markdown("", 80);
        assert!(text.lines.is_empty() || text.lines.iter().all(|l| l.spans.is_empty()));
    }

    #[test]
    fn test_horizontal_rule() {
        let md = "---\n";
        let text = render_markdown(md, 80);
        let plain = text_to_plain(&text);
        assert!(plain.contains('─'), "Horizontal rule should contain '─' characters");
    }

    #[test]
    fn test_heading_style() {
        let text = render_markdown("# Title\n", 80);
        let heading_line = &text.lines[0];
        let title_span = heading_line
            .spans
            .iter()
            .find(|s| s.content.contains("Title"))
            .unwrap();
        assert_eq!(title_span.style.fg, Some(Color::Cyan));
        assert!(title_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_task_list_markers() {
        let md = "- [x] Done\n- [ ] Pending\n";
        let text = render_markdown(md, 80);
        let plain = text_to_plain(&text);

        assert!(plain.contains("[✓]"), "Checked task should have ✓ marker");
        assert!(plain.contains("[ ]"), "Unchecked task should have [ ] marker");
    }

    #[test]
    fn test_edge_cases_fixture() {
        let md = load_fixture("edge-cases.md");
        let text = render_markdown(&md, 40);
        let plain = text_to_plain(&text);

        assert!(plain.contains("Empty Section"));
        assert!(plain.contains('─'), "Horizontal rules should render");

        let long_word_lines: Vec<&str> = plain
            .lines()
            .filter(|l| l.contains("Superlong") || l.contains("something"))
            .collect();
        assert!(!long_word_lines.is_empty(), "Long word should appear in output");

        assert!(plain.contains('🎉'), "Unicode emoji should pass through");
        assert!(plain.contains("中文"), "CJK characters should pass through");
    }

    // --- budget_columns ---

    #[test]
    fn test_budget_natural_fits() {
        let natural = vec![10, 15, 8];
        let result = budget_columns(&natural, 80);
        assert_eq!(result, natural);
    }

    #[test]
    fn test_budget_narrow_terminal() {
        let natural = vec![20, 30, 25];
        let width = 40;
        let result = budget_columns(&natural, width);
        let chrome = natural.len() * 3 + 1;
        let total: usize = result.iter().sum();
        assert!(
            total <= width - chrome,
            "Total {total} exceeds available {} (width {width} - chrome {chrome})",
            width - chrome
        );
    }

    #[test]
    fn test_budget_many_columns_tiny_terminal() {
        let natural = vec![10, 10, 10, 10, 10];
        let width = 30;
        let result = budget_columns(&natural, width);
        let chrome = natural.len() * 3 + 1;
        let total: usize = result.iter().sum();
        assert!(
            total <= width.saturating_sub(chrome),
            "Total {total} must not exceed available {}",
            width.saturating_sub(chrome)
        );
    }

    #[test]
    fn test_budget_single_column() {
        let natural = vec![50];
        let width = 30;
        let result = budget_columns(&natural, width);
        let chrome = 1 * 3 + 1;
        assert_eq!(result[0], width - chrome);
    }

    #[test]
    fn test_budget_small_and_large_mix() {
        let natural = vec![3, 50, 4];
        let width = 40;
        let result = budget_columns(&natural, width);
        let chrome = natural.len() * 3 + 1;
        let total: usize = result.iter().sum();
        assert!(total <= width - chrome);
        assert_eq!(result[0], 5, "Small column locks at min_col");
        assert_eq!(result[2], 5, "Small column locks at min_col");
    }

    // --- wrap_cell_spans ---

    #[test]
    fn test_wrap_fits_one_line() {
        let spans = vec![Span::raw("hello")];
        let result = wrap_cell_spans(&spans, 10, 5, Style::default());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].iter().map(|s| s.content.as_ref()).collect::<String>(), "hello");
    }

    #[test]
    fn test_wrap_at_word_boundary() {
        let spans = vec![Span::raw("hello world foo")];
        let result = wrap_cell_spans(&spans, 10, 5, Style::default());
        assert!(result.len() >= 2, "Should wrap into multiple lines");
    }

    #[test]
    fn test_wrap_truncation_ellipsis() {
        let spans = vec![Span::raw("one two three four five six seven eight nine ten")];
        let result = wrap_cell_spans(&spans, 8, 2, Style::default());
        assert_eq!(result.len(), 2);
        let last_line: String = result.last().unwrap().iter().map(|s| s.content.as_ref()).collect();
        assert!(last_line.contains('…'), "Truncated line should end with ellipsis");
    }

    #[test]
    fn test_wrap_empty_input() {
        let spans: Vec<Span<'static>> = vec![];
        let result = wrap_cell_spans(&spans, 10, 5, Style::default());
        assert_eq!(result.len(), 1, "Empty input should produce one empty line");
    }
}
