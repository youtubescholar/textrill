//! Installing packs (roadmap §5.4 and §6): one staged path every install
//! takes, so a refused or half-copied pack never appears in the catalog.
//!
//! The rule that shapes this module: nothing lands in the user's data
//! directory until it has been staged beside it, validated by
//! [`validate::load`], and (for a catalog install) hash-checked against the
//! index. The rename into place is the only step that makes a pack visible,
//! and it happens last. Catalog installs additionally refuse an entry with no
//! `sha256`, refuse a hash mismatch, refuse an unmet `min_textrill` naming
//! the reason, and refuse an archive whose manifest id differs from the id
//! the catalog lists it under. Covered by `tests/packtest.rs`.

use std::fs;
use std::path::Path;

use super::archive;
use super::catalog::{self, CatalogIndexEntry, VERSION};
use super::validate;

/// Install a pack from a directory on disk into `data_home/<id>`.
pub fn from_dir(source: &Path, data_home: &Path) -> Result<String, String> {
    let source = fs::canonicalize(source).map_err(|e| format!("{}: {e}", source.display()))?;
    install_staged(data_home, None, |staging| copy_tree(&source, staging))
}

/// Install a pack from a deterministic uncompressed `.tar` into
/// `data_home/<id>`.
pub fn from_archive(bytes: &[u8], data_home: &Path) -> Result<String, String> {
    install_staged(data_home, None, |staging| {
        archive::unpack(bytes, staging).map(|_| ())
    })
}

/// Install the pack a catalog entry names, fully verified: the archive must
/// exist inside the catalog, its bytes must hash to the catalog's `sha256`,
/// the engine must satisfy `min_textrill`, and the unpacked manifest must
/// carry the id the catalog lists. Nothing is installed unverified.
pub fn from_catalog_entry(entry: &CatalogIndexEntry, data_home: &Path) -> Result<String, String> {
    let min = entry.min_textrill.as_deref().unwrap_or("0");
    if !catalog::version_satisfies(VERSION, min)? {
        return Err(format!(
            "{} {} requires textrill >= {min}; this is textrill {VERSION}",
            entry.id, entry.version
        ));
    }
    let expect = entry.sha256.as_deref().ok_or_else(|| {
        format!(
            "the catalog entry for {} {} carries no sha256; an unverified \
             pack is not installed",
            entry.id, entry.version
        )
    })?;
    let bytes = fs::read(&entry.archive).map_err(|e| {
        format!(
            "{} {}: {}: {e}",
            entry.id,
            entry.version,
            entry.archive.display()
        )
    })?;
    let got = crate::pack::sha256::sha256_hex(&bytes);
    if !expect.eq_ignore_ascii_case(&got) {
        return Err(format!(
            "hash mismatch for {} {}: the catalog says {expect}, the archive \
             hashes to {got}",
            entry.id, entry.version
        ));
    }
    install_staged(data_home, Some(&entry.id), |staging| {
        archive::unpack(&bytes, staging).map(|_| ())
    })
}

/// The one install path: fill a staging directory beside `data_home`, validate
/// it, check it is the pack the caller expects, and only then rename it into
/// place. The staging directory is always removed, success or failure.
fn install_staged(
    data_home: &Path,
    expected_id: Option<&str>,
    fill: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<String, String> {
    fs::create_dir_all(data_home).map_err(|e| format!("{}: {e}", data_home.display()))?;
    let staging = data_home.join(format!(".install-{}-{}", std::process::id(), next_seq()));
    let result = (|| -> Result<String, String> {
        fs::create_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
        fill(&staging)?;
        let validated = validate::load(&staging)?;
        if let Some(want) = expected_id {
            if validated.id != want {
                return Err(format!(
                    "the archive installs {} {}, but the catalog lists it as \
                     {want}",
                    validated.id, validated.version
                ));
            }
        }
        let dest = data_home.join(&validated.id);
        if dest.exists() {
            return Err(format!(
                "install: {} is already installed at {}; remove it first",
                validated.id,
                dest.display()
            ));
        }
        fs::rename(&staging, &dest)
            .map_err(|e| format!("install: could not move {} into place: {e}", validated.id))?;
        Ok(format!(
            "installed {} at {}\n",
            validated.id,
            dest.display()
        ))
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

/// Copy a directory tree, refusing symlinks: an install that could be
/// redirected outside the pack would undo the whole point of validation.
fn copy_tree(src: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let entries = fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", src.display()))?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        let meta = entry
            .metadata()
            .map_err(|e| format!("{}: {e}", from.display()))?;
        if meta.is_symlink() {
            return Err(format!("{}: symlinks are not installed", from.display()));
        }
        if meta.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
        }
    }
    Ok(())
}

/// A per-process counter so two concurrent installs never share a staging
/// directory name.
fn next_seq() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    SEQ.fetch_add(1, Ordering::Relaxed)
}
