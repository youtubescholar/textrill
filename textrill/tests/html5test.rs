//! P5.1 — the opt-in HTML5 output mode.
//!
//! The contract is byte-identical output from the reference by default, so this
//! mode must be invisible until it is asked for. The first test pins that; the
//! rest pin what asking for it changes: the short doctype, an `<html>` element
//! with no XHTML namespace, and a charset declaration in the head.

use textrill::cli;
use textrill::convert::Converter;
use textrill::options::Options;

fn render(opts: Options) -> String {
    let mut conv = Converter::new(opts);
    conv.opts.instring = vec!["hi".to_string()];
    conv.convert()
}

#[test]
fn html5_is_off_by_default() {
    let conv = Converter::new(Options::default());
    assert!(!conv.opts.html5);
    let out = render(Options::default());
    // The XHTML prolog stays; nothing HTML5 leaks into the default output.
    assert!(out.contains("<!DOCTYPE html PUBLIC"), "{out:?}");
    assert!(!out.contains("<html>\n"), "{out:?}");
    assert!(!out.contains("<!DOCTYPE html>\n"), "{out:?}");
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
    // The default carries xhtml, which turns lower_case_tags on, so the bare
    // flag already gives idiomatic lower case. Asking for upper case explicitly
    // is still valid HTML5 and must not be overridden.
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
