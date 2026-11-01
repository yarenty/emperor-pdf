//! The smallest [`typst::World`] that compiles one in-memory source.
//!
//! No file system, no packages, no system fonts: the only file is `main.typ`, fonts are the
//! ones embedded by `typst-assets`, so a PDF looks the same on every machine and a document
//! cannot read anything from disk.

use std::sync::LazyLock;

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

/// Fonts embedded in the binary, parsed once.
struct Fonts {
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

static FONTS: LazyLock<Fonts> = LazyLock::new(|| {
    let fonts: Vec<Font> = typst_assets::fonts()
        .flat_map(|data| Font::iter(Bytes::new(data)))
        .collect();
    Fonts {
        book: LazyHash::new(FontBook::from_fonts(&fonts)),
        fonts,
    }
});

static LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| LazyHash::new(Library::default()));

/// Number of embedded font faces (for `doctor`).
pub fn font_count() -> usize {
    FONTS.fonts.len()
}

/// Font family names of the embedded fonts (for `doctor`).
pub fn font_families() -> Vec<String> {
    let mut names: Vec<String> = FONTS
        .book
        .families()
        .map(|(name, _)| name.to_string())
        .collect();
    names.dedup();
    names
}

/// A world holding a single source file.
pub struct MemoryWorld {
    main: Source,
    today: Option<Datetime>,
}

impl MemoryWorld {
    /// A world whose main file is `text`; `today` answers `datetime.today()`.
    pub fn new(text: String, today: Option<Datetime>) -> Self {
        let id = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("main.typ").expect("static path"),
        )
        .intern();
        Self {
            main: Source::new(id, text),
            today,
        }
    }
}

impl World for MemoryWorld {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &FONTS.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::AccessDenied)
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main.id() {
            Ok(Bytes::from_string(self.main.clone()))
        } else {
            Err(FileError::AccessDenied)
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        self.today
    }
}
