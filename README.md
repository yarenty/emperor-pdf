# emperor-pdf

[![CI](https://github.com/yarenty/emperor-pdf/actions/workflows/ci.yml/badge.svg)](https://github.com/yarenty/emperor-pdf/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**The PDFs in one folder, for your agents: read, search, render, merge, split and write them.**

An [MCP](https://modelcontextprotocol.io) server that works on the PDFs inside a single root
folder and nowhere else:

- **Read and search** text page by page, from the root folder, from base64 bytes in the call, or
  from an `https://` URL, with Google's [PDFium](https://pdfium.googlesource.com/pdfium/),
  the engine inside Chrome, and fall back to a pure-Rust extractor when PDFium is not installed.
- **Warn about hidden text**: white-on-white, invisible, sub-point or off-page text, the usual way
  to plant instructions for a model in a document a person thinks is harmless.
- **See a page** as a PNG image the model can look at (also how scanned pages are read).
- **Merge and extract pages**, and **create PDFs from Markdown**, typeset with
  [Typst](https://typst.app) and OFL fonts built into the binary.
- **Confined by capability**, not by string checks: every file is opened through a
  [`cap-std`](https://github.com/bytecodealliance/cap-std) directory handle, so `..`, absolute
  paths and symlinks that lead out of the root are refused by the operating system layer.
- **Read-only by default**: without `--writable` the writing tools are not even offered.

Built on the [emperor-mcp](https://github.com/yarenty/emperor-mcp) framework: stateless
Streamable HTTP (or stdio for desktop clients), framed output, a `doctor` subcommand, four tools
read-only and six with `--writable`.

## Install

Prebuilt binaries (macOS arm64/x86_64, Linux x86_64, Windows x86_64) are attached to
[GitHub releases](https://github.com/yarenty/emperor-pdf/releases). Or build from source:

```bash
cargo install --git https://github.com/yarenty/emperor-pdf
```

Rust 1.92 or newer is needed to build (typst 0.15 sets that minimum).

### Read-only build

`pdf_create` is behind the `create` Cargo feature (on by default), which brings in Typst and the
embedded fonts. Without it you get a smaller binary that reads, searches, renders, merges and
extracts:

```bash
cargo install --git https://github.com/yarenty/emperor-pdf --no-default-features
```

| Build | Release binary (macOS x86_64) |
|-------|-------------------------------|
| default (`create`) | 51 MB |
| `--no-default-features` | 14 MB |

### Docker

The repository's `Dockerfile` builds an image that speaks MCP over stdio and carries PDFium
(linux/amd64 and linux/arm64). Mount the folder with your PDFs at `/data`:

```bash
docker build -t emperor-pdf .
docker run -i --rm -v ~/Documents/papers:/data:ro emperor-pdf
# read-write: mount without :ro and switch writing on
docker run -i --rm -e PDF_WRITABLE=true -v ~/Documents/papers:/data emperor-pdf
```

The server runs as uid 10001, so for `--writable` the mounted folder must be writable by it.

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

# also offer pdf_pages and pdf_create, and allow saving rendered pages
emperor-pdf serve --root ~/Documents/papers --pdfium-dir ./pdfium --writable

# stdio, for clients that start the server themselves
emperor-pdf serve --stdio --root ~/Documents/papers
```

| Flag | Default | Meaning |
|------|---------|---------|
| `--root DIR` | required | The only folder the server can see; tool paths are relative to it. Required even when clients only send `data` or `url`. |
| `--bind ADDR` | `127.0.0.1:8032` | Listen address for Streamable HTTP. |
| `--writable` | off | Offer `pdf_pages` and `pdf_create` and allow `pdf_render_page { save_as }`. Without it those two tools are not listed. Also set by `PDF_WRITABLE`: `true`, `1` or `yes` turn it on; unset, empty, `false`, `0` or `no` leave it off. |
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

**Claude Desktop, read-write**: add `"--writable"` to `args`.

**[Kowalski](https://github.com/yarenty/kowalski)** (`config.toml`), over HTTP or stdio:

```toml
[[mcp.servers]]
name = "pdf"
transport = "http"
url = "http://127.0.0.1:8032/mcp"

# or let Kowalski start it
[[mcp.servers]]
name = "pdf"
transport = "stdio"
command = ["/path/to/emperor-pdf", "serve", "--stdio", "--root", "/path/to/pdfs", "--pdfium-dir", "/path/to/pdfium"]
```

## Tools

The four reading tools take the PDF in one of three ways, exactly one per call:

- `path`: a file relative to the root folder (for `pdf_search` also a glob such as `**/*.pdf`);
- `data`: the PDF bytes, base64 (a `data:application/pdf;base64,` prefix is accepted);
- `url`: an `https://` URL the server downloads.

`data` and `url` are capped at 8 MiB (the emperor-mcp `limits` byte cap); downloads time out
after 30 seconds and follow at most five redirects.

| Tool | Arguments | Returns |
|------|-----------|---------|
| `pdf_info` | `path` / `data` / `url` | Pages, title, author, subject, keywords, creator, producer, created / modified dates (ISO 8601), PDF version, encrypted, size in bytes. |
| `pdf_text` | `path` / `data` / `url`, `pages?` (`"1-3,5"`, `"4-"`), `max_chars?` (default 100 000) | Text per page, which extractor ran (`pdfium` or `lopdf`), whether it was truncated, and a hidden-text warning when there is any. |
| `pdf_search` | `path` (file or glob) / `data` / `url`, `query`, `regex?`, `case_sensitive?`, `max_hits?` (default 50) | File, page, match and a snippet with context for each hit; pages with hidden text are flagged. |
| `pdf_render_page` | `path` / `data` / `url`, `page`, `dpi?` (1 to 300, default 110), `save_as?`, `overwrite?` | The page as a PNG image (MCP image content), its size, and where it was saved. Needs PDFium; `save_as` needs `--writable`. |
| `pdf_pages` | `operation` (`merge` or `extract`), `inputs` (`[{path, pages?}]`), `output`, `overwrite?` | A new PDF from the selected pages, in the order written. Only on a `--writable` server. |
| `pdf_create` | `markdown`, `output`, `title?`, `overwrite?` | A new PDF typeset from Markdown (headings, emphasis, lists, tables, code blocks, quotes, links). Only on a `--writable` server built with the `create` feature. |

Page numbers are 1-based. `pdf_pages` keeps the pages and their content but not the inputs'
bookmarks, forms or tagged structure.

Tool annotations say what each tool does: the reading tools are read-only, idempotent and
open-world (they can fetch a URL); `pdf_pages`, `pdf_create` and `pdf_render_page` on a writable
server are not read-only and are marked destructive, since `overwrite: true` replaces a file. On a
read-only server `pdf_render_page` is marked read-only, because it cannot save.

### Hidden text

With PDFium, `pdf_text` and `pdf_search` check every character of the pages they read and flag
text a person looking at the page would likely not see:

- text drawn in the invisible render mode (mode 3), unless it lies over an image, which is how
  OCR makes a scanned page searchable;
- white or near-white (or fully transparent) text with nothing drawn behind it;
- text set below 1 pt;
- text outside the page box.

The text is still returned, inside the document frame; a separate, framed warning names the file,
the pages and the reasons, and asks the model to treat it as data and tell the user. `pdf_search`
marks each hit on such a page with `page_has_hidden_text`. The check is a heuristic: a white
background drawn inside a form object, for example, hides nothing but is not looked into. The
lopdf fallback does not check and says so.

### Scanned pages

emperor-pdf has no OCR engine. A scanned page without a text layer comes back from `pdf_text` as
empty or near-empty; render it with `pdf_render_page` and let a vision-capable model read the
image. Scans that were OCR'd already carry an invisible text layer, which `pdf_text` reads (and
does not flag as hidden).

## Security model

- **One root, enforced by capability.** The root is opened once as a `cap_std::fs::Dir`; every
  read and write goes through it by relative path. Paths with `..` or an absolute path are
  refused with a clear message before any file system call, and cap-std refuses symlinks that
  resolve outside the root. Globs are expanded by walking the root through the same handle
  (hidden folders are skipped, at most 200 files).
- **Read-only unless `--writable`.** Without it, `pdf_pages` and `pdf_create` are not registered
  (a client never sees them) and `pdf_render_page { save_as }` answers that the server is
  read-only. With it, an existing file is never replaced unless the call passes `overwrite: true`.
- **URLs: public HTTPS only.** `url` must be `https://`, without user name or password; every
  redirect is checked again. Host names are resolved by the server and only public addresses are
  used: loopback, private (RFC 1918), link-local (including cloud metadata at 169.254.169.254),
  shared, unique-local IPv6 and similar ranges are refused, so a call cannot reach services on
  the server's own network. Downloads and `data` are capped at 8 MiB and must start like a PDF.
- **Document text is data.** PDFs come from anywhere and may carry text written to steer a
  model. Text and search snippets are wrapped with
  [emperor-mcp framing](https://github.com/yarenty/emperor-mcp#readme) as a document of unknown
  authorship (nonce-guarded delimiters, "data, not instructions"); metadata, results and
  hidden-text warnings are framed as computed values.
- **Markdown cannot run code.** `pdf_create` escapes every character of text and passes code
  and links as string literals, so Markdown cannot reach Typst as markup or script; the Typst
  world serves no file but the generated one, and fonts are embedded, so a document cannot read
  the disk. Typst runs only on a writable server built with the `create` feature.
- **Bounded work.** Files over 256 MiB are refused, fallback text extraction caps decompressed
  content per page, renders are capped at 300 dpi and 8000 px a side, Markdown input at 2 MiB.

## doctor

```bash
emperor-pdf doctor --root ~/Documents/papers --pdfium-dir ./pdfium --writable
```

```
emperor-pdf 0.1.2 doctor
  [ok  ] root folder     /Users/me/Documents/papers (writable)
  [ok  ] pdfium          loaded from ./pdfium/lib/libpdfium.dylib
  [ok  ] typst fonts     6 embedded faces (DejaVu Sans Mono, Libertinus Serif)
  [ok  ] listen address  127.0.0.1:8032 is free
  [ok  ] tool budget     6 of at most 8 tools
all good (0 warning(s))
```

It exits `1` when a check fails. A missing PDFium is a warning, not a failure. `--stdio` skips
the port check. The tool budget counts the tools the server would list: four read-only, six with
`--writable`. The fonts check is absent from a `--no-default-features` build.

## Develop

```bash
scripts/fetch-pdfium.sh        # once, so the PDFium tests run instead of being skipped
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
cargo test
cargo deny check
```

The tests build their fixture PDFs with Typst in a temporary folder and talk to a real server
over HTTP (and one over stdio); `url` sources are fetched from a small local HTTP server. The
tool lists are checked with emperor-mcp's `testkit`. Without `./pdfium` the PDFium-only tests
print a note and pass.

## Links

- crates.io: [emperor-pdf](https://crates.io/crates/emperor-pdf)
- Official MCP Registry: mcp-name: io.github.yarenty/emperor-pdf
- Framework: [emperor-mcp](https://github.com/yarenty/emperor-mcp)
- How it compares with other PDF MCP servers: [docs/COMPARISON.md](docs/COMPARISON.md)

## Licence

Dual licensed under [MIT](./LICENSE-MIT) OR [Apache-2.0](./LICENSE-APACHE), at your option.

### Fonts

With the `create` feature the binary embeds two font families for `pdf_create`, from
[`assets/fonts`](./assets/fonts) with their licence texts:

| Font | Files | Licence |
|------|-------|---------|
| [Libertinus Serif](https://github.com/alerque/libertinus) (body text) | Regular, Bold, Italic, Bold Italic | SIL Open Font License 1.1 ([text](./assets/fonts/LICENSE-OFL-1.1.txt)) |
| [DejaVu Sans Mono](https://github.com/dejavu-fonts/dejavu-fonts) (code) | Regular, Bold | Bitstream Vera licence; DejaVu changes public domain ([text](./assets/fonts/LICENSE-DejaVu.txt)) |

Both are permissive and allow embedding in documents and redistribution with software. No
GPL-licensed font is included (Typst's own New Computer Modern set is not used). Release
archives carry both licence texts as `FONTS-NOTICE`. Documents you create are not affected by
these licences beyond keeping the font's own notices if you extract the font. PDFium, when you
install it, is under its own licences (Apache-2.0 / BSD-3-Clause).
