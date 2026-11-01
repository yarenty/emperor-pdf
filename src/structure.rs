//! Document structure with `lopdf`: metadata, page count, fallback text extraction, and
//! assembling new documents from pages of existing ones (merge / extract).

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow, bail};
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::Serialize;

/// Cap on decompressed content per page during fallback text extraction (decompression bombs).
const MAX_PAGE_CONTENT: usize = 64 * 1024 * 1024;

/// Page attributes a page may inherit from its parent `Pages` node (PDF 32000 §7.7.3.4).
const INHERITABLE: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// Load a document from bytes.
pub fn load(bytes: &[u8]) -> Result<Document> {
    Document::load_mem(bytes).map_err(|e| anyhow!("not a readable PDF: {e}"))
}

/// What `pdf_info` reports.
#[derive(Debug, Serialize)]
pub struct Info {
    /// Root-relative path.
    pub path: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Number of pages.
    pub pages: u32,
    /// PDF version from the header, e.g. `1.7`.
    pub pdf_version: String,
    /// Whether the file is encrypted.
    pub encrypted: bool,
    /// `Title` from the document information dictionary.
    pub title: Option<String>,
    /// `Author`.
    pub author: Option<String>,
    /// `Subject`.
    pub subject: Option<String>,
    /// `Keywords`.
    pub keywords: Option<String>,
    /// `Creator` (the authoring application).
    pub creator: Option<String>,
    /// `Producer` (the PDF writer).
    pub producer: Option<String>,
    /// `CreationDate`, ISO 8601 when parseable.
    pub created: Option<String>,
    /// `ModDate`, ISO 8601 when parseable.
    pub modified: Option<String>,
}

/// Read the metadata of a loaded document.
pub fn info(doc: &Document, path: &str, size_bytes: u64) -> Info {
    let dict = doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|o| doc.dereference(o).ok())
        .and_then(|(_, o)| o.as_dict().ok());
    let field = |key: &[u8]| -> Option<String> {
        let obj = dict?.get(key).ok()?;
        let (_, obj) = doc.dereference(obj).ok()?;
        lopdf::decode_text_string(obj)
            .ok()
            .map(|s| s.trim_matches('\0').trim().to_string())
            .filter(|s| !s.is_empty())
    };
    Info {
        path: path.to_string(),
        size_bytes,
        pages: doc.get_pages().len() as u32,
        pdf_version: doc.version.clone(),
        encrypted: doc.was_encrypted() || doc.is_encrypted(),
        title: field(b"Title"),
        author: field(b"Author"),
        subject: field(b"Subject"),
        keywords: field(b"Keywords"),
        creator: field(b"Creator"),
        producer: field(b"Producer"),
        created: field(b"CreationDate").map(|d| pdf_date(&d)),
        modified: field(b"ModDate").map(|d| pdf_date(&d)),
    }
}

/// Turn a PDF date (`D:YYYYMMDDHHmmSSOHH'mm`) into ISO 8601; anything else is returned as is.
pub fn pdf_date(raw: &str) -> String {
    let s = raw.strip_prefix("D:").unwrap_or(raw);
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return raw.to_string();
    }
    let part = |a: usize, b: usize, d: &str| digits.get(a..b).unwrap_or(d).to_string();
    let (y, mo, da) = (part(0, 4, ""), part(4, 6, "01"), part(6, 8, "01"));
    let (h, mi, se) = (part(8, 10, "00"), part(10, 12, "00"), part(12, 14, "00"));
    let rest = &s[digits.len()..];
    let tz = match rest.chars().next() {
        Some('Z') => "Z".to_string(),
        Some(sign @ ('+' | '-')) => {
            let t: String = rest[1..].chars().filter(char::is_ascii_digit).collect();
            let th = t.get(0..2).unwrap_or("00");
            let tm = t.get(2..4).unwrap_or("00");
            format!("{sign}{th}:{tm}")
        }
        _ => String::new(),
    };
    format!("{y}-{mo}-{da}T{h}:{mi}:{se}{tz}")
}

/// Extract the text of the given 1-based pages with lopdf (the fallback when PDFium is absent).
/// Pages that fail to decode come back as an empty string.
pub fn page_texts(doc: &Document, pages: &[u32]) -> Vec<String> {
    pages
        .iter()
        .map(|&p| {
            doc.extract_text_with_limit(&[p], MAX_PAGE_CONTENT)
                .unwrap_or_default()
        })
        .collect()
}

/// Build a new document from `parts`: each is a loaded document and the 1-based pages to take
/// from it, in order. Outlines, forms and the structure tree of the inputs are not carried over.
pub fn assemble(parts: Vec<(Document, Vec<u32>)>) -> Result<Vec<u8>> {
    let mut out = Document::with_version("1.7");
    let mut kids: Vec<ObjectId> = Vec::new();
    let mut next_id = 1;
    let mut page_dicts: BTreeMap<ObjectId, Dictionary> = BTreeMap::new();

    for (mut doc, pages) in parts {
        if doc.is_encrypted() {
            bail!("encrypted PDFs cannot be merged or split");
        }
        doc.renumber_objects_with(next_id);
        next_id = doc.max_id + 1;
        let ids = doc.get_pages();
        for p in pages {
            let id = *ids.get(&p).ok_or_else(|| anyhow!("page {p} not found"))?;
            let mut dict = doc
                .get_dictionary(id)
                .context("page object is not a dictionary")?
                .clone();
            for key in INHERITABLE {
                if !dict.has(key)
                    && let Some(v) = inherited(&doc, &dict, key)
                {
                    dict.set(key, v);
                }
            }
            kids.push(id);
            page_dicts.insert(id, dict);
        }
        out.objects.extend(doc.objects);
    }

    let pages_id = (next_id, 0);
    for (id, mut dict) in page_dicts {
        dict.set("Parent", Object::Reference(pages_id));
        out.objects.insert(id, Object::Dictionary(dict));
    }
    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Count", Object::Integer(kids.len() as i64));
    pages.set(
        "Kids",
        kids.iter()
            .map(|&id| Object::Reference(id))
            .collect::<Vec<_>>(),
    );
    out.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = (next_id + 1, 0);
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    out.objects.insert(catalog_id, Object::Dictionary(catalog));
    out.trailer.set("Root", Object::Reference(catalog_id));
    out.max_id = next_id + 1;

    // Drop everything the new page tree does not reach (old catalogs, unselected pages, ...).
    out.prune_objects();
    out.renumber_objects();
    out.compress();
    let mut buf = Vec::new();
    out.save_to(&mut buf).context("cannot write the new PDF")?;
    Ok(buf)
}

/// Walk up the `Parent` chain for an inheritable attribute.
fn inherited(doc: &Document, page: &Dictionary, key: &[u8]) -> Option<Object> {
    let mut node = page.get(b"Parent").ok()?.as_reference().ok()?;
    for _ in 0..64 {
        let dict = doc.get_dictionary(node).ok()?;
        if let Ok(v) = dict.get(key) {
            return Some(v.clone());
        }
        node = dict.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_dates() {
        assert_eq!(pdf_date("D:20261101093000Z"), "2026-11-01T09:30:00Z");
        assert_eq!(
            pdf_date("D:20261101093000+01'00'"),
            "2026-11-01T09:30:00+01:00"
        );
        assert_eq!(pdf_date("D:2026"), "2026-01-01T00:00:00");
        assert_eq!(pdf_date("yesterday"), "yesterday");
    }
}
