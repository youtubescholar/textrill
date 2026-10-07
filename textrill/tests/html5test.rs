//! P5.1 — the HTML5 output mode, which is the default (PLAN Phase 3).
//!
//! The reference's XHTML 1.0 Strict default is wrong for a new tool in 2026,
//! so the port emits HTML5 unless told otherwise; this suite pins both
//! directions: what the default does (short doctype, no namespace, one charset
//! meta, lower-case tags) and that the reference-compatible modes are still
//! reachable through `--no-html5` and `--xhtml` — the modes the corpus pins
//! case by case.

use textrill::cli;
use textrill::convert::Converter;
use textrill::options::Options;

fn render(opts: Options) -> String {
    let mut conv = Converter::new(opts);
    conv.opts.instring = vec!["hi".to_string()];
    conv.convert()
}

#[test]
fn html5_is_on_by_default() {
    let conv = Converter::new(Options::default());
    assert!(conv.opts.html5);
    let out = render(Options::default());
    // Short doctype, no namespace, exactly one charset meta, lower case.
    assert!(out.starts_with("<!DOCTYPE html>\n<html"), "{out:?}");
    assert!(!out.contains("xmlns"), "{out:?}");
    assert!(!out.contains("XHTML"), "{out:?}");
    assert_eq!(out.matches("charset").count(), 1, "{out:?}");
    assert!(out.contains("<meta charset=\"utf-8\">"), "{out:?}");
}

#[test]
fn the_reference_default_is_still_one_flag_away() {
    // Exempted, not deleted (PLAN Phase 3): the pre-flip default was the
    // reference's XHTML 1.0 Strict, and these are the assertions it made.
    // One --xhtml (the flag the corpus and fuzz pin) brings it back, and it
    // must take HTML5 mode with it rather than composing with it.
    let mut opts = Options::default();
    cli::set_value(&mut opts, "xhtml", "1").expect("xhtml");
    assert!(opts.xhtml);
    assert!(!opts.html5, "the two doctypes are mutually exclusive");
    let out = render(opts);
    assert!(out.contains("<!DOCTYPE html PUBLIC"), "{out:?}");
    assert!(!out.contains("<html>\n"), "{out:?}");
    assert!(!out.contains("<!DOCTYPE html>\n"), "{out:?}");
    assert!(
        out.contains("xmlns=\"http://www.w3.org/1999/xhtml\""),
        "{out:?}"
    );
    assert!(
        !out.contains("charset"),
        "no charset outside HTML5 mode: {out:?}"
    );
}

#[test]
fn html5_emits_the_short_doctype_and_no_namespace() {
    let out = render(Options {
        html5: true,
        ..Options::default()
    });
    assert!(out.starts_with("<!DOCTYPE html>\n<html"), "{out:?}");
    assert!(!out.contains("xmlns"), "{out:?}");
    assert!(!out.contains("XHTML"), "{out:?}");
}

#[test]
fn html5_forces_a_charset_even_with_meta_charset_off() {
    // An HTML5 document with no declared encoding is the thing the mode exists
    // to fix, so the charset does not depend on --meta_charset.
    let out = render(Options {
        html5: true,
        ..Options::default()
    });
    assert_eq!(out.matches("charset").count(), 1, "{out:?}");
    assert!(out.contains("charset=\"utf-8\""), "{out:?}");
}

#[test]
fn html5_and_meta_charset_do_not_emit_two_charsets() {
    let out = render(Options {
        html5: true,
        meta_charset: true,
        ..Options::default()
    });
    assert_eq!(out.matches("charset").count(), 1, "{out:?}");
}

#[test]
fn html5_respects_tag_case() {
    // The default carries lower_case_tags (it is the modern lower-case mode),
    // but HTML5 does not force it the way XHTML does: asking for upper case
    // explicitly is still valid HTML5 and must not be overridden.
    let out = render(Options {
        html5: true,
        xhtml: false,
        lower_case_tags: false,
        ..Options::default()
    });
    assert!(out.contains("<!DOCTYPE html>\n<HTML>"), "{out:?}");
    assert!(out.contains("<META CHARSET=\"utf-8\">"), "{out:?}");
}

#[test]
fn html5_is_settable_through_the_cli() {
    let mut opts = Options::default();
    cli::set_value(&mut opts, "html5", "1").expect("set html5");
    assert!(opts.html5);
    assert_eq!(cli::get_value(&opts, "html5").unwrap(), "true");
    cli::set_value(&mut opts, "html5", "no-1").expect("unset html5");
    assert!(!opts.html5);
}

#[test]
fn html5_still_honours_extract() {
    // --extract is the "body only" switch; the prolog is skipped regardless of
    // mode, so html5 must not resurrect the doctype there.
    let out = render(Options {
        html5: true,
        extract: true,
        ..Options::default()
    });
    assert!(!out.contains("DOCTYPE"), "{out:?}");
    assert!(!out.contains("<html"), "{out:?}");
    assert!(out.contains("<p>hi"), "{out:?}");
}
