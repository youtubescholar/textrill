use textrill::convert::Converter;
use textrill::links;
use textrill::options::Options;

#[test]
fn literal_system_links_match() {
    let mut opts = Options::default();
    opts.deal_with_options(); // xhtml default => lower_case_tags
    let mut p = links::load_links(&opts);
    assert!(!p.rules.is_empty(), "system dict rules should load");
    let txt2html_rules: Vec<String> = p
        .rules
        .iter()
        .filter(|r| r.label == "txt2html")
        .map(|r| r.pattern.clone())
        .collect();
    eprintln!("txt2html rules: {txt2html_rules:?}");
    assert!(!txt2html_rules.is_empty(), "rule for label txt2html exists");

    let mut s = "A test of txt2html.".to_string();
    p.check_dictionary_links(&mut s);
    eprintln!("result: {s}");
    assert!(
        s.contains("href"),
        "expected the txt2html literal to become a link; got: {s}"
    );
}

fn convert_string(text: &str, extract: bool) -> String {
    let mut opts = Options {
        extract,
        instring: vec![text.to_string()],
        ..Options::default()
    };
    opts.deal_with_options();
    let mut conv = Converter::new(opts);
    conv.convert()
}

/// A once-only link rule must see the text that precedes it.
///
/// Perl builds `$line_with_links` from the text *before* the match and passes
/// that to `in_link_context` before substituting.  If the guard is handed an
/// empty string it cannot see the `<a ...>` opened by an earlier rule, so a
/// dictionary word inside a URL gets linked a second time, producing nested
/// anchors — which is invalid HTML and differs from the reference output.
#[test]
fn no_nested_anchors_when_a_url_contains_a_dictionary_word() {
    let html = convert_string(
        "Visit https://github.com/resurrecting-open-source-projects/txt2html now.\n",
        true,
    );

    assert!(
        !html.contains("<a href=\"https://github.com/resurrecting-open-source-projects/<a "),
        "nested anchor generated: {html}"
    );
    let opens = html.matches("<a ").count();
    let closes = html.matches("</a>").count();
    assert_eq!(opens, closes, "unbalanced anchors in: {html}");
    assert_eq!(opens, 1, "expected exactly one link in: {html}");
    assert!(
        html.contains("<a href=\"https://github.com/resurrecting-open-source-projects/txt2html\">"),
        "the whole URL should be the link, got: {html}"
    );
}

/// The once-only guard must also protect against a *following* open tag, and
/// the same rule must not fire twice.
#[test]
fn once_only_rule_leaves_surrounding_markup_alone() {
    let html = convert_string("txt2html and again txt2html\n", true);

    // The system dictionary marks the literal as a once-only link, so the
    // second mention must stay plain text.
    let opens = html.matches("<a ").count();
    assert_eq!(opens, 1, "once-only rule fired twice: {html}");
    assert_eq!(
        html.matches("</a>").count(),
        1,
        "unbalanced anchors in: {html}"
    );
}

/// Upstream TODO 5 asks for a more thread-safe converter, because the Perl
/// original keeps its state in a hash shared across calls.  Each `Converter`
/// here owns its own state and there is no global mutable storage, so several
/// conversions can run at once and must all agree.
#[test]
fn conversions_are_independent_across_threads() {
    let texts = [
        "A test of txt2html and a URL https://example.com/txt2html here.\n",
        "Chapter\n=======\n\n1. one\n2. two\n\n- a\n- b\n",
        "Some text with a table:\n\n-e  File exists.\n-z  Zero size.\n",
    ];
    let expected: Vec<String> = texts.iter().map(|t| convert_string(t, true)).collect();

    let handles: Vec<_> = (0..24)
        .map(|i| {
            let (text, want) = (
                texts[i % texts.len()].to_string(),
                expected[i % texts.len()].clone(),
            );
            std::thread::spawn(move || {
                for _ in 0..5 {
                    let got = convert_string(&text, true);
                    assert_eq!(got, want, "concurrent conversion differed");
                }
            })
        })
        .collect();
    for h in handles {
        h.join().expect("conversion thread panicked");
    }
}

/// The 26 MB lorem-markdownum corpus from the separate C `txt2html` project is
/// a useful scale check: ~500k non-blank lines of headings, ordered and
/// unordered lists.  It must finish without quadratic blowup.
///
/// The fixture is the 26 MB `test2.txt` from the tracked `txt2html-master.zip`,
/// extracted by `make ref-large` into `ref/txt2html-master/`. It is not tracked
/// itself -- 26 MB of lorem ipsum does not belong in git -- and this test skips
/// when it is absent, which is the only reason the path is derived rather than
/// assumed. A skip here is a test that did not run, so it says so on stderr
/// rather than passing in silence.
#[test]
fn large_heading_list_document_is_not_quadratic() {
    // CARGO_MANIFEST_DIR is .../textrill, so the checkout root is one up.
    // Overridable, because a 26 MB fixture is the kind of thing a CI job may
    // want to place elsewhere.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate dir has a parent");
    let path = match std::env::var("LARGE_FIXTURE") {
        Ok(p) => std::path::PathBuf::from(p),
        Err(_) => root.join("ref").join("txt2html-master").join("test2.txt"),
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!(
            "SKIP large_heading_list_document_is_not_quadratic: no fixture at {} \
             (run `make ref-large`, or set LARGE_FIXTURE)",
            path.display()
        );
        return;
    };
    // Keep the slice small: these tests run in the unoptimized debug build.
    let Some(text) = text.get(..2_000_000) else {
        return;
    };
    let opts = Options::default();
    let half = text.len() / 2;
    let (Some(a), Some(b)) = (text.get(..half), text.get(half..)) else {
        return;
    };

    let run = |text: &str| {
        let mut o = opts.clone();
        o.instring = vec![text.to_string()];
        o.deal_with_options();
        let mut c = Converter::new(o);
        let t0 = std::time::Instant::now();
        let out = c.convert();
        (out, t0.elapsed().as_secs_f64())
    };

    let (full, one) = run(a);
    let (_, two) = run(b);

    assert!(!full.is_empty());
    // Two halves of equal size should take roughly equal time.  A generous
    // bound still fails loudly if some scan becomes quadratic.
    let (slow, fast) = if one > two { (one, two) } else { (two, one) };
    assert!(
        slow < fast * 3.0,
        "half-document times {one:.2}s / {two:.2}s look superlinear"
    );
}

// ---------------------------------------------------------------- P5

/// The criterion that drives the empty-match guard.
///
/// `|Perl\b|` is an alternation with two empty branches, so it matches at
/// every position without consuming anything. That is the inherited hang: the
/// substitution loop reassigns the paragraph to an unchanged value and spins.
/// The Perl original hangs identically, so the differential corpus cannot
/// catch it and this has to be asserted directly.
#[test]
fn empty_matching_patterns_are_detected() {
    assert!(links::can_match_empty("|Perl\\b|", false), "|x| is empty");
    assert!(links::can_match_empty("x|", false), "trailing |");
    assert!(links::can_match_empty("|x", false), "leading |");
    assert!(links::can_match_empty("(?:)", false), "empty group");
    assert!(links::can_match_empty("", false), "empty pattern");
}

/// Patterns that do *not* match empty must not be caught by the guard.
///
/// This is the regression that matters: `\b` is rewritten by
/// `translate_pattern` into a zero-width lookaround alternation, so `*` and `?`
/// globs look empty-capable in the source text but are not. Rejecting those
/// would silently drop links the reference produces.
#[test]
fn ordinary_patterns_are_not_mistaken_for_empty_matches() {
    assert!(!links::can_match_empty("\\bPerl\\b", false));
    assert!(!links::can_match_empty("http:[\\w/.:+@-]+", false));
    assert!(!links::can_match_empty("Perl", false));
    // the globs: `\b.*\b` and `\b.\b` require a character despite the lookarounds
    assert!(!links::can_match_empty(&links::glob2regexp("*"), false));
    assert!(!links::can_match_empty(&links::glob2regexp("?"), false));
    assert!(!links::can_match_empty("alt\\.[\\w.\\-]+", false));
}

/// None of the 52 built-in system-dictionary rules may be rejected.
///
/// If the guard ever starts firing on the shipped dictionary, the tool loses
/// URL detection, so this asserts the real load path rather than a hand-picked
/// sample.
#[test]
fn the_system_dictionary_has_no_empty_matching_rules() {
    let opts = Options::default();
    let parser = links::load_links(&opts);
    assert!(
        parser.rules.len() >= 50,
        "expected the system dictionary to load, got {} rules",
        parser.rules.len()
    );
    let offenders: Vec<&str> = parser
        .rules
        .iter()
        .filter(|r| r.regex.is_match("").unwrap_or(false))
        .map(|r| r.label.as_str())
        .collect();
    assert!(
        offenders.is_empty(),
        "built-in rules matching empty would be dropped: {offenders:?}"
    );
}

/// A `/|.../` entry is rejected with a diagnostic naming the pattern, and the
/// rest of the dictionary still works.
///
/// The rejection is what stops the hang, so this asserts both the message and
/// that the tool still produces output rather than hanging or dying.
#[test]
fn a_hanging_dictionary_entry_is_reported_and_skipped() {
    let opts = Options::default();
    let parser = links::load_links_from_text(
        &opts,
        "/|Perl\\b| -> http://example.invalid/perl\n\
         \"|real\\\\b| -> http://example.invalid/ok\n",
    );
    assert_eq!(
        parser.rejected_patterns.len(),
        1,
        "exactly the empty-matching entry should be rejected: {:?}",
        parser.rejected_patterns
    );
    let msg = &parser.rejected_patterns[0];
    assert!(
        msg.contains("|Perl"),
        "the diagnostic should name the offending pattern, got: {msg}"
    );
    assert!(
        msg.contains("empty"),
        "the diagnostic should explain why, got: {msg}"
    );
    // the good rule survives and still links
    assert!(
        parser.rules.iter().any(|r| r.label.contains("real")),
        "the valid entry in the same dictionary must still be loaded"
    );
}

/// `-o` and `-s` are deliberately NOT guarded, because they do not hang.
///
/// Both substitute at most once per paragraph or per section, so an
/// empty-matching pattern terminates there, and the Perl original accepts it.
/// Guarding them would be a Tier 1 byte-parity divergence for no benefit.
/// Measured against the reference: `/|x/ -o-> url` emits the same empty
/// anchor in both.
#[test]
fn once_only_rules_still_accept_empty_matching_patterns() {
    for sw in ["-o", "-s"] {
        let dict = format!("/|Perl\\b/ {sw}-> http://example.invalid/perl\n");
        let opts = Options::default();
        let parser = links::load_links_from_text(&opts, &dict);
        assert!(
            parser.rejected_patterns.is_empty(),
            "{sw} substitutes once and cannot hang, so {sw} must not be rejected: {:?}",
            parser.rejected_patterns
        );
    }
}
/// The pipe-delimited form is the documented workaround and must keep working.
#[test]
fn the_pipe_delimited_form_still_links() {
    let mut parser = links::LinkParser::new(false);
    let dict = "|Perl\\b| -> http://example.invalid/perl\n";
    let filtered = parser.filter_dict(dict);
    parser.parse_dict("test", &filtered);
    assert!(
        parser.rejected_patterns.is_empty(),
        "|...| is a valid dictionary form and must not be rejected: {:?}",
        parser.rejected_patterns
    );

    let mut s = "Perl is a language".to_string();
    parser.check_dictionary_links(&mut s);
    // `LinkParser::new(false)` means upper-case tags, which is what upstream
    // emits unless --xhtml is given.
    assert!(
        s.contains("HREF=\"http://example.invalid/perl\"") && s.contains(">Perl<"),
        "expected Perl to become a link, got: {s}"
    );
}
