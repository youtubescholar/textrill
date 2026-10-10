//! `--stream` reads and writes a paragraph at a time.
//!
//! The flag is off by default and must not change any output, so every test
//! here either pins the byte-for-byte equality with the buffered path or pins
//! the situations the streaming path refuses because it cannot read them
//! identically (whole-body post-passes, `--instring`, non-UTF-8 encodings).

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

const SAMPLE: &str =
    "Intro paragraph.\n\nOne\n===\n\nfirst *ital* and #bold#\n\n- a\n- b\n\n    pre\n\nTail.\n";

fn tmpdir() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("textrill-p54-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_with_stdin(args: &[&str], input: &[u8]) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_textrill"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn textrill");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input)
        .expect("write stdin");
    let out = child.wait_with_output().unwrap();
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn run(args: &[&str]) -> Run {
    run_with_stdin(args, b"")
}

#[test]
fn streamed_file_output_equals_the_buffered_output() {
    let dir = tmpdir();
    let input = dir.join("in.txt");
    std::fs::write(&input, SAMPLE).unwrap();
    let path = input.to_string_lossy().into_owned();

    let buffered = run(&[&path]);
    let streamed = run(&["--stream", &path]);
    assert_eq!(buffered.code, 0, "stderr: {}", buffered.stderr);
    assert_eq!(streamed.code, 0, "stderr: {}", streamed.stderr);
    assert_eq!(
        streamed.stdout, buffered.stdout,
        "--stream changed the output"
    );
    assert!(streamed.stdout.contains("<h1>"), "{}", streamed.stdout);
}

#[test]
fn stream_reads_standard_input() {
    let buffered = run_with_stdin(&[], SAMPLE.as_bytes());
    let streamed = run_with_stdin(&["--stream"], SAMPLE.as_bytes());
    assert_eq!(buffered.code, 0);
    assert_eq!(streamed.code, 0, "stderr: {}", streamed.stderr);
    assert_eq!(streamed.stdout, buffered.stdout);
}

#[test]
fn stream_writes_the_outfile_it_is_given() {
    let dir = tmpdir();
    let input = dir.join("in.txt");
    let output = dir.join("out.html");
    std::fs::write(&input, SAMPLE).unwrap();

    let run = run(&[
        "--stream",
        "--outfile",
        &output.to_string_lossy(),
        &input.to_string_lossy(),
    ]);
    assert_eq!(run.code, 0, "stderr: {}", run.stderr);
    assert!(run.stdout.is_empty(), "stdout: {:?}", run.stdout);
    let written = std::fs::read_to_string(&output).unwrap();
    assert!(written.contains("<h1>"), "{written}");
}

#[test]
fn stream_is_refused_with_whole_body_passes_and_instring() {
    for flag in ["--section", "--toc", "--number_headings", "--chunk"] {
        let run = run(&["--stream", flag]);
        assert_eq!(run.code, 1, "{flag} should be refused");
        assert!(
            run.stderr.contains("--stream is not valid"),
            "{flag}: {}",
            run.stderr
        );
    }
    let run = run(&["--stream", "--instring", "hello"]);
    assert_eq!(run.code, 1);
    assert!(run.stderr.contains("--stream is not valid with --instring"));
}

#[test]
fn stream_is_refused_with_a_non_utf8_encoding() {
    let run = run(&["--stream", "--encoding", "cp1252"]);
    assert_eq!(run.code, 1);
    assert!(
        run.stderr.contains("--stream needs UTF-8 input"),
        "{}",
        run.stderr
    );
}

#[test]
fn stream_errors_on_input_that_is_not_utf8() {
    // A streaming converter writes as it reads, so a bad byte part-way through
    // leaves a partial document behind. What matters is that it is reported as
    // a failure rather than silently replaced with U+FFFD, and that the bad
    // paragraph itself never reaches the output.
    let run = run_with_stdin(&["--stream"], b"ok\n\n\xff\xfe bad\n");
    assert_eq!(run.code, 1);
    assert!(run.stderr.contains("textrill:"), "{}", run.stderr);
    assert!(run.stdout.contains("ok"), "stdout: {:?}", run.stdout);
    assert!(!run.stdout.contains("bad"), "stdout: {:?}", run.stdout);
}

#[test]
fn stream_exits_non_zero_with_an_unreadable_input() {
    // The buffered path established that an unreadable input file is not an
    // empty input and must exit non-zero. The streaming path guarantees the
    // same, however late the unreadable file appears in the list, so a broken
    // pipe on a later file cannot swallow the earlier failure.
    let dir = tmpdir();
    let input = dir.join("stream-missing.txt");
    let _ = std::fs::remove_file(&input);
    let run = run(&["--stream", &input.to_string_lossy()]);
    assert_eq!(run.code, 1, "stderr: {}", run.stderr);
    assert!(
        run.stderr.contains("could not read 1 input file(s)"),
        "{}",
        run.stderr
    );
}
