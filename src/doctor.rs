//! `emperor-pdf doctor`: checks what the server needs before it starts.

use std::net::SocketAddr;
use std::path::Path;

use emperor_mcp::doctor::{Check, Report, port_free, tool_budget};

use crate::root::Root;
use crate::{pdfium, tool_count};

/// What `doctor` checks.
pub struct DoctorOptions<'a> {
    /// The root folder.
    pub root: &'a Path,
    /// Whether the server would be started with `--writable`.
    pub writable: bool,
    /// `--pdfium-dir`.
    pub pdfium_dir: Option<&'a Path>,
    /// Listen address, `None` for stdio.
    pub bind: Option<SocketAddr>,
}

/// Run every check.
pub fn run(opts: &DoctorOptions<'_>) -> Report {
    let mut report = Report::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    report.push(root_check(opts.root, opts.writable));
    report.push(match pdfium::load(opts.pdfium_dir) {
        Ok(engine) => Check::pass("pdfium", format!("loaded from {}", engine.path)),
        Err(e) => Check::warn(
            "pdfium",
            format!(
                "{e}; pdf_text and pdf_search fall back to lopdf, pdf_render_page is unavailable"
            ),
        ),
    });
    #[cfg(feature = "create")]
    report.push(fonts_check());
    match opts.bind {
        Some(addr) => {
            report.push(port_free(addr));
        }
        None => {
            report.push(Check::pass("listen address", "stdio, no port needed"));
        }
    }
    report.push(tool_budget(tool_count(opts.writable)));
    report
}

#[cfg(feature = "create")]
fn fonts_check() -> Check {
    use crate::world;
    let fonts = world::font_count();
    if fonts > 0 {
        Check::pass(
            "typst fonts",
            format!(
                "{fonts} embedded faces ({})",
                world::font_families().join(", ")
            ),
        )
    } else {
        Check::fail(
            "typst fonts",
            "no embedded fonts; pdf_create cannot typeset",
        )
    }
}

fn root_check(path: &Path, writable: bool) -> Check {
    let root = match Root::open(path) {
        Ok(r) => r,
        Err(e) => return Check::fail("root folder", format!("{e:#}")),
    };
    let shown = root.host_path().display().to_string();
    if !writable {
        return Check::pass("root folder", format!("{shown} (read-only)"));
    }
    match root.probe_writable() {
        Ok(()) => Check::pass("root folder", format!("{shown} (writable)")),
        Err(e) => Check::fail(
            "root folder",
            format!("{shown} is not writable ({e}); drop --writable or fix permissions"),
        ),
    }
}
