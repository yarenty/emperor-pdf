//! The one folder the server may touch.
//!
//! Every file is opened through a [`cap_std::fs::Dir`] by a root-relative path, so the operating
//! system (not string checks alone) keeps the server inside the root: `..`, absolute paths and
//! symlinks that lead outside are refused by cap-std. [`Root::check`] refuses the obvious cases
//! up front with a clear message.

use std::io::Write;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};

/// Largest file the server reads into memory.
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// Most files a glob may expand to.
pub const MAX_GLOB_FILES: usize = 200;

/// Deepest directory level a glob walks.
const MAX_GLOB_DEPTH: usize = 16;

/// A capability handle on the root folder.
#[derive(Debug)]
pub struct Root {
    dir: Dir,
    path: PathBuf,
}

impl Root {
    /// Open `path` as the root. This is the only place the server uses ambient authority.
    pub fn open(path: &Path) -> Result<Self> {
        let dir = Dir::open_ambient_dir(path, ambient_authority())
            .with_context(|| format!("cannot open root folder {}", path.display()))?;
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        Ok(Self { dir, path })
    }

    /// The root folder on the host (for logs and `doctor`, never for opening files).
    pub fn host_path(&self) -> &Path {
        &self.path
    }

    /// Validate a root-relative path: not empty, not absolute, no `..`.
    pub fn check(rel: &str) -> Result<PathBuf> {
        let rel = rel.trim();
        if rel.is_empty() {
            bail!("path is empty");
        }
        let p = Path::new(rel);
        for c in p.components() {
            match c {
                Component::Normal(_) | Component::CurDir => {}
                Component::ParentDir => {
                    bail!(
                        "path {rel:?} is refused: `..` is not allowed, paths are relative to the root folder"
                    )
                }
                Component::RootDir | Component::Prefix(_) => {
                    bail!(
                        "path {rel:?} is refused: absolute paths are not allowed, paths are relative to the root folder"
                    )
                }
            }
        }
        Ok(p.to_path_buf())
    }

    /// Read a whole file.
    pub fn read(&self, rel: &str) -> Result<Vec<u8>> {
        let p = Self::check(rel)?;
        let meta = self
            .dir
            .metadata(&p)
            .with_context(|| format!("cannot open {rel}"))?;
        if meta.is_dir() {
            bail!("{rel} is a folder, not a file");
        }
        if meta.len() > MAX_FILE_BYTES {
            bail!(
                "{rel} is {} bytes, more than the {MAX_FILE_BYTES}-byte limit",
                meta.len()
            );
        }
        self.dir
            .read(&p)
            .with_context(|| format!("cannot read {rel}"))
    }

    /// Write a file, creating parent folders. Refuses to replace an existing file unless
    /// `overwrite`.
    pub fn write(&self, rel: &str, bytes: &[u8], overwrite: bool) -> Result<()> {
        let p = Self::check(rel)?;
        if let Some(parent) = p.parent().filter(|p| !p.as_os_str().is_empty()) {
            self.dir
                .create_dir_all(parent)
                .with_context(|| format!("cannot create folder for {rel}"))?;
        }
        let mut opts = OpenOptions::new();
        opts.write(true);
        if overwrite {
            opts.create(true).truncate(true);
        } else {
            opts.create_new(true);
        }
        let mut f = self.dir.open_with(&p, &opts).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                anyhow::anyhow!("{rel} already exists; pass overwrite: true to replace it")
            } else {
                anyhow::Error::new(e).context(format!("cannot write {rel}"))
            }
        })?;
        f.write_all(bytes)
            .with_context(|| format!("cannot write {rel}"))?;
        Ok(())
    }

    /// Check that the root can be written to, by creating and removing a probe file.
    pub fn probe_writable(&self) -> Result<()> {
        let name = format!(".emperor-pdf-probe-{}", std::process::id());
        self.dir.write(&name, b"")?;
        self.dir.remove_file(&name)?;
        Ok(())
    }

    /// Expand `pattern` to the files it names, sorted. A pattern without glob characters names
    /// one file and is returned as is (after [`Root::check`]).
    pub fn expand(&self, pattern: &str) -> Result<Vec<String>> {
        Self::check(pattern)?;
        if !pattern.contains(['*', '?', '[']) {
            return Ok(vec![pattern.trim().to_string()]);
        }
        let pat = glob::Pattern::new(pattern.trim()).context("invalid glob pattern")?;
        let opts = glob::MatchOptions {
            case_sensitive: true,
            require_literal_separator: true,
            require_literal_leading_dot: true,
        };
        let mut out = Vec::new();
        walk(&self.dir, "", 0, &mut |rel| {
            if pat.matches_with(rel, opts) {
                out.push(rel.to_string());
            }
            out.len() < MAX_GLOB_FILES
        })?;
        out.sort();
        Ok(out)
    }
}

/// Visit every file under `dir` (relative names with `/`); stop when `f` returns false.
fn walk(dir: &Dir, prefix: &str, depth: usize, f: &mut dyn FnMut(&str) -> bool) -> Result<bool> {
    if depth > MAX_GLOB_DEPTH {
        return Ok(true);
    }
    let mut entries: Vec<_> = dir.entries()?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let ft = e.file_type()?;
        if ft.is_dir() {
            // Hidden folders (.git, ...) are not searched.
            if name.starts_with('.') {
                continue;
            }
            let sub = e.open_dir()?;
            if !walk(&sub, &rel, depth + 1, f)? {
                return Ok(false);
            }
        } else if ft.is_file() && !f(&rel) {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_refuses_escapes() {
        assert!(Root::check("a/b.pdf").is_ok());
        assert!(Root::check("./a.pdf").is_ok());
        assert!(Root::check("../a.pdf").is_err());
        assert!(Root::check("a/../../b.pdf").is_err());
        assert!(Root::check("/etc/passwd").is_err());
        assert!(Root::check("").is_err());
    }
}
