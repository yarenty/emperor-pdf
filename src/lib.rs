//! `emperor-pdf`: an MCP server that reads, searches, renders, combines and creates PDFs inside
//! one root folder.
//!
//! - [`root`]: the folder, opened as a `cap_std::fs::Dir`; every path is root-relative.
//! - [`pdfium`]: Google PDFium loaded at runtime for text and rendering.
//! - [`structure`]: `lopdf` for metadata, merge / extract, and fallback text extraction.
//! - [`markdown`], [`world`], [`create`]: Markdown to Typst to PDF.
//! - [`server`]: the six MCP tools; [`doctor`]: the `doctor` subcommand.

#![warn(missing_docs)]

pub mod create;
pub mod doctor;
pub mod markdown;
pub mod pages;
pub mod pdfium;
pub mod root;
pub mod server;
pub mod structure;
pub mod world;

pub use server::{Config, PdfServer, tool_count};
