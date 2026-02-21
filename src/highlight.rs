use std::sync::LazyLock;

use ansi_to_tui::IntoText;
use ratatui::text::Line;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

pub fn highlight_code(code: &str, lang: Option<&str>) -> Vec<Line<'static>> {
    let ss = &*SYNTAX_SET;
    let syntax = lang
        .and_then(|l| ss.find_syntax_by_token(l))
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let theme = &THEME_SET.themes["base16-ocean.dark"];
    let mut h = HighlightLines::new(syntax, theme);

    let mut ansi = String::new();
    for line in code.lines() {
        let ranges = h.highlight_line(line, ss).unwrap_or_default();
        ansi.push_str(&as_24_bit_terminal_escaped(&ranges, false));
        ansi.push('\n');
    }
    ansi.push_str("\x1b[0m");

    ansi.into_text()
        .map(|t| t.lines.into_iter().collect())
        .unwrap_or_else(|_| {
            code.lines()
                .map(|l| Line::raw(l.to_string()))
                .collect()
        })
}
