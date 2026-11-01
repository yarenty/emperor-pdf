# emperor-pdf — AI agent notes

**Crate**: `emperor-pdf` · **Version**: 0.1.0 (on emperor-mcp 0.2 from crates.io) · **Licence**: MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`) · **Rust**: 1.92 (`rust-version`; typst 0.15 sets the floor)

## Scope

A standalone MCP server for the PDFs inside one root folder, built on the
[emperor-mcp](https://github.com/yarenty/emperor-mcp) framework: rmcp `#[tool_router]` server
mounted by `emperor_mcp::serve` (stateless Streamable HTTP at `/mcp`, `/health`), or rmcp stdio
with `--stdio`. Six tools: `pdf_info`, `pdf_text`, `pdf_search`, `pdf_render_page`, `pdf_pages`,
`pdf_create`.

Layout (one crate, library + binary):

| Module | Use |
|--------|-----|
| `src/main.rs` | clap CLI: `serve`, `doctor`; tracing to stderr |
| `src/server.rs` | `Config`, `PdfServer`, the six tools, argument types, search |
| `src/root.rs` | `Root`: the `cap_std::fs::Dir` handle, path checks, read / write / glob |
| `src/pdfium.rs` | PDFium lookup and binding (once per process), text per page, PNG render |
| `src/structure.rs` | lopdf: metadata, fallback text, `assemble` (merge / extract) |
| `src/pages.rs` | page range syntax (`"1-3,5,8-"`) |
| `src/markdown.rs` | Markdown to Typst markup (escaping) |
| `src/world.rs` | the minimal `typst::World` (one in-memory file, embedded fonts) |
| `src/create.rs` | Typst compile + `typst-pdf` export |
| `src/doctor.rs` | `doctor` checks on `emperor_mcp::doctor` |

Tests: `tests/tools.rs` (every tool over real HTTP, real PDFs generated with Typst into a temp
folder), `tests/cli.rs` (the binary: `doctor` exit codes, `serve --stdio`), shared harness in
`tests/common/mod.rs`; unit tests sit next to the code.

## Before you change code

1. `scripts/fetch-pdfium.sh` once, so the PDFium tests run (they print a note and pass without it).
2. `cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo deny check`.
3. After a tool change, run `emperor-pdf doctor --root <dir>` and a `tools/list` against `serve`.

## Conventions

- **Every file goes through `Root`.** No `std::fs` on user paths, ever; ambient authority is used
  once, in `Root::open`. New file access is a `Root` method. Path escapes are tested in
  `paths_outside_the_root_are_refused`.
- **Writes need `--writable`** and refuse to replace a file unless the call passes `overwrite`.
  A new writing tool checks `config.writable` first and is added to `read_only_server_refuses_writes`.
- **Framing.** Text that comes out of a PDF (page text, search snippets) is framed
  `FrameKind::ExternalWeb`; metadata and results are framed `FrameKind::Computed`
  (`structured_framed` when there is a JSON value).
- **PDFium is optional.** Text tools must keep working with lopdf when it is absent and say so in
  their output; anything that needs PDFium returns a clear error naming `scripts/fetch-pdfium.sh`.
  PDFium binds once per process (`pdfium::load`); tests pick an engine per server through
  `Config::pdfium`, not by unloading.
- **Blocking work** (PDF parsing, rendering, typesetting) runs in `spawn_blocking`.
- **Markdown is untrusted.** `markdown.rs` escapes every ASCII punctuation character of text and
  passes code and URLs as Typst string literals; nothing user-written reaches Typst as markup or
  code. The Typst `World` serves only `main.typ` and embedded fonts.
- **3 to 8 tools** (emperor-mcp §11). Six today; split the server before adding a ninth.
- **No GPL/AGPL code dependencies, no single-maintainer PDF crates.** PDF work stays on PDFium,
  lopdf and typst. `cargo deny check` gates licences and advisories (`deny.toml`; every ignore
  carries a reason).
- **CLI flags are a public contract**: `--root`, `--bind`, `--writable`, `--pdfium-dir`, `--stdio`.
- Logs go to stderr; stdout belongs to the protocol under `--stdio`.

## Documentation closure (mandatory)

After any behaviour change update [`README.md`](./README.md) (tools table, flags, security
model), [`manifest.yaml`](./manifest.yaml) (tool list) and add a [`CHANGELOG.md`](./CHANGELOG.md)
entry under `[Unreleased]`. Shipping code without updating docs is incomplete work.

## Release

Tag `vX.Y.Z` on `main`: `.github/workflows/release.yml` builds four targets and attaches the
archives to the GitHub release. PDFium is not bundled. Publishing to crates.io needs the
emperor-mcp dependency on a crates.io version first (see the comment in `Cargo.toml`).
