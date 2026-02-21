# mdview

A terminal markdown viewer built with Rust, ratatui, and pulldown-cmark.

## Build & Test

```bash
cargo build              # dev build
cargo build --release    # release build
cargo test               # run all tests
cargo run -- <file.md>   # TUI mode
cargo run -- --dump -w 80 <file.md>  # dump rendered output to stdout
```

## Architecture (4 files)

- **`src/main.rs`** — CLI argument parsing, TUI event loop (crossterm), scrollbar, `--dump` mode with ANSI output
- **`src/render.rs`** — Core renderer: converts markdown → ratatui `Text` via pulldown-cmark event state machine. Contains table layout (`budget_columns`), word-aware wrapping (`wrap_cell_spans`), and all inline/block formatting
- **`src/highlight.rs`** — Syntax highlighting for code blocks via syntect, outputs ANSI then converts to ratatui spans
- **`src/watch.rs`** — File watcher using notify crate, sends reload signals via mpsc channel

## Validation Workflow

Use `--dump -w WIDTH` to render markdown to stdout without entering the TUI. Combine with test fixtures for visual verification:

```bash
cargo run -- --dump -w 80 tests/fixtures/tables.md
cargo run -- --dump -w 40 tests/fixtures/tables.md   # narrow terminal
cargo run -- --dump -w 80 tests/fixtures/lists.md
```

## Test Fixtures (`tests/fixtures/`)

| File | Covers |
|------|--------|
| `headings.md` | H1–H6, headings with inline formatting |
| `inline.md` | Bold, italic, strikethrough, inline code, nested combos, links |
| `lists.md` | Tight/loose, ordered/unordered, nested, task lists, multi-paragraph items |
| `tables.md` | Alignment, inline code in cells, wide tables, long paths |
| `code-blocks.md` | Fenced with language, fenced without, indented, multiple languages |
| `blockquotes.md` | Simple, nested, with inline formatting and lists inside |
| `edge-cases.md` | Empty sections, long words, special unicode, consecutive horizontal rules |

## Key Patterns

- **Pulldown-cmark state machine:** `Renderer::process` iterates events; `Start(Tag)` pushes state/styles, `End(TagEnd)` pops and flushes. Tables accumulate cells into `table_header`/`table_rows` vectors, then render all at once in `render_table()`.
- **Style stack:** `push_style`/`pop_style` maintain nested inline formatting (bold inside italic inside link, etc.)
- **Column budget algorithm:** `budget_columns` distributes terminal width fairly across table columns — locks small columns first, then divides remaining budget among the rest.
- **Word-aware wrapping:** `wrap_cell_spans` splits styled text into words, wraps at column boundaries, and truncates with `…` when exceeding `max_lines`.
