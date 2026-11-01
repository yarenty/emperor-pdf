//! Google PDFium, loaded at runtime through `pdfium-render`.
//!
//! PDFium is a dynamic library (`libpdfium.dylib` / `libpdfium.so` / `pdfium.dll`) that is not
//! bundled: prebuilt binaries come from <https://github.com/bblanchon/pdfium-binaries>
//! (`scripts/fetch-pdfium.sh`). The library is bound once per process; when it cannot be found
//! the text tools fall back to lopdf and rendering is unavailable.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Result, anyhow, bail};
use pdfium_render::prelude::*;

/// Environment variable naming a folder (or the library file itself) to load PDFium from.
pub const ENV_VAR: &str = "PDFIUM_DYNAMIC_LIB_PATH";

/// Highest resolution `pdf_render_page` accepts.
pub const MAX_DPI: u32 = 300;

/// Longest side of a rendered page, in pixels, whatever the DPI (guards huge page sizes).
const MAX_SIDE_PX: i32 = 8000;

/// A bound PDFium library and where it was loaded from.
pub struct Engine {
    pdfium: Pdfium,
    /// The library file that was loaded (`system` when the OS loader found it by name).
    pub path: String,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").field("path", &self.path).finish()
    }
}

static ENGINE: OnceLock<std::result::Result<Engine, String>> = OnceLock::new();

/// Bind PDFium once per process. Later calls return the first result whatever `dir` they pass.
///
/// Search order: `dir` (the `--pdfium-dir` flag), the `PDFIUM_DYNAMIC_LIB_PATH` variable, the
/// folder of the running binary, then the system library search path. Inside a folder the
/// library may sit directly or under `lib/` or `bin/` (the layout of the prebuilt archives).
pub fn load(dir: Option<&Path>) -> std::result::Result<&'static Engine, &'static str> {
    ENGINE
        .get_or_init(|| bind(dir))
        .as_ref()
        .map_err(String::as_str)
}

fn bind(dir: Option<&Path>) -> std::result::Result<Engine, String> {
    let mut problems = Vec::new();
    let searched: Vec<String> = roots(dir).iter().map(|r| r.display().to_string()).collect();
    for candidate in candidates(dir) {
        if !candidate.is_file() {
            continue;
        }
        match Pdfium::bind_to_library(&candidate) {
            Ok(b) => {
                return Ok(Engine {
                    pdfium: Pdfium::new(b),
                    path: candidate.display().to_string(),
                });
            }
            Err(e) => problems.push(format!("{} does not load: {e}", candidate.display())),
        }
    }
    match Pdfium::bind_to_system_library() {
        Ok(b) => Ok(Engine {
            pdfium: Pdfium::new(b),
            path: "system".into(),
        }),
        Err(_) => {
            problems.push(format!(
                "no {} in {} or on the system library path",
                Pdfium::pdfium_platform_library_name().to_string_lossy(),
                if searched.is_empty() {
                    "the configured folders".to_string()
                } else {
                    searched.join(", ")
                }
            ));
            Err(format!(
                "PDFium not found: {}; run scripts/fetch-pdfium.sh or pass --pdfium-dir",
                problems.join("; ")
            ))
        }
    }
}

/// Folders (or library files) to search, in order.
fn roots(dir: Option<&Path>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(d) = dir {
        roots.push(d.to_path_buf());
    }
    if let Some(v) = std::env::var_os(ENV_VAR).filter(|v| !v.is_empty()) {
        roots.push(PathBuf::from(v));
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        roots.push(exe_dir);
    }
    roots
}

/// Library files to try, in order.
fn candidates(dir: Option<&Path>) -> Vec<PathBuf> {
    let name = Pdfium::pdfium_platform_library_name();
    let mut out = Vec::new();
    for r in roots(dir) {
        if r.is_file() {
            out.push(r);
            continue;
        }
        for sub in ["", "lib", "bin"] {
            out.push(r.join(sub).join(&name));
        }
    }
    out
}

impl Engine {
    /// Number of pages.
    pub fn page_count(&self, bytes: &[u8]) -> Result<u32> {
        Ok(self.open(bytes)?.pages().len() as u32)
    }

    /// Page count and the text of the pages `select` picks (1-based) given the page count.
    pub fn page_texts(
        &self,
        bytes: &[u8],
        select: impl FnOnce(u32) -> Result<Vec<u32>>,
    ) -> Result<(u32, Vec<(u32, String)>)> {
        let doc = self.open(bytes)?;
        let count = doc.pages().len() as u32;
        let texts = select(count)?
            .into_iter()
            .map(|p| {
                let page = doc
                    .pages()
                    .get(index(p)?)
                    .map_err(|e| anyhow!("page {p}: {e}"))?;
                let text = page.text().map_err(|e| anyhow!("page {p}: {e}"))?;
                Ok((p, text.all()))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((count, texts))
    }

    /// Render a 1-based page at `dpi` to PNG bytes; returns `(png, width, height)`.
    pub fn render_png(&self, bytes: &[u8], page: u32, dpi: u32) -> Result<(Vec<u8>, u32, u32)> {
        if dpi == 0 || dpi > MAX_DPI {
            bail!("dpi must be between 1 and {MAX_DPI}");
        }
        let doc = self.open(bytes)?;
        let count = doc.pages().len() as u32;
        if page == 0 || page > count {
            bail!("page {page} is out of range: the document has {count} page(s)");
        }
        let pdf_page = doc
            .pages()
            .get(index(page)?)
            .map_err(|e| anyhow!("page {page}: {e}"))?;
        let config = PdfRenderConfig::new()
            .scale_page_by_factor(dpi as f32 / 72.0)
            .set_maximum_width(MAX_SIDE_PX)
            .set_maximum_height(MAX_SIDE_PX)
            .set_format(PdfBitmapFormat::BGRA)
            .render_form_data(true)
            .render_annotations(true);
        let bitmap = pdf_page
            .render_with_config(&config)
            .map_err(|e| anyhow!("render failed: {e}"))?;
        let (w, h) = (bitmap.width() as u32, bitmap.height() as u32);
        let rgba = bitmap.as_rgba_bytes();
        Ok((encode_png(&rgba, w, h)?, w, h))
    }

    fn open<'a>(&'a self, bytes: &'a [u8]) -> Result<PdfDocument<'a>> {
        self.pdfium
            .load_pdf_from_byte_slice(bytes, None)
            .map_err(|e| match e {
                PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
                    anyhow!("the PDF is password protected")
                }
                e => anyhow!("PDFium cannot open the file: {e}"),
            })
    }
}

fn index(page: u32) -> Result<PdfPageIndex> {
    PdfPageIndex::try_from(page.saturating_sub(1)).map_err(|_| anyhow!("page {page} too large"))
}

/// Encode RGBA8 pixels as PNG.
fn encode_png(rgba: &[u8], w: u32, h: u32) -> Result<Vec<u8>> {
    if rgba.len() != (w as usize) * (h as usize) * 4 {
        bail!("unexpected bitmap size from PDFium");
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header()?;
        writer.write_image_data(rgba)?;
    }
    Ok(out)
}
