//! Pack validation (roadmap §5.3): fail loud, before any output.
//!
//! A pack is checked when it is discovered, again before it is installed, and
//! on demand by `textrill templates validate`. The checks are the roadmap's:
//! manifest semantics (known `schema`, reverse-DNS `id`, semver `version`,
//! license recorded), every template file present and inside the pack,
//! declared `slots` known, declared `vars` matching the `{{textrill:var:*}}`
//! tokens the file uses, `model` ∈ {body, document}, and a filesystem pass
//! refusing symlinks that leave the pack and executable files.
//! [`crate::template::validate`] is reused for the token-level rules, so a
//! template is checked the same way on every path. Covered by the tests below.

use std::fs;
use std::path::{Component, Path, PathBuf};

use super::manifest::{self, TemplateEntry};
use crate::library::Model;
use crate::template;

/// The manifest filename every pack directory carries.
pub const MANIFEST: &str = "template.toml";

/// The schema this build understands. A pack naming another is refused with
/// a message that says so rather than guessed at.
pub const SCHEMA: i64 = 1;

/// The license the shipped set carries: a pack under this license need not
/// ship a LICENSE file of its own. `CONTRIBUTING.md` requires every other
/// pack to.
pub const SHIPPED_LICENSE: &str = "GPL-3.0-or-later";

/// One validated template: everything a caller needs to select and use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidTemplate {
    /// The name this template answers to inside its pack, taken from the
    /// file stem (`book.html` → `book`), so `pack-id:book` addresses it.
    pub name: String,
    /// The template file, resolved to a path inside the pack root.
    pub file: PathBuf,
    pub model: Model,
    /// The `slots` the manifest declares, for listing and the GUI.
    pub slots: Vec<String>,
    pub vars: Vec<Var>,
    pub preview: Option<PathBuf>,
}

/// One declared parameter of a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Var {
    pub name: String,
    pub required: bool,
}

/// A pack that passed every check: the normalized form, with no `Option`
/// left where the schema makes a field required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validated {
    pub id: String,
    pub name: String,
    pub version: String,
    pub license: String,
    pub description: String,
    pub authors: Vec<String>,
    pub tags: Vec<String>,
    pub templates: Vec<ValidTemplate>,
}

/// Load and validate a pack directory: the manifest, every template it
/// declares, and the filesystem rules over the whole tree. The first problem
/// is reported; nothing about a rejected pack is usable.
pub fn load(root: &Path) -> Result<Validated, String> {
    let root = fs::canonicalize(root).map_err(|e| format!("{}: {e}", root.display()))?;
    if !root.is_dir() {
        return Err(format!("{}: not a directory", root.display()));
    }
    let manifest_path = root.join(MANIFEST);
    let text = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let parsed = manifest::parse(&text).map_err(|e| format!("{MANIFEST}: {e}"))?;

    let schema = parsed
        .schema
        .ok_or_else(|| format!("{MANIFEST}: `schema` is required"))?;
    if schema != SCHEMA {
        return Err(format!(
            "{MANIFEST}: unknown schema {schema}; this build understands {SCHEMA}"
        ));
    }
    let id = parsed
        .id
        .ok_or_else(|| format!("{MANIFEST}: `id` is required"))?;
    check_id(&id)?;
    let name = parsed
        .name
        .ok_or_else(|| format!("{MANIFEST}: `name` is required"))?;
    if name.trim().is_empty() {
        return Err(format!("{MANIFEST}: `name` must not be empty"));
    }
    let version = parsed
        .version
        .ok_or_else(|| format!("{MANIFEST}: `version` is required"))?;
    check_version(&version)?;
    let license = parsed
        .license
        .ok_or_else(|| format!("{MANIFEST}: `license` is required"))?;
    if license.trim().is_empty() {
        return Err(format!("{MANIFEST}: `license` must not be empty"));
    }
    if license != SHIPPED_LICENSE && !root.join("LICENSE").is_file() {
        return Err(format!(
            "{MANIFEST}: license {license:?} is not the shipped \
             {SHIPPED_LICENSE:?}, so the pack needs a LICENSE file"
        ));
    }
    if parsed.templates.is_empty() {
        return Err(format!(
            "{MANIFEST}: the pack declares no templates (add a [[templates]] section)"
        ));
    }

    let mut templates = Vec::new();
    for entry in &parsed.templates {
        let label = entry.file.as_deref().unwrap_or("<unnamed>");
        templates.push(validate_template(&root, entry).map_err(|e| format!("{label}: {e}"))?);
    }
    for (i, t) in templates.iter().enumerate() {
        if templates[..i].iter().any(|w| w.name == t.name) {
            return Err(format!(
                "{MANIFEST}: two templates are both named {:?}; template names \
                 must be unique within a pack",
                t.name
            ));
        }
    }
    check_tree(&root)?;

    Ok(Validated {
        id,
        name,
        version,
        license,
        description: parsed.description.unwrap_or_default(),
        authors: parsed.authors.unwrap_or_default(),
        tags: parsed.tags.unwrap_or_default(),
        templates,
    })
}

/// Validate one `[[templates]]` entry of a pack directory: the file exists
/// inside the pack, the model and slots are known, the tokens validate, and
/// the declared vars match the ones the file uses. [`load`] uses this per
/// entry; the GUI can call it for one template to show an inline problem.
pub fn validate_template(root: &Path, entry: &TemplateEntry) -> Result<ValidTemplate, String> {
    let file = entry.file.as_deref().ok_or("`file` is required")?;
    let path = inside(root, file, "file")?;
    let model = match entry.model.as_deref() {
        Some("document") => Model::Document,
        Some("body") => Model::Body,
        Some(other) => {
            return Err(format!(
                "`model` must be \"body\" or \"document\", not {other:?}"
            ));
        }
        None => return Err("`model` is required (\"body\" or \"document\")".to_string()),
    };
    let slots = entry.slots.clone().unwrap_or_default();
    for slot in &slots {
        if !template::SLOTS.contains(&slot.as_str()) {
            return Err(format!(
                "declared slot `{{{{textrill:{slot}}}}}` is not a known slot; \
                 expected one of: {}",
                template::SLOTS.join(", ")
            ));
        }
    }
    let mut vars = Vec::new();
    for v in &entry.vars {
        let name = v
            .name
            .clone()
            .ok_or("`[[templates.vars]]` entries need a `name`")?;
        vars.push(Var {
            name,
            required: v.required.unwrap_or(false),
        });
    }
    for (i, v) in vars.iter().enumerate() {
        if v.name.is_empty() {
            return Err("a declared variable name must not be empty".to_string());
        }
        if vars[..i].iter().any(|w| w.name == v.name) {
            return Err(format!("duplicate declared variable {:?}", v.name));
        }
    }
    let html = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let declared: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
    template::validate(&html, &declared).map_err(|e| format!("{}: {e}", path.display()))?;
    check_declared_vars_used(&html, &declared).map_err(|e| format!("{}: {e}", path.display()))?;
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("`file` {file:?} has no usable name"))?
        .to_string();
    let preview = match &entry.preview {
        Some(p) => Some(inside(root, p, "preview")?),
        None => None,
    };
    Ok(ValidTemplate {
        name,
        file: path,
        model,
        slots,
        vars,
        preview,
    })
}

/// Resolve a manifest-declared path inside the pack: relative, no `..`,
/// present, a regular file, and — once every symlink is followed — still
/// inside the pack root.
fn inside(root: &Path, rel: &str, what: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("`{what}` must not be empty"));
    }
    for part in Path::new(rel).components() {
        match part {
            Component::Normal(_) | Component::CurDir => {}
            _ => {
                return Err(format!(
                    "`{what}` {rel:?} must be a path inside the pack: absolute \
                     paths and `..` are not allowed"
                ));
            }
        }
    }
    let full = root.join(rel);
    let meta = fs::symlink_metadata(&full).map_err(|e| format!("{}: {e}", full.display()))?;
    let file_type = meta.file_type();
    if !file_type.is_symlink() && !file_type.is_file() {
        return Err(format!("{}: not a regular file", full.display()));
    }
    let canon_root = fs::canonicalize(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let canon = fs::canonicalize(&full).map_err(|e| format!("{}: {e}", full.display()))?;
    if !canon.starts_with(&canon_root) {
        return Err(format!("{}: resolves outside the pack", rel));
    }
    if file_type.is_symlink() && !canon.is_file() {
        return Err(format!("{}: not a regular file", full.display()));
    }
    Ok(full)
}

/// Walk the whole pack once: no executable regular file anywhere, and no
/// symlink — declared or not — whose target leaves the pack. Archive
/// extraction and a `git clone` of a shared pack can both deliver either, so
/// this is what keeps a hostile pack inert on disk.
fn check_tree(root: &Path) -> Result<(), String> {
    let croot = fs::canonicalize(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let mut stack = vec![croot.clone()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
            let path = entry.path();
            let meta =
                fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let file_type = meta.file_type();
            if file_type.is_symlink() {
                let target = fs::canonicalize(&path)
                    .map_err(|e| format!("{}: unusable symlink: {e}", path.display()))?;
                if !target.starts_with(&croot) {
                    return Err(format!(
                        "{}: symlink target leaves the pack",
                        path.display()
                    ));
                }
            } else if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if meta.permissions().mode() & 0o111 != 0 {
                        return Err(format!(
                            "{}: executable files are not allowed in a pack",
                            path.display()
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// A declared var the template never uses is as much a defect as an undeclared
/// one: the manifest would advertise a parameter that does nothing.
/// [`template::validate`] already refuses the undeclared direction.
fn check_declared_vars_used(html: &str, declared: &[&str]) -> Result<(), String> {
    let mut used: Vec<&str> = Vec::new();
    let token = "{{textrill:var:";
    let mut rest = html;
    while let Some(pos) = rest.find(token) {
        rest = &rest[pos + token.len()..];
        let Some(end) = rest.find("}}") else {
            break;
        };
        used.push(&rest[..end]);
        rest = &rest[end + 2..];
    }
    for name in declared {
        if !used.contains(name) {
            return Err(format!(
                "declared variable {name:?} is never used by the template"
            ));
        }
    }
    Ok(())
}

/// A pack id is reverse-DNS: lowercase labels of letters, digits, `-` and `_`
/// joined by dots, at least two of them, so two people cannot pick the same
/// name by accident. `org.example.academic-book` passes; `book`, `Acme.Book`
/// and `org..book` do not.
fn check_id(id: &str) -> Result<(), String> {
    let labels: Vec<&str> = id.split('.').collect();
    if labels.len() < 2 {
        return Err(format!(
            "`id` {id:?} must be reverse-DNS with at least two labels, like \
             \"org.example.book\""
        ));
    }
    for label in labels {
        if label.is_empty() {
            return Err(format!(
                "`id` {id:?} has an empty label (no leading, trailing or \
                 doubled dots)"
            ));
        }
        let ok = label
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
        if !ok {
            return Err(format!(
                "`id` {id:?} has a label that is not lowercase letters, digits, \
                 `-` or `_`"
            ));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(format!(
                "`id` {id:?} has a label starting or ending with `-`"
            ));
        }
    }
    Ok(())
}

/// A version is semver: `MAJOR.MINOR.PATCH` with optional `-prerelease` and
/// `+build` identifiers. Numeric components carry no leading zeros.
fn check_version(v: &str) -> Result<(), String> {
    fn not<T>(v: &str, why: &str) -> Result<T, String> {
        Err(format!("`version` {v:?} is not semver: {why}"))
    }
    let (core, build) = match v.split_once('+') {
        Some((core, build)) => (core, Some(build)),
        None => (v, None),
    };
    let (core, pre) = match core.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (core, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return not(v, "expected MAJOR.MINOR.PATCH");
    }
    for part in parts {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return not(v, "the numeric components must be digits");
        }
        if part.len() > 1 && part.starts_with('0') {
            return not(v, "a numeric component has a leading zero");
        }
    }
    if let Some(pre) = pre {
        if pre.is_empty() {
            return not(v, "the pre-release after `-` must not be empty");
        }
        for id in pre.split('.') {
            if id.is_empty() {
                return not(v, "pre-release identifiers must not be empty");
            }
            let ok = id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
            if !ok {
                return not(v, "pre-release identifiers take letters, digits and `-`");
            }
            let numeric = id.bytes().all(|b| b.is_ascii_digit());
            if numeric && id.len() > 1 && id.starts_with('0') {
                return not(v, "a numeric pre-release identifier has a leading zero");
            }
        }
    }
    if let Some(build) = build {
        if build.is_empty() {
            return not(v, "the build after `+` must not be empty");
        }
        for id in build.split('.') {
            if id.is_empty() {
                return not(v, "build identifiers must not be empty");
            }
            let ok = id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
            if !ok {
                return not(v, "build identifiers take letters, digits and `-`");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn scratch(tag: &str) -> PathBuf {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d =
            std::env::temp_dir().join(format!("textrill-pack-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, name: &str, body: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, body).unwrap();
    }

    /// A minimal pack that passes every check, with its manifest text
    /// optionally rewritten by `edit_manifest`.
    fn pack_with(tag: &str, edit_manifest: &str) -> PathBuf {
        let d = scratch(tag);
        write(&d, MANIFEST, edit_manifest);
        write(
            &d,
            "good.html",
            "<!doctype html><title>t</title><p>{{textrill:content}}</p>",
        );
        d
    }

    const GOOD: &str = "schema = 1\n\
        id = \"org.example.good\"\n\
        name = \"Good\"\n\
        version = \"1.0.0\"\n\
        license = \"GPL-3.0-or-later\"\n\
        \n\
        [[templates]]\n\
        file = \"good.html\"\n\
        model = \"document\"\n\
        slots = [\"content\", \"title\"]\n";

    /// A pack built by appending `extra` to the good manifest.
    fn pack(tag: &str, extra: &str) -> PathBuf {
        pack_with(tag, &format!("{GOOD}\n{extra}"))
    }

    fn ok_pack(tag: &str) -> Validated {
        let d = pack(tag, "");
        let v = load(&d).expect("the good pack validates");
        let _ = std::fs::remove_dir_all(&d);
        v
    }

    fn rejected(tag: &str, manifest: &str) -> String {
        let d = pack_with(tag, manifest);
        let got = load(&d).expect_err("expected a rejection").to_string();
        let _ = std::fs::remove_dir_all(&d);
        got
    }

    #[test]
    fn a_minimal_pack_validates() {
        let v = ok_pack("minimal");
        assert_eq!(v.id, "org.example.good");
        assert_eq!(v.name, "Good");
        assert_eq!(v.version, "1.0.0");
        assert_eq!(v.license, SHIPPED_LICENSE);
        assert!(v.description.is_empty());
        assert!(v.authors.is_empty());
        assert!(v.tags.is_empty());
        assert_eq!(v.templates.len(), 1);
        let t = &v.templates[0];
        assert_eq!(t.name, "good");
        assert_eq!(t.model, Model::Document);
        assert_eq!(t.slots, vec!["content".to_string(), "title".to_string()]);
        assert!(t.vars.is_empty());
        assert!(t.preview.is_none());
    }

    #[test]
    fn a_missing_manifest_names_the_file_it_wanted() {
        let d = scratch("no-manifest");
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains(MANIFEST), "{err}");
    }

    #[test]
    fn a_not_a_directory_is_refused() {
        let d = scratch("file-root");
        let f = d.join("afile");
        std::fs::write(&f, "x").unwrap();
        let err = load(&f).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains("not a directory"), "{err}");
    }

    #[test]
    fn a_manifest_syntax_error_names_its_line() {
        let err = rejected("syntax", "schema = 1\nid = 5\n");
        assert!(err.starts_with("template.toml: line 2:"), "{err}");
    }

    #[test]
    fn schema_is_required_and_must_be_known() {
        let err = rejected("no-schema", "id = \"org.example.x\"\n");
        assert!(err.contains("`schema` is required"), "{err}");
        let d = pack_with("schema-two", &GOOD.replace("schema = 1", "schema = 2"));
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains("unknown schema 2"), "{err}");
        assert!(err.contains("understands 1"), "{err}");
    }

    #[test]
    fn id_must_be_reverse_dns() {
        assert!(
            rejected("id-bare", &GOOD.replace("org.example.good", "book"))
                .contains("at least two labels")
        );
        assert!(
            rejected("id-case", &GOOD.replace("org.example.good", "Org.Book"))
                .contains("lowercase")
        );
        assert!(
            rejected("id-dot", &GOOD.replace("org.example.good", "org..book"))
                .contains("empty label")
        );
        assert!(
            rejected("id-edge", &GOOD.replace("org.example.good", "org.example."))
                .contains("empty label")
        );
        assert!(
            rejected("id-dash", &GOOD.replace("org.example.good", "org.-book"))
                .contains("starting or ending with `-`")
        );
    }

    #[test]
    fn version_must_be_semver() {
        assert!(rejected("ver-bare", &GOOD.replace("1.0.0", "1.0")).contains("MAJOR.MINOR.PATCH"));
        assert!(rejected("ver-zero", &GOOD.replace("1.0.0", "01.0.0")).contains("leading zero"));
        assert!(rejected("ver-pre", &GOOD.replace("1.0.0", "1.0.3-")).contains("pre-release"));
        assert!(rejected("ver-build", &GOOD.replace("1.0.0", "1.0.3+")).contains("build after `+`"));
        assert!(
            rejected("ver-num-pre", &GOOD.replace("1.0.0", "1.0.0-01")).contains("leading zero")
        );
        assert!(rejected("ver-word", &GOOD.replace("1.0.0", "one.two.three")).contains("digits"));
        // Full semver stays accepted.
        let d = pack_with("ver-full", &GOOD.replace("1.0.0", "1.2.3-rc.1+build.5"));
        let v = load(&d).unwrap();
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(v.version, "1.2.3-rc.1+build.5");
    }

    #[test]
    fn license_is_required() {
        let err = rejected(
            "no-license",
            &GOOD.replace("license = \"GPL-3.0-or-later\"\n", ""),
        );
        assert!(err.contains("`license` is required"), "{err}");
        let err = rejected(
            "empty-license",
            &GOOD.replace("license = \"GPL-3.0-or-later\"", "license = \"\""),
        );
        assert!(err.contains("`license` must not be empty"), "{err}");
    }

    #[test]
    fn a_foreign_license_needs_a_license_file() {
        let err = rejected("mit", &GOOD.replace("GPL-3.0-or-later", "MIT"));
        assert!(err.contains("LICENSE"), "{err}");
        let d = pack_with("mit-file", &GOOD.replace("GPL-3.0-or-later", "MIT"));
        write(&d, "LICENSE", "MIT\n");
        let v = load(&d).unwrap();
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(v.license, "MIT");
    }

    #[test]
    fn the_pack_needs_at_least_one_template() {
        // The good manifest minus its [[templates]] block.
        let stripped = GOOD.split("[[templates]]").next().unwrap();
        let d = pack_with("no-templates", stripped);
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains("declares no templates"), "{err}");
    }

    #[test]
    fn model_must_be_body_or_document() {
        assert!(
            rejected("model-missing", &GOOD.replace("model = \"document\"\n", ""))
                .contains("`model` is required")
        );
        assert!(
            rejected("model-junk", &GOOD.replace("\"document\"", "\"page\""))
                .contains("not \"page\"")
        );
    }

    #[test]
    fn the_template_file_must_exist() {
        let d = pack_with("missing-file", &GOOD.replace("good.html", "gone.html"));
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains("gone.html"), "{err}");
    }

    #[test]
    fn the_preview_file_must_exist_when_declared() {
        let d = pack("preview-missing", "preview = \"missing.png\"\n");
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains("missing.png"), "{err}");
        let d = pack("preview-ok", "preview = \"good.html\"\n");
        load(&d).unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn declared_slots_must_be_known() {
        let err = rejected("slot-junk", &GOOD.replace("\"title\"", "\"ttl\""));
        assert!(err.contains("not a known slot"), "{err}");
        assert!(
            err.contains("content"),
            "the message lists the known slots: {err}"
        );
    }

    #[test]
    fn the_template_file_must_place_content() {
        let d = scratch("no-content");
        write(&d, MANIFEST, GOOD);
        write(&d, "good.html", "<p>no slot here</p>");
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains("content"), "{err}");
    }

    #[test]
    fn an_undeclared_var_is_refused() {
        let d = scratch("undeclared-var");
        write(&d, MANIFEST, GOOD);
        write(
            &d,
            "good.html",
            "<p>{{textrill:var:author}}</p>{{textrill:content}}",
        );
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains("author"), "{err}");
        assert!(err.contains("--var"), "the fix hint survives: {err}");
    }

    #[test]
    fn a_declared_var_must_be_used() {
        let d = pack("unused-var", "[[templates.vars]]\nname = \"author\"\n");
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains("author"), "{err}");
        assert!(err.contains("never used"), "{err}");
    }

    #[test]
    fn a_declared_var_that_is_used_passes() {
        let d = scratch("used-var");
        write(
            &d,
            MANIFEST,
            &format!("{GOOD}\n[[templates.vars]]\nname = \"author\"\nrequired = true\n"),
        );
        write(
            &d,
            "good.html",
            "<p>{{textrill:var:author}}</p>{{textrill:content}}",
        );
        let v = load(&d).unwrap();
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(
            v.templates[0].vars,
            vec![Var {
                name: "author".to_string(),
                required: true,
            }]
        );
    }

    #[test]
    fn vars_need_names_and_names_must_be_unique() {
        assert!(rejected(
            "var-noname",
            &format!("{GOOD}\n[[templates.vars]]\nrequired = true\n")
        )
        .contains("need a `name`"));
        let err = rejected(
            "var-dup",
            &format!(
                "{GOOD}\n[[templates.vars]]\nname = \"author\"\n\
                 [[templates.vars]]\nname = \"author\"\n"
            ),
        );
        assert!(err.contains("duplicate declared variable"), "{err}");
        assert!(err.contains("author"), "{err}");
    }

    #[test]
    fn template_names_must_be_unique_within_a_pack() {
        let d = pack(
            "dup-stems",
            "[[templates]]\nfile = \"good.htm\"\nmodel = \"body\"\n",
        );
        write(&d, "good.htm", "{{textrill:content}}");
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains("both named"), "{err}");
    }

    #[test]
    fn a_declared_path_must_stay_inside_the_pack() {
        assert!(
            rejected("parent-path", &GOOD.replace("good.html", "../x.html"))
                .contains("inside the pack")
        );
        assert!(
            rejected("abs-path", &GOOD.replace("good.html", "/etc/x.html"))
                .contains("inside the pack")
        );
    }

    #[test]
    fn a_directory_is_not_a_template_file() {
        let d = scratch("dir-file");
        write(&d, MANIFEST, GOOD);
        std::fs::create_dir_all(d.join("good.html")).unwrap();
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains("not a regular file"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn an_in_tree_symlink_is_allowed_but_an_outward_one_is_not() {
        use std::os::unix::fs::symlink;
        let d = scratch("symlink-in");
        write(&d, MANIFEST, GOOD);
        write(&d, "good.html", "{{textrill:content}}");
        symlink("good.html", d.join("alias.html")).unwrap();
        let v = load(&d).unwrap();
        assert_eq!(v.templates.len(), 1);
        let _ = std::fs::remove_dir_all(&d);

        let d = scratch("symlink-out");
        write(&d, MANIFEST, &GOOD.replace("good.html", "leak.html"));
        symlink("/etc/passwd", d.join("leak.html")).unwrap();
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        // The declared-file check fires before the whole-tree walk.
        assert!(err.contains("outside the pack"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn an_undeclared_symlink_out_of_the_pack_is_still_refused() {
        use std::os::unix::fs::symlink;
        let d = scratch("symlink-hidden");
        write(&d, MANIFEST, GOOD);
        write(&d, "good.html", "{{textrill:content}}");
        symlink("/etc/passwd", d.join("innocent.txt")).unwrap();
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains("leaves the pack"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn an_unusable_symlink_is_reported() {
        use std::os::unix::fs::symlink;
        let d = scratch("symlink-broken");
        write(&d, MANIFEST, GOOD);
        write(&d, "good.html", "{{textrill:content}}");
        symlink("nowhere", d.join("dangling")).unwrap();
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains("unusable symlink"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn executable_files_are_refused_anywhere_in_the_pack() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("executable");
        write(&d, MANIFEST, GOOD);
        write(&d, "good.html", "{{textrill:content}}");
        write(&d, "helper.sh", "#!/bin/sh\ntrue\n");
        std::fs::set_permissions(d.join("helper.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains("executable"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn executable_bits_on_the_template_file_itself_are_refused() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("exec-template");
        write(&d, MANIFEST, GOOD);
        write(&d, "good.html", "{{textrill:content}}");
        std::fs::set_permissions(d.join("good.html"), fs::Permissions::from_mode(0o755)).unwrap();
        let err = load(&d).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(err.contains("executable"), "{err}");
    }
}
