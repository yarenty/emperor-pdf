# emperor-pdf next to other PDF MCP servers

How emperor-pdf 0.1.1 compares with the most popular public MCP servers that work with PDFs. For
the shared background (the 2026-07-28 spec revision, annotations, the "lethal trifecta") and what
every emperor server gets from the framework, see
[the emperor-mcp comparison](https://github.com/yarenty/emperor-mcp/blob/main/docs/COMPARISON.md).

Figures for other projects were read on **2026-11-01** from the GitHub API (stars, last
release) and from each project's README and source (tool counts). A **(?)** marks a figure that
could not be confirmed.

## Why this domain is risky

A PDF comes from anywhere and can carry text a person never sees: white on white, a tiny font,
text off the page or in the invisible render mode. A model reads it all. That is the easiest way
to plant "ignore your instructions and..." in a document someone thinks is harmless. A PDF
server that also fetches URLs can be pointed at internal addresses, and one that writes files can
overwrite them.

## The table

| Server | Stars | Last release | Language | Tools | Read | Search | Render | Merge / extract | Create | OCR | Hidden text |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **emperor-pdf** | new | 0.1.1, 2026-11-01 | Rust | 6 (4 when read-only) | ✓ path, base64 or `https://` URL | ✓ text or regex, over globs | ✓ PDFium | ✓ | ✓ Markdown → Typst | – (render for a vision model) | ✓ flagged |
| [SylphxAI/anymd](https://github.com/SylphxAI/anymd) | 1.06k | v8.5.7, 2026-10-30 | Rust (npm wrapper) | 4 (`inspect` has several operations) | ✓ PDF, Office, EPUB, HTML, media | ✓ | ✓ | – | – | ✓ tesseract or local doc-VLM | (?) |
| [jztan/pdf-mcp](https://github.com/jztan/pdf-mcp) | 165 | v3.6.0, 2026-10-31 | Python | 13 | ✓ | ✓ hybrid keyword + semantic, over folders | ✓ | – | – | ✓ tesseract | ✓ flagged |
| [hanweg/mcp-pdf-tools](https://github.com/hanweg/mcp-pdf-tools) | 76 | no releases (last commit 2026-10-22) | Python | 5 | – | ✓ regex, "find related" | – | ✓ | – | – | – |
| [markitdown-mcp](https://github.com/microsoft/markitdown/tree/main/packages/markitdown-mcp) | (library 193k) | v0.1.9, 2026-10-21 | Python | 1 | ✓ any format → Markdown | – | – | – | – | (?) | – |

anymd has a paid Pro tier for two extra features; its core is MIT. markitdown-mcp's own README
warns that its single tool can read any file the server's user can read, and any URL.

## emperor-pdf: pros

- **The only complete one**: read, search, render, merge/extract and create.
- **The PDF can come from anywhere**: a path under the root folder, base64 bytes in the call, or a
  public `https://` URL. URLs resolving to loopback, private, link-local (cloud metadata) and
  similar ranges are refused, every redirect is checked again, and downloads are capped at 8 MiB.
- **Hidden text is flagged**: with PDFium, pages with invisible-mode text (except OCR layers over
  images), white or transparent text with nothing behind it, text under 1 pt or off the page get
  a separate framed warning naming the pages and reasons. The text is still returned, as data.
- **PDFium**, the engine inside Chrome, for text and rendering, with a pure-Rust fallback for
  text when PDFium is missing (which says so).
- **Typeset output**: `pdf_create` turns Markdown into a PDF through Typst, not an HTML
  screenshot. Markdown is escaped so it cannot run Typst code, and the fonts (Libertinus Serif,
  DejaVu Sans Mono; OFL and Bitstream Vera licences, no GPL) are embedded.
- **Read-only by default**, with the writing tools not listed; a read-only build without Typst is
  14 MB instead of 51 MB.
- **Annotations on every tool**, checked against the live tool list; output framed as documents of
  unknown authorship.
- **Docker image** with a pinned, checksum-verified PDFium; release binaries with `SHA256SUMS`
  and build provenance attestations.

## emperor-pdf: cons

- **No OCR.** A scanned page without a text layer comes back empty from `pdf_text`; render it
  with `pdf_render_page` and let a vision-capable model read the image. A text-only model cannot
  read scans. This is a deliberate choice, not a missing week of work.
- **No semantic search, corpus or cache.** pdf-mcp's search-then-read-pages pattern over a whole
  folder saves many tokens on long documents.
- **No table or chart extraction**, and **PDF only**: anymd and markitdown read Office files,
  EPUB and HTML too.
- **PDFium is a separate install** outside Docker (a fetch script is provided).
- **51 MB** default binary (Typst and fonts); building needs Rust 1.92.
- `pdf_pages` keeps pages and their content, but not bookmarks, forms or tagged structure.
- New, with no users yet.

## Where a competitor is better

- **pdf-mcp**: OCR, hybrid keyword and semantic search across a folder with a cache, table and
  chart extraction, a table of contents tool. If your job is "answer questions over 200 papers",
  it is the better fit.
- **anymd**: OCR with local document models, many formats in one `read`, an outline tool, a
  public benchmark.
- **markitdown-mcp**: one call turns almost any document into Markdown; nothing simpler exists.

## When to pick something else

- You read scans with a text-only model: anymd or pdf-mcp.
- You search a large library of PDFs by meaning: pdf-mcp.
- Your documents are mostly Word, PowerPoint or HTML: anymd or markitdown-mcp.

Pick emperor-pdf when the agent has to produce PDFs as well as read them, when PDFs come from
untrusted senders and hidden text matters, when the server must fetch URLs without reaching your
internal network, or when you want one read-only binary with nothing to install at run time.
