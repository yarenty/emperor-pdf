//! Shared test harness: real PDFs in a temp folder, a real server over HTTP.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use emperor_mcp::{ServeOptions, serve};
use emperor_pdf::root::Root;
use emperor_pdf::source::{FetchPolicy, Fetcher};
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

/// The fetch policy of test servers: plain HTTP to the local test file server is allowed.
pub fn local_fetch() -> FetchPolicy {
    FetchPolicy {
        allow_local_http: true,
        ..FetchPolicy::default()
    }
}

/// Start a server on a free port; returns its base URL.
pub async fn start(root: &Path, writable: bool, engine: Engine) -> String {
    start_with(root, writable, engine, local_fetch()).await
}

/// [`start`] with a given fetch policy.
pub async fn start_with(root: &Path, writable: bool, engine: Engine, fetch: FetchPolicy) -> String {
    let config = Arc::new(Config {
        root: Root::open(root).unwrap(),
        writable,
        pdfium: match engine {
            Engine::Pdfium => pdfium(),
            Engine::LopdfOnly => None,
        },
        fetcher: Fetcher::new(fetch).unwrap(),
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

/// A minimal HTTP/1.1 file server on a free local port, for `url` sources. Routes: `/<name>`
/// serves `files[name]` as `application/pdf`; `/redirect/<name>` answers 302 to `/<name>`;
/// anything else is 404. Returns the base URL.
pub async fn file_server(files: Vec<(&'static str, Vec<u8>)>) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let files = Arc::new(files);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let files = files.clone();
            tokio::spawn(async move {
                let mut req = Vec::new();
                let mut buf = [0u8; 1024];
                while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                    match sock.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => req.extend_from_slice(&buf[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&req);
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (status, extra, body) = if let Some(name) = path.strip_prefix("/redirect/") {
                    ("302 Found", format!("Location: /{name}\r\n"), Vec::new())
                } else {
                    match files.iter().find(|(n, _)| path == format!("/{n}")) {
                        Some((_, b)) => ("200 OK", String::new(), b.clone()),
                        None => ("404 Not Found", String::new(), b"missing".to_vec()),
                    }
                };
                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/pdf\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(header.as_bytes()).await;
                let _ = sock.write_all(&body).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    format!("http://{addr}")
}
