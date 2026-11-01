//! Page range syntax shared by the tools: `"1-3,5,8-"` (1-based, inclusive, open end allowed).

use anyhow::{Result, bail};

/// A page range that does not fit the document (as opposed to a document that cannot be read).
#[derive(Debug)]
pub struct RangeError(pub String);

impl std::fmt::Display for RangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RangeError {}

/// Resolve `spec` against a document of `count` pages into 1-based page numbers, in the order
/// written. `None` or an empty spec means every page. Duplicates are refused. Errors carry a
/// [`RangeError`].
pub fn parse(spec: Option<&str>, count: u32) -> Result<Vec<u32>> {
    parse_inner(spec, count).map_err(|e| anyhow::Error::new(RangeError(e.to_string())))
}

fn parse_inner(spec: Option<&str>, count: u32) -> Result<Vec<u32>> {
    let spec = spec.map(str::trim).unwrap_or("");
    if spec.is_empty() || spec.eq_ignore_ascii_case("all") {
        return Ok((1..=count).collect());
    }
    let mut out: Vec<u32> = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => {
                let a = if a.trim().is_empty() {
                    1
                } else {
                    num(a, part)?
                };
                let b = if b.trim().is_empty() {
                    count
                } else {
                    num(b, part)?
                };
                (a, b)
            }
            None => {
                let n = num(part, part)?;
                (n, n)
            }
        };
        if a == 0 || b == 0 {
            bail!("page numbers start at 1 (got {part:?})");
        }
        if a > b {
            bail!("range {part:?} runs backwards");
        }
        if b > count {
            bail!("page {b} is out of range: the document has {count} page(s)");
        }
        for n in a..=b {
            if out.contains(&n) {
                bail!("page {n} is listed twice in {spec:?}");
            }
            out.push(n);
        }
    }
    if out.is_empty() {
        bail!("page range {spec:?} selects no pages");
    }
    Ok(out)
}

fn num(s: &str, part: &str) -> Result<u32> {
    s.trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid page range {part:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse(None, 3).unwrap(), vec![1, 2, 3]);
        assert_eq!(parse(Some("2-"), 4).unwrap(), vec![2, 3, 4]);
        assert_eq!(parse(Some("3,1"), 4).unwrap(), vec![3, 1]);
        assert_eq!(parse(Some("1-2, 4"), 4).unwrap(), vec![1, 2, 4]);
        assert!(parse(Some("0"), 4).is_err());
        assert!(parse(Some("5"), 4).is_err());
        assert!(parse(Some("3-1"), 4).is_err());
        assert!(parse(Some("1,1"), 4).is_err());
        assert!(parse(Some("x"), 4).is_err());
    }
}
