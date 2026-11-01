# Changelog

All notable changes to emperor-pdf. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [semantic versioning](https://semver.org/).

## [Unreleased]

## [0.1.1] - 2026-11-01

### Added
- **Docker image** - a `Dockerfile` for linux/amd64 and linux/arm64 that serves over stdio with a pinned, checksum-verified PDFium in `/opt/pdfium`, runs as a non-root user and reads the PDFs mounted at `/data`.
- **MCP Registry entry** - `server.json` describes the crates.io package (`serve --stdio`, `--root`, `--pdfium-dir`, `--writable`), and the README carries the registry name `io.github.yarenty/emperor-pdf`.

## [0.1.0] - 2026-11-01

> First release: the PDFs in one folder, for agents.

### Added
- **Six MCP tools** - `pdf_info` (pages, metadata, encryption, size), `pdf_text` (text per page, page ranges, character limit), `pdf_search` (text or regex over one file or a glob, page and snippet per hit), `pdf_render_page` (PNG as MCP image content, optionally saved), `pdf_pages` (merge and extract with page ranges), `pdf_create` (Markdown to PDF).
- **PDF in the call** - the reading tools take the PDF as a root-relative `path`, as base64 `data`, or as an `https://` `url`, exactly one per call; `data` and `url` are capped at 8 MiB (emperor-mcp `limits`), downloads time out after 30 s, follow at most five checked redirects and reach public addresses only.
- **Hidden-text warning** - with PDFium, `pdf_text` and `pdf_search` flag pages whose text a reader would likely not see (invisible render mode outside OCR layers, white or transparent fill with nothing behind it, under 1 pt, outside the page box) with a framed warning naming the pages and reasons; search hits on such pages are marked.
- **Read-only build** - Cargo feature `create` (default) gates Typst and `pdf_create`; `--no-default-features` builds a smaller binary without them.
- **Release integrity** - `SHA256SUMS` and GitHub build-provenance attestations for every release archive; issue and pull-request templates.
- **PDFium at runtime** - text and rendering through Google PDFium via `pdfium-render`, looked up in `--pdfium-dir`, `PDFIUM_DYNAMIC_LIB_PATH`, next to the binary, then the system; `scripts/fetch-pdfium.sh` installs a prebuilt build. Without it, text falls back to lopdf and says so.
- **Markdown to PDF with Typst** - headings, emphasis, lists, tables, code blocks, quotes and links, typeset in Libertinus Serif (OFL) and DejaVu Sans Mono embedded in the binary with their licence texts, no GPL fonts; Markdown is escaped so it cannot run Typst code.
- **One root folder, enforced with cap-std** - `..`, absolute paths and symlinks out of the root are refused; read-only unless `--writable`, and a read-only server does not list `pdf_pages` or `pdf_create`; existing files are replaced only with `overwrite: true`.
- **Accurate tool annotations** - explicit read-only, destructive, idempotent and open-world hints on every tool, checked against a live tool list with emperor-mcp `testkit`.
- **Framed output** - document text and snippets framed as documents of unknown authorship, metadata, results and warnings as computed values (emperor-mcp 0.3 framing).
- **Transports** - stateless Streamable HTTP at `/mcp` (default `127.0.0.1:8032`) and stdio (`--stdio`).
- **`doctor`** - root folder (and writability), PDFium (warning when missing), embedded fonts, listen address, tool budget for the mode.
