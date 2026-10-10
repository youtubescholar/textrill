//! `--body_template` and `--document_template`.
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
    let wrapped = run(&["--body_template", &template, &input]);
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
    let out = run(&["--body_template", &template, &input]);
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
    let out = run(&["--body_template", &template, "--toc", &input]);
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
    let out = run(&["--body_template", &template, "--title", "A & B", &input]);
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
    let out = run(&["--body_template", &template, &input]);
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
    let out = run(&["--body_template", &template, &input]);
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
    let out = run(&["--body_template", &template, &input]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("content"), "{}", out.stderr);
}

#[test]
fn a_missing_template_file_is_reported() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let missing = dir.join("nope.html").to_string_lossy().into_owned();
    let out = run(&["--body_template", &missing, &input]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("nope.html"), "{}", out.stderr);
}

#[test]
fn the_two_template_options_are_mutually_exclusive() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let wrapper = write_file(&dir, "wrap.html", "{{textrill:content}}");
    let doc = write_file(&dir, "doc.html", "{{textrill:content}}");
    let out = run(&[
        "--body_template",
        &wrapper,
        "--document_template",
        &doc,
        &input,
    ]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("cannot be combined"), "{}", out.stderr);
}

#[test]
fn templates_are_refused_with_modes_that_own_the_document() {
    let dir = tmpdir();
    let wrapper = write_file(&dir, "wrap.html", "{{textrill:content}}");
    for flag in ["--extract", "--chunk", "--stream"] {
        let out = run(&["--body_template", &wrapper, flag]);
        assert_eq!(out.code, 1, "{flag} should be refused");
        assert!(
            out.stderr.contains("--body_template is not valid"),
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

/// 2.3: the legacy `--template` spelling still works (an rc file may use it),
/// but it is deprecated and says so, naming the pair so the trap is visible.
#[test]
fn the_legacy_template_alias_still_works_and_warns() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let template = write_file(&dir, "wrap.html", "{{textrill:content}}");
    let out = run(&["--template", &template, &input]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("<h1>"),
        "alias must still convert, got: {}",
        out.stdout
    );
    assert!(
        out.stderr.contains("--template"),
        "deprecation must name the alias: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("body_template"),
        "deprecation must name the replacement: {}",
        out.stderr
    );
}

// ---- `--var name=value` -> `{{textrill:var:name}}` ----

/// A declared var is substituted verbatim, in both template models, and the
/// engine's own slots survive around it (nothing is squished by the insertion).
#[test]
fn var_slots_substitute_in_both_template_models() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let vars = [
        "--var",
        "title=A &amp; B",
        "--var",
        "year=2026",
        "--var",
        "by=A & One",
    ];

    let wrap = write_file(
        &dir,
        "wrap.html",
        "<main class=\"page\">\n<h1 class=\"t\">{{textrill:var:title}}</h1>\n{{textrill:content}}<footer>\
         {{textrill:var:by}}, {{textrill:var:year}}</footer>\n</main>",
    );
    let mut args: Vec<&str> = vec!["--body_template", &wrap];
    args.extend(vars);
    args.push(&input);
    let out = run(&args);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("<h1 class=\"t\">A &amp; B</h1>"),
        "value must be inserted verbatim, not re-escaped: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("<footer>A & One, 2026</footer>"),
        "multiple vars, in template order: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("<body>"),
        "engine prolog intact around a var frame:",
    );

    let doc = write_file(
        &dir,
        "doc.html",
        "<!DOCTYPE html>\n<html>\n<head><title>{{textrill:var:title}}</title></head>\n\
         <body>{{textrill:content}}</body>\n</html>",
    );
    let out = run(&["--document_template", &doc]
        .into_iter()
        .chain(vars)
        .collect::<Vec<_>>());
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("<title>A &amp; B</title>"),
        "document template var: {}",
        out.stdout
    );
    assert_eq!(
        out.stdout.lines().next().unwrap(),
        "<!DOCTYPE html>",
        "the document template is the whole page, var or not"
    );
}

/// A var value that is (or contains) a block element is inserted literally,
/// newlines and all: the engine does not reflow or re-indent it.
#[test]
fn var_value_is_inserted_literally_not_squished() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let block = "<aside class=\"note\">\n  <p>first line</p>\n  <p>second line</p>\n</aside>";
    let template = write_file(
        &dir,
        "wrap.html",
        "{{textrill:var:extra}}{{textrill:content}}",
    );
    let out = run(&[
        "--body_template",
        &template,
        "--var",
        &format!("extra={block}"),
        &input,
    ]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let expect = format!("{block}<p>Intro paragraph.");
    assert!(
        out.stdout.contains(&expect),
        "the block must survive byte for byte, directly before the content: {}",
        out.stdout
    );
}

/// A value that itself looks like a slot token is not reinterpreted: the
/// substitution is one pass, so a var cannot smuggle the engine's own slots in
/// (no failure cascade), and a later real slot still substitutes.
#[test]
fn var_value_is_never_reinterpreted() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let template = write_file(
        &dir,
        "wrap.html",
        "<p>{{textrill:var:payload}}</p>{{textrill:content}}",
    );
    let out = run(&[
        "--body_template",
        &template,
        "--var",
        "payload={{textrill:toc}}",
        &input,
    ]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("<p>{{textrill:toc}}</p>"),
        "the inserted token must stay literal, not become a TOC: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("<h1>"),
        "the real content slot after it must still substitute: {}",
        out.stdout
    );
}

/// An undeclared `{{textrill:var:name}}` is the same class of defect as an
/// unknown slot: a hard error before any output, naming what is missing.
#[test]
fn undeclared_var_slot_is_a_hard_error() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);

    let template = write_file(
        &dir,
        "wrap.html",
        "{{textrill:var:nope}}{{textrill:content}}",
    );
    let out = run(&["--body_template", &template, &input]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("nope"), "{}", out.stderr);
    assert!(out.stderr.contains("no --var"), "{}", out.stderr);
    assert!(out.stdout.is_empty(), "no half-built page: {}", out.stdout);

    let template = write_file(
        &dir,
        "wrap2.html",
        "{{textrill:var:nope}}{{textrill:content}}",
    );
    let out = run(&["--body_template", &template, "--var", "author=x", &input]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("nope"), "{}", out.stderr);
    assert!(
        out.stderr.contains("author"),
        "the message must list what *was* declared: {}",
        out.stderr
    );
}

/// The unknown-slot guard still stands for the wide family: `var` and `varx`
/// are not slots and stay hard errors even when vars are declared.
#[test]
fn the_unknown_slot_guard_is_extended_not_loosened() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    for token in ["var", "varx", "vary"] {
        let template = write_file(
            &dir,
            &format!("wrap-{token}.html"),
            &format!("{{{{textrill:{token}}}}}{{{{textrill:content}}}}"),
        );
        let out = run(&["--body_template", &template, "--var", "author=x", &input]);
        assert_eq!(out.code, 1, "{}: {token} must be refused", out.stderr);
        assert!(
            out.stderr.contains("unknown template slot"),
            "{token}: {}",
            out.stderr
        );
    }
}

/// `--var` is parsed eagerly: a missing `=` or an invalid name is a CLI error,
/// not something discovered when a template later fails to load.
#[test]
fn malformed_var_arguments_are_rejected_up_front() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let template = write_file(&dir, "wrap.html", "{{textrill:content}}");

    let out = run(&["--body_template", &template, "--var", "author", &input]);
    assert_eq!(out.code, 1, "missing `=` must be refused");
    assert!(
        out.stderr.contains("name=value"),
        "missing `=`: {}",
        out.stderr
    );

    let out = run(&["--body_template", &template, "--var", "a b=x", &input]);
    assert_eq!(out.code, 1, "name with a space must be refused");
    assert!(out.stderr.contains("invalid name"), "{}", out.stderr);

    let out = run(&["--body_template", &template, "--var", "=x", &input]);
    assert_eq!(out.code, 1, "empty name must be refused");
    assert!(out.stderr.contains("non-empty"), "{}", out.stderr);
}

// ---- the shipped template library ----

/// Every shipped template converts a document out of the box. The four
/// whole-document templates own the page and start with a doctype; `bare`
/// wraps only the body and is byte-identical to no template at all. Nothing
/// leaves a literal slot in the output.
#[test]
fn every_shipped_template_converts_a_document() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let plain = run(&[&input]);

    let document = [
        ("article", "id=\"article-body\""),
        ("book", "id=\"book-body\""),
        ("manpage", "id=\"manpage-body\""),
        ("slide", "id=\"deck-body\""),
    ];
    for (name, marker) in document {
        let out = run(&["--template_library", name, &input]);
        assert_eq!(out.code, 0, "{name}: stderr: {}", out.stderr);
        assert!(
            out.stdout.starts_with("<!DOCTYPE html>"),
            "{name}: a document template must own the page: {}",
            out.stdout
        );
        assert!(
            !out.stdout.contains("{{textrill:"),
            "{name}: a slot must not survive as literal text: {}",
            out.stdout
        );
        assert!(
            out.stdout
                .contains("<h1><a name=\"section_1\">One</a></h1>"),
            "{name}: the converted body must be inside the frame: {}",
            out.stdout
        );
        assert!(
            out.stdout.contains(marker),
            "{name}: the template's own structure must be present: {}",
            out.stdout
        );
    }

    let out = run(&["--template_library", "bare", &input]);
    assert_eq!(out.code, 0, "bare: stderr: {}", out.stderr);
    assert_eq!(
        out.stdout, plain.stdout,
        "bare must wrap the body with no change: byte-identical to no template"
    );
}

/// A name that is not in the library is a hard error that names the library,
/// so a typo reads as a message, not as an unexpectly bare conversion.
#[test]
fn an_unknown_shipped_template_is_a_hard_error() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let out = run(&["--template_library", "nope", &input]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("nope"), "{}", out.stderr);
    assert!(out.stderr.contains("article"), "{}", out.stderr);
    assert!(out.stdout.is_empty(), "no half-built page: {}", out.stdout);
}

/// `--template_library` is the third member of a mutually exclusive family:
/// combined with the file templates (the legacy `--template` alias included)
/// it is refused up front.
#[test]
fn the_library_mutually_excludes_the_file_templates() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let file = write_file(&dir, "t.html", "{{textrill:content}}");
    for other in ["--body_template", "--document_template", "--template"] {
        let out = run(&["--template_library", "article", other, &file, &input]);
        assert_eq!(out.code, 1, "{other} must be refused beside the library");
        assert!(
            out.stderr.contains("cannot be combined"),
            "{other}: {}",
            out.stderr
        );
    }
}

/// The refusals follow the library template's model: a whole-document
/// template is refused with the modes that own the surrounding document
/// (`--extract`, `--chunk`, `--stream`) and with `--prepend_file`; the body
/// wrapper `bare` is refused with the first three but composes with
/// `--prepend_file` like any body template.
#[test]
fn the_library_refusals_follow_the_model() {
    let dir = tmpdir();
    let input = write_file(&dir, "in.txt", SAMPLE);
    let other = write_file(&dir, "other", "");

    for mode in ["--extract", "--chunk", "--stream"] {
        let out = run(&["--template_library", "article", mode, &input]);
        assert_eq!(out.code, 1, "{mode} must be refused for article");
    }
    let out = run(&[
        "--template_library",
        "article",
        "--prepend_file",
        &other,
        &input,
    ]);
    assert_eq!(
        out.code, 1,
        "--prepend_file must be refused with a whole-document template"
    );
    assert!(
        out.stderr.contains("not valid with --prepend_file"),
        "{}",
        out.stderr
    );

    for mode in ["--extract", "--chunk", "--stream"] {
        let out = run(&["--template_library", "bare", mode, &input]);
        assert_eq!(out.code, 1, "{mode} must be refused for bare");
    }
    let out = run(&[
        "--template_library",
        "bare",
        "--prepend_file",
        &other,
        &input,
    ]);
    assert_eq!(
        out.code, 0,
        "a body template composes with --prepend_file: stderr: {}",
        out.stderr
    );
}
