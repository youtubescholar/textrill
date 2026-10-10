//! Exit-code tests for the numeric options.
//!
//! A numeric option could once take the process down rather than be
//! rejected: `tab_width=0` divided by zero and exited 101, and a large value
//! asked the allocator for an impossible size and exited 134 with SIGABRT,
//! which cannot be caught even by the GUI's handler. Both were reachable from
//! the command line, and `tab_width=0` was reachable from the GUI as well,
//! because its spin box minimum was 0.
//!
//! The property worth pinning down is therefore not "these values are
//! accepted" but "no value of a numeric option can abort the process". That is
//! what most of this file asserts, across every numeric option, because the
//! value that crashes is whichever one feeds an allocation and whichever
//! downstream code divides by it.
//!
//! Exit codes observed before the fix, kept here so a regression is legible:
//! 101 for a Rust panic, 134 for an abort. `exit_ok` and `exit_clean_error`
//! name the two acceptable outcomes so the assertions read as intent.

use std::io::Write;
use std::process::{Command, Stdio};

/// Every numeric option, as the CLI spells it.
const NUMERIC_OPTIONS: &[&str] = &[
    "hrule_min",
    "indent_width",
    "min_caps_length",
    "par_indent",
    "preformat_whitespace_min",
    "short_line_length",
    "tab_width",
    "underline_length_tolerance",
    "underline_offset_tolerance",
];

/// The options bounded by [`NUMERIC_OPTION_MAX`], and the hazard each reaches.
/// Every one of these used to be able to take the process down, which is the
/// point of listing them together.
const BOUNDED: &[(&str, &str)] = &[
    (
        "tab_width",
        "the divisor in `tab % tw`, and the size of a repeat",
    ),
    ("indent_width", "the size of a space repeat"),
    ("preformat_whitespace_min", "the `\\s{{n},}` quantifier"),
    ("hrule_min", "the `{{n},}` quantifier"),
    ("min_caps_length", "the `[A-Z]{{n,}}` quantifier"),
];

/// The numeric options that are only ever compared, and are therefore left
/// unbounded so the port does not diverge from the reference.
const COMPARISON_ONLY: &[&str] = &[
    "par_indent",
    "short_line_length",
    "underline_length_tolerance",
    "underline_offset_tolerance",
];

const NUMERIC_OPTION_MAX: u32 = 999;

/// Values worth trying, including the ones that used to be fatal.
const VALUES: &[&str] = &[
    "0",
    "-1",
    "-128",
    "1",
    "999",
    "1000",
    "65536",
    "2147483647",
    "9223372036854775807",
];

struct Run {
    code: i32,
    stderr: String,
}

fn run(args: &[&str]) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_textrill"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run txt2html");
    // Dropping the stdin handle at all, let alone writing to it, races the
    // child. An option the engine rejects is reported before it reads any
    // input, so the child can legitimately be gone by the time this runs, and
    // the write then fails with EPIPE. That is the product behaving correctly,
    // so a broken pipe here is ignored: what these tests assert is the child's
    // exit code and diagnostic, and a child that exited on our input before
    // reading it has still told us what we asked about. Only a genuine I/O
    // error is worth failing over.
    match child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"a\tb\n\none two\n")
    {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(e) => panic!("write stdin: {e}"),
    }
    // Close the pipe so a child that *does* read input sees EOF and does not
    // block waiting for a writer that will never come.
    drop(child.stdin.take());
    let out = child.wait_with_output().expect("wait");
    Run {
        // 128 + signal, the way a shell reports it, so an abort is visible as
        // 134 rather than being folded into the exit code.
        code: out.status.code().unwrap_or_else(|| {
            use std::os::unix::process::ExitStatusExt;
            128 + out.status.signal().expect("killed by an unhandled signal")
        }),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn assert_not_crash(r: &Run, what: &str) {
    assert_ne!(r.code, 101, "{what}: exited 101, which is a Rust panic");
    assert_ne!(
        r.code, 134,
        "{what}: exited 134 (SIGABRT), which is an abort"
    );
    assert_ne!(r.code, 139, "{what}: exited 139 (SIGSEGV)");
    assert!(
        r.code == 0 || r.code == 1,
        "{what}: expected 0 or 1, got {}",
        r.code
    );
}

fn arg(option: &str, value: &str) -> String {
    format!("--{option}={value}")
}

/// No numeric option, at any value, may take the process down.
#[test]
fn no_numeric_option_can_crash_the_process() {
    for opt in NUMERIC_OPTIONS {
        for v in VALUES {
            let what = format!("--{opt}={v}");
            let r = run(&[&arg(opt, v)]);
            assert_not_crash(&r, &what);
        }
    }
}

/// `tab_width` is the one option with a floor as well as a ceiling: 0 was the
/// divisor, and a negative value reached the same place by way of the existing
/// `v.max(0)` clamp.
#[test]
fn tab_width_outside_its_range_is_a_clean_error() {
    for v in ["0", "-1", "-128", "1000", "65536", "9223372036854775807"] {
        let r = run(&[&arg("tab_width", v)]);
        assert_eq!(
            r.code,
            1,
            "--tab_width={v} should be rejected, got {r:?}",
            r = r.code
        );
        assert!(
            r.stderr.contains("tab_width must be between 1 and 999"),
            "--tab_width={v}: unhelpful message: {}",
            r.stderr.trim()
        );
    }
    for v in ["1", "8", "999"] {
        let r = run(&[&arg("tab_width", v)]);
        assert_eq!(
            r.code,
            0,
            "--tab_width={v} is in range and should convert: {r:?}",
            r = r.stderr
        );
    }
}

/// Every bounded option, one past its ceiling, is rejected with a message that
/// names the option, its value and the range.
#[test]
fn bounded_options_reject_one_past_the_ceiling() {
    for (opt, hazard) in BOUNDED {
        let v = (NUMERIC_OPTION_MAX + 1).to_string();
        let r = run(&[&arg(opt, &v)]);
        assert_eq!(
            r.code,
            1,
            "--{opt}={v} should be rejected: {r:?}",
            r = r.stderr
        );
        let low = if *opt == "tab_width" { 1 } else { 0 };
        let want = format!("{opt} must be between {low} and {NUMERIC_OPTION_MAX}");
        assert!(
            r.stderr.contains(&want),
            "--{opt}={v} is bounded by {hazard}, so the message should say \
             `{want}`, got: {}",
            r.stderr.trim()
        );
    }
}

/// The same options at their ceiling, and at the floor, still convert. A bound
/// that rejects ordinary values is not a fix.
#[test]
fn bounded_options_accept_their_range() {
    for (opt, _) in BOUNDED {
        for v in ["0", "1", "999"] {
            let r = run(&[&arg(opt, v)]);
            let accepted = if *opt == "tab_width" && v == "0" {
                false // the one value that must be rejected: it is the divisor
            } else {
                true
            };
            assert_eq!(
                r.code == 0,
                accepted,
                "--{opt}={v} should {}convert: exit {} / {}",
                if accepted { "" } else { "not " },
                r.code,
                r.stderr.trim()
            );
        }
    }
}

/// The four comparison-only options are used only in comparisons, so the port
/// accepts whatever the reference accepts rather than inventing a limit. This is
/// the compatibility policy in a test: no reason to diverge where the reference
/// is not wrong. Written out as its own list, and checked against
/// `NUMERIC_OPTIONS`, so a new numeric option cannot quietly fall through the
/// gap between "bounded" and "known to be harmless".
#[test]
fn comparison_only_options_are_not_bounded() {
    for opt in COMPARISON_ONLY {
        for v in ["0", "999", "65536", "9223372036854775807"] {
            let r = run(&[&arg(opt, v)]);
            assert_eq!(
                r.code,
                0,
                "--{opt}={v} must be accepted for parity with the reference, got {r:?}",
                r = r.stderr
            );
        }
    }

    // Every option is accounted for, on one side or the other.
    let mut all: Vec<&str> = BOUNDED.iter().map(|(o, _)| *o).collect();
    all.extend_from_slice(COMPARISON_ONLY);
    all.sort_unstable();
    let mut expected: Vec<&str> = NUMERIC_OPTIONS.to_vec();
    expected.sort_unstable();
    assert_eq!(
        all, expected,
        "a numeric option is neither bounded nor known harmless"
    );
}

// ---------------------------------------------------------------- unreadable input
//
// `--infile /nonexistent` printed `Could not open …` to stderr, carried on, and
// exited 0 having written a 0-byte output file. Perl does the same, so this is a
// declared deviation -- but the reference's behaviour is the one that breaks a
// build: `make` and CI read exit 0 as success and hand the next step an empty
// document. `--outfile` to an unwritable path was already exit 1, so the tool
// was inconsistent with itself.
//
// What is asserted here is the exit code, not the output: the output is
// deliberately unchanged, since an unreadable file contributed nothing to it
// either way, and changing it would move goldens for no gain.

fn tmp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("txt2html-cliexit-infile");
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir.join(name)
}

/// A missing input file, writing to a file.
#[test]
fn a_missing_input_file_exits_non_zero() {
    let out_path = tmp("missing-out.html");
    let _ = std::fs::remove_file(&out_path);
    let missing = tmp("no-such-file.txt");
    let _ = std::fs::remove_file(&missing);
    assert!(!missing.exists(), "test bug: {} exists", missing.display());

    let r = run(&[
        "--infile",
        missing.to_str().expect("path"),
        "--outfile",
        out_path.to_str().expect("path"),
    ]);
    assert_eq!(
        r.code,
        1,
        "a missing input must not read as success; stderr: {}",
        r.stderr.trim()
    );
    assert!(
        r.stderr.contains(missing.to_str().expect("path")),
        "diagnostic does not name the file it could not read: {}",
        r.stderr.trim()
    );
    // The reference's own message is still printed: the port reports the same
    // thing it always did, plus the exit code.
    assert!(
        r.stderr.contains("Could not open"),
        "the reference diagnostic was lost: {}",
        r.stderr.trim()
    );
    let _ = std::fs::remove_file(&out_path);
}

/// Same thing to stdout, since that is the common pipeline shape and a reader
/// of stdout is the one who most needs the failure to be visible.
#[test]
fn a_missing_input_file_to_stdout_exits_non_zero() {
    let missing = tmp("no-such-file-2.txt");
    let _ = std::fs::remove_file(&missing);
    let r = run(&["--infile", missing.to_str().expect("path")]);
    assert_eq!(r.code, 1, "stderr: {}", r.stderr.trim());
}

/// An input that cannot be read is not the same as an empty input. Both produce
/// a 0-byte or near-empty document, and that is exactly why the exit code has
/// to distinguish them.
#[test]
fn an_empty_input_still_exits_zero() {
    let empty = tmp("empty.txt");
    std::fs::write(&empty, "").expect("write");
    let r = run(&["--infile", empty.to_str().expect("path")]);
    assert_eq!(
        r.code,
        0,
        "an empty but readable file is a success, not a failure: {}",
        r.stderr.trim()
    );
    let _ = std::fs::remove_file(&empty);
}

/// The exit code must not be a blunt "any non-zero input list is a failure":
/// with several inputs, the readable ones are still converted, and the partial
/// document is written. Only the exit code differs from the reference.
#[test]
fn a_mix_of_readable_and_unreadable_still_converts_and_still_exits_non_zero() {
    let good = tmp("good.txt");
    let out_path = tmp("mix-out.html");
    std::fs::write(&good, "readable text\n").expect("write");
    let _ = std::fs::remove_file(&out_path);
    let missing = tmp("no-such-file-3.txt");
    let _ = std::fs::remove_file(&missing);

    let r = run(&[
        "--infile",
        good.to_str().expect("path"),
        "--infile",
        missing.to_str().expect("path"),
        "--outfile",
        out_path.to_str().expect("path"),
    ]);
    assert_eq!(
        r.code,
        1,
        "one unreadable input still means failure; stderr: {}",
        r.stderr.trim()
    );
    let written = std::fs::read_to_string(&out_path).expect("output should exist");
    assert!(
        written.contains("<p>readable text"),
        "the readable input was not converted: {written}"
    );
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&good);
}

/// A file that exists but cannot be read is the same failure, reached without a
/// path that does not exist. A directory is the portable way to ask for it: it
/// opens and then fails to read, on every platform this runs on.
#[test]
fn an_unreadable_input_exits_non_zero() {
    let dir = tmp("a-directory");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let r = run(&["--infile", dir.to_str().expect("path")]);
    assert_eq!(
        r.code,
        1,
        "a directory as input is not a readable file: {}",
        r.stderr.trim()
    );
    let _ = std::fs::remove_dir(&dir);
}

/// `--infile -` is standard input, and the caller supplies it. It must not be
/// caught by the unreadable-input check, and it must still exit 0.
#[test]
fn stdin_is_not_treated_as_an_unreadable_input() {
    let r = run(&["--infile", "-"]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr.trim());
    assert!(
        !r.stderr.contains("could not read"),
        "stdin was reported as unreadable: {}",
        r.stderr.trim()
    );
}

// ---------------------------------------------------------------- uncompilable regexp
//
// A user-supplied regular expression that does not compile used to abort the
// process with a panic and exit 101. `--custom_heading_regexp 'a('` is the
// smallest reproduction, and the GUI exposed that option as a free-text list,
// so one character of typo was a crash rather than a message.
//
// These assert the property, not the individual values: *no value of an option
// that takes a regular expression may take the process down*, and a rejected
// pattern is reported before any output exists.

/// The options whose value is a regular expression, as the CLI spells them.
const REGEXP_OPTIONS: &[&str] = textrill::options::Options::REGEXP_OPTIONS;

/// The CLI's own option table, which is the source of truth for which options
/// take a regexp. Kept in sync with `Options::user_patterns` by
/// `every_regexp_option_is_validated`, and deliberately not hand-copied here:
/// a list that quietly falls behind the code reads as coverage when it is
/// nothing of the kind.
const CLI_REGEXP_OPTIONS: &[&str] = &[
    "custom_heading_regexp",
    "preformat_start_marker",
    "preformat_end_marker",
];

/// Patterns that fail to compile, plus the parse error each should report.
const BAD_PATTERNS: &[(&str, &str)] = &[
    ("a(", "Opening parenthesis"),
    ("[", "Invalid character class"),
    ("*", "Target of repeat operator"),
    ("(?P<n>x)(", "Opening parenthesis"),
    // An inverted repetition range reaches the engine's "Error compiling regex"
    // path rather than a parse error. Both are covered because the point is that
    // a diagnostic naming the pattern arrives, not which of the two produces it.
    ("a{2,1}", "Error compiling regex"),
];

/// Every option the CLI describes as taking a regular expression must be
/// validated by `Options::validate`.
///
/// This is the guard on the guard. The fix is a hand-written list of three
/// options, and the failure mode of a hand-written list is not that it is wrong
/// today but that the next regexp option is added and not added to it -- which
/// reinstates the panic it removed, with nothing failing. So the list is
/// compared against the option table here.
#[test]
fn every_regexp_option_is_validated() {
    let declared: Vec<&str> = REGEXP_OPTIONS.to_vec();
    let mut expected = CLI_REGEXP_OPTIONS.to_vec();
    expected.sort_unstable();
    let mut got = declared.clone();
    got.sort_unstable();
    assert_eq!(
        got, expected,
        "Options::REGEXP_OPTIONS and the CLI option table disagree; a regexp \
         option that is not validated will panic the process"
    );

    // And the list is not merely present but wired up: a bad pattern on each
    // declared option must actually be rejected.
    for opt in &declared {
        let r = run(&[&arg(opt, "a(")]);
        assert_eq!(
            r.code, 1,
            "--{opt}: declared as taking a regexp but a bad one is accepted"
        );
    }
}

/// A pattern that compiles must still be accepted. Without this, "reject
/// everything" would pass every test above.
const GOOD_PATTERNS: &[&str] = &[
    "^\\d+\\. +\\w+",
    "a\\(",
    "[A-Za-z]+",
    "^(:?(:?&lt;)|<)PRE(:?(:?&gt;)|>)$",
    r"\s*\n\s*\n",
    "(?i)hello",
    "^$",
    "a{2}",
];

#[test]
fn no_regexp_option_can_crash_the_process() {
    for opt in REGEXP_OPTIONS {
        for (pat, _) in BAD_PATTERNS {
            let what = format!("--{opt}={pat:?}");
            let r = run(&[&arg(opt, pat)]);
            assert_not_crash(&r, &what);
        }
    }
}

#[test]
fn a_bad_pattern_is_a_clean_error_naming_the_option_and_the_pattern() {
    for opt in REGEXP_OPTIONS {
        for (pat, expect) in BAD_PATTERNS {
            let r = run(&[&arg(opt, pat)]);
            assert_eq!(
                r.code, 1,
                "--{opt}={pat:?}: expected a clean exit 1, got {}",
                r.code
            );
            assert!(
                r.stderr.contains(opt),
                "--{opt}={pat:?}: diagnostic does not name the option: {}",
                r.stderr.trim()
            );
            assert!(
                r.stderr.contains(pat),
                "--{opt}={pat:?}: diagnostic does not quote the pattern: {}",
                r.stderr.trim()
            );
            assert!(
                r.stderr.contains(expect),
                "--{opt}={pat:?}: expected the parser's complaint about {expect:?}, got: {}",
                r.stderr.trim()
            );
        }
    }
}

#[test]
fn valid_patterns_are_still_accepted() {
    for opt in REGEXP_OPTIONS {
        for pat in GOOD_PATTERNS {
            let what = format!("--{opt}={pat:?}");
            let r = run(&[&arg(opt, pat)]);
            assert_eq!(
                r.code,
                0,
                "{what}: rejected a valid pattern: {}",
                r.stderr.trim()
            );
        }
    }
}

/// The defaults must validate, or every bare invocation would fail.
#[test]
fn the_default_patterns_validate() {
    let r = run(&[]);
    assert_eq!(r.code, 0, "a bare run failed: {}", r.stderr.trim());
}

/// A rejected pattern must be reported before any output is written, so a
/// caller reading stdout never sees a half-converted document.
#[test]
fn a_rejected_pattern_writes_no_output() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_textrill"))
        .args(["--custom_heading_regexp", "a(", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    // The child rejects the pattern and exits without reading stdin, which
    // closes the pipe; whether this write lands before or after is a scheduling
    // race, not the behaviour under test. What matters is the empty stdout and
    // exit 1 asserted below.
    if let Err(e) = child.stdin.as_mut().expect("stdin").write_all(b"one two\n") {
        assert_eq!(
            e.kind(),
            std::io::ErrorKind::BrokenPipe,
            "unexpected write error: {e}"
        );
    }
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        out.stdout.is_empty(),
        "wrote {} bytes to stdout despite rejecting the pattern",
        out.stdout.len()
    );
}

/// A link-dictionary pattern that does not compile is a different path: it
/// reaches the engine through `LinkParser::add_regexp`, not `Options::validate`,
/// and it aborted the same way. It is reported and skipped rather than fatal,
/// because `convert_text` returns `String`.
#[test]
fn a_bad_link_dictionary_pattern_is_reported_and_skipped() {
    let dir = std::env::temp_dir().join("txt2html-cliexit-dict");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let dict = dir.join("bad.dict");
    // The /pattern/ form is the one that reaches the regex engine verbatim.
    std::fs::write(&dict, "/a(/ --> http://example.com/\n").expect("write dict");
    let out_path = dir.join("out.html");
    let _ = std::fs::remove_file(&out_path);

    let mut child = Command::new(env!("CARGO_BIN_EXE_textrill"))
        .args([
            "--links_dictionaries",
            dict.to_str().expect("path"),
            "--outfile",
            out_path.to_str().expect("path"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"a( heading here\n")
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    let code = out.status.code().unwrap_or_else(|| {
        use std::os::unix::process::ExitStatusExt;
        128 + out.status.signal().expect("killed by an unhandled signal")
    });
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_ne!(code, 101, "a bad dictionary pattern panicked: {stderr}");
    assert_ne!(code, 134, "a bad dictionary pattern aborted: {stderr}");
    assert_eq!(code, 0, "expected 0 with the pattern skipped, got {code}");
    assert!(
        stderr.contains("a(") && stderr.to_lowercase().contains("pattern"),
        "diagnostic does not identify the offending pattern: {}",
        stderr.trim()
    );
    assert!(
        out_path.exists(),
        "the rest of the dictionary should still have been applied"
    );
    let _ = std::fs::remove_file(&out_path);
}

/// A rejected value must be reported before any output is written, so a caller
/// reading stdout never sees a half-converted document.
#[test]
fn rejection_happens_before_any_output() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_textrill"))
        .args(["--tab_width=0", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    // The child rejects `--tab_width=0` and exits without reading stdin, which
    // closes the pipe. Whether this write lands before or after that is a
    // scheduling race, not the behaviour under test, so a broken pipe is fine;
    // what matters is the empty stdout and exit 1 asserted below.
    if let Err(e) = child.stdin.as_mut().expect("stdin").write_all(b"one two\n") {
        assert_eq!(
            e.kind(),
            std::io::ErrorKind::BrokenPipe,
            "unexpected write error: {e}"
        );
    }
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        out.stdout.is_empty(),
        "wrote {} bytes to stdout despite rejecting the options",
        out.stdout.len()
    );
}
