#!/usr/bin/env bash
# Smoke-test a built emperor-pdf image over stdio:
#   scripts/docker-smoke.sh [image]   (default: emperor-pdf:smoke)
# Read-only: stdout is JSON-RPC only, four tools are listed, pdf_text answers through PDFium.
# PDF_WRITABLE=true: six tools are listed.
set -euo pipefail

image="${1:-emperor-pdf:smoke}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
chmod 755 "$tmp"

# A one-page PDF with a known sentence, offsets computed so the xref table is exact.
python3 - "$tmp/hello.pdf" <<'PY'
import sys
text = b"Hello from the emperor-pdf smoke test"
stream = b"BT /F1 18 Tf 72 720 Td (" + text + b") Tj ET"
objs = [
    b"<< /Type /Catalog /Pages 2 0 R >>",
    b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
    b"/Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    b"<< /Length " + str(len(stream)).encode() + b" >>\nstream\n" + stream + b"\nendstream",
    b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
]
out = bytearray(b"%PDF-1.4\n")
offsets = []
for i, body in enumerate(objs, 1):
    offsets.append(len(out))
    out += f"{i} 0 obj\n".encode() + body + b"\nendobj\n"
xref = len(out)
out += f"xref\n0 {len(objs) + 1}\n0000000000 65535 f \n".encode()
for off in offsets:
    out += f"{off:010d} 00000 n \n".encode()
out += f"trailer\n<< /Size {len(objs) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
open(sys.argv[1], "wb").write(out)
PY
chmod 644 "$tmp/hello.pdf"

timeout=""
command -v timeout >/dev/null && timeout="timeout 120"

session() {
  # Keep stdin open until the answers are in, then close it so the server exits.
  {
    printf '%s\n' \
      '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}' \
      '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
      '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
      '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"pdf_text","arguments":{"path":"hello.pdf"}}}'
    sleep "${SMOKE_WAIT:-10}"
  } | ${timeout} docker run -i --rm "$@" -v "$tmp:/data:ro" "$image"
}

check() {
  python3 - "$1" "$2" <<'PY'
import json, sys
path, want_tools = sys.argv[1], int(sys.argv[2])
lines = [l for l in open(path).read().splitlines() if l.strip()]
msgs = {}
for l in lines:
    m = json.loads(l)  # anything that is not JSON fails here
    assert m.get("jsonrpc") == "2.0", f"not JSON-RPC: {l}"
    if "id" in m:
        msgs[m["id"]] = m
assert set(msgs) == {1, 2, 3}, f"responses: {sorted(msgs)}"
assert msgs[1]["result"]["serverInfo"]["name"] == "emperor-pdf", msgs[1]
tools = sorted(t["name"] for t in msgs[2]["result"]["tools"])
assert len(tools) == want_tools, tools
for name in ["pdf_info", "pdf_render_page", "pdf_search", "pdf_text"]:
    assert name in tools, tools
if want_tools == 6:
    assert "pdf_pages" in tools and "pdf_create" in tools, tools
r = msgs[3]["result"]
assert r.get("isError") is False, r
texts = "\n".join(c.get("text", "") for c in r["content"])
assert '"extractor": "pdfium"' in texts, texts
assert "Hello from the emperor-pdf smoke test" in texts, texts
print(f"ok: {len(tools)} tools ({', '.join(tools)}); pdf_text via PDFium")
PY
}

echo "== read-only"
session > "$tmp/ro.out"
check "$tmp/ro.out" 4

echo "== PDF_WRITABLE=true"
session -e PDF_WRITABLE=true > "$tmp/rw.out"
check "$tmp/rw.out" 6
