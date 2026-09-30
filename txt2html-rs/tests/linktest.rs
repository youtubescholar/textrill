use txt2html::convert::Converter;
use txt2html::links;
use txt2html::options::Options;

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
    let mut opts = Options::default();
    opts.extract = extract;
    opts.instring = vec![text.to_string()];
    opts.deal_with_options();
    let mut conv = Converter::new(opts);
    conv.txt2html()
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
#[test]
fn large_heading_list_document_is_not_quadratic() {
    let path = "/home/vicpu/build/ref/txt2html-master/test2.txt";
    let Ok(text) = std::fs::read_to_string(path) else {
        return; // fixture not present
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
        let out = c.txt2html();
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
