//! `emperor-pdf` command line.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use emperor_mcp::{ServeOptions, serve};
use emperor_pdf::doctor::{self, DoctorOptions};
use emperor_pdf::root::Root;
use emperor_pdf::{Config, PdfServer, pdfium};
use rmcp::ServiceExt;

#[derive(Parser)]
#[command(
    name = "emperor-pdf",
    version,
    about = "MCP server for the PDFs in one folder"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the MCP server (Streamable HTTP at /mcp, or stdio with --stdio).
    Serve(Common),
    /// Check the root folder, PDFium, fonts and the listen address, then exit (1 on failure).
    Doctor(Common),
}

#[derive(Args)]
struct Common {
    /// The folder every path is relative to; nothing outside it is read or written.
    #[arg(long, value_name = "DIR")]
    root: PathBuf,
    /// Address for Streamable HTTP.
    #[arg(long, default_value = "127.0.0.1:8032")]
    bind: SocketAddr,
    /// Allow pdf_pages, pdf_create and saving rendered pages to write files in the root.
    #[arg(long)]
    writable: bool,
    /// Folder holding the PDFium library (also searched: $PDFIUM_DYNAMIC_LIB_PATH, the binary's
    /// folder, the system library path).
    #[arg(long, value_name = "DIR")]
    pdfium_dir: Option<PathBuf>,
    /// Speak MCP over stdin/stdout instead of HTTP (for desktop clients).
    #[arg(long)]
    stdio: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor(c) => {
            let report = doctor::run(&DoctorOptions {
                root: &c.root,
                writable: c.writable,
                pdfium_dir: c.pdfium_dir.as_deref(),
                bind: (!c.stdio).then_some(c.bind),
            });
            report.print();
            std::process::exit(report.exit_code());
        }
        Command::Serve(c) => tokio::runtime::Runtime::new()?.block_on(run(c)),
    }
}

async fn run(c: Common) -> anyhow::Result<()> {
    // Logs go to stderr so stdout stays the protocol stream under --stdio.
    init_tracing();
    let root = Root::open(&c.root)?;
    let pdfium = match pdfium::load(c.pdfium_dir.as_deref()) {
        Ok(e) => {
            tracing::info!(path = %e.path, "PDFium loaded");
            Some(e)
        }
        Err(e) => {
            tracing::warn!("{e}; text falls back to lopdf, rendering is unavailable");
            None
        }
    };
    tracing::info!(root = %root.host_path().display(), writable = c.writable, "serving PDFs");
    let config = Arc::new(Config {
        root,
        writable: c.writable,
        pdfium,
    });
    if c.stdio {
        let service = PdfServer::new(config)
            .serve(rmcp::transport::stdio())
            .await
            .context("stdio session failed to start")?;
        service.waiting().await?;
        Ok(())
    } else {
        serve(ServeOptions::new(c.bind), move || {
            PdfServer::new(config.clone())
        })
        .await
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,rmcp=warn".into()),
        )
        .try_init();
}
