//! A11 — the URL scheme policy on generated `href` values.
//!
//! `<URL:javascript:…>` in ordinary prose became a live `javascript:` link in
//! the Perl reference, so the engine now decides once, in one place, which
//! schemes may reach an `href`. These tests pin the three properties that make
//! that safe to ship: the default refuses the script-bearing schemes, a refused
//! anchor keeps its text, and nothing else about the output moves.
//!
//! # Which `<URL:…>` forms are actually dangerous
//!
//! The system dictionary has **two** `<URL:…>` rules, in this order
//! (`src/links.rs`):
//!
//! ```text
//! <URL:([-\w\.\/:~_\@]+):([a-zA-Z0-9'() ]+)>  ->  <A HREF="$1">$2</A>   # :829
//! <URL:\s*(\S+?)\s*>                         ->  <A HREF="$1">$1</A>   # :832
//! ```
//!
//! The first is upstream's `<URL:foo:label>` spelling, so `<URL:javascript:alert(1)>`
//! becomes `<a href="javascript">alert(1)</a>` — the href is the bare word
//! `javascript`, a *relative* path, and harmless. It only applies when the label
//! is made of `[a-zA-Z0-9'() ]`. Put a `.`, `/` or `;` in the label and rule 829
//! no longer matches, rule 832 takes the whole thing as the href, and
//! `<URL:javascript:alert(document.domain)>` becomes a live `javascript:` link.
//!
//! So the payloads below all carry a `.` or `/`. Writing `alert(1)` here would
//! be a test that passes for the wrong reason.

use textrill::convert::Converter;
use textrill::options::Options;
use textrill::urlscheme::{scheme_of, scrub_hrefs, UrlPolicy};

fn convert(text: &str, opts: Options) -> String {
    let mut conv = Converter::new(opts);
    conv.opts.extract = true;
    conv.opts.instring = vec![text.to_string()];
    conv.convert()
}

/// Every href in `html`, so a test can assert on the set rather than substring
/// order.
fn hrefs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("href=\"") {
        let at = from + rel + "href=\"".len();
        let end = html[at..].find('"').map(|p| at + p).unwrap_or(html.len());
        out.push(html[at..end].to_string());
        from = end;
    }
    out
}

// ------------------------------------------------------------ the default --

#[test]
fn the_default_policy_refuses_a_javascript_url_tag() {
    let out = convert(
        "Click <URL:javascript:alert(document.domain)> now",
        Options::default(),
    );
    // The scheme survives as *text*, which is the point: nothing links it.
    assert!(hrefs(&out).is_empty(), "{out:?}");
    assert!(out.contains("javascript:alert(document.domain)"), "{out:?}");
}

#[test]
fn a_refused_anchor_keeps_the_words_the_reader_would_have_seen() {
    let out = convert(
        "Click <URL:javascript:alert(document.domain)> now",
        Options::default(),
    );
    // The text is the document's, and losing it would be data loss (A5). What is
    // removed is the element that would have executed it.
    assert!(
        out.contains("Click javascript:alert(document.domain) now"),
        "{out:?}"
    );
    assert!(!out.contains("<a "), "{out:?}");
    assert!(!out.contains("</a>"), "{out:?}");
}

#[test]
fn every_script_bearing_scheme_is_refused() {
    for url in [
        "javascript:alert(document.domain)",
        "JaVaScRiPt:alert(document.domain)",
        "data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==",
        "vbscript:msgbox(document.domain)",
        // Not script-bearing, but it points the reader at their own filesystem,
        // so it is refused by default too.
        "file:///etc/passwd",
    ] {
        let out = convert(&format!("x <URL:{url}> y"), Options::default());
        assert!(!out.contains("<a "), "{url} produced {out:?}");
    }
}

#[test]
fn the_label_spelling_of_url_produces_a_relative_href_and_is_left_alone() {
    // Upstream parity, recorded because it looks like a near miss: rule 829
    // splits `<URL:foo:label>`, so the href here is the word `javascript`, not
    // the scheme. A relative href is always allowed, and the scrub must leave it
    // be rather than pattern-match on the substring.
    let out = convert("x <URL:javascript:alert(1)> y", Options::default());
    assert!(out.contains("href=\"javascript\""), "{out:?}");
}

#[test]
fn ordinary_links_are_untouched() {
    let out = convert(
        "See https://example.com/a and mailto:a@b.example and ftp://h/f",
        Options::default(),
    );
    assert_eq!(
        hrefs(&out),
        vec!["https://example.com/a".to_string(), "ftp://h/f".to_string(),],
        "{out:?}"
    );
}

// ------------------------------------------------- the scheme is read right --

#[test]
fn a_scheme_split_by_a_tab_is_still_found() {
    // A browser strips TAB/LF/CR out of a URL before it looks at the scheme, so
    // this resolves to `javascript:` there. The policy has to agree. This is
    // reachable: a dictionary rule may put a capture in the middle of a scheme.
    assert_eq!(
        scheme_of("java\tscript:alert(1)").as_deref(),
        Some("javascript")
    );
    assert_eq!(scheme_of("java\nscript:x").as_deref(), Some("javascript"));
    assert_eq!(scheme_of("  javascript:x").as_deref(), Some("javascript"));
}

#[test]
fn an_escaped_ampersand_is_not_decoded_into_a_colon() {
    // The engine escapes `&` before the link pass, so a document writing `&#58;`
    // puts `&amp;#58;` in the attribute. The browser's one decode pass turns that
    // back into the literal text `&#58;`, which is neither a colon nor a valid
    // scheme character, so the browser treats the href as relative too.
    //
    // Decoding entities here would therefore be the *bug*: it would refuse a
    // link that is harmless in a browser, and break a legitimate
    // `<URL:foo&amp;bar>` label. Asserting the href survives verbatim is what
    // pins that choice.
    let out = convert("x <URL:javascript&#58;alert(1)> y", Options::default());
    assert!(
        out.contains("href=\"javascript&amp;#58;alert(1)\""),
        "{out:?}"
    );
}

// ------------------------------------------------------------- the options --

#[test]
fn an_operator_can_allow_a_scheme_again() {
    let out = convert(
        "Click <URL:javascript:alert(document.domain)> now",
        Options {
            allowed_url_schemes: Some(vec!["javascript".to_string()]),
            ..Options::default()
        },
    );
    assert!(
        out.contains("href=\"javascript:alert(document.domain)\""),
        "{out:?}"
    );
}

#[test]
fn an_operator_can_narrow_the_policy() {
    // The point of the option: make it stricter than the default.
    let out = convert(
        "a <URL:ftp://h/f> b <URL:https://e.com/> c",
        Options {
            allowed_url_schemes: Some(vec!["https".to_string()]),
            ..Options::default()
        },
    );
    assert_eq!(hrefs(&out), vec!["https://e.com/".to_string()], "{out:?}");
}

#[test]
fn style_url_with_a_refused_scheme_is_an_error_not_a_silent_drop() {
    // `--style_url` is operator input, so there is no untrusted text to preserve
    // and no reason to carry on producing a document whose stylesheet is
    // missing. Contrast with the document-derived case above, which must not
    // fail: a hostile document should not be able to deny service.
    let opts = Options {
        style_url: "javascript:alert(1)".to_string(),
        ..Options::default()
    };
    let err = opts.validate().unwrap_err();
    assert!(err.contains("style_url"), "{err}");
    assert!(err.contains("javascript"), "{err}");
}

#[test]
fn a_relative_style_url_is_fine() {
    let opts = Options {
        style_url: "style.css".to_string(),
        ..Options::default()
    };
    assert!(opts.validate().is_ok());
}

// -------------------------------------------------------------- the option --

#[test]
fn the_option_is_parsed_as_a_comma_separated_list() {
    let mut opts = Options::default();
    for spec in textrill::cli::SPECS {
        if spec.names[0] == "allowed_url_schemes" {
            textrill::cli::set_str(&mut opts, spec, "https, http ,").unwrap();
        }
    }
    // Setting it again replaces rather than accumulates: a front end writes the
    // whole value back, and accumulating would double the list every time.
    for spec in textrill::cli::SPECS {
        if spec.names[0] == "allowed_url_schemes" {
            textrill::cli::set_str(&mut opts, spec, "ftp").unwrap();
        }
    }
    let policy = opts.url_policy();
    assert_eq!(
        policy,
        UrlPolicy::Strict(vec!["ftp".to_string()]),
        "replaced, not appended"
    );
}

#[test]
fn an_unset_option_round_trips_through_a_front_end_and_stays_safe() {
    // A GUI reads the defaults with `get_value` and saves them. So "unset" has
    // to survive a save/load cycle without becoming something weaker, and both
    // candidate encodings were wrong in a different way:
    //
    // * spelling the standard list out in `Options::default` drifts from the
    //   engine's own list, so the two can disagree about what is allowed; and
    // * encoding unset as an empty *allow*list would mean "allow everything",
    //   and a script-bearing scheme would quietly start working again on the
    //   next run.
    //
    // So unset is `""` and `""` resolves to the default tier, which still
    // refuses javascript. Checked both ways round the loop, because a front end
    // only ever does these two operations.
    let text = textrill::cli::get_value(&Options::default(), "allowed_url_schemes").unwrap();
    assert_eq!(text, "", "unset has to persist as unset");

    let mut saved = Options::default();
    textrill::cli::set_value(&mut saved, "allowed_url_schemes", &text).unwrap();
    assert!(saved.url_policy().is_default());
    assert!(!saved.url_policy().allows("javascript:x"));

    // A list that is set is preserved exactly, and survives a second cycle.
    let mut narrowed = Options::default();
    textrill::cli::set_value(&mut narrowed, "allowed_url_schemes", "https,mail").unwrap();
    let back = textrill::cli::get_value(&narrowed, "allowed_url_schemes").unwrap();
    assert_eq!(back, "https,mail");
    let mut again = Options::default();
    textrill::cli::set_value(&mut again, "allowed_url_schemes", &back).unwrap();
    assert_eq!(again.url_policy(), narrowed.url_policy());
}

#[test]
fn an_unknown_custom_scheme_is_allowed_by_default() {
    // The reason the default is not an allowlist. Upstream's own CI fixture is
    // a dictionary rule `|xyz:[\w/\.:+\-]+| -> $&`, and its smoke test links
    // `xyz://example.com`. No allowlist short of "include xyz" survives that,
    // and an unknown scheme is not an executing one: a browser navigating to
    // it does nothing and runs nothing.
    let out = convert("a <URL:xyz://example.com> b", Options::default());
    assert!(out.contains("href=\"xyz://example.com\""), "{out:?}");
}

#[test]
fn a_strict_policy_refuses_an_unknown_custom_scheme() {
    // And the operator who wants the allowlist guarantee gets it, including the
    // schemes nobody had thought of when this was written.
    let out = convert(
        "a <URL:xyz://example.com> b <URL:https://e.com/> c",
        Options {
            allowed_url_schemes: Some(vec!["https".to_string()]),
            ..Options::default()
        },
    );
    assert_eq!(hrefs(&out), vec!["https://e.com/".to_string()], "{out:?}");
}

// ------------------------------------------------ the dictionary is loud --

#[test]
fn a_dictionary_rule_with_a_refused_url_is_reported_at_load() {
    let mut parser = textrill::links::LinkParser::new(false, UrlPolicy::default());
    parser.parse_dict("test", "evil -> javascript:alert(1)\n");
    assert!(
        parser
            .rejected_patterns
            .iter()
            .any(|m| m.contains("javascript")),
        "{:?}",
        parser.rejected_patterns
    );
    assert!(parser.rules.is_empty());
}

#[test]
fn a_dictionary_rule_with_an_unknown_custom_scheme_is_kept_by_default() {
    // The `xyz://` case from upstream's CI fixture, as a dictionary rule rather
    // than a `<URL:...>` tag: this is the link the default tier must not break.
    let mut parser = textrill::links::LinkParser::new(false, UrlPolicy::default());
    parser.parse_dict("test", "|xyz:[\\w/\\.:+\\-]+|      -> $&\n");
    assert!(
        parser.rejected_patterns.is_empty(),
        "{:?}",
        parser.rejected_patterns
    );
    assert_eq!(parser.rules.len(), 1);
}

#[test]
fn a_dictionary_rule_with_an_allowed_url_is_kept() {
    let mut parser = textrill::links::LinkParser::new(false, UrlPolicy::default());
    parser.parse_dict("test", "cats -> http://example.com/cats\n");
    assert!(
        parser.rejected_patterns.is_empty(),
        "{:?}",
        parser.rejected_patterns
    );
    assert_eq!(parser.rules.len(), 1);
}

// ----------------------------------------------------------- the invariant --

#[test]
fn no_disallowed_scheme_survives_any_producer() {
    // The point of doing this as a scan over finished markup rather than a check
    // at each construction site: a producer nobody thought about is still
    // covered. Here three different producers all try, and all three lose.
    let html = concat!(
        r#"<a href="javascript:a">a</a>"#,
        r#"<a href="https://e.com/">keep</a>"#,
        r#"<A HREF="data:text/html,x">b</A>"#,
        r#"<a name="anchor">no href</a>"#,
    );
    let mut dropped = Vec::new();
    let out = scrub_hrefs(html, &UrlPolicy::default(), &mut dropped).unwrap();
    assert_eq!(
        out,
        r#"a<a href="https://e.com/">keep</a>b<a name="anchor">no href</a>"#
    );
    assert_eq!(dropped, vec!["javascript".to_string(), "data".to_string()]);
}

#[test]
fn the_toc_and_pager_links_are_never_scrubbed() {
    // They are relative, which the policy always allows, so the generated
    // navigation cannot be broken by the security pass.
    //
    // A file rather than `instring`: in string mode each element is one
    // paragraph, so a setext heading shares a line with the preceding `</p>` and
    // the sectioning pass never sees a heading. Same reason as `sectiontest.rs`.
    let dir = std::env::temp_dir().join(format!("textrill-a11-toc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("in.txt");
    std::fs::write(
        &input,
        "Intro.\n\nOne\n===\n\nfirst\n\nTwo\n===\n\nsecond\n",
    )
    .unwrap();
    let mut conv = Converter::new(Options {
        section: true,
        toc: true,
        infile: vec![input.to_string_lossy().into_owned()],
        ..Options::default()
    });
    let out = conv.try_convert().expect("readable input");
    assert!(out.contains("href=\"#chunk-1\""), "{out:?}");
    assert!(out.contains("href=\"#chunk-2\""), "{out:?}");
    for h in hrefs(&out) {
        assert!(h.starts_with('#'), "{h} in {out:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------- attribute injection into a tag
//
// The scheme scrubber above checks the *value* of an href. That is not enough on
// its own: the href sits inside `href="..."`, so a `"` reaching it ends the
// attribute and everything after it is parsed as further attributes on a tag the
// engine generated. A document could then supply its own `onmouseover=`, and
// hovering the link would run it in the origin serving the converted file.
//
// The engine escapes `&`, `<` and `>`, which stops tag injection but not this:
// `"` is not escaped, because in ordinary prose it never needs to be. The four
// built-in rules that captured `\S` therefore admitted it.

/// The attribute names on every `<a …>` tag in `html`.
fn anchor_attributes(html: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("<a ") {
        let start = from + rel;
        let end = match html[start..].find('>') {
            Some(p) => start + p,
            None => break,
        };
        let tag = &html[start + 3..end];
        let mut names = Vec::new();
        let mut rest = tag;
        while let Some(eq) = rest.find('=') {
            let name = rest[..eq]
                .rsplit(|c: char| c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_string();
            names.push(name);
            rest = &rest[eq + 1..];
            // Skip the value, quoted or not.
            rest = match rest.strip_prefix('"') {
                Some(v) => match v.find('"') {
                    Some(q) => &v[q + 1..],
                    None => "",
                },
                None => match rest.find(char::is_whitespace) {
                    Some(sp) => &rest[sp..],
                    None => "",
                },
            };
        }
        out.push(names);
        from = end;
    }
    out
}

/// Payloads that try to smuggle a second attribute past the engine.
const INJECTIONS: &[&str] = &[
    "<URL:x\"onmouseover=\"alert(1)>",
    "<URL:x\" onmouseover=\"alert(1)>",
    "<URL:\"onmouseover=\"alert(1)>",
    "<http://e.com/\"onmouseover=\"alert(1)>",
    "<http://\"onmouseover=\"alert(1)>",
    "www.e.com/a\"onmouseover=\"alert(1)",
    "www.e.com/a\" target=\"_blank",
    "ftp.e.com/a\"onmouseover=\"alert(1)",
    "<URL:https://e.com/a\"onerror=\"alert(1)>",
    "<URL:x\"onfocus=\"alert(1) autofocus>",
];

#[test]
fn a_document_cannot_inject_an_attribute_into_a_generated_anchor() {
    let opts = Options {
        make_links: true,
        ..Default::default()
    };
    for payload in INJECTIONS {
        let html = convert(payload, opts.clone());
        for attrs in anchor_attributes(&html) {
            for name in &attrs {
                let n = name.to_ascii_lowercase();
                assert!(
                    !n.starts_with("on"),
                    "{payload:?} produced an event handler {name:?}:\n{html}"
                );
                assert_ne!(n, "target", "{payload:?} produced target=:\n{html}");
            }
        }
    }
}

#[test]
fn no_generated_anchor_carries_target_in_any_mode() {
    // The reason `rel="noopener noreferrer"` is deliberately absent: it is only
    // meaningful on a link that opens a new browsing context, and the engine
    // emits none. Reverse tabnabbing needs `target`; with no `target`, there is
    // no `window.opener` to hand over and nothing for `noopener` to protect.
    // A document cannot add one either -- see the test above.
    type Configure = fn(&mut Options);
    let mut sources: Vec<(String, Options)> = Vec::new();
    let base = "A {{textrill:cite:k}} and {{textrill:gloss:t}} link <URL:https://e.com/x> \
                and <http://e.com/y> and www.e.com/z\n\n\
                {{textrill:def:cite:k}}\nK.\n{{/textrill:def:cite:k}}\n\
                {{textrill:def:gloss:t}}\nT.\n{{/textrill:def:gloss:t}}\n"
        .to_string();
    let mut flags: Vec<(&str, Configure)> = vec![
        ("plain", |_| {}),
        ("make_links", |o| o.make_links = true),
        ("make_links+anchors", |o| {
            o.make_links = true;
            o.make_anchors = true;
        }),
        ("toc", |o| {
            o.make_links = true;
            o.toc = true;
        }),
        ("section+toc", |o| {
            o.make_links = true;
            o.section = true;
            o.toc = true;
        }),
        ("notes", |o| {
            o.citations = true;
            o.glossary = true;
        }),
        ("html5", |o| {
            o.citations = true;
            o.glossary = true;
            o.html5 = true;
        }),
    ];
    flags.push(("everything", move |o: &mut Options| {
        o.make_links = true;
        o.citations = true;
        o.glossary = true;
        o.toc = true;
        o.section = true;
        o.make_anchors = true;
        o.number_headings = true;
    }));
    for (name, f) in flags {
        let mut o = Options::default();
        f(&mut o);
        sources.push((format!("{name}: {base}"), o));
    }
    for (label, o) in sources {
        let html = convert(&label, o);
        for attrs in anchor_attributes(&html) {
            for name in &attrs {
                assert_ne!(
                    name.to_ascii_lowercase(),
                    "target",
                    "{label}: a target= appeared:\n{html}"
                );
            }
        }
    }
}
