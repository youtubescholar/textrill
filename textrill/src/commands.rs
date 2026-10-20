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
    "list", "inspect", "new", "install", "remove", "pack", "unpack", "validate", "search",
    "verify", "sources", "update",
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
        "search" => verb_search(rest),
        "verify" => verb_verify(rest),
        "sources" => verb_sources(rest),
        "update" => verb_update(rest),
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
     \x20 search <query>                 search registered catalog indexes\n\
     \x20 inspect <id>[:<template>]      show a pack's manifest, slots and vars\n\
     \x20 new <name>                     scaffold a pack under ./.textrill/templates\n\
     \x20 install <PATH>                 install a pack directory or a .tar\n\
     \x20 install <id>[@ver]             install from a registered catalog\n\
     \x20 remove <id>                    remove a user-installed pack\n\
     \x20 pack <dir>                     write a deterministic .tar on stdout\n\
     \x20 unpack <archive> [DEST]        extract a .tar into DEST (or its stem)\n\
     \x20 validate <dir|archive>         run the pack checks and report\n\
     \x20 verify <id>                    verify an installed pack against catalog hashes\n\
     \x20 sources                        list registered catalog directories\n\
     \x20 update                         rescan catalogs (fetches nothing)\n\
     \n\
     Every command also accepts --template_dir DIR and --catalog_dir DIR (repeatable).\n"
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
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let root = cwd.join(".textrill").join("templates");
    let planned = match pack::plan_scaffold(&root, &cwd, &name) {
        Some(p) => p,
        None => {
            return usage_error(&format!(
                "new: {name:?} has no usable characters for a pack or file name"
            ))
        }
    };
    if planned.dir.exists() {
        return failure(&format!("new: {} already exists", planned.dir.display()));
    }
    if let Err(e) = pack::scaffold::write(&planned) {
        return failure(&format!("new: {e}"));
    }
    let out = format!(
        "created {}\n  {}\n  {leaf}.html\n\nid: {id}\nselect it with: \
         --template_library {id}:{leaf}\n",
        planned.dir.display(),
        pack::MANIFEST,
        leaf = planned.leaf,
        id = planned.id,
    );
    Outcome {
        stdout: out.into_bytes(),
        stderr: Vec::new(),
        code: 0,
    }
}

/// `install <PATH>` or `install <id>[@version]` from a registered catalog.
/// A path that exists on disk is a pack directory or a `.tar`, installed
/// directly; anything else is a catalog address, resolved across every
/// `--catalog_dir`, and installed only through the verified catalog path
/// (hash, `min_textrill`, manifest id). Both paths land in the user data
/// home via the engine's one staged install.
fn verb_install(args: &[String]) -> Outcome {
    let p = match parse_verb("install", args) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    let source = match expect_one("install", &p, "<PATH> or <id>[@version>") {
        Ok(s) => s,
        Err(e) => return usage_error(&e),
    };
    let search = p.opts.search_path();
    let data_home = match &search.data_home {
        Some(dir) => dir.clone(),
        None => return failure("install: no user data directory (set XDG_DATA_HOME or HOME)"),
    };
    let path = std::path::Path::new(&source);
    let mut problems = String::new();
    let result = if path.exists() {
        let meta = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) => return failure(&format!("install: {source}: {e}")),
        };
        if meta.is_dir() {
            pack::from_dir(path, &data_home)
        } else {
            match std::fs::read(path) {
                Ok(bytes) => pack::from_archive(&bytes, &data_home),
                Err(e) => Err(format!("install: {source}: {e}")),
            }
        }
    } else {
        let catalogs = pack::load_catalogs(&search);
        for cat in &catalogs {
            for problem in &cat.problems {
                problems.push_str(problem);
                problems.push('\n');
            }
        }
        match resolve_catalog_entry(&catalogs, &source) {
            Ok(entry) => pack::from_catalog_entry(entry, &data_home),
            Err(e) => Err(e),
        }
    };
    match result {
        Ok(text) => Outcome {
            stdout: text.into_bytes(),
            stderr: problems.into_bytes(),
            code: 0,
        },
        Err(e) => {
            let mut stderr = problems.into_bytes();
            let _ = std::io::Write::write_all(&mut stderr, format!("textrill: {e}\n").as_bytes());
            Outcome {
                stdout: Vec::new(),
                stderr,
                code: 1,
            }
        }
    }
}

/// Resolve `id[@version]` across every registered catalog, in
/// `--catalog_dir` order. A miss names what was searched rather than
/// silently falling through to a path error.
fn resolve_catalog_entry<'a>(
    catalogs: &'a [pack::CatalogIndex],
    spec: &str,
) -> Result<&'a pack::CatalogIndexEntry, String> {
    if catalogs.is_empty() {
        return Err(format!(
            "{spec} is not a path on disk and no catalogs are registered \
             (pass --catalog_dir DIR)"
        ));
    }
    let mut tried = Vec::new();
    for cat in catalogs {
        match cat.find(spec) {
            Ok(e) => return Ok(e),
            Err(e) => tried.push(e),
        }
    }
    Err(tried.join("\n"))
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

/// A process-wide sequence for scratch directory names, so two runs in the
/// same process (a test harness) never collide on a path.
fn next_seq() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    SEQ.fetch_add(1, Ordering::Relaxed)
}

/// `search <query>` over every registered catalog index. Matching is
/// case-insensitive over id, name, description and tags. An entry whose
/// `min_textrill` this build does not satisfy is still listed — with the
/// requirement named, never hidden (roadmap §6, workstream 5).
fn verb_search(args: &[String]) -> Outcome {
    let p = match parse_verb("search", args) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    if p.positionals.is_empty() {
        return usage_error("search needs a query");
    }
    let query = p.positionals.join(" ").to_lowercase();
    let catalogs = pack::load_catalogs(&p.opts.search_path());
    if catalogs.is_empty() {
        return failure("search: no catalog indexes registered (pass --catalog_dir DIR)");
    }
    let mut problems = String::new();
    let mut out = String::new();
    let mut hits = 0usize;
    for cat in &catalogs {
        for problem in &cat.problems {
            problems.push_str(problem);
            problems.push('\n');
        }
        for e in &cat.entries {
            let matches = e.id.to_lowercase().contains(&query)
                || e.name.to_lowercase().contains(&query)
                || e.description.to_lowercase().contains(&query)
                || e.tags.iter().any(|t| t.to_lowercase().contains(&query));
            if !matches {
                continue;
            }
            hits += 1;
            let mut line = format!(
                "{} \u{b7} {} \u{b7} {} \u{b7} {}",
                e.id,
                e.name,
                e.version,
                cat.path.display()
            );
            if let Some(min) = e.min_textrill.as_deref() {
                if matches!(pack::version_satisfies(pack::VERSION, min), Ok(false)) {
                    line.push_str(&format!(" (requires textrill >= {min})"));
                }
            }
            let _ = writeln!(out, "{line}");
        }
    }
    if hits == 0 {
        out.push_str("(no matches)\n");
    }
    Outcome {
        stdout: out.into_bytes(),
        stderr: problems.into_bytes(),
        code: 0,
    }
}

/// `sources` — the registered catalog directories, in `--catalog_dir` order,
/// each marked when it is missing its index file.
fn verb_sources(args: &[String]) -> Outcome {
    let p = match parse_verb("sources", args) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    let sp = p.opts.search_path();
    let mut out = String::new();
    if sp.catalog_dirs.is_empty() {
        out.push_str("(none; pass --catalog_dir DIR)\n");
    }
    for (i, dir) in sp.catalog_dirs.iter().enumerate() {
        let mark = if dir.join(pack::CATALOG_INDEX).is_file() {
            ""
        } else {
            "  (no catalog.toml)"
        };
        let _ = writeln!(out, "[{i}] {}{mark}", dir.display());
    }
    Outcome {
        stdout: out.into_bytes(),
        stderr: Vec::new(),
        code: 0,
    }
}

/// `update` — rescan every registered catalog. Nothing is fetched: the point
/// of the offline model is that re-running this sees whatever files arrived
/// since the last look, from whatever mechanism moved them there.
fn verb_update(args: &[String]) -> Outcome {
    let p = match parse_verb("update", args) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    let catalogs = pack::load_catalogs(&p.opts.search_path());
    let mut problems = String::new();
    let mut out = format!("rescanned {} catalog(s)\n", catalogs.len());
    for cat in &catalogs {
        let _ = writeln!(
            out,
            "{} \u{b7} {} pack(s)",
            cat.path.display(),
            cat.entries.len()
        );
        for problem in &cat.problems {
            problems.push_str(problem);
            problems.push('\n');
        }
    }
    Outcome {
        stdout: out.into_bytes(),
        stderr: problems.into_bytes(),
        code: 0,
    }
}

/// `verify <id>` — repack the installed directory with the deterministic
/// archive writer and compare against the `sha256` the catalog records. The
/// archive format is byte-stable by construction (§5.4), so a pack that is
/// byte-identical to what was installed hashes identically; any edit, added
/// file or removed file changes the hash. An entry with no recorded hash
/// cannot be verified and says so rather than passing vacuously.
fn verb_verify(args: &[String]) -> Outcome {
    let p = match parse_verb("verify", args) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    let id = match expect_one("verify", &p, "<id>") {
        Ok(s) => s,
        Err(e) => return usage_error(&e),
    };
    if !is_plain_component(&id) {
        return failure(&format!("verify: {id:?} is not a valid pack id"));
    }
    let search = p.opts.search_path();
    let pack_dir = match &search.data_home {
        Some(dir) => dir.join(&id),
        None => return failure("verify: no user data directory (set XDG_DATA_HOME or HOME)"),
    };
    if !pack_dir.is_dir() {
        return failure(&format!(
            "verify: {id} is not installed under {}",
            search.data_home.as_ref().unwrap().display()
        ));
    }
    let catalogs = pack::load_catalogs(&search);
    let mut problems = String::new();
    for cat in &catalogs {
        for problem in &cat.problems {
            problems.push_str(problem);
            problems.push('\n');
        }
    }
    let mut matched: Option<&pack::CatalogIndexEntry> = None;
    for cat in &catalogs {
        if let Ok(e) = cat.find(&id) {
            matched = Some(e);
            break;
        }
    }
    let entry = match matched {
        Some(e) => e,
        None => {
            let mut err = problems.clone();
            let _ = writeln!(
                err,
                "verify: no catalog lists {id}; pass --catalog_dir DIR to the \
                 catalog it came from"
            );
            return Outcome {
                stdout: Vec::new(),
                stderr: err.into_bytes(),
                code: 1,
            };
        }
    };
    let Some(expect) = entry.sha256.as_deref() else {
        return failure(&format!(
            "verify: the catalog entry for {id} carries no sha256; there is \
             nothing to check against"
        ));
    };
    let bytes = match pack::pack(&pack_dir) {
        Ok(b) => b,
        Err(e) => return failure(&format!("verify: {e}")),
    };
    let got = crate::pack::sha256::sha256_hex(&bytes);
    if expect.eq_ignore_ascii_case(&got) {
        let mut out = format!("{id} {} OK\n", entry.version);
        out.insert_str(0, &problems);
        Outcome {
            stdout: out.into_bytes(),
            stderr: Vec::new(),
            code: 0,
        }
    } else {
        failure(&format!(
            "verify: hash mismatch for {id} {}: the catalog says {expect}, \
             the installed pack hashes to {got}",
            entry.version
        ))
    }
}
