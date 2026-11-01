//! Shared test harness: real PDFs in a temp folder, a real server over HTTP.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use emperor_mcp::{ServeOptions, serve};
use emperor_pdf::root::Root;
use emperor_pdf::{Config, PdfServer, create, pdfium};
use serde_json::{Value, json};

/// A three-page PDF with metadata, typeset by typst.
pub const THREE_PAGES: &str = r#"#set document(title: "Fixture Three", author: "Test Author")
Page one alpha.
#pagebreak()
Page two beta with a Needle inside.
#pagebreak()
Page three gamma.
"#;

/// A two-page PDF.
pub const TWO_PAGES: &str = r#"#set document(title: "Fixture Two")
Second file delta.
#pagebreak()
Second file epsilon needle.
"#;

/// PDFium for tests: ./pdfium (from scripts/fetch-pdfium.sh) or $PDFIUM_DYNAMIC_LIB_PATH.
pub fn pdfium() -> Option<&'static pdfium::Engine> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("pdfium");
    pdfium::load(Some(&dir)).ok()
}

/// Print why a PDFium test is skipped.
pub fn skip_without_pdfium(test: &str) -> Option<&'static pdfium::Engine> {
    let e = pdfium();
    if e.is_none() {
        eprintln!("note: {test} skipped: PDFium not found (run scripts/fetch-pdfium.sh)");
    }
    e
}

/// A temp root holding `three.pdf`, `two.pdf` and `sub/nested.pdf`.
pub fn fixture_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_typst(dir.path(), "three.pdf", THREE_PAGES);
    write_typst(dir.path(), "two.pdf", TWO_PAGES);
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    write_typst(
        dir.path(),
        "sub/nested.pdf",
        "Nested file with the NEEDLE in capitals.",
    );
    dir
}

pub fn write_typst(dir: &Path, name: &str, source: &str) -> PathBuf {
    let pdf = create::compile(source.to_string()).expect("fixture compiles");
    let path = dir.join(name);
    std::fs::write(&path, pdf).unwrap();
    path
}

/// Which PDF engine a test server gets.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// PDFium if it can be loaded.
    Pdfium,
    /// lopdf only, even when PDFium is installed.
    LopdfOnly,
}

/// Start a server on a free port; returns its base URL.
pub async fn start(root: &Path, writable: bool, engine: Engine) -> String {
    let config = Arc::new(Config {
        root: Root::open(root).unwrap(),
        writable,
        pdfium: match engine {
            Engine::Pdfium => pdfium(),
            Engine::LopdfOnly => None,
        },
    });
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    tokio::spawn(serve(ServeOptions::new(addr), move || {
        PdfServer::new(config.clone())
    }));
    let base = format!("http://{addr}");
    for _ in 0..100 {
        if reqwest::get(format!("{base}/health")).await.is_ok() {
            return base;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("server did not start");
}

/// One JSON-RPC call over stateless Streamable HTTP.
pub async fn rpc(base: &str, method: &str, params: Value) -> Value {
    let resp = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2025-06-18")
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "status {}", resp.status());
    let text = resp.text().await.unwrap();
    let body = text
        .lines()
        .find_map(|l| l.strip_prefix("data:"))
        .unwrap_or(&text)
        .trim()
        .to_string();
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("bad body {text}: {e}"))
}

/// A finished tool call.
pub struct Call {
    pub raw: Value,
}

impl Call {
    pub fn is_error(&self) -> bool {
        self.raw["result"]["isError"].as_bool().unwrap_or(false)
    }

    /// All text content joined.
    pub fn text(&self) -> String {
        self.raw["result"]["content"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|c| c["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }

    pub fn structured(&self) -> &Value {
        &self.raw["result"]["structuredContent"]
    }

    pub fn content(&self) -> &Vec<Value> {
        self.raw["result"]["content"].as_array().expect("content")
    }
}

pub async fn call(base: &str, tool: &str, args: Value) -> Call {
    let raw = rpc(base, "tools/call", json!({"name": tool, "arguments": args})).await;
    assert!(raw.get("error").is_none(), "protocol error: {raw}");
    Call { raw }
}

/// Page count of a PDF on disk, read with lopdf.
pub fn page_count(path: &Path) -> usize {
    lopdf::Document::load(path).unwrap().get_pages().len()
}
