//! `emperor-pdf` command line.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::{ArgAction, Args, Parser, Subcommand};
use emperor_mcp::cli::parse_switch;
use emperor_mcp::{ServeOptions, serve};
use emperor_pdf::doctor::{self, DoctorOptions};
use emperor_pdf::root::Root;
use emperor_pdf::source::{FetchPolicy, Fetcher};
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
    /// Allow pdf_pages, pdf_create and saving rendered pages to write files in the root (without
    /// it, pdf_pages and pdf_create are not offered at all). Also read from $PDF_WRITABLE:
    /// `true`, `1`, `yes`, `on` turn it on; unset, empty, `false`, `0`, `no`, `off` leave it off;
    /// any other value is refused at start-up.
    #[arg(long, env = "PDF_WRITABLE", action = ArgAction::SetTrue, value_parser = parse_switch)]
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
    tracing::info!(
        root = %root.host_path().display(),
        writing = if c.writable { "on" } else { "off" },
        "serving PDFs"
    );
    let config = Arc::new(Config {
        root,
        writable: c.writable,
        pdfium,
        fetcher: Fetcher::new(FetchPolicy::default())?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn writable_arg() -> clap::Arg {
        let cmd = Cli::command();
        let serve = cmd.find_subcommand("serve").unwrap();
        serve
            .get_arguments()
            .find(|a| a.get_id() == "writable")
            .unwrap()
            .clone()
    }

    #[test]
    fn writable_reads_pdf_writable() {
        let arg = writable_arg();
        assert_eq!(arg.get_env(), Some(std::ffi::OsStr::new("PDF_WRITABLE")));
    }

    // The parser `--writable` and $PDF_WRITABLE share; the env wiring is covered by tests/cli.rs.
    #[test]
    fn pdf_writable_values() {
        for off in ["", "false", "False", "0", "no", "off"] {
            assert_eq!(parse_switch(off), Ok(false), "{off:?} must mean off");
        }
        for on in ["true", "TRUE", "1", "yes", "on"] {
            assert_eq!(parse_switch(on), Ok(true), "{on:?} must mean on");
        }
        for bad in ["maybe", "2", "enabled"] {
            assert!(parse_switch(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn writable_flag_without_env_value() {
        let cli =
            Cli::try_parse_from(["emperor-pdf", "serve", "--root", ".", "--writable"]).unwrap();
        let Command::Serve(c) = cli.command else {
            panic!("serve")
        };
        assert!(c.writable);
    }
}
