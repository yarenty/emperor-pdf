//! Where a reading tool gets its PDF: a root-relative `path`, the bytes themselves in the call
//! (`data`, base64), or an `https://` `url`.
//!
//! `data` and `url` are capped at the emperor-mcp [`Limits`] byte cap (8 MiB by default). A URL
//! is fetched over HTTPS only, with a timeout, at most five redirects (each checked again), and
//! only from public addresses: loopback, private, link-local and similar ranges are refused, both
//! as literal IPs and as what a host name resolves to, so a call cannot reach services on the
//! server's own network.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use emperor_mcp::limits::Limits;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use crate::root::Root;

/// Redirects followed before a fetch gives up.
const MAX_REDIRECTS: usize = 5;

/// How URLs are fetched.
#[derive(Debug, Clone, Copy)]
pub struct FetchPolicy {
    /// Byte cap for `data` and `url` (from emperor-mcp [`Limits`]).
    pub limits: Limits,
    /// Whole-request timeout.
    pub timeout: Duration,
    /// Accept `http://` and loopback or private addresses. For tests against a local server;
    /// the command line never sets it.
    pub allow_local_http: bool,
}

impl Default for FetchPolicy {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            timeout: Duration::from_secs(30),
            allow_local_http: false,
        }
    }
}

/// An HTTP client set up by a [`FetchPolicy`].
#[derive(Debug)]
pub struct Fetcher {
    policy: FetchPolicy,
    client: reqwest::Client,
}

impl Fetcher {
    /// A client for `policy`.
    pub fn new(policy: FetchPolicy) -> Result<Self> {
        // reqwest is built without a bundled crypto provider; use ring (a no-op when another
        // part of the process installed one first).
        let _ = rustls::crypto::ring::default_provider().install_default();
        let allow = policy.allow_local_http;
        let mut builder = reqwest::Client::builder()
            .timeout(policy.timeout)
            .connect_timeout(policy.timeout.min(Duration::from_secs(10)))
            .user_agent(concat!("emperor-pdf/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    attempt.error(format!("more than {MAX_REDIRECTS} redirects"))
                } else if let Err(e) = check_url(attempt.url(), allow) {
                    attempt.error(e.to_string())
                } else {
                    attempt.follow()
                }
            }));
        if !allow {
            builder = builder.https_only(true).dns_resolver(PublicOnly);
        }
        let client = builder.build().context("cannot set up the HTTP client")?;
        Ok(Self { policy, client })
    }

    /// The byte cap for `data` and `url`.
    pub fn max_bytes(&self) -> u64 {
        self.policy.limits.max_bytes()
    }

    /// Download `url`, refusing anything over the byte cap.
    pub async fn get(&self, url: &str) -> Result<Vec<u8>> {
        let parsed = reqwest::Url::parse(url).map_err(|e| anyhow!("invalid url: {e}"))?;
        check_url(&parsed, self.policy.allow_local_http)?;
        let max = self.max_bytes();
        let mut resp = self
            .client
            .get(parsed)
            .send()
            .await
            .map_err(|e| anyhow!("cannot fetch {url}: {}", chain(&e)))?;
        if !resp.status().is_success() {
            bail!("{url} answered HTTP {}", resp.status());
        }
        if let Some(len) = resp.content_length()
            && !self.policy.limits.bytes_allowed(len)
        {
            bail!("{url} is {len} bytes, more than the {max}-byte limit");
        }
        let mut body = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| anyhow!("cannot fetch {url}: {}", chain(&e)))?
        {
            body.extend_from_slice(&chunk);
            if !self.policy.limits.bytes_allowed(body.len() as u64) {
                bail!("{url} is more than the {max}-byte limit");
            }
        }
        check_pdf(&body).with_context(|| format!("{url} did not return a PDF"))?;
        Ok(body)
    }
}

/// An error and its causes on one line (reqwest's own message hides the cause).
fn chain(e: &dyn std::error::Error) -> String {
    let mut out = e.to_string();
    let mut cur = e.source();
    while let Some(s) = cur {
        out.push_str(": ");
        out.push_str(&s.to_string());
        cur = s.source();
    }
    out
}

/// Where one PDF comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Root-relative path (or, for `pdf_search`, a glob).
    Path(String),
    /// Base64 bytes from the call.
    Data(String),
    /// An `https://` URL.
    Url(String),
}

impl Source {
    /// Exactly one of `path`, `data`, `url`.
    pub fn pick(path: Option<String>, data: Option<String>, url: Option<String>) -> Result<Self> {
        match (path, data, url) {
            (Some(p), None, None) => Ok(Self::Path(p)),
            (None, Some(d), None) => Ok(Self::Data(d)),
            (None, None, Some(u)) => Ok(Self::Url(u)),
            _ => bail!("give exactly one of `path`, `data` (base64 PDF bytes) or `url` (https)"),
        }
    }

    /// Fetch or decode `data` and `url`; a path is read later, on the blocking pool.
    pub async fn resolve(self, fetcher: &Fetcher) -> Result<Resolved> {
        match self {
            Self::Path(p) => Ok(Resolved::Path(p)),
            Self::Data(d) => Ok(Resolved::Bytes {
                label: "data".into(),
                bytes: decode_data(&d, fetcher.max_bytes())?,
            }),
            Self::Url(u) => {
                let bytes = fetcher.get(&u).await?;
                Ok(Resolved::Bytes { label: u, bytes })
            }
        }
    }
}

/// A source ready to read.
#[derive(Debug)]
pub enum Resolved {
    /// A root-relative path, read through [`Root`].
    Path(String),
    /// Bytes already in memory and a label for results (the URL, or `data`).
    Bytes {
        /// The URL, or `data`.
        label: String,
        /// The PDF.
        bytes: Vec<u8>,
    },
}

impl Resolved {
    /// The path, URL, or `data`, for results and messages.
    pub fn label(&self) -> &str {
        match self {
            Self::Path(p) => p,
            Self::Bytes { label, .. } => label,
        }
    }

    /// Fields naming the source in a JSON result: `path`, `url`, or `data_bytes`.
    pub fn describe(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut m = serde_json::Map::new();
        match self {
            Self::Path(p) => m.insert("path".into(), p.clone().into()),
            Self::Bytes { label, bytes } if label == "data" => {
                m.insert("data_bytes".into(), bytes.len().into())
            }
            Self::Bytes { label, .. } => m.insert("url".into(), label.clone().into()),
        };
        m
    }

    /// The PDF bytes.
    pub fn read(self, root: &Root) -> Result<Vec<u8>> {
        match self {
            Self::Path(p) => root.read(&p),
            Self::Bytes { bytes, .. } => Ok(bytes),
        }
    }
}

/// Decode base64 PDF bytes; a `data:...;base64,` prefix, whitespace and missing padding are
/// accepted.
pub fn decode_data(data: &str, max_bytes: u64) -> Result<Vec<u8>> {
    let mut s = data.trim();
    if s.starts_with("data:") {
        s = s
            .split_once(',')
            .map(|(_, rest)| rest)
            .ok_or_else(|| anyhow!("data: URL without a comma"))?;
    }
    let compact: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if compact.is_empty() {
        bail!("data is empty");
    }
    let approx = (compact.len() as u64 / 4) * 3;
    if approx > max_bytes + 3 {
        bail!("data is about {approx} bytes, more than the {max_bytes}-byte limit");
    }
    let engine = GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
    );
    let bytes = engine
        .decode(compact.as_bytes())
        .map_err(|e| anyhow!("data is not valid base64: {e}"))?;
    if bytes.len() as u64 > max_bytes {
        bail!(
            "data is {} bytes, more than the {max_bytes}-byte limit",
            bytes.len()
        );
    }
    check_pdf(&bytes).context("data is not a PDF")?;
    Ok(bytes)
}

/// A PDF starts with `%PDF-` (within the first KiB, as readers allow).
fn check_pdf(bytes: &[u8]) -> Result<()> {
    let head = &bytes[..bytes.len().min(1024)];
    if head.windows(5).any(|w| w == b"%PDF-") {
        Ok(())
    } else {
        bail!("no %PDF- header")
    }
}

/// Scheme, credentials and literal-address checks for a URL (and every redirect).
fn check_url(url: &reqwest::Url, allow_local: bool) -> Result<()> {
    match url.scheme() {
        "https" => {}
        "http" if allow_local => {}
        s => bail!("only https:// URLs are fetched, not {s}://"),
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("URLs with user names or passwords are refused");
    }
    let host = url.host_str().ok_or_else(|| anyhow!("URL has no host"))?;
    if allow_local {
        return Ok(());
    }
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<IpAddr>()
        && !is_public(ip)
    {
        bail!("{host} is a private or local address; refused");
    }
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") {
        bail!("{host} is a local address; refused");
    }
    Ok(())
}

/// Resolves host names and keeps only public addresses.
struct PublicOnly;

impl Resolve for PublicOnly {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let host = name.as_str().to_string();
            let found: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            let public: Vec<SocketAddr> = found.into_iter().filter(|a| is_public(a.ip())).collect();
            if public.is_empty() {
                return Err(format!(
                    "{host} does not resolve to a public address; private and local addresses are refused"
                )
                .into());
            }
            Ok(Box::new(public.into_iter()) as Addrs)
        })
    }
}

/// Globally routable unicast addresses only.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_public_v4(v4),
            None => is_public_v6(v6),
        },
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || a == 0
        || a >= 240
        || (a == 100 && (b & 0xc0) == 64) // 100.64.0.0/10 shared address space
        || (a == 192 && b == 0 && c == 0) // 192.0.0.0/24 protocol assignments
        || (a == 198 && (b & 0xfe) == 18)) // 198.18.0.0/15 benchmarking
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00 // unique local
        || (s[0] & 0xffc0) == 0xfe80 // link-local
        || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
        || (s[0] == 0x64 && s[1] == 0xff9b)) // NAT64, may reach private IPv4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_one_source() {
        assert!(Source::pick(Some("a.pdf".into()), None, None).is_ok());
        assert!(Source::pick(None, None, None).is_err());
        assert!(Source::pick(Some("a".into()), Some("b".into()), None).is_err());
        assert!(Source::pick(None, Some("b".into()), Some("c".into())).is_err());
    }

    #[test]
    fn data_is_decoded_and_checked() {
        let pdf = b"%PDF-1.7\n...";
        let b64 = base64::engine::general_purpose::STANDARD.encode(pdf);
        assert_eq!(decode_data(&b64, 1024).unwrap(), pdf);
        let prefixed = format!("data:application/pdf;base64,{}", b64.trim_end_matches('='));
        assert_eq!(decode_data(&prefixed, 1024).unwrap(), pdf);
        assert!(
            decode_data(&b64, 4)
                .unwrap_err()
                .to_string()
                .contains("limit")
        );
        let not_pdf = base64::engine::general_purpose::STANDARD.encode(b"hello world");
        assert!(decode_data(&not_pdf, 1024).is_err());
        assert!(decode_data("!!!", 1024).is_err());
    }

    #[test]
    fn only_public_addresses() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn urls_are_checked() {
        let u = |s: &str| reqwest::Url::parse(s).unwrap();
        assert!(check_url(&u("https://example.org/a.pdf"), false).is_ok());
        assert!(check_url(&u("http://example.org/a.pdf"), false).is_err());
        assert!(check_url(&u("file:///etc/passwd"), false).is_err());
        assert!(check_url(&u("https://127.0.0.1/a.pdf"), false).is_err());
        assert!(check_url(&u("https://[::1]/a.pdf"), false).is_err());
        assert!(check_url(&u("https://localhost/a.pdf"), false).is_err());
        assert!(check_url(&u("https://user:pw@example.org/a.pdf"), false).is_err());
        assert!(check_url(&u("http://127.0.0.1:8080/a.pdf"), true).is_ok());
    }
}
