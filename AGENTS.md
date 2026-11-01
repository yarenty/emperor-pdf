# emperor-pdf — AI agent notes

**Crate**: `emperor-pdf` · **Version**: 0.1.3 (on emperor-mcp 0.4 from crates.io; Emperor Profile P1.1) · **Licence**: MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`) · **Rust**: 1.92 (`rust-version`; typst 0.15 sets the floor)

## Scope

A standalone MCP server for the PDFs inside one root folder, built on the
[emperor-mcp](https://github.com/yarenty/emperor-mcp) framework: rmcp `#[tool_router]` server
mounted by `emperor_mcp::serve` (stateless Streamable HTTP at `/mcp`, `/health`), or rmcp stdio
with `--stdio`. Six tools: `pdf_info`, `pdf_text`, `pdf_search`, `pdf_render_page` (always),
`pdf_pages`, `pdf_create` (only on a `--writable` server; `pdf_create` only with the `create`
feature, on by default).

Layout (one crate, library + binary):

| Module | Use |
|--------|-----|
| `src/main.rs` | clap CLI: `serve`, `doctor`; tracing to stderr |
| `src/server.rs` | `Config`, `PdfServer`, the router per mode (`read_tools`, `write_tools`, `create_tools`), argument types, search, hidden-text warning |
| `src/source.rs` | `path` / `data` / `url` sources: base64 decoding, the HTTPS fetcher (public addresses only, byte cap, timeout) |
| `src/root.rs` | `Root`: the `cap_std::fs::Dir` handle, path checks, read / write / glob |
| `src/pdfium.rs` | PDFium lookup and binding (once per process), text per page with hidden-text detection, PNG render |
| `src/structure.rs` | lopdf: metadata, fallback text, `assemble` (merge / extract) |
| `src/pages.rs` | page range syntax (`"1-3,5,8-"`) |
| `src/markdown.rs` | Markdown to Typst markup (escaping); feature `create` |
| `src/world.rs` | the minimal `typst::World` (one in-memory file, fonts from `assets/fonts`); feature `create` |
| `src/create.rs` | Typst compile + `typst-pdf` export; feature `create` |
| `src/doctor.rs` | `doctor` checks on `emperor_mcp::doctor` |

Tests: `tests/tools.rs` (every tool over real HTTP, real PDFs generated with Typst into a temp
folder, `url` sources from a local HTTP server, tool lists checked with emperor-mcp `testkit`;
needs the `create` feature), `tests/cli.rs` (the binary: `doctor` exit codes, `serve --stdio`), shared harness in
`tests/common/mod.rs`; unit tests sit next to the code.

## Before you change code

1. `scripts/fetch-pdfium.sh` once, so the PDFium tests run (they print a note and pass without it).
2. `cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo clippy --all-targets --no-default-features -- -D warnings && cargo test && cargo deny check`.
3. After a tool change, run `emperor-pdf doctor --root <dir>` and a `tools/list` against `serve`.

## Conventions

- **Every file goes through `Root`.** No `std::fs` on user paths, ever; ambient authority is used
  once, in `Root::open`. New file access is a `Root` method. Path escapes are tested in
  `paths_outside_the_root_are_refused`.
- **Writes need `--writable`** and refuse to replace a file unless the call passes `overwrite`.
  A new writing tool goes in `write_tools` (or `create_tools`), so a read-only server never
  lists it, still checks `config.writable`, and is added to `read_only_server_refuses_writes`.
- **Annotations are explicit and accurate**: `read_only_hint` on every tool, `destructive_hint`
  on every tool that writes (true when `overwrite` can replace a file), `idempotent_hint` and
  `open_world_hint` (true for anything that can fetch a `url`). The tool-list tests run
  `emperor_mcp::testkit::assert_tools_json` and `assert_compact_schemas_json` against a live
  `tools/list` in both modes, and `assert_read_only_default_json` on the read-only one.
- **Sources**: a reading tool takes exactly one of `path`, `data`, `url` (`Source::pick`); `data`
  and `url` are resolved before `spawn_blocking`, a path is read inside it. URLs go through
  `source::Fetcher` only: HTTPS, public addresses, the emperor-mcp `Limits` byte cap, a timeout.
  `FetchPolicy::allow_local_http` exists for tests and is never set by the CLI.
- **Framing.** Text that comes out of a PDF (page text, search snippets) is framed
  `FrameKind::Document`; metadata, results and the hidden-text warning are framed
  `FrameKind::Computed` (`structured_framed` when there is a JSON value).
- **Hidden text** is detected per character with PDFium (`pdfium::hidden_text`): invisible
  render mode not over an image, white or transparent fill with no backdrop, under 1 pt, outside
  the page box. It is reported, never removed.
- **Fonts**: only the files in `assets/fonts` (Libertinus Serif, OFL; DejaVu Sans Mono, Bitstream
  Vera) are embedded, with their licence texts; no `typst-assets` fonts feature, no GPL fonts.
- **No OCR engine.** Scanned pages are read by rendering them for a vision model.
- **PDFium is optional.** Text tools must keep working with lopdf when it is absent and say so in
  their output; anything that needs PDFium returns a clear error naming `scripts/fetch-pdfium.sh`.
  PDFium binds once per process (`pdfium::load`); tests pick an engine per server through
  `Config::pdfium`, not by unloading.
- **Blocking work** (PDF parsing, rendering, typesetting) runs in `spawn_blocking`.
- **Markdown is untrusted.** `markdown.rs` escapes every ASCII punctuation character of text and
  passes code and URLs as Typst string literals; nothing user-written reaches Typst as markup or
  code. The Typst `World` serves only `main.typ` and embedded fonts.
- **3 to 8 tools** (emperor-mcp §11). Four read-only, six writable; split the server before
  adding a ninth.
- **No GPL/AGPL code dependencies, no single-maintainer PDF crates.** PDF work stays on PDFium,
  lopdf and typst. `cargo deny check` gates licences and advisories (`deny.toml`; every ignore
  carries a reason).
- **CLI flags are a public contract**: `--root`, `--bind`, `--writable` (also `PDF_WRITABLE`, read with
  `emperor_mcp::cli::parse_switch`: an unknown value stops the server), `--pdfium-dir`, `--stdio`.
- **Lean definitions** (Emperor Profile E12): the router is built with `.compact_schemas()`; a tool
  description says what the tool does and when to use it rather than another, under 300
  characters (unit test `default_router_is_read_only_and_lean` fails on a `testkit` warning).
  Limits, defaults and formats go in the parameter descriptions.
- Logs go to stderr; stdout belongs to the protocol under `--stdio`.

## Documentation closure (mandatory)

After any behaviour change update [`README.md`](./README.md) (tools table, flags, security
model), [`manifest.yaml`](./manifest.yaml) (tool list) and add a [`CHANGELOG.md`](./CHANGELOG.md)
entry under `[Unreleased]`. Shipping code without updating docs is incomplete work.

## Release

Tag `vX.Y.Z` on `main`: `.github/workflows/release.yml` builds four targets, writes
`SHA256SUMS`, attests build provenance (`actions/attest-build-provenance`) and attaches the
archives to the GitHub release. PDFium is not bundled; the fonts in
`assets/fonts` are, with their licence texts.
