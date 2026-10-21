//! `--preset NAME` end to end (roadmap §8.1): a user preset in the config
//! file expands into the same options the GUI applies, sits above the rc
//! files and below the rest of the command line, and is refused inside an
//! option file rather than half-working.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

/// A scratch tree with a private HOME (and therefore XDG config) and project.
struct Scratch {
    root: PathBuf,
    home: PathBuf,
    project: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("textrill-presets-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        let project = root.join("project");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        Scratch {
            root,
            home,
            project,
        }
    }

    /// The `presets.toml` this scratch's `--preset` reads.
    fn write_presets(&self, text: &str) {
        let dir = self.home.join(".config").join("textrill");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("presets.toml"), text).unwrap();
    }

    fn run(&self, args: &[&str], stdin: &[u8]) -> Run {
        use std::io::Write;
        let mut child = Command::new(env!("CARGO_BIN_EXE_textrill"))
            .current_dir(&self.project)
            .env_clear()
            .env("HOME", &self.home)
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
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

const PRESET_FILE: &str = "schema = 1\n\n\
    [[presets]]\n\
    key = \"study\"\n\
    name = \"Study notes\"\n\
    description = \"Article with a sectioned TOC.\"\n\
    template_library = \"article\"\n\
    section = true\n\
    toc = true\n";

#[test]
fn a_preset_expands_into_the_conversion() {
    let s = Scratch::new("expand");
    s.write_presets(PRESET_FILE);
    let out = s.run(&["--preset", "study", "-"], b"Head\n=====\nbody\n");
    assert_eq!(out.code, 0, "{}", out.err());
    assert!(
        out.out().contains("class=\"toc\""),
        "the preset's --toc must be active: {}",
        out.out()
    );
    assert!(
        out.out().contains("class=\"section\""),
        "the preset's --section must be active: {}",
        out.out()
    );
}

#[test]
fn the_command_line_beats_the_preset_wherever_it_stands() {
    let s = Scratch::new("override");
    s.write_presets(PRESET_FILE);
    // Both orders: the preset is expanded before the line is parsed, so
    // `--toc` after it and `--no-toc` before it both win.
    for args in [
        vec!["--preset", "study", "--no-toc", "-"],
        vec!["--no-toc", "--preset", "study", "-"],
    ] {
        let out = s.run(&args, b"Head\n=====\nbody\n");
        assert_eq!(out.code, 0, "{}", out.err());
        assert!(
            !out.out().contains("class=\"toc\""),
            "{args:?}: the command line must win over the preset: {}",
            out.out()
        );
    }
}

#[test]
fn the_preset_beats_the_rc_file() {
    let s = Scratch::new("rc");
    s.write_presets(PRESET_FILE);
    std::fs::write(
        s.project.join(".textrillrc"),
        "--tab_width 7\n--template_library bare\n",
    )
    .unwrap();
    let out = s.run(&["--preset", "study", "-"], b"Head\n=====\nbody\n");
    assert_eq!(out.code, 0, "{}", out.err());
    // The preset replaced the rc state: `bare` is gone (article's own frame
    // and TOC are back), so the preset sat above the file.
    assert!(
        out.out().contains("class=\"toc\""),
        "the preset must sit above the rc file: {}",
        out.out()
    );
}

#[test]
fn a_missing_preset_file_makes_the_name_a_loud_error() {
    let s = Scratch::new("missing");
    let out = s.run(&["--preset", "nope", "-"], b"text\n");
    assert_eq!(out.code, 1, "a missing preset must fail loudly");
    assert!(out.err().contains("no preset `nope`"), "{}", out.err());
    assert!(out.err().contains("presets.toml"), "{}", out.err());
}

#[test]
fn a_broken_preset_file_names_the_line() {
    let s = Scratch::new("broken");
    s.write_presets("schema = 1\n[[presets]]\nkey = \"x\"\nsction = true\n");
    let out = s.run(&["--preset", "x", "-"], b"text\n");
    assert_eq!(out.code, 1, "{}", out.err());
    assert!(out.err().contains("line 4:"), "{}", out.err());
    assert!(out.err().contains("`sction`"), "{}", out.err());
}

#[test]
fn preset_is_refused_inside_an_option_file() {
    let s = Scratch::new("in-rc");
    s.write_presets(PRESET_FILE);
    std::fs::write(s.project.join(".textrillrc"), "--preset study\n").unwrap();
    let out = s.run(&["-"], b"text\n");
    assert_eq!(out.code, 1, "{}", out.err());
    assert!(out.err().contains(".textrillrc:1"), "{}", out.err());
    assert!(
        out.err().contains("`--preset` belongs on the command line"),
        "{}",
        out.err()
    );
}
