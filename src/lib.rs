//! `emperor-pdf`: an MCP server that reads, searches, renders, combines and creates PDFs inside
//! one root folder.
//!
//! - [`root`]: the folder, opened as a `cap_std::fs::Dir`; every path is root-relative.
//! - [`pdfium`]: Google PDFium loaded at runtime for text and rendering.
//! - [`structure`]: `lopdf` for metadata, merge / extract, and fallback text extraction.
//! - [`source`]: where a reading tool gets its PDF (`path`, base64 `data`, https `url`).
//! - `markdown`, `world`, `create` (feature `create`): Markdown to Typst to PDF.
//! - [`server`]: the MCP tools; [`doctor`]: the `doctor` subcommand.

#![warn(missing_docs)]

#[cfg(feature = "create")]
pub mod create;
pub mod doctor;
#[cfg(feature = "create")]
pub mod markdown;
pub mod pages;
pub mod pdfium;
pub mod root;
pub mod server;
pub mod source;
pub mod structure;
#[cfg(feature = "create")]
pub mod world;

pub use server::{Config, PdfServer, tool_count};
