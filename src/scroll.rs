use ratatui::text::Text;
use unicode_width::UnicodeWidthStr;

pub struct App {
    pub text: Text<'static>,
    pub scroll: u16,
    pub viewport_height: u16,
    pub render_width: u16,
}

pub fn wrapped_row_count(text: &Text<'_>, width: u16) -> u32 {
    if width == 0 {
        return text.lines.len() as u32;
    }
    let w = width as u32;
    text.lines
        .iter()
        .map(|line| {
            let display: u32 = line
                .spans
                .iter()
                .map(|s| UnicodeWidthStr::width(s.content.as_ref()) as u32)
                .sum();
            if display == 0 { 1 } else { display.div_ceil(w) }
        })
        .sum()
}

impl App {
    pub fn max_scroll(&self) -> u16 {
        let content_height = wrapped_row_count(&self.text, self.render_width)
            .min(u16::MAX as u32) as u16;
        content_height.saturating_sub(self.viewport_height)
    }

    pub fn scroll_down(&mut self, n: u16) {
        self.scroll = self.scroll.saturating_add(n).min(self.max_scroll());
    }

    pub fn scroll_up(&mut self, n: u16) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    pub fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::{Line, Span};

    fn line_of(s: &str) -> Line<'static> {
        Line::from(Span::raw(s.to_string()))
    }

    #[test]
    fn wrapped_row_count_short_line_is_one_row() {
        let text = Text::from(vec![line_of("short")]);
        assert_eq!(wrapped_row_count(&text, 80), 1);
    }

    #[test]
    fn wrapped_row_count_empty_line_is_one_row() {
        let text = Text::from(vec![Line::default()]);
        assert_eq!(wrapped_row_count(&text, 80), 1);
    }

    #[test]
    fn wrapped_row_count_wraps_long_line() {
        let text = Text::from(vec![line_of(&"a".repeat(150))]);
        assert_eq!(wrapped_row_count(&text, 60), 3);
    }

    #[test]
    fn wrapped_row_count_sums_across_lines() {
        let text = Text::from(vec![
            line_of("aaa"),
            line_of(&"b".repeat(9)),
            line_of("ccc"),
        ]);
        assert_eq!(wrapped_row_count(&text, 4), 5);
    }

    #[test]
    fn wrapped_row_count_uses_display_width_for_cjk() {
        let text = Text::from(vec![line_of("你好世界")]);
        assert_eq!(wrapped_row_count(&text, 4), 2);
    }

    #[test]
    fn wrapped_row_count_sums_span_widths_within_line() {
        let text = Text::from(vec![Line::from(vec![
            Span::raw("hello "),
            Span::raw("world"),
        ])]);
        assert_eq!(wrapped_row_count(&text, 4), 3);
    }

    #[test]
    fn wrapped_row_count_width_zero_falls_back_to_line_count() {
        let text = Text::from(vec![line_of("anything"), line_of("here")]);
        assert_eq!(wrapped_row_count(&text, 0), 2);
    }

    #[test]
    fn max_scroll_accounts_for_line_wrapping() {
        let text = Text::from(vec![line_of(&"a".repeat(150))]);
        let app = App {
            text,
            scroll: 0,
            viewport_height: 2,
            render_width: 50,
        };
        assert_eq!(app.max_scroll(), 1);
    }

    #[test]
    fn max_scroll_is_zero_when_content_fits() {
        let text = Text::from(vec![line_of("hello")]);
        let app = App {
            text,
            scroll: 0,
            viewport_height: 10,
            render_width: 80,
        };
        assert_eq!(app.max_scroll(), 0);
    }
}
