//! Exit-code tests for the numeric options.
//!
//! A3 was that a numeric option could take the process down rather than be
//! rejected: `tab_width=0` divided by zero and exited 101, and a large value
//! asked the allocator for an impossible size and exited 134 with SIGABRT,
//! which cannot be caught even by A4's GUI handler. Both were reachable from
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_txt2html"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run txt2html");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"a\tb\n\none two\n")
        .expect("write stdin");
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

/// A rejected value must be reported before any output is written, so a caller
/// reading stdout never sees a half-converted document.
#[test]
fn rejection_happens_before_any_output() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_txt2html"))
        .args(["--tab_width=0", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"one two\n")
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        out.stdout.is_empty(),
        "wrote {} bytes to stdout despite rejecting the options",
        out.stdout.len()
    );
}
