//! Markdown to PDF: [`crate::markdown`] produces Typst markup, typst lays it out, `typst-pdf`
//! writes the PDF.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use typst::foundations::{Datetime, Smart};
use typst_layout::PagedDocument;
use typst_pdf::{PdfOptions, Timestamp};

use crate::markdown;
use crate::world::MemoryWorld;

/// Largest Markdown input `pdf_create` accepts.
pub const MAX_MARKDOWN_BYTES: usize = 2 * 1024 * 1024;

/// Render `markdown` to PDF bytes. `title` sets the document title (metadata) and is printed
/// above the content.
pub fn markdown_to_pdf(markdown_text: &str, title: Option<&str>) -> Result<Vec<u8>> {
    if markdown_text.len() > MAX_MARKDOWN_BYTES {
        return Err(anyhow!(
            "markdown is {} bytes, more than the {MAX_MARKDOWN_BYTES}-byte limit",
            markdown_text.len()
        ));
    }
    let title = title.map(str::trim).filter(|t| !t.is_empty());
    let mut src = String::new();
    if let Some(t) = title {
        src.push_str(&format!("#set document(title: {})\n", markdown::string(t)));
    }
    src.push_str(concat!(
        "#set page(paper: \"a4\", margin: (x: 2.2cm, y: 2.4cm), numbering: \"1\")\n",
        "#set text(font: \"Libertinus Serif\", size: 11pt)\n",
        "#show raw: set text(font: \"DejaVu Sans Mono\")\n",
        "#set par(justify: true)\n",
        "#show raw.where(block: true): block.with(fill: luma(245), inset: 8pt, radius: 3pt, width: 100%)\n",
        "#show link: underline\n",
    ));
    if let Some(t) = title {
        src.push_str(&format!(
            "#align(center, text(size: 20pt, weight: \"bold\", {}))\n\n",
            markdown::string(t)
        ));
    }
    src.push_str(&markdown::to_typst(markdown_text));
    compile(src)
}

/// Compile a Typst source to PDF.
pub fn compile(source: String) -> Result<Vec<u8>> {
    let now = utc_now();
    let world = MemoryWorld::new(source, now.map(|(d, _)| d));
    let doc: PagedDocument = typst::compile(&world).output.map_err(|errors| {
        let msgs: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
        anyhow!("typesetting failed: {}", msgs.join("; "))
    })?;
    let options = PdfOptions {
        ident: Smart::Auto,
        creator: Smart::Custom(Some(format!("emperor-pdf {}", env!("CARGO_PKG_VERSION")))),
        timestamp: now.map(|(_, ts)| Timestamp::new_utc(ts)),
        ..PdfOptions::default()
    };
    typst_pdf::pdf(&doc, &options).map_err(|errors| {
        let msgs: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
        anyhow!("PDF export failed: {}", msgs.join("; "))
    })
}

/// Today's date and the current time in UTC, from the system clock.
fn utc_now() -> Option<(Datetime, Datetime)> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let date = Datetime::from_ymd(y as i32, m as u8, d as u8)?;
    let time = Datetime::from_ymd_hms(y as i32, m as u8, d as u8, h as u8, mi as u8, s as u8)?;
    Some((date, time))
}

/// Days since 1970-01-01 to a proleptic Gregorian date (H. Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_758), (2026, 11, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }
}
