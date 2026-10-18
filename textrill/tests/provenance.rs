// The generator meta is textrill's own provenance.
//!
//! The corpus harness (tests/corpus/normalize.py) canonicalises the generator
//! meta line out of every byte comparison, because the port and the Perl
//! reference cannot both name themselves in it and keeping the reference's
//! string meant every document textrill wrote claimed HTML::TextToHTML
//! produced it. That canonicalisation means the harness does not check *which*
//! generator is named, so this file is the other half of the gate: it pins the
//! exact expected value.
//!
//! Attribution and provenance are separate claims. The GPL-3.0-or-later
//! credit for HTML::TextToHTML lives in LICENSE and is deliberately not
//! asserted here -- asserting it in output would be the mistake this removed.

use std::process::Command;

use textrill::convert::Converter;
use textrill::options::Options;

fn render(opts: Options) -> String {
    let mut conv = Converter::new(opts);
    conv.opts.instring = vec!["hi".to_string()];
    conv.convert()
}

#[test]
fn the_generator_names_textrill_and_nothing_else() {
    let out = render(Options::default());
    let expected = format!(r#"content="textrill v{}""#, env!("CARGO_PKG_VERSION"));
    assert!(
        out.contains(&expected),
        "expected {expected:?} in:\n{out:?}"
    );

    // The specific failure this guards against. Asserted separately and by name so a
    // regression reads as the thing it is, not as a generic content mismatch.
    assert!(
        !out.contains("HTML::TextToHTML"),
        "output still claims the Perl module generated it:\n{out:?}"
    );
    // Not "TextToHTML" alone: that also catches a future edit that reassembles
    // the string from pieces, or spells it "Text::ToHTML".
    assert!(!out.contains("TextToHTML"), "{out:?}");
    // And not the bare version, which would survive a partial revert.
    assert!(
        !out.contains(r#"content="HTML"#) && !out.contains(r#"CONTENT="HTML"#),
        "{out:?}"
    );
}

#[test]
fn the_generator_version_tracks_the_crate_version() {
    // The point of using env!("CARGO_PKG_VERSION") rather than a literal: a
    // version bump cannot leave a stale string in every generated document.
    // This test is what makes that guarantee real -- it fails if someone
    // replaces the constant with "3.0" again, because that would not match the
    // crate version.
    let out = render(Options::default());
    let crate_version = env!("CARGO_PKG_VERSION");
    assert!(
        out.contains(&format!("textrill v{crate_version}")),
        "generator version {crate_version} does not match Cargo.toml:\n{out:?}"
    );
    assert_ne!(
        crate_version, "3.0",
        "Cargo.toml reverted to the Perl version"
    );
}

#[test]
fn the_lower_case_tag_form_also_names_textrill() {
    // The meta is emitted in two shapes depending on lower_case_tags, and both
    // carry the same constant. Pin the second one so the fix cannot be applied
    // to only the branch that the default options happen to reach.
    let opts = Options {
        lower_case_tags: true,
        ..Default::default()
    };
    let out = render(opts);
    assert!(
        out.contains(&format!(
            r#"content="textrill v{}""#,
            env!("CARGO_PKG_VERSION")
        )),
        "{out:?}"
    );
    assert!(!out.contains("HTML::TextToHTML"), "{out:?}");
}

#[test]
fn html5_mode_also_names_textrill() {
    let opts = Options {
        html5: true,
        ..Default::default()
    };
    let out = render(opts);
    assert!(
        out.contains(&format!(
            r#"content="textrill v{}""#,
            env!("CARGO_PKG_VERSION")
        )),
        "{out:?}"
    );
    assert!(!out.contains("TextToHTML"), "{out:?}");
}

#[test]
fn the_default_document_declares_its_encoding() {
    // The default doctype is HTML5, and HTML5 always declares
    // an encoding whether or not --meta_charset was asked for. The reference
    // emits no charset at all, and the differential never sees this line
    // because every corpus case pins a reference-compatible mode --
    // so this test is where the deliberate divergence is pinned, with the
    // html5test checks on the mode's own contract as the other half.
    let out = render(Options::default());
    assert_eq!(out.matches("charset").count(), 1, "{out:?}");
    assert!(out.contains(r#"<meta charset="utf-8">"#), "{out:?}");
    // In the reference-compatible modes it stays absent (exempted, not
    // deleted: the pre-flip default emitted none either).
    let legacy = Options {
        html5: false,
        ..Options::default()
    };
    let legacy_out = render(legacy);
    assert!(
        !legacy_out.contains("charset"),
        "HTML4 mode: {legacy_out:?}"
    );
}

#[test]
fn a_full_document_from_the_cli_names_textrill() {
    // The tests above drive the library directly. This one spawns the real
    // binary, following tests/cliexit.rs, so the assertion covers the path a
    // user actually takes -- and so a future refactor that writes the document
    // from a different place cannot satisfy the unit tests while the shipped
    // binary regresses.
    let dir = std::env::temp_dir().join("textrill-provenance-cli");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let infile = dir.join("in.txt");
    let outfile = dir.join("out.html");
    std::fs::write(&infile, "hi\n").expect("write input");

    let status = Command::new(env!("CARGO_BIN_EXE_textrill"))
        .args([
            "--infile",
            infile.to_str().expect("utf-8 path"),
            "--outfile",
            outfile.to_str().expect("utf-8 path"),
        ])
        .status()
        .expect("spawn textrill");
    assert!(status.success(), "exit {status:?}");

    let out = std::fs::read_to_string(&outfile).expect("read output");
    assert!(
        out.contains(&format!(
            "content=\"textrill v{}\"",
            env!("CARGO_PKG_VERSION")
        )),
        "{out:?}"
    );
    assert!(!out.contains("TextToHTML"), "{out:?}");
    // The CLI path carries the default-mode charset too, so a refactor that
    // builds the document differently cannot satisfy the library tests while
    // the shipped binary stops declaring an encoding.
    assert_eq!(out.matches("charset").count(), 1, "{out:?}");
    assert!(out.contains(r#"<meta charset="utf-8">"#), "{out:?}");

    let _ = std::fs::remove_file(&infile);
    let _ = std::fs::remove_file(&outfile);
}

#[test]
fn help_describes_textrill_and_not_the_reference() {
    // The first line a user read was "A reimplementation of txt2html
    // 3.0": the tool describing itself in someone else's voice. It now
    // describes what it does, and the reference is named only where it is
    // factually relevant -- the legacy option-file names, which are still read.
    let out = Command::new(env!("CARGO_BIN_EXE_textrill"))
        .arg("--help")
        .output()
        .expect("spawn textrill --help");
    assert!(out.status.success(), "{:?}", out.status);
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines[0].starts_with("Usage: textrill "),
        "usage line: {:?}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("Convert plain text to HTML"),
        "help line 1: {:?}",
        lines[1]
    );
    assert!(
        !lines[1].to_lowercase().contains("txt2html"),
        "help line 1 still names the reference: {:?}",
        lines[1]
    );
    assert!(
        !text.to_lowercase().contains("reimplementation"),
        "help still defines textrill against the reference:\n{text}"
    );
    // The rc epilog names the preferred files and says the legacy ones work.
    assert!(text.contains("~/.textrillrc"), "{text}");
    assert!(text.contains("legacy names ~/.txt2htmlrc"), "{text}");
}
