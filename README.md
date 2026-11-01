# emperor-pdf

[![CI](https://github.com/yarenty/emperor-pdf/actions/workflows/ci.yml/badge.svg)](https://github.com/yarenty/emperor-pdf/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**The PDFs in one folder, for your agents: read, search, render, merge, split and write them.**

An [MCP](https://modelcontextprotocol.io) server that works on the PDFs inside a single root
folder and nowhere else:

- **Read and search** text page by page with Google's [PDFium](https://pdfium.googlesource.com/pdfium/),
  the engine inside Chrome, and fall back to a pure-Rust extractor when PDFium is not installed.
- **See a page** as a PNG image the model can look at.
- **Merge and extract pages**, and **create PDFs from Markdown**, typeset with
  [Typst](https://typst.app) and fonts built into the binary.
- **Confined by capability**, not by string checks: every file is opened through a
  [`cap-std`](https://github.com/bytecodealliance/cap-std) directory handle, so `..`, absolute
  paths and symlinks that lead out of the root are refused by the operating system layer.
- **Read-only by default**; writing needs `--writable`.

Built on the [emperor-mcp](https://github.com/yarenty/emperor-mcp) framework: stateless
Streamable HTTP (or stdio for desktop clients), framed output, a `doctor` subcommand, six tools.

## Install

Prebuilt binaries (macOS arm64/x86_64, Linux x86_64, Windows x86_64) are attached to
[GitHub releases](https://github.com/yarenty/emperor-pdf/releases). Or build from source:

```bash
cargo install --git https://github.com/yarenty/emperor-pdf
```

Rust 1.92 or newer is needed to build (typst 0.15 sets that minimum).

### PDFium (recommended)

PDFium is a native library loaded at runtime; it is **not** bundled with emperor-pdf. Prebuilt
PDFium binaries for every platform come from
[bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries) (Apache-2.0 /
BSD-3-Clause). From a checkout:

```bash
scripts/fetch-pdfium.sh            # downloads the right archive for this machine into ./pdfium/
```

or download `pdfium-<os>-<cpu>.tgz` from a release yourself and unpack it anywhere. The
release is pinned to `chromium/7881`, the PDFium API version the `pdfium-render` binding is
built for; `PDFIUM_VERSION=<n> scripts/fetch-pdfium.sh` picks another.

emperor-pdf looks for `libpdfium.dylib` / `libpdfium.so` / `pdfium.dll`, in this order:

1. `--pdfium-dir DIR` (the library in `DIR`, `DIR/lib` or `DIR/bin`),
2. `PDFIUM_DYNAMIC_LIB_PATH` (a folder, with the same `lib/` and `bin/` lookup, or the file),
3. the folder holding the `emperor-pdf` binary,
4. the system library path.

Without PDFium, `pdf_text` and `pdf_search` use [lopdf](https://github.com/J-F-Liu/lopdf)'s
extractor (and say so in their output: it misses or garbles text in some PDFs), and
`pdf_render_page` answers that it needs PDFium. Everything else works the same.

## Run

```bash
# Streamable HTTP at http://127.0.0.1:8032/mcp, read-only
emperor-pdf serve --root ~/Documents/papers --pdfium-dir ./pdfium

# allow pdf_pages, pdf_create and saving rendered pages
emperor-pdf serve --root ~/Documents/papers --pdfium-dir ./pdfium --writable

# stdio, for clients that start the server themselves
emperor-pdf serve --stdio --root ~/Documents/papers
```

| Flag | Default | Meaning |
|------|---------|---------|
| `--root DIR` | required | The only folder the server can see; tool paths are relative to it. |
| `--bind ADDR` | `127.0.0.1:8032` | Listen address for Streamable HTTP. |
| `--writable` | off | Allow the tools that write files. |
| `--pdfium-dir DIR` | none | Where to look for PDFium first. |
| `--stdio` | off | Speak MCP over stdin/stdout instead of HTTP. |

Logs go to stderr (`RUST_LOG=debug` for more). `GET /health` answers `OK`.

## Connect a client

**Claude Code**

```bash
# HTTP (start the server first)
claude mcp add --transport http pdf http://127.0.0.1:8032/mcp
# or stdio
claude mcp add pdf -- emperor-pdf serve --stdio --root /path/to/pdfs --pdfium-dir /path/to/pdfium
```

**Claude Desktop** (`claude_desktop_config.json`)

```json
{
  "mcpServers": {
    "pdf": {
      "command": "/path/to/emperor-pdf",
      "args": ["serve", "--stdio", "--root", "/path/to/pdfs", "--pdfium-dir", "/path/to/pdfium"]
    }
  }
}
```

**kowalski** (`resources/config.toml`)

```toml
[[mcp.servers]]
name = "pdf"
transport = "http"
url = "http://127.0.0.1:8032/mcp"
```

## Tools

| Tool | Arguments | Returns |
|------|-----------|---------|
| `pdf_info` | `path` | Pages, title, author, subject, keywords, creator, producer, created / modified dates (ISO 8601), PDF version, encrypted, size in bytes. |
| `pdf_text` | `path`, `pages?` (`"1-3,5"`, `"4-"`), `max_chars?` (default 100 000) | Text per page, which extractor ran (`pdfium` or `lopdf`), whether it was truncated. |
| `pdf_search` | `path` (file or glob such as `**/*.pdf`), `query`, `regex?`, `case_sensitive?`, `max_hits?` (default 50) | File, page, match and a snippet with context for each hit. |
| `pdf_render_page` | `path`, `page`, `dpi?` (1 to 300, default 110), `save_as?`, `overwrite?` | The page as a PNG image (MCP image content), its size, and where it was saved. Needs PDFium; `save_as` needs `--writable`. |
| `pdf_pages` | `operation` (`merge` or `extract`), `inputs` (`[{path, pages?}]`), `output`, `overwrite?` | A new PDF from the selected pages, in the order written. Needs `--writable`. |
| `pdf_create` | `markdown`, `output`, `title?`, `overwrite?` | A new PDF typeset from Markdown (headings, emphasis, lists, tables, code blocks, quotes, links). Needs `--writable`. |

Page numbers are 1-based. `pdf_pages` keeps the pages and their content but not the inputs'
bookmarks, forms or tagged structure.

## Security model

- **One root, enforced by capability.** The root is opened once as a `cap_std::fs::Dir`; every
  read and write goes through it by relative path. Paths with `..` or an absolute path are
  refused with a clear message before any file system call, and cap-std refuses symlinks that
  resolve outside the root. Globs are expanded by walking the root through the same handle
  (hidden folders are skipped, at most 200 files).
- **Read-only unless `--writable`.** Without it, `pdf_pages`, `pdf_create` and
  `pdf_render_page { save_as }` answer that the server is read-only. With it, an existing file
  is never replaced unless the call passes `overwrite: true`.
- **Document text is data.** PDFs come from anywhere and may carry text written to steer a
  model. Text and search snippets are wrapped with
  [emperor-mcp framing](https://github.com/yarenty/emperor-mcp#readme) as external content
  (nonce-guarded delimiters, "data, not instructions"); metadata and results are framed as
  computed values.
- **Markdown cannot run code.** `pdf_create` escapes every character of text and passes code
  and links as string literals, so Markdown cannot reach Typst as markup or script; the Typst
  world serves no file but the generated one, and fonts are embedded, so a document cannot read
  the disk.
- **Bounded work.** Files over 256 MiB are refused, fallback text extraction caps decompressed
  content per page, renders are capped at 300 dpi and 8000 px a side, Markdown input at 2 MiB.

## doctor

```bash
emperor-pdf doctor --root ~/Documents/papers --pdfium-dir ./pdfium --writable
```

```
emperor-pdf 0.1.0 doctor
  [ok  ] root folder     /Users/me/Documents/papers (writable)
  [ok  ] pdfium          loaded from ./pdfium/lib/libpdfium.dylib
  [ok  ] typst fonts     17 embedded faces (DejaVu Sans Mono, Libertinus Serif, New Computer Modern, New Computer Modern Math)
  [ok  ] listen address  127.0.0.1:8032 is free
  [ok  ] tool budget     6 of at most 8 tools
all good (0 warning(s))
```

It exits `1` when a check fails. A missing PDFium is a warning, not a failure. `--stdio` skips
the port check.

## Develop

```bash
scripts/fetch-pdfium.sh        # once, so the PDFium tests run instead of being skipped
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo deny check
```

The tests build their fixture PDFs with Typst in a temporary folder and talk to a real server
over HTTP (and one over stdio). Without `./pdfium` the PDFium-only tests print a note and pass.

## Licence

Dual licensed under [MIT](./LICENSE-MIT) OR [Apache-2.0](./LICENSE-APACHE), at your option.

The binary embeds the fonts shipped by
[typst-assets](https://github.com/typst/typst-assets) (Libertinus Serif under the SIL Open Font
License 1.1, New Computer Modern under the GUST Font License and, for NewCM10-Regular, GPL-3.0+
with the font and distribution exceptions, DejaVu Sans Mono under the Bitstream Vera licence);
their licence texts are in that crate's `NOTICE` file, which release archives include as
`FONTS-NOTICE`. Documents you create are not affected by these licences. PDFium, when you
install it, is under its own licences (Apache-2.0 / BSD-3-Clause).
