# mdview

A terminal markdown viewer that renders markdown files with syntax highlighting, tables, and live reloading — right in your terminal.

Built because `glow`, `mdcat`, and friends don't handle tables well, and I wanted vim-style navigation with auto-reload on file changes.

## Features

- Syntax-highlighted code blocks (via syntect)
- Unicode box-drawing tables with column wrapping and alignment
- Live file watching — edit your markdown and see changes instantly
- Vim-style key bindings (j/k, d/u, g/G)
- `--dump` mode for piping rendered output to stdout

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
# binary at target/release/mdview
```

## Usage

```bash
mdview README.md
```

### Dump mode

Render to stdout instead of the TUI (useful for piping or testing):

```bash
mdview --dump -w 80 README.md
```

### Key bindings

| Key | Action |
|-----|--------|
| `j` / `↓` | Scroll down |
| `k` / `↑` | Scroll up |
| `d` | Half page down |
| `u` | Half page up |
| `g` | Go to top |
| `G` | Go to bottom |
| `Space` / `PgDn` | Page down |
| `PgUp` | Page up |
| `q` / `Esc` | Quit |

## License

MIT
