//! Deterministic uncompressed tar for packs (roadmap §5.4).
//!
//! Core ships directory packs plus an uncompressed `.tar` only: gzip and zip
//! need DEFLATE, which a dependency-free engine would have to hand-roll — a
//! large, security-sensitive surface — so users unpack `.tar.gz`/`.zip` with
//! `tar`/`unzip` first. [`pack`] produces POSIX ustar with sorted entries, a
//! fixed mtime, zero owners and normalized modes (files 0644, directories
//! 0755), so the same tree archives to the same bytes on any machine and
//! round-trips through [`unpack`] byte for byte.
//!
//! [`unpack`] treats an archive as hostile input: it refuses paths that would
//! leave the destination, symlink and hardlink entries, special files,
//! executable bits on files, duplicate entries, corrupted headers and anything
//! after the end marker — the payloads a shared archive might carry. Covered
//! by the tests below.

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use super::validate;

const BLOCK: usize = 512;
/// Every entry gets this mtime, so no clock or file timestamp can change the
/// output bytes.
const MTIME: u64 = 0;
const FILE_MODE: u64 = 0o644;
const DIR_MODE: u64 = 0o755;
const TYPE_FILE: u8 = b'0';
const TYPE_DIR: u8 = b'5';

/// Pack a pack directory into a deterministic uncompressed ustar archive.
///
/// The directory must validate first ([`validate::load`]), and symlinks are
/// refused even though a directory pack may contain them: an archive with
/// link entries is exactly the shape [`unpack`] will not consume, so this
/// fails at the source instead.
pub fn pack(root: &Path) -> Result<Vec<u8>, String> {
    let root = fs::canonicalize(root).map_err(|e| format!("{}: {e}", root.display()))?;
    if !root.is_dir() {
        return Err(format!("{}: not a directory", root.display()));
    }
    validate::load(&root)?;
    let mut entries = Vec::new();
    collect(&root, Path::new(""), &mut entries)?;
    // One sort on the (path, kind) tuple: paths are unique on disk, so the
    // order is total and the archive bytes are reproducible. A directory's
    // trailing `/` makes it sort before its own children.
    entries.sort();

    let mut out = Vec::new();
    for (path, kind) in entries {
        let is_dir = kind == TYPE_DIR;
        let (mode, data) = if is_dir {
            (DIR_MODE, Vec::new())
        } else {
            let file = root.join(&path);
            let data = fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            (FILE_MODE, data)
        };
        out.extend_from_slice(&header(&path, mode, data.len() as u64, kind)?);
        out.extend_from_slice(&data);
        let rem = data.len() % BLOCK;
        if rem != 0 {
            out.resize(out.len() + BLOCK - rem, 0);
        }
    }
    // The end-of-archive marker: two blocks of zeroes, nothing after them.
    out.resize(out.len() + 2 * BLOCK, 0);
    Ok(out)
}

/// Unpack an uncompressed tar archive into `dest`, creating it if needed, and
/// return the files (relative paths) that were written. Existing files of
/// the same name are overwritten, tar's normal behaviour; every other shape
/// of input is refused with the offending entry named.
pub fn unpack(archive: &[u8], dest: &Path) -> Result<Vec<PathBuf>, String> {
    if archive.is_empty() {
        return Err("the archive is empty".to_string());
    }
    if !archive.len().is_multiple_of(BLOCK) {
        return Err(format!(
            "the archive is {} bytes, not a whole number of {BLOCK}-byte blocks",
            archive.len()
        ));
    }
    fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut made = Vec::new();
    let mut offset = 0usize;
    loop {
        if offset + BLOCK > archive.len() {
            return Err(format!(
                "the archive is truncated: no end-of-archive marker after \
                 {offset} bytes"
            ));
        }
        let block = &archive[offset..offset + BLOCK];
        if block.iter().all(|&b| b == 0) {
            if archive[offset..].iter().any(|&b| b != 0) {
                return Err("there is data after the end-of-archive marker".to_string());
            }
            return Ok(made);
        }
        check_magic(block)?;
        check_checksum(block)?;
        let mode = octal(&block[100..108], "mode")?;
        let size = octal(&block[124..136], "size")?;
        let name = read_name(block)?;
        let rel = safe_rel(&name)?;
        if !seen.insert(rel.clone()) {
            return Err(format!("the entry {name:?} appears twice in the archive"));
        }
        let padded = size
            .checked_add(u64::from(BLOCK as u32 - 1))
            .map(|s| s / u64::from(BLOCK as u32) * u64::from(BLOCK as u32))
            .ok_or_else(|| format!("the entry {name:?} claims an enormous size"))?;
        let next = (offset + BLOCK)
            .checked_add(
                usize::try_from(padded)
                    .map_err(|_| format!("the entry {name:?} claims an enormous size"))?,
            )
            .ok_or_else(|| format!("the entry {name:?} claims an enormous size"))?;
        if next > archive.len() {
            return Err(format!(
                "the archive is truncated: {name:?} claims {size} bytes"
            ));
        }
        let full = dest.join(&rel);
        match block[156] {
            TYPE_FILE | 0 => {
                if mode & 0o111 != 0 {
                    return Err(format!(
                        "{name:?} carries executable bits; pack archives hold \
                         data files only"
                    ));
                }
                if let Some(parent) = full.parent() {
                    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
                }
                fs::write(
                    &full,
                    &archive[offset + BLOCK..offset + BLOCK + size as usize],
                )
                .map_err(|e| format!("{}: {e}", full.display()))?;
                set_mode(&full, FILE_MODE);
                made.push(rel);
            }
            TYPE_DIR => {
                if size != 0 {
                    return Err(format!(
                        "{name:?} is a directory carrying {size} bytes of data"
                    ));
                }
                fs::create_dir_all(&full).map_err(|e| format!("{}: {e}", full.display()))?;
                set_mode(&full, DIR_MODE);
            }
            b'1' | b'2' => {
                return Err(format!(
                    "{name:?} is a hardlink or symlink; archives this tool \
                     accepts never carry links"
                ));
            }
            other => {
                return Err(format!(
                    "{name:?} has unsupported entry type {other:?}; a pack \
                     archive holds only files and directories"
                ));
            }
        }
        offset = next;
    }
}

/// Walk one directory level into `entries` as `(relative path, type)`,
/// refusing symlinks: they cannot round-trip through an archive this tool
/// is willing to read back.
fn collect(dir: &Path, rel: &Path, entries: &mut Vec<(String, u8)>) -> Result<(), String> {
    let mut kids: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    kids.sort();
    for path in kids {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .ok_or_else(|| {
                format!(
                    "{}: pack file names must be valid UTF-8 to be archived",
                    path.display()
                )
            })?;
        let rel_path = rel.join(name);
        let rel_str = rel_path
            .to_str()
            .expect("a UTF-8 name in a UTF-8 parent path stays UTF-8");
        let meta = fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let file_type = meta.file_type();
        if file_type.is_symlink() {
            return Err(format!(
                "{}: pack archives do not carry symlinks; copy the file instead",
                path.display()
            ));
        } else if file_type.is_dir() {
            entries.push((format!("{rel_str}/"), TYPE_DIR));
            collect(&path, &rel_path, entries)?;
        } else if file_type.is_file() {
            entries.push((rel_str.to_string(), TYPE_FILE));
        } else {
            return Err(format!(
                "{}: neither a regular file nor a directory",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Build one ustar header block.
fn header(path: &str, mode: u64, size: u64, kind: u8) -> Result<[u8; BLOCK], String> {
    let mut h = [0u8; BLOCK];
    let (name, prefix) = split_name(path)?;
    h[..name.len()].copy_from_slice(name.as_bytes());
    put_octal(&mut h[100..108], mode)?;
    put_octal(&mut h[108..116], 0)?; // uid
    put_octal(&mut h[116..124], 0)?; // gid
    put_octal(&mut h[124..136], size)?;
    put_octal(&mut h[136..148], MTIME)?;
    h[156] = kind;
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    // uname and gname stay empty: an owner name would leak the packer's
    // machine into the bytes.
    h[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
    seal(&mut h);
    Ok(h)
}

/// Fill in the checksum field over the header's other bytes. Separated from
/// [`header`] so a test can forge a header and re-seal it.
fn seal(h: &mut [u8; BLOCK]) {
    for b in &mut h[148..156] {
        *b = b' ';
    }
    let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
    let digits = format!("{sum:06o}");
    h[148..148 + digits.len()].copy_from_slice(digits.as_bytes());
    h[154] = 0;
    h[155] = b' ';
}

/// Split a path across ustar's name (100) and prefix (155) fields, or refuse
/// a path that fits in neither.
fn split_name(path: &str) -> Result<(&str, &str), String> {
    if path.len() <= 100 {
        return Ok((path, ""));
    }
    for (i, _) in path.match_indices('/') {
        let (pre, rest) = path.split_at(i);
        let rest = &rest[1..];
        if rest.len() <= 100 && pre.len() <= 155 {
            return Ok((rest, pre));
        }
    }
    Err(format!(
        "the path {path:?} is too long for the ustar format \
         (at most 155 + 1 + 100 bytes)"
    ))
}

/// Write `value` as a zero-padded octal number filling `dst` minus its
/// terminating NUL, or refuse a value the field cannot hold.
fn put_octal(dst: &mut [u8], value: u64) -> Result<(), String> {
    let digits = dst.len() - 1;
    let text = format!("{value:0w$o}", w = digits);
    if text.len() > digits {
        return Err(format!(
            "the value {value} does not fit in a {digits}-digit octal tar field"
        ));
    }
    dst[..digits].copy_from_slice(text.as_bytes());
    dst[digits] = 0;
    Ok(())
}

/// Read one octal numeric field (up to its NUL or space), refusing the
/// GNU base-256 extension and anything malformed.
fn octal(field: &[u8], what: &str) -> Result<u64, String> {
    if field.first().is_some_and(|&b| b & 0x80 != 0) {
        return Err(format!(
            "the {what} field uses a GNU base-256 extension this tool does not read"
        ));
    }
    let digits: Vec<u8> = field
        .iter()
        .copied()
        .take_while(|&b| b != 0 && b != b' ')
        .collect();
    if digits.is_empty() || !digits.iter().all(|b| (b'0'..=b'7').contains(b)) {
        return Err(format!("the {what} field is not a valid octal number"));
    }
    let text = std::str::from_utf8(&digits).expect("checked ASCII above");
    u64::from_str_radix(text, 8).map_err(|_| format!("the {what} field is too large to read"))
}

/// Read the entry name from ustar's name and prefix fields.
fn read_name(block: &[u8]) -> Result<String, String> {
    let name = field(&block[0..100], "name")?;
    let prefix = field(&block[345..500], "prefix")?;
    let full = match (prefix.is_empty(), name.is_empty()) {
        (true, false) => name,
        (false, true) => prefix,
        (false, false) => format!("{prefix}/{name}"),
        (true, true) => String::new(),
    };
    if full.is_empty() {
        return Err("an entry has an empty name".to_string());
    }
    Ok(full)
}

/// Read a NUL-terminated text field, refusing names that are not UTF-8.
fn field(bytes: &[u8], what: &str) -> Result<String, String> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end])
        .map(str::to_string)
        .map_err(|_| format!("the entry {what} is not valid UTF-8"))
}

/// Refuse anything that would land outside `dest`.
fn safe_rel(name: &str) -> Result<PathBuf, String> {
    let mut rel = PathBuf::new();
    for part in Path::new(name).components() {
        match part {
            Component::Normal(p) => rel.push(p),
            Component::CurDir => {}
            _ => {
                return Err(format!(
                    "the entry {name:?} escapes the destination (absolute \
                     paths and `..` are not allowed)"
                ));
            }
        }
    }
    if rel.as_os_str().is_empty() {
        return Err(format!("the entry {name:?} names no file"));
    }
    Ok(rel)
}

fn check_magic(block: &[u8]) -> Result<(), String> {
    // POSIX writes "ustar\0" and GNU "ustar  \0"; both share the prefix, and
    // anything else is not a format this reader claims to understand.
    if &block[257..262] == b"ustar" {
        Ok(())
    } else {
        Err("not a ustar archive: the magic is wrong".to_string())
    }
}

/// Verify the header checksum, accepting both the unsigned and the legacy
/// signed sums so a header with UTF-8 bytes in its name still checks out.
fn check_checksum(block: &[u8]) -> Result<(), String> {
    let stored = octal(&block[148..154], "checksum")?;
    let mut unsigned = 0u32;
    let mut signed = 0i32;
    for (i, &b) in block.iter().enumerate() {
        if (148..156).contains(&i) {
            unsigned += 32;
            signed += 32;
        } else {
            unsigned += u32::from(b);
            signed += i32::from(b as i8);
        }
    }
    if stored == u64::from(unsigned) || (signed >= 0 && stored == u64::from(signed as u32)) {
        Ok(())
    } else {
        Err(
            "the header checksum does not match: the archive is corrupt or was \
             not written by this tool"
                .to_string(),
        )
    }
}

/// Set a normalized mode where the platform has modes.
#[cfg(unix)]
fn set_mode(path: &Path, mode: u64) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode as u32));
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u64) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn scratch(tag: &str) -> PathBuf {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("textrill-tar-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, name: &str, body: &[u8]) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, body).unwrap();
    }

    /// A minimal valid manifest: [`pack`] validates before it archives, so
    /// every sample tree must be a real pack.
    const SAMPLE_MANIFEST: &str = "schema = 1\n\
        id = \"org.example.sample\"\n\
        name = \"Sample\"\n\
        version = \"1.0.0\"\n\
        license = \"GPL-3.0-or-later\"\n\
        \n\
        [[templates]]\n\
        file = \"book.html\"\n\
        model = \"body\"\n";

    /// A tree with the shapes an archive must reproduce: the manifest, a
    /// template, an empty directory, a nested file, an empty file, boundary
    /// sizes around one block, and a UTF-8 name.
    fn sample(tag: &str) -> PathBuf {
        let d = scratch(tag);
        write(&d, "template.toml", SAMPLE_MANIFEST.as_bytes());
        write(&d, "book.html", b"{{textrill:content}}");
        std::fs::create_dir_all(d.join("empty")).unwrap();
        write(&d, "nested/deep/file.txt", b"deep");
        write(&d, "zero.bin", b"");
        write(&d, "small.bin", &[b'a'; 511]);
        write(&d, "exact.bin", &[b'b'; 512]);
        write(&d, "over.bin", &[b'c'; 513]);
        write(&d, "h\u{e9}llo.txt", "unicode".as_bytes());
        d
    }

    /// The tree as (sorted relative path, contents) with directories marked
    /// by a trailing `/`, for whole-tree comparisons.
    fn tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![(PathBuf::new(), dir.to_path_buf())];
        while let Some((rel, path)) = stack.pop() {
            let mut kids: Vec<PathBuf> = std::fs::read_dir(&path)
                .unwrap()
                .filter_map(|e| e.ok().map(|e| e.path()))
                .collect();
            kids.sort();
            for kid in kids {
                let name = kid.file_name().unwrap().to_string_lossy().into_owned();
                let rel_path = rel.join(&name);
                let meta = std::fs::symlink_metadata(&kid).unwrap();
                if meta.is_dir() {
                    out.push((format!("{}/", rel_path.to_string_lossy()), Vec::new()));
                    stack.push((rel_path, kid));
                } else {
                    out.push((
                        rel_path.to_string_lossy().into_owned(),
                        std::fs::read(&kid).unwrap(),
                    ));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn a_round_trip_reproduces_the_tree() {
        let src = sample("round");
        let expected = tree(&src);
        let bytes = pack(&src).expect("packs");
        let dst = scratch("round-out");
        let made = unpack(&bytes, &dst).expect("unpacks");
        assert_eq!(tree(&dst), expected);
        assert_eq!(made.len(), 8);
        assert!(made.contains(&PathBuf::from("book.html")));
        assert!(made.contains(&PathBuf::from("nested/deep/file.txt")));
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn packing_is_deterministic() {
        let a = sample("det-a");
        let b = sample("det-b");
        let first = pack(&a).unwrap();
        let second = pack(&a).unwrap();
        assert_eq!(first, second, "packing the same tree twice differs");
        assert_eq!(
            first,
            pack(&b).unwrap(),
            "the location leaks into the bytes"
        );
        // A changed mtime cannot change the output: the header mtime is fixed.
        let file = a.join("book.html");
        let times = fs::FileTimes::new().set_modified(
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000),
        );
        fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_times(times)
            .unwrap();
        assert_eq!(
            first,
            pack(&a).unwrap(),
            "a file's mtime leaks into the bytes"
        );
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    /// The header and data of one named entry inside a packed archive.
    fn find<'a>(bytes: &'a [u8], wanted: &str) -> Option<(&'a [u8], &'a [u8])> {
        let mut off = 0;
        while off + BLOCK <= bytes.len() {
            let h = &bytes[off..off + BLOCK];
            if h.iter().all(|&b| b == 0) {
                return None;
            }
            let size = octal(&h[124..136], "size").unwrap() as usize;
            let end = off + BLOCK + size;
            if read_name(h).ok().as_deref() == Some(wanted) {
                return Some((h, &bytes[off + BLOCK..end]));
            }
            off = end.div_ceil(BLOCK) * BLOCK;
        }
        None
    }

    #[test]
    fn the_archive_is_the_documented_ustar_shape() {
        let src = sample("shape");
        let bytes = pack(&src).unwrap();
        let _ = std::fs::remove_dir_all(&src);
        assert_eq!(bytes.len() % BLOCK, 0, "block aligned");
        assert_eq!(
            &bytes[bytes.len() - 1024..],
            &[0u8; 1024],
            "ends with two zero blocks"
        );
        // First header: `book.html` sorts first of the sample's names.
        let h = &bytes[..BLOCK];
        assert_eq!(&h[0..9], b"book.html");
        assert_eq!(&h[257..263], b"ustar\0");
        assert_eq!(&h[263..265], b"00");
        assert_eq!(&h[136..148], b"00000000000\0", "fixed mtime");
        assert_eq!(h[156], TYPE_FILE);
        assert_eq!(
            octal(&h[100..108], "mode").unwrap(),
            FILE_MODE,
            "normalized mode"
        );
        check_checksum(h).expect("the checksum we wrote verifies");
        let size = octal(&h[124..136], "size").unwrap() as usize;
        assert_eq!(&bytes[BLOCK..BLOCK + size], b"{{textrill:content}}");
        // uid and gid are zero-filled octal and the owner names stay empty,
        // so neither a machine nor a clock leaks into the bytes.
        assert_eq!(&h[108..116], b"0000000\0");
        assert_eq!(&h[116..124], b"0000000\0");
        assert_eq!(&h[265..329], &[0u8; 64], "empty uname and gname");
        // The manifest rides along further down the sort order, intact.
        let (mh, data) = find(&bytes, "template.toml").expect("the manifest is archived");
        assert_eq!(mh[156], TYPE_FILE);
        let text = std::str::from_utf8(data).expect("the manifest is UTF-8");
        assert!(text.starts_with("schema = 1\n"), "{text}");
    }

    #[test]
    fn modes_are_normalized() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let src = sample("modes");
            // A file that would be group-writable on disk still archives 0644.
            std::fs::set_permissions(src.join("book.html"), fs::Permissions::from_mode(0o660))
                .unwrap();
            let bytes = pack(&src).unwrap();
            let _ = std::fs::remove_dir_all(&src);
            let mut seen_file = false;
            let mut off = 0;
            while off + BLOCK <= bytes.len() {
                let h = &bytes[off..off + BLOCK];
                if h.iter().all(|&b| b == 0) {
                    break;
                }
                let size = octal(&h[124..136], "size").unwrap();
                if h[156] == TYPE_FILE && &h[0..9] == b"book.html" {
                    assert_eq!(octal(&h[100..108], "mode").unwrap(), 0o644);
                    seen_file = true;
                }
                // Advance past this header and its padded data.
                off += BLOCK + size as usize;
                let rem = size as usize % BLOCK;
                if rem != 0 {
                    off += BLOCK - rem;
                }
            }
            assert!(seen_file, "book.html header was not visited");
        }
    }

    #[test]
    fn unpack_creates_the_destination_and_lists_its_files() {
        let src = sample("mkdest");
        let bytes = pack(&src).unwrap();
        let _ = std::fs::remove_dir_all(&src);
        let dst = std::env::temp_dir().join(format!(
            "textrill-tar-{}-fresh/nested/out",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dst);
        let made = unpack(&bytes, &dst).unwrap();
        assert!(!made.is_empty());
        assert!(dst.join("template.toml").is_file());
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn an_unpacked_pack_still_validates() {
        let src = sample("reval");
        write(&src, "template.toml", b"schema = 1\nid = \"org.example.x\"\nname = \"X\"\nversion = \"1.0.0\"\nlicense = \"GPL-3.0-or-later\"\n\n[[templates]]\nfile = \"book.html\"\nmodel = \"body\"\n");
        write(&src, "book.html", b"{{textrill:content}}");
        let bytes = pack(&src).unwrap();
        let dst = scratch("reval-out");
        unpack(&bytes, &dst).unwrap();
        let v = validate::load(&dst).expect("the round trip is still a valid pack");
        assert_eq!(v.id, "org.example.x");
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn a_long_path_is_split_across_prefix_and_name() {
        let long_dir = "d".repeat(90);
        let long_name = format!("{}.html", "f".repeat(90));
        let src = sample("long");
        let rel = format!("{long_dir}/{long_name}");
        assert!(rel.len() > 100, "the path must overflow the name field");
        write(&src, &rel, b"x");
        let bytes = pack(&src).unwrap();
        let dst = scratch("long-out");
        unpack(&bytes, &dst).unwrap();
        assert!(dst.join(&rel).is_file());
        assert_eq!(tree(&dst), tree(&src));
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn a_path_ustar_cannot_hold_is_refused() {
        let name = format!("{}.txt", "x".repeat(300));
        let err = header(&name, 0o644, 0, TYPE_FILE).unwrap_err();
        assert!(err.contains("too long"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn packing_refuses_symlinks_and_executables() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let src = scratch("pack-bad");
        write(&src, "template.toml", SAMPLE_MANIFEST.as_bytes());
        write(&src, "book.html", b"{{textrill:content}}");
        write(&src, "a.txt", b"a");
        symlink("a.txt", src.join("link")).unwrap();
        let err = pack(&src).unwrap_err();
        assert!(err.contains("symlinks"), "{err}");
        let _ = std::fs::remove_dir_all(&src);

        let src = scratch("pack-exec");
        write(&src, "template.toml", SAMPLE_MANIFEST.as_bytes());
        write(&src, "book.html", b"{{textrill:content}}");
        write(&src, "run.sh", b"#!/bin/sh\n");
        std::fs::set_permissions(src.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        let err = pack(&src).unwrap_err();
        assert!(err.contains("executable"), "{err}");
        let _ = std::fs::remove_dir_all(&src);
    }

    #[test]
    fn packing_a_missing_directory_is_named() {
        let err = pack(Path::new("/nonexistent-textrill-pack")).unwrap_err();
        assert!(err.contains("nonexistent-textrill-pack"), "{err}");
    }

    // ---- Hostile archives. Each is assembled from a hand-built header, the
    // ---- shape `unpack` must refuse on sight.

    /// A well-formed header plus `data`, padded, plus the end marker.
    fn archive_of(entries: &[(String, u64, u8, u32)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, size, kind, mode) in entries {
            let h = header(name, u64::from(*mode), *size, *kind).unwrap();
            out.extend_from_slice(&h);
            if *kind == TYPE_FILE && *size > 0 {
                out.resize(out.len() + *size as usize, b'x');
                let rem = out.len() % BLOCK;
                if rem != 0 {
                    out.resize(out.len() + BLOCK - rem, 0);
                }
            }
        }
        out.resize(out.len() + 2 * BLOCK, 0);
        out
    }

    fn refuse(archive: Vec<u8>, fragment: &str) {
        let dst = scratch("evil");
        let err = unpack(&archive, &dst).unwrap_err();
        let _ = std::fs::remove_dir_all(&dst);
        assert!(err.contains(fragment), "expected {fragment:?} in: {err}");
    }

    #[test]
    fn absolute_and_parent_paths_are_refused() {
        refuse(
            archive_of(&[("/etc/passwd".into(), 0, TYPE_FILE, 0o644)]),
            "escapes",
        );
        refuse(
            archive_of(&[("../escape".into(), 0, TYPE_FILE, 0o644)]),
            "escapes",
        );
        refuse(
            archive_of(&[("a/../../b".into(), 0, TYPE_FILE, 0o644)]),
            "escapes",
        );
        refuse(archive_of(&[("/".into(), 0, TYPE_DIR, 0o755)]), "escapes");
    }

    #[test]
    fn link_and_special_entries_are_refused() {
        refuse(
            archive_of(&[("link".into(), 0, b'2', 0o777)]),
            "hardlink or symlink",
        );
        refuse(
            archive_of(&[("link".into(), 0, b'1', 0o644)]),
            "hardlink or symlink",
        );
        refuse(
            archive_of(&[("dev".into(), 0, b'3', 0o644)]),
            "unsupported entry type",
        );
        refuse(
            archive_of(&[("fifo".into(), 0, b'6', 0o644)]),
            "unsupported entry type",
        );
    }

    #[test]
    fn executable_files_are_refused_on_input() {
        refuse(
            archive_of(&[("run.sh".into(), 0, TYPE_FILE, 0o755)]),
            "executable bits",
        );
    }

    #[test]
    fn directories_carry_no_data() {
        refuse(
            archive_of(&[("dir".into(), 512, TYPE_DIR, 0o755)]),
            "carrying 512 bytes",
        );
    }

    #[test]
    fn duplicate_entries_are_refused() {
        refuse(
            archive_of(&[
                ("file".into(), 0, TYPE_FILE, 0o644),
                ("file".into(), 0, TYPE_FILE, 0o644),
            ]),
            "appears twice",
        );
        // The same path once as a directory and once as a file.
        refuse(
            archive_of(&[
                ("thing".into(), 0, TYPE_DIR, 0o755),
                ("thing/".into(), 0, TYPE_FILE, 0o644),
            ]),
            "appears twice",
        );
    }

    #[test]
    fn a_corrupt_header_is_refused() {
        let mut a = archive_of(&[("file".into(), 0, TYPE_FILE, 0o644)]);
        a[0] = b'X'; // the name changes, the stored checksum does not
        refuse(a, "checksum");
    }

    #[test]
    fn a_non_ustar_header_is_refused() {
        let mut a = archive_of(&[("file".into(), 0, TYPE_FILE, 0o644)]);
        a[257] = b'X';
        refuse(a, "magic");
    }

    #[test]
    fn a_truncated_archive_is_refused() {
        // A header claiming data that is not there.
        let mut a = header("file", 0o644, 4096, TYPE_FILE).unwrap().to_vec();
        a.extend_from_slice(&[b'x'; 100]);
        while !a.len().is_multiple_of(BLOCK) {
            a.push(0);
        }
        refuse(a, "truncated");
        // No end marker at all.
        refuse(
            header("file", 0o644, 0, TYPE_FILE).unwrap().to_vec(),
            "end-of-archive",
        );
    }

    #[test]
    fn data_after_the_end_marker_is_refused() {
        let mut a = archive_of(&[("file".into(), 0, TYPE_FILE, 0o644)]);
        let mut tail = [0u8; BLOCK];
        tail[BLOCK - 1] = b'x';
        a.extend_from_slice(&tail);
        refuse(a, "after the end");
    }

    #[test]
    fn a_misaligned_or_empty_archive_is_refused() {
        refuse(vec![0u8; 513], "not a whole number");
        refuse(Vec::new(), "empty");
    }

    #[test]
    fn a_base256_size_field_is_refused() {
        let mut h = header("file", 0o644, 0, TYPE_FILE).unwrap();
        h[124] = 0x80; // GNU's base-256 marker
        seal(&mut h); // so the checksum passes and the size field is what fails
        let mut a = h.to_vec();
        a.resize(a.len() + 2 * BLOCK, 0);
        refuse(a, "base-256");
    }

    #[test]
    fn non_utf8_entry_names_are_refused() {
        let mut h = header("file", 0o644, 0, TYPE_FILE).unwrap();
        h[0] = 0xff;
        seal(&mut h); // so the checksum passes and the name field is what fails
        let mut a = h.to_vec();
        a.resize(a.len() + 2 * BLOCK, 0);
        refuse(a, "UTF-8");
    }

    #[test]
    fn an_empty_archive_is_two_clean_zero_blocks() {
        let a = vec![0u8; 2 * BLOCK];
        let dst = scratch("empty-tar");
        assert_eq!(unpack(&a, &dst).unwrap(), Vec::<PathBuf>::new());
        let _ = std::fs::remove_dir_all(&dst);
    }
}
