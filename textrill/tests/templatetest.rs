//! P5.5 — `--template` and `--document_template`.
//!
//! Templates are off by default, so the reference output must not move. These
//! tests pin the wrapper and whole-document models, the slot substitution, the
//! pass-through of another engine's `{{ }}` tokens, and the refusals that keep
//! a malformed or incoherent template from producing a half-built page.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

const SAMPLE: &str =
    "Intro paragraph.\n\nOne\n===\n\nsecond paragraph\n\nTwo\n===\n\nthird paragraph\n";

fn tmpdir() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("textrill-p55-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_file(dir: &Path, name: &str, body: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    path.to_string_lossy().into_owned()
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

/// A wrapper whose only token is the content slot must be byte-identical to the
/// untemplated document: the engine's prolog and epilog are still emitted.
#[test]
fn content_only_wrapper_is_byte_identical_to_default() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let template = write_file(&dir, "wrap.html", "{{textrill:content}}");

    let plain = run(&[&input]);
    let wrapped = run(&["--template", &template, &input]);
    assert_eq!(plain.code, 0, "stderr: {}", plain.stderr);
    assert_eq!(wrapped.code, 0, "stderr: {}", wrapped.stderr);
    assert_eq!(
        wrapped.stdout, plain.stdout,
        "a content-only wrapper changed the document"
    );
    assert!(
        wrapped.stdout.starts_with("<!DOCTYPE"),
        "{}",
        wrapped.stdout
    );
    assert!(wrapped.stdout.contains("</html>"), "{}", wrapped.stdout);
}

#[test]
fn wrapper_supplies_surrounding_markup() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let template = write_file(
        &dir,
        "wrap.html",
        "<main class=\"page\">\n{{textrill:content}}</main>\n",
    );
    let out = run(&["--template", &template, &input]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("<main class=\"page\">"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("</main>"), "{}", out.stdout);
    // The content really is inside the wrapper, not appended after it.
    let open = out.stdout.find("<main").unwrap();
    let body = out.stdout.find("<h1>").unwrap();
    let close = out.stdout.find("</main>").unwrap();
    assert!(open < body && body < close, "{}", out.stdout);
}

#[test]
fn toc_slot_places_the_generated_navigation() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let template = write_file(
        &dir,
        "wrap.html",
        "<aside>{{textrill:toc}}</aside>\n{{textrill:content}}",
    );
    let out = run(&["--template", &template, "--toc", &input]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let aside = out.stdout.find("<aside>").unwrap();
    let nav = out.stdout.find("<nav class=\"toc\"").unwrap();
    let body = out.stdout.find("<h1>").unwrap();
    assert!(aside < nav && nav < body, "{}", out.stdout);
    assert!(
        out.stdout.contains("<a href=\"#chunk-1\">"),
        "{}",
        out.stdout
    );
}

#[test]
fn document_template_owns_the_whole_page() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let template = write_file(
        &dir,
        "doc.html",
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head><meta charset=\"utf-8\">{{textrill:head}}</head>\n<body class=\"doc\">\n{{textrill:content}}</body>\n</html>\n",
    );
    let out = run(&["--document_template", &template, &input]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.starts_with("<!DOCTYPE html>\n"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("<body class=\"doc\">"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.ends_with("</html>\n"), "{}", out.stdout);
    // The engine's own prolog is gone, but `{{textrill:head}}` carried the head.
    assert!(!out.stdout.contains("XHTML 1.0 Strict"), "{}", out.stdout);
    assert!(
        out.stdout.contains("<head><meta charset=\"utf-8\"><title>"),
        "{}",
        out.stdout
    );
}

#[test]
fn title_slot_carries_the_escaped_title() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let template = write_file(
        &dir,
        "wrap.html",
        "<h1 class=\"banner\">{{textrill:title}}</h1>\n{{textrill:content}}",
    );
    let out = run(&["--template", &template, "--title", "A & B", &input]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("<h1 class=\"banner\">A &amp; B</h1>"),
        "{}",
        out.stdout
    );
}

#[test]
fn other_engine_tokens_are_passed_through() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let template = write_file(
        &dir,
        "wrap.html",
        "{{#each sections}}{{title}}{{/each}}\n{{textrill:content}}",
    );
    let out = run(&["--template", &template, &input]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("{{#each sections}}{{title}}{{/each}}"),
        "{}",
        out.stdout
    );
}

#[test]
fn unknown_textrill_slot_is_a_hard_error() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let template = write_file(&dir, "wrap.html", "{{textrill:nope}}{{textrill:content}}");
    let out = run(&["--template", &template, &input]);
    assert_eq!(out.code, 1);
    assert!(
        out.stderr.contains("unknown template slot"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("nope"), "{}", out.stderr);
    assert!(
        out.stdout.is_empty(),
        "no output on error: {:?}",
        out.stdout
    );
}

#[test]
fn a_template_without_a_content_slot_is_refused() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let template = write_file(&dir, "wrap.html", "<div>{{textrill:toc}}</div>");
    let out = run(&["--template", &template, &input]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("content"), "{}", out.stderr);
}

#[test]
fn a_missing_template_file_is_reported() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let missing = dir.join("nope.html").to_string_lossy().into_owned();
    let out = run(&["--template", &missing, &input]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("nope.html"), "{}", out.stderr);
}

#[test]
fn the_two_template_options_are_mutually_exclusive() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let wrapper = write_file(&dir, "wrap.html", "{{textrill:content}}");
    let doc = write_file(&dir, "doc.html", "{{textrill:content}}");
    let out = run(&["--template", &wrapper, "--document_template", &doc, &input]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("cannot be combined"), "{}", out.stderr);
}

#[test]
fn templates_are_refused_with_modes_that_own_the_document() {
    let dir = tmpdir();
    let wrapper = write_file(&dir, "wrap.html", "{{textrill:content}}");
    for flag in ["--extract", "--chunk", "--stream"] {
        let out = run(&["--template", &wrapper, flag]);
        assert_eq!(out.code, 1, "{flag} should be refused");
        assert!(
            out.stderr.contains("--template is not valid"),
            "{flag}: {}",
            out.stderr
        );
    }
}

#[test]
fn document_template_is_refused_with_prepend_file() {
    let dir = tmpdir();
    let doc = write_file(&dir, "doc.html", "{{textrill:content}}");
    let other = write_file(&dir, "other", "");
    let out = run(&["--document_template", &doc, "--prepend_file", &other]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("--prepend_file"), "{}", out.stderr);
}
