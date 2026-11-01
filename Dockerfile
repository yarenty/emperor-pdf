# syntax=docker/dockerfile:1
# emperor-pdf over stdio, with PDFium, for linux/amd64 and linux/arm64.
#
#   docker build -t emperor-pdf .
#   docker run -i --rm -v "$PWD/papers:/data:ro" emperor-pdf
#
# The PDFs go in /data (the root folder). Add `--root /data --writable` after the image name to
# offer the writing tools (the mount must then be writable by uid 10001).

FROM rust:1.92-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked \
    && cp target/release/emperor-pdf /usr/local/bin/emperor-pdf

# Prebuilt PDFium from bblanchon/pdfium-binaries (Apache-2.0 / BSD-3-Clause), pinned to the
# release scripts/fetch-pdfium.sh uses, checked against its sha256.
FROM debian:bookworm-slim AS pdfium
ARG TARGETARCH
ARG PDFIUM_VERSION=7881
ARG PDFIUM_SHA256_AMD64=1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d
ARG PDFIUM_SHA256_ARM64=ee7f7b7d5468958336a818c1cd580bdd20972846b7377b13f9a923d92d1d4674
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
RUN set -eu; \
    case "${TARGETARCH:-amd64}" in \
      amd64) cpu=x64; sum="$PDFIUM_SHA256_AMD64" ;; \
      arm64) cpu=arm64; sum="$PDFIUM_SHA256_ARM64" ;; \
      *) echo "unsupported architecture: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    curl -fsSL --retry 3 -o /tmp/pdfium.tgz \
      "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F${PDFIUM_VERSION}/pdfium-linux-${cpu}.tgz"; \
    echo "$sum  /tmp/pdfium.tgz" | sha256sum -c -; \
    mkdir -p /tmp/pdfium /opt/pdfium; \
    tar -xzf /tmp/pdfium.tgz -C /tmp/pdfium; \
    cp /tmp/pdfium/lib/libpdfium.so /tmp/pdfium/LICENSE /opt/pdfium/; \
    cp -r /tmp/pdfium/licenses /opt/pdfium/licenses

FROM debian:bookworm-slim
LABEL org.opencontainers.image.title="emperor-pdf" \
      org.opencontainers.image.description="MCP server for PDFs in one folder: read, search, render, merge, extract and create from Markdown" \
      org.opencontainers.image.source="https://github.com/yarenty/emperor-pdf" \
      org.opencontainers.image.url="https://github.com/yarenty/emperor-pdf" \
      org.opencontainers.image.licenses="MIT OR Apache-2.0"
# ca-certificates: `url` sources verify HTTPS against the system trust store.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin emperor \
    && mkdir -p /data
COPY --from=pdfium /opt/pdfium /opt/pdfium
COPY --from=build /usr/local/bin/emperor-pdf /usr/local/bin/emperor-pdf
COPY LICENSE-MIT LICENSE-APACHE /usr/share/doc/emperor-pdf/
USER emperor
WORKDIR /data
ENTRYPOINT ["emperor-pdf", "serve", "--stdio", "--pdfium-dir", "/opt/pdfium"]
CMD ["--root", "/data"]
