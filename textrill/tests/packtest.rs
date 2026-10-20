//! The `textrill templates` surface and pack templates on the command line
//! (roadmap §5.4). Every test here drives the real binary with a controlled
//! environment — its own `HOME`, `XDG_DATA_HOME`, `XDG_DATA_DIRS` and working
//! directory — so discovery, installation and removal are exercised against a
//! scratch tree rather than whatever the developer happens to have installed.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

/// A scratch directory with a private home, data home and project. The search
/// path a child sees is exactly what is set here: the data dirs are pinned to
/// a non-existent path so a real system install cannot leak into a test.
struct Scratch {
    root: PathBuf,
    home: PathBuf,
    data: PathBuf,
    project: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("textrill-pack-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        let data = root.join("data");
        let project = root.join("project");
        for d in [&home, &data, &project] {
            std::fs::create_dir_all(d).unwrap();
        }
        Scratch {
            root,
            home,
            data,
            project,
        }
    }

    /// A command for the built binary, its environment and working directory
    /// fully controlled.
    fn cmd(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_textrill"));
        c.current_dir(&self.project)
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_DATA_HOME", &self.data)
            // A directory that does not exist, so no system-wide pack shows up.
            .env("XDG_DATA_DIRS", self.root.join("no-system-share"))
            .env("PATH", std::env::var_os("PATH").unwrap_or_default());
        c
    }

    /// Run the binary with `args`, feeding `stdin`, and capture the result.
    fn run(&self, args: &[&str], stdin: &[u8]) -> Run {
        use std::io::Write;
        let mut child = self
            .cmd()
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn textrill");
        child.stdin.as_mut().unwrap().write_all(stdin).unwrap();
        let out = child.wait_with_output().unwrap();
        Run {
            code: out.status.code().unwrap_or(-1),
            stdout: out.stdout,
            stderr: out.stderr,
        }
    }

    /// The `templates/<id>` root `install` writes to.
    fn installed_root(&self) -> PathBuf {
        self.data.join("textrill").join("templates")
    }

    /// The project pack root discovery reads with no flags.
    fn project_root(&self) -> PathBuf {
        self.project.join(".textrill").join("templates")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Run {
    code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl Run {
    fn out(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
    fn err(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// A valid pack `id` under `root` with one document template named `name`,
/// carrying `tag`. Written directly so the test does not depend on `new`.
fn write_pack(root: &Path, id: &str, name: &str, tag: Option<&str>) {
    let dir = root.join(id);
    std::fs::create_dir_all(&dir).unwrap();
    let tags = match tag {
        Some(t) => format!("tags = [{t:?}]\n"),
        None => String::new(),
    };
    let manifest = format!(
        "schema = 1\nid = {id:?}\nname = \"Pack {id}\"\nversion = \"1.2.3\"\n\
         license = \"GPL-3.0-or-later\"\ndescription = \"desc {id}\"\n{tags}\n\
         [[templates]]\nfile = \"{name}.html\"\nmodel = \"document\"\n"
    );
    std::fs::write(dir.join("template.toml"), manifest).unwrap();
    std::fs::write(
        dir.join(format!("{name}.html")),
        format!(
            "<!DOCTYPE html><html><head><title>{{{{textrill:title}}}}</title>\
                 </head><body><section class=\"{name}\">{{{{textrill:content}}}}\
                 </section></body></html>"
        ),
    )
    .unwrap();
}

// ------------------------------------------------------------ --template_dir --

#[test]
fn template_dir_makes_a_pack_discoverable_and_selectable() {
    let s = Scratch::new("dir");
    let roots = s.root.join("extra");
    write_pack(&roots, "org.test.theme", "page", Some("demo"));

    let out = s.run(
        &[
            "--list_templates",
            "--template_dir",
            roots.to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(out.code, 0, "{}", out.err());
    assert!(out.out().contains("org.test.theme"), "{}", out.out());

    let conv = s.run(
        &[
            "--template_dir",
            roots.to_str().unwrap(),
            "--template_library",
            "org.test.theme:page",
            "--title",
            "Doc",
        ],
        b"Body text.\n",
    );
    assert_eq!(conv.code, 0, "{}", conv.err());
    assert!(conv.out().contains("class=\"page\""), "{}", conv.out());
    assert!(conv.out().contains("Body text."), "{}", conv.out());
}

#[test]
fn template_dir_beats_the_default_roots_and_names_the_collision() {
    let s = Scratch::new("precedence");
    // Same pack id in a project pack and in an explicit `--template_dir`.
    write_pack(&s.project_root(), "org.test.dup", "theme", None);
    let explicit = s.root.join("explicit");
    write_pack(&explicit, "org.test.dup", "theme", None);

    let out = s.run(
        &[
            "--list_templates",
            "--template_dir",
            explicit.to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(out.code, 0);
    // The explicit root wins; the loser is named as a problem on stderr.
    assert!(out.err().contains("already provided"), "{}", out.err());
}

// ------------------------------------------------------ --list_templates ---

#[test]
fn list_templates_json_is_machine_readable_and_carries_the_fields() {
    let s = Scratch::new("json");
    let roots = s.root.join("extra");
    write_pack(&roots, "org.test.json", "jpage", Some("jtag"));

    let out = s.run(
        &[
            "--list_templates=json",
            "--template_dir",
            roots.to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(out.code, 0, "{}", out.err());
    let text = out.out();
    assert!(text.starts_with("[\n"), "an array: {text}");
    assert!(text.trim_end().ends_with(']'), "closed: {text}");
    for needle in [
        "\"address\": \"org.test.json:jpage\"",
        "\"pack_id\": \"org.test.json\"",
        "\"name\": \"jpage\"",
        "\"model\": \"document\"",
        "\"source\": \"local\"",
        "\"version\": \"1.2.3\"",
        "\"tags\": [\"jtag\"]",
    ] {
        assert!(text.contains(needle), "missing {needle} in {text}");
    }
}

#[test]
fn a_bogus_list_templates_value_is_refused() {
    let s = Scratch::new("bogus");
    let out = s.run(&["--list_templates=bogus"], b"");
    assert_eq!(out.code, 1);
    assert!(out.err().contains("--list_templates"), "{}", out.err());
    assert!(out.err().contains("bogus"), "{}", out.err());
    assert!(out.out().is_empty(), "nothing printed: {}", out.out());
}

#[test]
fn the_bare_flag_and_no_flag_behave() {
    let s = Scratch::new("bare");
    let list = s.run(&["--list_templates"], b"");
    assert_eq!(list.code, 0);
    assert!(list.out().contains("shipped"), "{}", list.out());
    // `--no-list_templates` is the plain conversion path.
    let plain = s.run(&["--no-list_templates"], b"Hello\n");
    assert_eq!(plain.code, 0, "{}", plain.err());
    assert!(plain.out().contains("Hello"), "{}", plain.out());
}

#[test]
fn the_shipped_five_still_resolve_and_convert() {
    let s = Scratch::new("shipped");
    for name in ["article", "book", "manpage", "slide", "bare"] {
        let out = s.run(&["--template_library", name], b"One\n===\n\nText.\n");
        assert_eq!(out.code, 0, "{name}: {}", out.err());
        assert!(!out.out().is_empty(), "{name} produced nothing");
    }
    // A name nobody provides is a hard error naming what exists.
    let bad = s.run(&["--template_library", "nope"], b"x\n");
    assert_eq!(bad.code, 1);
    assert!(bad.err().contains("article"), "{}", bad.err());
}

// ------------------------------------------------------------- list jest ----

#[test]
fn templates_list_tag_filters_to_matching_packs() {
    let s = Scratch::new("tag");
    let roots = s.root.join("extra");
    write_pack(&roots, "org.test.tagged", "a", Some("alpha"));
    write_pack(&roots, "org.test.other", "b", Some("beta"));

    let out = s.run(
        &[
            "templates",
            "list",
            "--tag",
            "alpha",
            "--template_dir",
            roots.to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(out.code, 0, "{}", out.err());
    assert!(out.out().contains("org.test.tagged"), "{}", out.out());
    assert!(!out.out().contains("org.test.other"), "{}", out.out());
    // A tag filter drops the untagged shipped set too.
    assert!(!out.out().contains("shipped"), "{}", out.out());
}

#[test]
fn templates_list_json_matches_the_flag() {
    let s = Scratch::new("listjson");
    let roots = s.root.join("extra");
    write_pack(&roots, "org.test.lj", "p", None);
    let a = s.run(
        &[
            "templates",
            "list",
            "--json",
            "--template_dir",
            roots.to_str().unwrap(),
        ],
        b"",
    );
    let b = s.run(
        &[
            "--list_templates=json",
            "--template_dir",
            roots.to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(a.code, 0, "{}", a.err());
    assert_eq!(a.stdout, b.stdout, "list --json and =json must agree");
}

// ------------------------------------------------------------------ new ----

#[test]
fn templates_new_scaffolds_a_valid_selectable_pack() {
    let s = Scratch::new("new");
    let out = s.run(&["templates", "new", "My Theme"], b"");
    assert_eq!(out.code, 0, "{}", out.err());
    // The pack landed under the project root, so discovery finds it with no
    // flags at all.
    let listed = s.run(&["--list_templates"], b"");
    assert_eq!(listed.code, 0);
    assert!(listed.out().contains("my-theme"), "{}", listed.out());

    // It validates.
    let dir = only_dir(&s.project_root());
    let valid = s.run(&["templates", "validate", dir.to_str().unwrap()], b"");
    assert_eq!(valid.code, 0, "{}", valid.err());
    assert!(valid.out().contains("is valid"), "{}", valid.out());

    // Scaffolding again refuses rather than overwrite.
    let again = s.run(&["templates", "new", "My Theme"], b"");
    assert_eq!(again.code, 1);
    assert!(again.err().contains("already exists"), "{}", again.err());
}

/// The single subdirectory of `root`.
fn only_dir(root: &Path) -> PathBuf {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    assert_eq!(dirs.len(), 1, "expected one pack in {}", root.display());
    dirs.pop().unwrap()
}

#[test]
fn templates_new_scaffold_converts_out_of_the_box() {
    let s = Scratch::new("newconv");
    assert_eq!(s.run(&["templates", "new", "demo"], b"").code, 0);
    let dir = only_dir(&s.project_root());
    let id = read_id(&dir);
    let conv = s.run(
        &["--template_library", &format!("{id}:demo"), "--title", "T"],
        b"Body line.\n",
    );
    assert_eq!(conv.code, 0, "{}", conv.err());
    assert!(conv.out().contains("class=\"demo\""), "{}", conv.out());
    assert!(conv.out().contains("Body line."), "{}", conv.out());
}

/// The `id = "..."` value from a pack's manifest.
fn read_id(dir: &Path) -> String {
    let text = std::fs::read_to_string(dir.join("template.toml")).unwrap();
    text.lines()
        .find_map(|l| l.strip_prefix("id = "))
        .map(|v| v.trim().trim_matches('"').to_string())
        .expect("manifest has an id")
}

// --------------------------------------------------------------- inspect ---

#[test]
fn templates_inspect_reports_manifest_slots_and_vars() {
    let s = Scratch::new("inspect");
    let roots = s.root.join("extra");
    write_pack(&roots, "org.test.insp", "view", Some("iv"));
    let out = s.run(
        &[
            "templates",
            "inspect",
            "org.test.insp:view",
            "--template_dir",
            roots.to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(out.code, 0, "{}", out.err());
    let text = out.out();
    for needle in [
        "address:     org.test.insp:view",
        "version:     1.2.3",
        "source:      local",
        "model:       document",
        "tags:        iv",
    ] {
        assert!(text.contains(needle), "missing {needle} in {text}");
    }
}

// --------------------------------------------------------------- validate --

#[test]
fn templates_validate_refuses_a_pack_missing_the_content_slot() {
    let s = Scratch::new("badpack");
    let dir = s.root.join("broken");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("template.toml"),
        "schema = 1\nid = \"org.test.bad\"\nname = \"B\"\nversion = \"1.0.0\"\n\
         license = \"GPL-3.0-or-later\"\n\n[[templates]]\nfile = \"bad.html\"\n\
         model = \"document\"\n",
    )
    .unwrap();
    // No `{{textrill:content}}` slot: the pack must be refused.
    std::fs::write(dir.join("bad.html"), "<html><body>no slot</body></html>").unwrap();
    let out = s.run(&["templates", "validate", dir.to_str().unwrap()], b"");
    assert_eq!(out.code, 1);
    assert!(
        out.err().to_lowercase().contains("content"),
        "the refusal names the missing slot: {}",
        out.err()
    );
}

// ----------------------------------------------------------- pack / unpack --

#[test]
fn pack_then_unpack_reproduces_the_tree_byte_for_byte() {
    let s = Scratch::new("roundtrip");
    let roots = s.root.join("extra");
    write_pack(&roots, "org.test.rt", "page", None);

    let archive = s.root.join("rt.tar");
    let packed = s.run(
        &[
            "templates",
            "pack",
            roots.join("org.test.rt").to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(packed.code, 0, "{}", packed.err());
    std::fs::write(&archive, &packed.stdout).unwrap();

    // Same pack, twice, gives the same bytes (deterministic archive).
    let packed2 = s.run(
        &[
            "templates",
            "pack",
            roots.join("org.test.rt").to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(
        packed.stdout, packed2.stdout,
        "the archive must be deterministic"
    );

    let dest = s.root.join("extracted");
    let unpacked = s.run(
        &[
            "templates",
            "unpack",
            archive.to_str().unwrap(),
            dest.to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(unpacked.code, 0, "{}", unpacked.err());
    assert!(trees_equal(&roots.join("org.test.rt"), &dest));
}

/// Whether two directory trees hold the same relative paths with equal bytes.
fn trees_equal(a: &Path, b: &Path) -> bool {
    let mut files_a = list_files(a);
    let mut files_b = list_files(b);
    files_a.sort();
    files_b.sort();
    files_a == files_b
        && files_a
            .iter()
            .all(|rel| std::fs::read(a.join(rel)).ok() == std::fs::read(b.join(rel)).ok())
}

fn list_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    out
}

#[test]
fn unpack_refuses_an_existing_destination() {
    let s = Scratch::new("overflow");
    let roots = s.root.join("extra");
    write_pack(&roots, "org.test.ex", "p", None);
    let archive = s.root.join("ex.tar");
    let packed = s.run(
        &[
            "templates",
            "pack",
            roots.join("org.test.ex").to_str().unwrap(),
        ],
        b"",
    );
    std::fs::write(&archive, &packed.stdout).unwrap();
    let dest = s.root.join("already");
    std::fs::create_dir_all(&dest).unwrap();
    let out = s.run(
        &[
            "templates",
            "unpack",
            archive.to_str().unwrap(),
            dest.to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(out.code, 1);
    assert!(out.err().contains("already exists"), "{}", out.err());
}

// --------------------------------------------------------------- install ---

#[test]
fn install_from_a_directory_then_from_an_archive() {
    let s = Scratch::new("install");
    let roots = s.root.join("extra");
    write_pack(&roots, "org.test.one", "page", None);

    // Directory install.
    let out = s.run(
        &[
            "templates",
            "install",
            roots.join("org.test.one").to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(out.code, 0, "{}", out.err());
    assert!(s.installed_root().join("org.test.one").is_dir());
    assert!(
        out.out().contains("installed org.test.one"),
        "{}",
        out.out()
    );
    // Idempotence refused: it is already there.
    let again = s.run(
        &[
            "templates",
            "install",
            roots.join("org.test.one").to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(again.code, 1);
    assert!(again.err().contains("already installed"), "{}", again.err());

    // The installed pack is now discoverable with no flags.
    let listed = s.run(&["--list_templates"], b"");
    assert!(listed.out().contains("installed"), "{}", listed.out());

    // Archive install of a second pack.
    write_pack(&roots, "org.test.two", "view", None);
    let archive = s.root.join("two.tar");
    let packed = s.run(
        &[
            "templates",
            "pack",
            roots.join("org.test.two").to_str().unwrap(),
        ],
        b"",
    );
    std::fs::write(&archive, &packed.stdout).unwrap();
    let from_archive = s.run(&["templates", "install", archive.to_str().unwrap()], b"");
    assert_eq!(from_archive.code, 0, "{}", from_archive.err());
    assert!(s.installed_root().join("org.test.two").is_dir());
}

#[test]
fn install_refuses_a_pack_that_does_not_validate() {
    let s = Scratch::new("installbad");
    let dir = s.root.join("broken");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("template.toml"), "schema = 1\n").unwrap();
    let out = s.run(&["templates", "install", dir.to_str().unwrap()], b"");
    assert_eq!(out.code, 1);
    // Nothing was installed, and no staging directory was left behind.
    assert!(!s.installed_root().join("broken").exists());
    let leftovers: Vec<_> = std::fs::read_dir(s.installed_root())
        .map(|d| d.flatten().map(|e| e.file_name()).collect::<Vec<_>>())
        .unwrap_or_default();
    assert!(
        leftovers
            .iter()
            .all(|n| !n.to_string_lossy().starts_with(".install-")),
        "staging left behind: {leftovers:?}"
    );
}

// ---------------------------------------------------------------- remove ---

#[test]
fn remove_uninstalls_a_user_pack_and_refuses_the_shipped_set() {
    let s = Scratch::new("remove");
    let roots = s.root.join("extra");
    write_pack(&roots, "org.test.gone", "p", None);
    assert_eq!(
        s.run(
            &[
                "templates",
                "install",
                roots.join("org.test.gone").to_str().unwrap()
            ],
            b""
        )
        .code,
        0
    );
    let out = s.run(&["templates", "remove", "org.test.gone"], b"");
    assert_eq!(out.code, 0, "{}", out.err());
    assert!(!s.installed_root().join("org.test.gone").exists());

    // Removing again reports it is not installed.
    let again = s.run(&["templates", "remove", "org.test.gone"], b"");
    assert_eq!(again.code, 1);
    assert!(again.err().contains("not installed"), "{}", again.err());

    // The shipped set cannot be named for removal.
    let shipped = s.run(&["templates", "remove", "shipped"], b"");
    assert_eq!(shipped.code, 1);
    assert!(shipped.err().contains("shipped"), "{}", shipped.err());
}

#[test]
fn remove_refuses_a_traversal_id() {
    let s = Scratch::new("traversal");
    for id in ["..", "../x", "a/b", "."] {
        let out = s.run(&["templates", "remove", id], b"");
        assert_eq!(out.code, 1, "{id} must be refused");
        assert!(
            out.err().contains("valid pack id") || out.err().contains("not installed"),
            "{id}: {}",
            out.err()
        );
    }
}

// --------------------------------------------------------------- dispatch --

#[test]
fn a_bare_templates_word_is_a_filename_not_the_command_group() {
    // The command group is additive: `textrill templates` with no verb must be
    // an ordinary conversion invocation, so a file that happens to be named
    // `templates` still converts.
    let s = Scratch::new("bareword");
    std::fs::write(s.project.join("templates"), "Hello there.\n").unwrap();
    let out = s.run(&["templates"], b"");
    assert_eq!(out.code, 0, "{}", out.err());
    assert!(out.out().contains("Hello there."), "{}", out.out());
    assert!(
        !out.out().contains("Usage: textrill templates"),
        "{}",
        out.out()
    );
}

#[test]
fn an_unknown_first_word_after_templates_is_not_the_group() {
    // A known verb is required to open the group, so `templates frobnicate` is
    // two input filenames: `frobnicate` converts, `templates` is unreadable,
    // and the result is a non-zero conversion exit, not a usage dump.
    let s = Scratch::new("badverb");
    std::fs::write(s.project.join("frobnicate"), "file body.\n").unwrap();
    let out = s.run(&["templates", "frobnicate"], b"");
    assert!(
        !out.out().contains("Usage: textrill templates"),
        "must not open the command group: {}",
        out.out()
    );
    assert_eq!(out.code, 1, "the missing `templates` file is unreadable");
}

#[test]
fn templates_help_is_the_verb_table() {
    let s = Scratch::new("help");
    let out = s.run(&["templates", "--help"], b"");
    assert_eq!(out.code, 0, "{}", out.err());
    assert!(
        out.out().contains("Usage: textrill templates"),
        "{}",
        out.out()
    );
    // Every verb is listed.
    for verb in [
        "list", "inspect", "new", "install", "remove", "pack", "unpack", "validate",
    ] {
        assert!(out.out().contains(verb), "help omits {verb}: {}", out.out());
    }
}

#[test]
fn the_global_help_mentions_templates() {
    let s = Scratch::new("ghelp");
    let out = s.run(&["--help"], b"");
    assert_eq!(out.code, 0);
    assert!(out.out().contains("templates"), "{}", out.out());
}
