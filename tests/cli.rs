//! The binary itself: `doctor` exit codes and output.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_emperor-pdf"))
}

#[test]
fn doctor_passes_on_a_good_root() {
    let root = tempfile::tempdir().unwrap();
    let out = bin()
        .args(["doctor", "--stdio", "--writable", "--root"])
        .arg(root.path())
        .arg("--pdfium-dir")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/pdfium"))
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("(writable)"), "{text}");
    assert!(
        text.contains("tool budget")
            && text.contains(if cfg!(feature = "create") {
                "6 of at most 8"
            } else {
                "5 of at most 8"
            }),
        "{text}"
    );
    assert_eq!(
        text.contains("typst fonts"),
        cfg!(feature = "create"),
        "{text}"
    );
    // PDFium missing is a warning, not a failure.
    assert!(text.contains("pdfium"), "{text}");
}

#[test]
fn doctor_fails_on_a_missing_root() {
    let out = bin()
        .args(["doctor", "--stdio", "--root", "/definitely/not/here"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("[FAIL] root folder"), "{text}");
}

#[test]
fn doctor_fails_on_a_busy_port() {
    let root = tempfile::tempdir().unwrap();
    let busy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = busy.local_addr().unwrap().to_string();
    let out = bin()
        .args(["doctor", "--bind", &addr, "--root"])
        .arg(root.path())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("listen address"), "{text}");
}

#[test]
fn serve_over_stdio_read_only_lists_four_tools() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;

    let root = tempfile::tempdir().unwrap();
    let mut child = bin()
        .args(["serve", "--stdio", "--root"])
        .arg(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut send = |v: serde_json::Value| writeln!(stdin, "{v}").unwrap();
    send(
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-06-18","capabilities":{},
        "clientInfo":{"name":"test","version":"0"}}}),
    );
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let init: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(
        init["result"]["serverInfo"]["name"], "emperor-pdf",
        "{line}"
    );
    send(serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    send(serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
    line.clear();
    stdout.read_line(&mut line).unwrap();
    let list: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(
        list["result"]["tools"].as_array().unwrap().len(),
        4,
        "{line}"
    );
    child.kill().unwrap();
    let _ = child.wait();
}
