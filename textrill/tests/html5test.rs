//! The HTML5 output mode, which is the default.
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

fn render_text(text: &str, opts: Options) -> String {
    let mut conv = Converter::new(opts);
    conv.opts.instring = vec![text.to_string()];
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
    // Exempted, not deleted: the pre-flip default was the
    // reference's XHTML 1.0 Strict, and these are the assertions it made.
    // One --xhtml (the flag the corpus pins) brings it back, and it
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
    assert!(
        out.contains("<!DOCTYPE html>\n<HTML LANG=\"en\">"),
        "{out:?}"
    );
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

fn render_table(opts: Options) -> String {
    let mut opts = opts;
    opts.make_tables = true;
    opts.extract = true;
    let mut conv = Converter::new(opts);
    conv.opts.instring =
        vec!["  Fruit   Vegetable\n Banana   Carrot\n  Apple   Celery\n".to_string()];
    conv.convert()
}

#[test]
fn html5_table_alignment_is_a_class_not_the_obsolete_align_attribute() {
    // `align` on `<td>`/`<th>` is obsolete and non-conforming in HTML5, so the
    // default mode emits the documented `align-right` hook instead. The corpus
    // pins `--xhtml` before each case's own flags, so this default-mode path is
    // otherwise untested -- the blind spot that hid the `align=` in the first
    // place.
    let out = render_table(Options::default());
    assert!(out.contains("<td class=\"align-right\">"), "{out}");
    assert!(
        !out.contains("align="),
        "no obsolete align attribute: {out}"
    );
    assert!(
        !out.contains("style="),
        "no inline CSS in HTML5 mode: {out}"
    );
}

#[test]
fn xhtml_table_alignment_keeps_the_reference_inline_style() {
    let mut opts = Options::default();
    cli::set_value(&mut opts, "xhtml", "1").expect("xhtml");
    let out = render_table(opts);
    assert!(out.contains("<td style=\"text-align: right;\">"), "{out}");
    assert!(!out.contains("class=\"align-right\""), "{out}");
}

#[test]
fn the_legacy_html4_mode_keeps_the_align_attribute() {
    // `--no-html5` is the reference's HTML 4.01 mode, where the attribute is
    // deprecated but still emitted for parity; only HTML5 avoids it.
    let mut opts = Options::default();
    cli::set_value(&mut opts, "html5", "0").expect("no-html5");
    let out = render_table(opts);
    assert!(out.contains("<TD ALIGN=\"RIGHT\">"), "{out}");
}

#[test]
fn the_html5_default_closes_the_optional_end_tags() {
    // HTML allows `</p>` and `</li>` to be omitted; the default emits them
    // anyway, so the output is well-formed (and DOM-equivalent to the terse
    // reference bytes). This is the one deliberate serialisation departure in
    // the default mode; `--no-close_optional_tags` gets the terse shape back.
    let out = render_text("Hi there.\n\n- one\n- two\n", Options::default());
    assert!(out.contains("<p>Hi there.\n</p>"), "{out:?}");
    assert!(out.contains("</li>"), "{out:?}");
    assert!(!out.contains("<li>one\n  <li>two"), "{out:?}");
}

#[test]
fn no_close_optional_tags_restores_the_terse_shape() {
    let mut opts = Options::default();
    cli::set_value(&mut opts, "close_optional_tags", "0").expect("off");
    let out = render_text("Hi there.\n\n- one\n- two\n", opts);
    assert!(!out.contains("</p>"), "{out:?}");
    assert!(!out.contains("</li>"), "{out:?}");
    assert!(out.contains("<li>one\n  <li>two"), "{out:?}");
}

#[test]
fn the_reference_html4_mode_stays_terse() {
    // --no-html5 is the reference's HTML 4.01: optional end tags are omitted
    // for byte parity regardless of --close_optional_tags.
    let mut opts = Options::default();
    cli::set_value(&mut opts, "html5", "0").expect("no-html5");
    opts.close_optional_tags = true;
    let out = render_text("Hi there.\n\n- one\n- two\n", opts);
    assert!(!out.contains("</p>"), "{out:?}");
    assert!(!out.contains("</li>"), "{out:?}");
}

#[test]
fn close_optional_tags_round_trips_through_the_cli() {
    let mut opts = Options::default();
    assert_eq!(
        cli::get_value(&opts, "close_optional_tags").unwrap(),
        "true",
        "on by default"
    );
    cli::set_value(&mut opts, "close_optional_tags", "0").expect("off");
    assert!(!opts.close_optional_tags);
    cli::set_value(&mut opts, "close_optional_tags", "1").expect("on");
    assert!(opts.close_optional_tags);
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

#[test]
fn html5_declares_the_default_language_on_the_root_element() {
    // The declared language rides on <html> where a screen reader and a
    // validator can find it; "en" is the default dominant language.
    let out = render(Options::default());
    assert!(
        out.starts_with("<!DOCTYPE html>\n<html lang=\"en\">"),
        "{out:?}"
    );
}

#[test]
fn html5_declares_custom_language_tags() {
    let out = render(Options {
        html5: true,
        lang: "zh-Hans".to_string(),
        ..Options::default()
    });
    assert!(
        out.starts_with("<!DOCTYPE html>\n<html lang=\"zh-Hans\">"),
        "{out:?}"
    );
}

#[test]
fn upper_case_tags_upper_case_the_lang_attribute() {
    // LANG follows the tag case the document is serialised in, like any
    // other attribute.
    let out = render(Options {
        html5: true,
        xhtml: false,
        lower_case_tags: false,
        lang: "de".to_string(),
        ..Options::default()
    });
    assert!(
        out.starts_with("<!DOCTYPE html>\n<HTML LANG=\"de\">"),
        "{out:?}"
    );
}

#[test]
fn the_parity_modes_keep_the_bare_html_element() {
    // --xhtml and --no-html5 reproduce the reference's byte-for-byte
    // skeleton (the corpus pins them), so `lang` is themselves' only in
    // HTML5 mode; an empty --lang suppresses it even there.
    let mut xhtml = Options::default();
    cli::set_value(&mut xhtml, "xhtml", "1").expect("xhtml");
    let out = render(xhtml);
    assert!(
        out.contains("<html xmlns=\"http://www.w3.org/1999/xhtml\">"),
        "{out:?}"
    );
    assert!(!out.contains("lang="), "{out:?}");

    let mut legacy = Options::default();
    cli::set_value(&mut legacy, "html5", "0").expect("no-html5");
    let out = render(legacy);
    assert!(!out.contains("lang="), "{out:?}");

    let out = render(Options {
        html5: true,
        lang: String::new(),
        ..Options::default()
    });
    assert!(!out.contains("lang="), "{out:?}");
    assert!(out.contains("<html>\n"), "{out:?}");
}

/// A titled document in HTML5 mode opens its body with the header landmark the
/// GUI's Title field is wired to: `--title` becomes an `<h1>` at the top of
/// the default (untemplated) body.
#[test]
fn a_titled_document_leads_its_body_with_a_header() {
    let out = render(Options {
        html5: true,
        title: "Chapter One".to_string(),
        ..Options::default()
    });
    // Straight after the `<body>` the paragrapher emits, before any prose.
    assert!(
        out.contains(
            "<body>\n<header class=\"document-header\">\n<h1>Chapter One</h1>\n</header>\n"
        ),
        "{out:?}"
    );
    assert!(
        out.contains("<title>Chapter One</title>"),
        "the title element is untouched: {out:?}"
    );
}

/// An untitled document emits no header, so a heading is never left empty.
#[test]
fn an_untitled_document_emits_no_header() {
    let out = render(Options::default());
    assert!(!out.contains("document-header"), "{out:?}");
    assert!(!out.contains("<h1>"), "{out:?}");
}

/// The title goes into the heading text escaped exactly as it went into the
/// `<title>` element, so a title that could break out of its element cannot.
#[test]
fn the_header_escapes_the_title() {
    let out = render(Options {
        html5: true,
        title: "</header><script>alert(1)</script>".to_string(),
        ..Options::default()
    });
    assert!(
        out.contains("<h1>&lt;/header&gt;&lt;script&gt;alert(1)&lt;/script&gt;</h1>"),
        "{out:?}"
    );
    assert!(!out.contains("<script>"), "{out:?}");
}

/// The parity modes keep the reference's byte-for-byte output, so the header
/// landmark is HTML5-mode-only: `--xhtml` and `--no-html5` titled documents
/// emit no `<h1>`, exactly as the corpus pins them.
#[test]
fn the_parity_modes_keep_no_header() {
    let mut xhtml = Options::default();
    cli::set_value(&mut xhtml, "xhtml", "1").expect("xhtml");
    xhtml.title = "Chapter One".to_string();
    let out = render(xhtml);
    assert!(!out.contains("document-header"), "{out:?}");

    let mut legacy = Options::default();
    cli::set_value(&mut legacy, "html5", "0").expect("no-html5");
    legacy.title = "Chapter One".to_string();
    let out = render(legacy);
    assert!(!out.contains("document-header"), "{out:?}");
}

/// The header landmark opens the TOC too: the title comes before the contents.
#[test]
fn the_header_precedes_a_generated_toc() {
    // A heading must end its paragraph to be seen by the sectioning pass, so
    // this goes through a real file (as `sectiontest` explains for itself).
    let dir = std::env::temp_dir().join(format!(
        "textrill-h5-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let input = dir.join("in.txt");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&input, "One\n===\n\nfirst\n").unwrap();
    let mut opts = Options {
        html5: true,
        toc: true,
        title: "Chapter One".to_string(),
        ..Options::default()
    };
    opts.infile = vec![input.to_string_lossy().into_owned()];
    let out = Converter::new(opts).try_convert().expect("readable input");
    let _ = std::fs::remove_dir_all(&dir);

    let header_at = out
        .find("document-header")
        .expect("header should be present");
    let toc_at = out
        .find("<nav class=\"toc\"")
        .expect("toc should be present");
    assert!(header_at < toc_at, "{out:?}");
}
