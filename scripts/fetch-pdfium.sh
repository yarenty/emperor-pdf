#!/usr/bin/env bash
# Download a prebuilt PDFium for this machine into ./pdfium/ (lib/ or bin/ holds the library).
#
# Binaries: https://github.com/bblanchon/pdfium-binaries (Apache-2.0 / BSD-3-Clause, built from
# Google's PDFium). The release is pinned to the PDFium API version pdfium-render is built for;
# override with PDFIUM_VERSION=<number> (e.g. 7881) or PDFIUM_VERSION=latest.
#
# Usage: scripts/fetch-pdfium.sh [target-dir]      (default: ./pdfium)
set -euo pipefail

VERSION="${PDFIUM_VERSION:-7881}"
DEST="${1:-pdfium}"

os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Darwin) platform=mac ;;
  Linux)
    platform=linux
    if ldd --version 2>&1 | grep -qi musl; then platform=linux-musl; fi
    ;;
  MINGW*|MSYS*|CYGWIN*|Windows_NT) platform=win ;;
  *) echo "unsupported OS: $os" >&2; exit 1 ;;
esac
case "$arch" in
  x86_64|amd64) cpu=x64 ;;
  arm64|aarch64) cpu=arm64 ;;
  *) echo "unsupported CPU: $arch" >&2; exit 1 ;;
esac

asset="pdfium-${platform}-${cpu}.tgz"
if [ "$VERSION" = "latest" ]; then
  url="https://github.com/bblanchon/pdfium-binaries/releases/latest/download/${asset}"
else
  url="https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F${VERSION}/${asset}"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "downloading $url"
curl -fsSL --retry 3 -o "$tmp/$asset" "$url"
mkdir -p "$DEST"
tar -xzf "$tmp/$asset" -C "$DEST"

lib="$(find "$DEST" -maxdepth 2 \( -name 'libpdfium.dylib' -o -name 'libpdfium.so' -o -name 'pdfium.dll' \) | head -n1)"
if [ -z "$lib" ]; then
  echo "archive did not contain a PDFium library" >&2
  exit 1
fi
echo "PDFium $VERSION installed: $lib"
echo "use it with: --pdfium-dir $DEST   (or PDFIUM_DYNAMIC_LIB_PATH=$(dirname "$lib"))"
