// P1.1 — the generator meta is textrill's own provenance.
//!
//! The corpus harness (tests/corpus/normalize.py) canonicalises the generator
//! meta line out of every byte comparison, because the port and the Perl
//! reference cannot both name themselves in it and keeping the reference's
//! string meant every document textrill wrote claimed HTML::TextToHTML
//! produced it. That canonicalisation means the harness does not check *which*
//! generator is named, so this file is the other half of the gate: it pins the
//! exact expected value.
//!
//! Attribution and provenance are separate claims. The GPLv3 credit for
//! HTML::TextToHTML lives in LICENSE and is deliberately not asserted here --
//! asserting it in output would be the mistake P1.1 removed.

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

    // The specific failure P1.1 fixed. Asserted separately and by name so a
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

    let _ = std::fs::remove_file(&infile);
    let _ = std::fs::remove_file(&outfile);
}
