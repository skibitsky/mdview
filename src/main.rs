mod highlight;
mod render;
mod watch;

use std::io::{self, Write as _};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Terminal;
use ratatui::text::Text;

use render::render_markdown;

struct App {
    text: Text<'static>,
    scroll: u16,
    viewport_height: u16,
}

impl App {
    fn max_scroll(&self) -> u16 {
        let content_height = (self.text.height() as u32).min(u16::MAX as u32) as u16;
        content_height.saturating_sub(self.viewport_height)
    }

    fn scroll_down(&mut self, n: u16) {
        self.scroll = (self.scroll + n).min(self.max_scroll());
    }

    fn scroll_up(&mut self, n: u16) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll());
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let dump = args.iter().any(|a| a == "--dump");
    let width_override = args.iter()
        .position(|a| a == "-w" || a == "--width")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<u16>().ok());
    let skip_args: Vec<&str> = ["-w", "--width"].into();
    let mut skip_next = false;
    let path = args
        .iter()
        .skip(1)
        .filter(|a| {
            if skip_next { skip_next = false; return false; }
            if skip_args.contains(&a.as_str()) { skip_next = true; return false; }
            !a.starts_with('-')
        })
        .next()
        .map(PathBuf::from)
        .context("Usage: mdview [--dump] [-w WIDTH] <file.md>")?;

    let path = path
        .canonicalize()
        .with_context(|| format!("Cannot resolve path: {}", path.display()))?;

    let mut content = std::fs::read_to_string(&path)
        .with_context(|| format!("Cannot read {}", path.display()))?;

    if dump {
        return dump_text(&content, width_override);
    }

    install_panic_hook();

    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let size = terminal.size()?;
    let mut render_width = size.width;
    let mut app = App {
        text: render_markdown(&content, render_width),
        scroll: 0,
        viewport_height: size.height,
    };

    let (tx, rx) = mpsc::channel();
    let _watcher = watch::setup(&path, tx)?;

    loop {
        terminal.draw(|f| {
            let area = f.area();
            app.viewport_height = area.height;

            let paragraph = Paragraph::new(app.text.clone())
                .wrap(Wrap { trim: false })
                .scroll((app.scroll, 0));

            f.render_widget(paragraph, area);

            let max = app.max_scroll();
            if max > 0 {
                render_scrollbar(f, area, app.scroll, max);
            }
        })?;

        if rx.try_recv().is_ok() {
            while rx.try_recv().is_ok() {}
            if let Ok(new_content) = std::fs::read_to_string(&path) {
                content = new_content;
                render_width = terminal.size()?.width;
                app.text = render_markdown(&content, render_width);
                app.clamp_scroll();
            }
        }

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break
                    }
                    KeyCode::Char('j') | KeyCode::Down => app.scroll_down(1),
                    KeyCode::Char('k') | KeyCode::Up => app.scroll_up(1),
                    KeyCode::Char('d') => app.scroll_down(app.viewport_height / 2),
                    KeyCode::Char('u') => app.scroll_up(app.viewport_height / 2),
                    KeyCode::Char('g') => app.scroll = 0,
                    KeyCode::Char('G') => app.scroll = app.max_scroll(),
                    KeyCode::Char(' ') | KeyCode::PageDown => {
                        app.scroll_down(app.viewport_height.saturating_sub(2))
                    }
                    KeyCode::PageUp => {
                        app.scroll_up(app.viewport_height.saturating_sub(2))
                    }
                    _ => {}
                },
                Event::Resize(w, h) => {
                    app.viewport_height = h;
                    if w != render_width {
                        render_width = w;
                        app.text = render_markdown(&content, render_width);
                    }
                    app.clamp_scroll();
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn dump_text(content: &str, width_override: Option<u16>) -> Result<()> {
    let width = width_override
        .unwrap_or_else(|| crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80));
    let text = render_markdown(content, width);
    let mut out = io::stdout().lock();

    for line in &text.lines {
        for span in &line.spans {
            let mut preamble = String::new();
            let mut has_style = false;

            if let Some(fg) = span.style.fg {
                if let Some(seq) = color_to_ansi_fg(fg) {
                    preamble.push_str(&seq);
                    has_style = true;
                }
            }
            if let Some(bg) = span.style.bg {
                if let Some(seq) = color_to_ansi_bg(bg) {
                    if has_style { preamble.push(';'); }
                    preamble.push_str(&seq);
                    has_style = true;
                }
            }

            let mods = span.style.add_modifier;
            for (flag, code) in [
                (ratatui::style::Modifier::BOLD, "1"),
                (ratatui::style::Modifier::DIM, "2"),
                (ratatui::style::Modifier::ITALIC, "3"),
                (ratatui::style::Modifier::UNDERLINED, "4"),
                (ratatui::style::Modifier::CROSSED_OUT, "9"),
            ] {
                if mods.contains(flag) {
                    if has_style { preamble.push(';'); }
                    preamble.push_str(code);
                    has_style = true;
                }
            }

            if has_style {
                write!(out, "\x1b[{preamble}m{}\x1b[0m", span.content)?;
            } else {
                write!(out, "{}", span.content)?;
            }
        }
        writeln!(out)?;
    }
    Ok(())
}

fn color_to_ansi_fg(color: ratatui::style::Color) -> Option<String> {
    use ratatui::style::Color;
    match color {
        Color::Black => Some("30".into()),
        Color::Red => Some("31".into()),
        Color::Green => Some("32".into()),
        Color::Yellow => Some("33".into()),
        Color::Blue => Some("34".into()),
        Color::Magenta => Some("35".into()),
        Color::Cyan => Some("36".into()),
        Color::White | Color::Gray => Some("37".into()),
        Color::DarkGray => Some("90".into()),
        Color::LightRed => Some("91".into()),
        Color::LightGreen => Some("92".into()),
        Color::LightYellow => Some("93".into()),
        Color::LightBlue => Some("94".into()),
        Color::LightMagenta => Some("95".into()),
        Color::LightCyan => Some("96".into()),
        Color::Rgb(r, g, b) => Some(format!("38;2;{r};{g};{b}")),
        Color::Indexed(i) => Some(format!("38;5;{i}")),
        _ => None,
    }
}

fn color_to_ansi_bg(color: ratatui::style::Color) -> Option<String> {
    use ratatui::style::Color;
    match color {
        Color::Black => Some("40".into()),
        Color::Red => Some("41".into()),
        Color::Green => Some("42".into()),
        Color::Yellow => Some("43".into()),
        Color::Blue => Some("44".into()),
        Color::Magenta => Some("45".into()),
        Color::Cyan => Some("46".into()),
        Color::White | Color::Gray => Some("47".into()),
        Color::DarkGray => Some("100".into()),
        Color::LightRed => Some("101".into()),
        Color::LightGreen => Some("102".into()),
        Color::LightYellow => Some("103".into()),
        Color::LightBlue => Some("104".into()),
        Color::LightMagenta => Some("105".into()),
        Color::LightCyan => Some("106".into()),
        Color::Rgb(r, g, b) => Some(format!("48;2;{r};{g};{b}")),
        Color::Indexed(i) => Some(format!("48;5;{i}")),
        _ => None,
    }
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = io::stdout().execute(LeaveAlternateScreen);
        original(info);
    }));
}

fn render_scrollbar(f: &mut ratatui::Frame, area: Rect, scroll: u16, max_scroll: u16) {
    let track_height = area.height.saturating_sub(1) as f64;
    let pos = if max_scroll == 0 {
        0
    } else {
        (scroll as f64 / max_scroll as f64 * track_height) as u16
    };

    let x = area.right().saturating_sub(1);
    let y = area.y + pos;

    if y < area.bottom() {
        let bar = Paragraph::new("█");
        f.render_widget(
            bar,
            Rect::new(x, y, 1, 1),
        );
    }
}
