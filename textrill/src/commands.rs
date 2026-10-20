//! The `textrill templates` command surface (roadmap §5.4).
//!
//! The subcommands share one shape: parse into [`Options`] (so rc files and
//! `--template_dir` reach discovery), act, and report through [`Outcome`],
//! whose two streams and exit code the binary writes for it — a test can run
//! one with a controlled environment instead of touching the real one. The
//! catalog listing itself is shared by `--list_templates` and
//! `textrill templates list`; the verbs are added alongside the dispatch in
//! `main.rs`.

use std::fmt::Write as _;

use crate::options::Options;
use crate::pack::{self, Entry};

/// What a `templates` command wants written: the two streams and the process
/// exit code. Discovery never fails, so a listing always exits 0; a verb
/// reports its own refusal on stderr with code 1.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub code: u8,
}

/// `--list_templates` (roadmap §5.4): print the discovered catalog as text or
/// JSON per [`Options::list_templates`], discovery problems on stderr. Always
/// exits 0 — a broken pack is a warning, not a reason to hide the rest.
pub fn list_templates(opts: &Options) -> Outcome {
    let catalog = pack::discover(&opts.search_path());
    listing(&catalog, opts.list_templates == "json", None)
}

/// The listing both `--list_templates` and `textrill templates list` print:
/// one line per template on stdout, the problems seen on the way on stderr.
/// `tag` keeps only templates carrying that tag; the shipped set carries none,
/// so a tag filter shows pack templates only. Catalog order is already
/// deterministic (roots, then sorted packs, then manifest order).
fn listing(catalog: &pack::Catalog, json: bool, tag: Option<&str>) -> Outcome {
    let entries: Vec<&Entry> = catalog
        .entries
        .iter()
        .filter(|e| tag.is_none_or(|t| e.tags.iter().any(|have| have == t)))
        .collect();
    let text = if json {
        json_listing(&entries)
    } else {
        text_listing(&entries)
    };
    let mut problems = String::new();
    for problem in &catalog.problems {
        problems.push_str(problem);
        problems.push('\n');
    }
    Outcome {
        stdout: text.into_bytes(),
        stderr: problems.into_bytes(),
        code: 0,
    }
}

/// Text form: `pack-id · name · model · source-label`, one per line.
fn text_listing(entries: &[&Entry]) -> String {
    let mut out = String::new();
    for e in entries {
        let _ = writeln!(
            out,
            "{} \u{b7} {} \u{b7} {} \u{b7} {}",
            e.pack_id,
            e.name,
            e.model.name(),
            e.source.label()
        );
    }
    out
}

/// JSON form: one object per template inside a pretty array, one array element
/// per line so a pipe into `jq` (or a line-wise reader) sees whole records.
/// The whole catalog's fields, not just the text columns: a caller that wants
/// to inspect a pack must not need a second command to get at it.
fn json_listing(entries: &[&Entry]) -> String {
    if entries.is_empty() {
        return "[]\n".to_string();
    }
    let mut out = String::from("[\n");
    for (i, e) in entries.iter().enumerate() {
        let root = match &e.root {
            Some(p) => jstr(&p.display().to_string()),
            None => "null".to_string(),
        };
        let tags = jstr_list(&e.tags);
        let slots = jstr_list(&e.slots);
        let vars: Vec<String> = e
            .vars
            .iter()
            .map(|v| {
                format!(
                    "{{\"name\": {}, \"required\": {}}}",
                    jstr(&v.name),
                    v.required
                )
            })
            .collect();
        let _ = writeln!(
            out,
            "  {{\"address\": {}, \"pack_id\": {}, \"pack_name\": {}, \"name\": {}, \
             \"model\": {}, \"source\": {}, \"version\": {}, \"description\": {}, \
             \"tags\": {}, \"slots\": {}, \"vars\": [{}], \"root\": {}}}{}",
            jstr(&e.address()),
            jstr(&e.pack_id),
            jstr(&e.pack_name),
            jstr(&e.name),
            jstr(e.model.name()),
            jstr(e.source.label()),
            jstr(&e.version),
            jstr(&e.description),
            tags,
            slots,
            vars.join(", "),
            root,
            if i + 1 == entries.len() { "" } else { "," }
        );
    }
    out.push_str("]\n");
    out
}

/// A JSON array of strings on one line.
fn jstr_list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|s| jstr(s)).collect();
    format!("[{}]", quoted.join(", "))
}

/// A JSON string literal: quote, backslash and the control characters escaped,
/// everything else (including the UTF-8 the templates carry) verbatim.
fn jstr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// --------------------------------------------------------------- the verbs --

/// The verbs `templates` recognises. A first word of `templates` is only a
/// command if the next word is one of these (or `--help`): otherwise
/// `templates` is an ordinary filename and the conversion path runs, so an
/// existing invocation that happens to name a file `templates` is unchanged.
pub const VERBS: &[&str] = &[
    "list", "inspect", "new", "install", "remove", "pack", "unpack", "validate",
];

/// Whether a `textrill ...` command line opens the `templates` command group.
pub fn opens_command_group(args: &[String]) -> bool {
    args.first().is_some_and(|a| a == "templates")
        && args.get(1).is_some_and(|a| {
            VERBS.contains(&a.as_str()) || matches!(a.as_str(), "--help" | "-h" | "help")
        })
}

/// Run `textrill templates <verb> [args]`. `args` is the tail after
/// `"templates"`: the verb, then its own words (which may include options such
/// as `--json` and `--template_dir`). A missing or unknown verb is a usage
/// error naming the choice.
pub fn run(args: &[String]) -> Outcome {
    let Some((verb, rest)) = args.split_first() else {
        return usage_error("`templates` needs a command");
    };
    match verb.as_str() {
        "list" => verb_list(rest),
        "inspect" => verb_inspect(rest),
        "new" => verb_new(rest),
        "install" => verb_install(rest),
        "remove" => verb_remove(rest),
        "pack" => verb_pack(rest),
        "unpack" => verb_unpack(rest),
        "validate" => verb_validate(rest),
        "--help" | "-h" | "help" | "" => Outcome {
            stdout: verb_usage().into_bytes(),
            stderr: Vec::new(),
            code: 0,
        },
        other => usage_error(&format!("unknown `templates` command {other:?}")),
    }
}

/// The verb table, printed by `templates --help`.
pub fn verb_usage() -> String {
    "Usage: textrill templates <command> [args]\n\
     \n\
     \x20 list [--tag TAG] [--json]      list discovered templates\n\
     \x20 inspect <id>[:<template>]      show a pack's manifest, slots and vars\n\
     \x20 new <name>                     scaffold a pack under ./.textrill/templates\n\
     \x20 install <PATH>                 install a pack directory or a .tar\n\
     \x20 remove <id>                    remove a user-installed pack\n\
     \x20 pack <dir>                     write a deterministic .tar on stdout\n\
     \x20 unpack <archive> [DEST]        extract a .tar into DEST (or its stem)\n\
     \x20 validate <dir|archive>         run the pack checks and report\n\
     \n\
     Every command also accepts --template_dir DIR (repeatable).\n"
        .to_string()
}

/// A usage error: `textrill templates: <message>` on stderr, exit 1, and the
/// verb table so a mistyped command shows what it could have been.
fn usage_error(message: &str) -> Outcome {
    let mut err = format!("textrill templates: {message}\n");
    err.push_str(&verb_usage());
    Outcome {
        stdout: Vec::new(),
        stderr: err.into_bytes(),
        code: 1,
    }
}

/// A verb's own options are parsed with the same [`crate::cli`] machinery as
/// the main command line, so `--template_dir` composes from the environment,
/// rc files and the flags identically. Only the arguments are captured; the
/// verb's positional words are returned for its own count check.
struct Parsed {
    opts: Options,
    positionals: Vec<String>,
    tag: Option<String>,
    json: bool,
}

/// Split a verb's arguments into its recognised flags and positionals, then
/// parse the option-shaped remainder (rc files first, flags last) so nothing a
/// verb adds can drift from the main parser's rules.
fn parse_verb(verb: &str, args: &[String]) -> Result<Parsed, String> {
    let mut tag: Option<String> = None;
    let mut json = false;
    let mut pass: Vec<String> = Vec::new();
    let mut positionals: Vec<String> = Vec::new();
    let mut it = args.iter().peekable();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--tag" => {
                let v = it.next().ok_or_else(|| "--tag needs a value".to_string())?;
                tag = Some(v.clone());
            }
            "--" => {
                positionals.extend(it.by_ref().cloned());
                break;
            }
            other if other.starts_with("--tag=") => {
                tag = Some(other["--tag=".len()..].to_string());
            }
            _ => pass.push(arg.clone()),
        }
    }
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let opts = Options::default();
    let mut opts = opts;
    crate::cli::parse_args_with_rc(&pass, &mut opts, home.as_deref(), &cwd)
        .map_err(|e| format!("{verb}: {e}"))?;
    // Anything the parser did not consume as an option is a positional word;
    // the parse above pushes unknown bare words into `infile`, so recover them
    // from there rather than re-splitting.
    positionals.extend(std::mem::take(&mut opts.infile));
    Ok(Parsed {
        opts,
        positionals,
        tag,
        json,
    })
}

/// Refuse a verb's positional count with the count and the usage it wants.
fn expect_one(verb: &str, p: &Parsed, what: &str) -> Result<String, String> {
    match p.positionals.len() {
        1 => Ok(p.positionals[0].clone()),
        0 => Err(format!("{verb} needs {what}")),
        n => Err(format!(
            "{verb} takes exactly one {what}; got {n} arguments"
        )),
    }
}

/// `list [--tag TAG] [--json]`.
fn verb_list(args: &[String]) -> Outcome {
    let p = match parse_verb("list", args) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    // `list` ignores positionals: an rc file's `--` section can drop stray
    // words into `infile`, and refusing a listing over them would be hostile.
    let catalog = pack::discover(&p.opts.search_path());
    listing(&catalog, p.json, p.tag.as_deref())
}

/// `inspect <id>[:<template>]`.
fn verb_inspect(args: &[String]) -> Outcome {
    let p = match parse_verb("inspect", args) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    let spec = match expect_one("inspect", &p, "<id>[:<template>]") {
        Ok(s) => s,
        Err(e) => return usage_error(&e),
    };
    let catalog = pack::discover(&p.opts.search_path());
    let entry = match catalog.resolve(&spec) {
        Ok(e) => e,
        Err(e) => return failure(&format!("inspect: {e}")),
    };
    let mut out = String::new();
    let _ = writeln!(out, "address:     {}", entry.address());
    let _ = writeln!(out, "pack:        {} ({})", entry.pack_name, entry.pack_id);
    let _ = writeln!(out, "version:     {}", entry.version);
    let _ = writeln!(out, "source:      {}", entry.source.label());
    let _ = writeln!(out, "model:       {}", entry.model.name());
    if !entry.description.is_empty() {
        let _ = writeln!(out, "description: {}", entry.description);
    }
    if !entry.tags.is_empty() {
        let _ = writeln!(out, "tags:        {}", entry.tags.join(", "));
    }
    if !entry.slots.is_empty() {
        let _ = writeln!(out, "slots:       {}", entry.slots.join(", "));
    }
    if !entry.vars.is_empty() {
        let vars: Vec<String> = entry
            .vars
            .iter()
            .map(|v| {
                if v.required {
                    format!("{}=required", v.name)
                } else {
                    v.name.clone()
                }
            })
            .collect();
        let _ = writeln!(out, "vars:        {}", vars.join(", "));
    }
    match &entry.root {
        Some(root) => {
            let _ = writeln!(out, "path:        {}", root.display());
            let file = root.join(&entry.name);
            let _ = writeln!(out, "file:        {}.html", file.display());
        }
        None => {
            let _ = writeln!(out, "path:        (compiled in)");
        }
    }
    Outcome {
        stdout: out.into_bytes(),
        stderr: Vec::new(),
        code: 0,
    }
}

/// A failure that is not a usage error: one line on stderr, exit 1.
fn failure(message: &str) -> Outcome {
    Outcome {
        stdout: Vec::new(),
        stderr: format!("textrill: {message}\n").into_bytes(),
        code: 1,
    }
}

/// `new <name>` — scaffold a pack under `./.textrill/templates/<name>/`, which
/// discovery already reads as the project root, so it is selectable with no
/// flags the moment it is created. The pack id is derived from the working
/// directory: a reversed, sanitized set of its component labels plus the pack
/// name, so two projects do not collide by accident.
fn verb_new(args: &[String]) -> Outcome {
    let p = match parse_verb("new", args) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    let name = match expect_one("new", &p, "<name>") {
        Ok(s) => s,
        Err(e) => return usage_error(&e),
    };
    let leaf = match sanitize(&name) {
        Some(leaf) => leaf,
        None => {
            return usage_error(&format!(
                "new: {name:?} has no usable characters for a pack or file name"
            ))
        }
    };
    let id = scaffold_id(&leaf);
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let root = cwd.join(".textrill").join("templates").join(&id);
    if root.exists() {
        return failure(&format!("new: {} already exists", root.display()));
    }
    let manifest = scaffold_manifest(&id, &leaf);
    let html = scaffold_html(&leaf);
    let write = std::fs::create_dir_all(&root)
        .map_err(|e| format!("{}: {e}", root.display()))
        .and_then(|()| {
            std::fs::write(root.join(pack::MANIFEST), &manifest)
                .map_err(|e| format!("{}: {e}", root.join(pack::MANIFEST).display()))
        })
        .and_then(|()| {
            std::fs::write(root.join(format!("{leaf}.html")), &html)
                .map_err(|e| format!("{}: {e}", root.join(format!("{leaf}.html")).display()))
        });
    if let Err(e) = write {
        return failure(&format!("new: {e}"));
    }
    let out = format!(
        "created {}\n  {}\n  {leaf}.html\n\nid: {id}\nselect it with: \
         --template_library {id}:{leaf}\n",
        root.display(),
        pack::MANIFEST
    );
    Outcome {
        stdout: out.into_bytes(),
        stderr: Vec::new(),
        code: 0,
    }
}

/// A pack or file name from a user's word: lowercase, with runs of anything
/// else folded to a single `-`. `None` when nothing usable is left.
fn sanitize(word: &str) -> Option<String> {
    let mut out = String::new();
    let mut last_dash = false;
    for c in word.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// A reverse-DNS id for a scaffolded pack, from the working directory's
/// components (reversed, sanitized, non-numeric) plus `leaf`. An empty prefix
/// (say, a scratch directory of digits) becomes the label `local`, which keeps
/// the id valid and readable.
fn scaffold_id(leaf: &str) -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let mut labels: Vec<String> = cwd
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => sanitize(&s.to_string_lossy()),
            _ => None,
        })
        .filter(|s| s.chars().any(|c| c.is_ascii_alphabetic()))
        .collect();
    labels.reverse();
    labels.push(leaf.to_string());
    if labels.len() < 2 {
        labels.insert(0, "local".to_string());
    }
    labels.join(".")
}

/// The minimal manifest a scaffolded pack needs, valid against §5.3 with no
/// edits: one document-model template using only the fixed `title` slot.
fn scaffold_manifest(id: &str, leaf: &str) -> String {
    format!(
        "schema = 1\n\
         id = {id:?}\n\
         name = {name:?}\n\
         version = \"0.1.0\"\n\
         license = \"GPL-3.0-or-later\"\n\
         description = \"A scaffolded textrill template pack.\"\n\
         \n\
         [[templates]]\n\
         file = {file:?}\n\
         model = \"document\"\n",
        name = format!("{leaf} templates"),
        file = format!("{leaf}.html"),
    )
}

/// A minimal whole-document template: the required `content` slot plus a title,
/// so it validates and converts out of the box.
fn scaffold_html(leaf: &str) -> String {
    format!(
        "<!DOCTYPE html>\n\
         <html>\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <title>{{{{textrill:title}}}}</title>\n\
         </head>\n\
         <body>\n\
         <main class=\"{leaf}\">\n\
         {{{{textrill:content}}}}\n\
         </main>\n\
         </body>\n\
         </html>\n"
    )
}

/// `install <PATH>` — a pack directory or a `.tar`, staged beside its
/// destination and renamed into place only after it validates, so a refused or
/// half-copied pack never appears in the catalog.
fn verb_install(args: &[String]) -> Outcome {
    let p = match parse_verb("install", args) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    let path = match expect_one("install", &p, "<PATH>") {
        Ok(s) => s,
        Err(e) => return usage_error(&e),
    };
    let data_home = match p.opts.search_path().data_home {
        Some(dir) => dir,
        None => return failure("install: no user data directory (set XDG_DATA_HOME or HOME)"),
    };
    let source = std::path::Path::new(&path);
    let meta = match std::fs::metadata(source) {
        Ok(m) => m,
        Err(e) => return failure(&format!("install: {path}: {e}")),
    };

    let staging = data_home.join(format!(".install-{}-{}", std::process::id(), next_seq()));
    let result = (|| -> Result<String, String> {
        std::fs::create_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
        if meta.is_dir() {
            copy_tree(source, &staging)?;
        } else {
            let bytes = std::fs::read(source).map_err(|e| format!("{}: {e}", source.display()))?;
            pack::unpack(&bytes, &staging)?;
        }
        let validated = pack::load(&staging)?;
        let dest = data_home.join(&validated.id);
        if dest.exists() {
            return Err(format!(
                "install: {} is already installed at {}; remove it first",
                validated.id,
                dest.display()
            ));
        }
        std::fs::rename(&staging, &dest)
            .map_err(|e| format!("install: could not move {} into place: {e}", validated.id))?;
        Ok(format!(
            "installed {} at {}\n",
            validated.id,
            dest.display()
        ))
    })();

    let _ = std::fs::remove_dir_all(&staging);
    match result {
        Ok(text) => Outcome {
            stdout: text.into_bytes(),
            stderr: Vec::new(),
            code: 0,
        },
        Err(e) => failure(&e),
    }
}

/// `remove <id>` — user-installed packs only. The id must be a single plain
/// path component (no separators, no `..`) so the removal can never reach
/// outside `data_home`, and the shipped set cannot be named at all.
fn verb_remove(args: &[String]) -> Outcome {
    let p = match parse_verb("remove", args) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    let id = match expect_one("remove", &p, "<id>") {
        Ok(s) => s,
        Err(e) => return usage_error(&e),
    };
    if id == pack::SHIPPED_ID {
        return failure("remove: the shipped templates cannot be removed");
    }
    if !is_plain_component(&id) {
        return failure(&format!("remove: {id:?} is not a valid pack id"));
    }
    let data_home = match p.opts.search_path().data_home {
        Some(dir) => dir,
        None => return failure("remove: no user data directory (set XDG_DATA_HOME or HOME)"),
    };
    let dir = data_home.join(&id);
    if !dir.is_dir() {
        // Where a pack by this name is actually visible may be elsewhere
        // (a project `.textrill/templates/<id>`, an installed system copy),
        // so name that rather than a bare "not found".
        let catalog = pack::discover(&p.opts.search_path());
        let found: Vec<String> = catalog
            .entries
            .iter()
            .filter(|e| e.pack_id == id)
            .map(|e| e.source.label().to_string())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if found.is_empty() {
            return failure(&format!("remove: {id} is not installed"));
        }
        return failure(&format!(
            "remove: {id} is not installed here; it was found as a {} pack and \
             can only be removed from its own location",
            found.join(" / ")
        ));
    }
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => Outcome {
            stdout: format!("removed {id}\n").into_bytes(),
            stderr: Vec::new(),
            code: 0,
        },
        Err(e) => failure(&format!("remove: {}: {e}", dir.display())),
    }
}

/// `pack <dir>` — a deterministic `.tar` of the pack on stdout. Validation runs
/// first, so a pack that would not install is not archived either.
fn verb_pack(args: &[String]) -> Outcome {
    let p = match parse_verb("pack", args) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    let dir = match expect_one("pack", &p, "<dir>") {
        Ok(s) => s,
        Err(e) => return usage_error(&e),
    };
    match pack::pack(std::path::Path::new(&dir)) {
        Ok(bytes) => Outcome {
            stdout: bytes,
            stderr: Vec::new(),
            code: 0,
        },
        Err(e) => failure(&format!("pack: {e}")),
    }
}

/// `unpack <archive> [DEST]` — extract into DEST, defaulting to the archive's
/// own stem beside it. An existing destination is refused rather than merged.
fn verb_unpack(args: &[String]) -> Outcome {
    let p = match parse_verb("unpack", args) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    if p.positionals.is_empty() || p.positionals.len() > 2 {
        return usage_error("unpack <archive> [DEST]");
    }
    let archive = std::path::Path::new(&p.positionals[0]);
    let dest = match p.positionals.get(1) {
        Some(d) => std::path::PathBuf::from(d),
        None => match default_dest(archive) {
            Some(d) => d,
            None => {
                return failure(&format!(
                    "unpack: cannot derive a destination from {}; pass one",
                    archive.display()
                ))
            }
        },
    };
    if dest.exists() {
        return failure(&format!("unpack: {} already exists", dest.display()));
    }
    let bytes = match std::fs::read(archive) {
        Ok(b) => b,
        Err(e) => return failure(&format!("unpack: {}: {e}", archive.display())),
    };
    match pack::unpack(&bytes, &dest) {
        Ok(made) => Outcome {
            stdout: format!("unpacked {} entries into {}\n", made.len(), dest.display())
                .into_bytes(),
            stderr: Vec::new(),
            code: 0,
        },
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dest);
            failure(&format!("unpack: {e}"))
        }
    }
}

/// `validate <dir|archive>` — the §5.3 checks. A directory is validated in
/// place; an archive is unpacked into a staging directory that is always
/// removed, so validation leaves nothing behind.
fn verb_validate(args: &[String]) -> Outcome {
    let p = match parse_verb("validate", args) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    let target = match expect_one("validate", &p, "<dir|archive>") {
        Ok(s) => s,
        Err(e) => return usage_error(&e),
    };
    let path = std::path::Path::new(&target);
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return failure(&format!("validate: {target}: {e}")),
    };
    let (validated, cleanup) = if meta.is_dir() {
        (pack::load(path), None)
    } else {
        let staging = std::env::temp_dir().join(format!(
            "textrill-validate-{}-{}",
            std::process::id(),
            next_seq()
        ));
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return failure(&format!("validate: {target}: {e}")),
        };
        let staged = pack::unpack(&bytes, &staging).and_then(|_| pack::load(&staging));
        (staged, Some(staging))
    };
    if let Some(dir) = cleanup {
        let _ = std::fs::remove_dir_all(&dir);
    }
    match validated {
        Ok(pack) => {
            let mut out = format!("{} is valid\n", target);
            let _ = writeln!(out, "id:        {}", pack.id);
            let _ = writeln!(out, "name:      {}", pack.name);
            let _ = writeln!(out, "version:   {}", pack.version);
            let _ = writeln!(out, "license:   {}", pack.license);
            let _ = writeln!(out, "templates: {}", pack.templates.len());
            for t in &pack.templates {
                let _ = writeln!(out, "  {} ({})", t.name, t.model.name());
            }
            Outcome {
                stdout: out.into_bytes(),
                stderr: Vec::new(),
                code: 0,
            }
        }
        Err(e) => failure(&format!("validate: {e}")),
    }
}

/// Whether `id` is a single normal path component: not empty, not `.`/`..`,
/// and carrying no separator. `remove` builds `data_home/<id>` from it, so this
/// is what keeps the removal inside the data home.
fn is_plain_component(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && !id.contains('/')
        && !id.contains('\\')
        && std::path::Path::new(id).components().count() == 1
        && matches!(
            std::path::Path::new(id).components().next(),
            Some(std::path::Component::Normal(_))
        )
}

/// The default `unpack` destination: the archive path with a trailing `.tar`
/// removed. `None` when the result would be empty or the archive has no name.
fn default_dest(archive: &std::path::Path) -> Option<std::path::PathBuf> {
    let name = archive.file_name()?.to_str()?;
    let stem = name.strip_suffix(".tar").unwrap_or(name);
    if stem.is_empty() {
        return None;
    }
    Some(archive.with_file_name(stem))
}

/// Copy a directory tree into `dest`, refusing symlinks: a pack is regular
/// files and directories only (§5.3), and a copy that carried a symlink out of
/// the tree would defeat the containment the validation then checks for.
fn copy_tree(src: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    for entry in std::fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", src.display()))?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        let meta =
            std::fs::symlink_metadata(&from).map_err(|e| format!("{}: {e}", from.display()))?;
        let ft = meta.file_type();
        if ft.is_symlink() {
            return Err(format!(
                "install: {} is a symlink; a pack must be regular files and \
                 directories",
                from.display()
            ));
        }
        if ft.is_dir() {
            std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
            copy_tree(&from, &to)?;
        } else if ft.is_file() {
            std::fs::copy(&from, &to).map_err(|e| format!("{}: {e}", to.display()))?;
        } else {
            return Err(format!(
                "install: {} is not a regular file or directory",
                from.display()
            ));
        }
    }
    Ok(())
}

/// A process-wide sequence for staging directory names, so two runs in the same
/// process (a test harness) never collide on a path.
fn next_seq() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    SEQ.fetch_add(1, Ordering::Relaxed)
}
