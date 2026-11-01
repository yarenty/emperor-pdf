# Changelog

All notable changes to emperor-pdf. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [semantic versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - 2026-11-01

> First release: the PDFs in one folder, for agents.

### Added
- **Six MCP tools** - `pdf_info` (pages, metadata, encryption, size), `pdf_text` (text per page, page ranges, character limit), `pdf_search` (text or regex over one file or a glob, page and snippet per hit), `pdf_render_page` (PNG as MCP image content, optionally saved), `pdf_pages` (merge and extract with page ranges), `pdf_create` (Markdown to PDF).
- **PDFium at runtime** - text and rendering through Google PDFium via `pdfium-render`, looked up in `--pdfium-dir`, `PDFIUM_DYNAMIC_LIB_PATH`, next to the binary, then the system; `scripts/fetch-pdfium.sh` installs a prebuilt build. Without it, text falls back to lopdf and says so.
- **Markdown to PDF with Typst** - headings, emphasis, lists, tables, code blocks, quotes and links, typeset with fonts embedded in the binary; Markdown is escaped so it cannot run Typst code.
- **One root folder, enforced with cap-std** - `..`, absolute paths and symlinks out of the root are refused; read-only unless `--writable`; existing files are replaced only with `overwrite: true`.
- **Framed output** - document text and snippets framed as external content, metadata and results as computed values (emperor-mcp framing).
- **Transports** - stateless Streamable HTTP at `/mcp` (default `127.0.0.1:8032`) and stdio (`--stdio`).
- **`doctor`** - root folder (and writability), PDFium (warning when missing), embedded fonts, listen address, tool budget.
