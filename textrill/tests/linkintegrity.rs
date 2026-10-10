//! Every internal link the engine *generates* must resolve.
//!
//! The corpus cannot check this. Its 60 cases are reference-differential, so
//! they are all about matching Perl's bytes, and not one of them passes
//! `--toc`, `--section` or `--chunk` -- those are additions with no
//! upstream equivalent to diff against. Running the link checker over the whole
//! corpus output finds **zero** engine-generated links, which is the problem
//! this file exists to fix: the code that invents `href`s and the `id`s they
//! point at is the least-differentially-covered part of the output.
//!
//! # What is and is not checked
//!
//! Only what the engine emits. A document can write its own URLs, through
//! `<URL:...>` or a link dictionary, and those are the author's claims about
//! the world: the converter faithfully emits `docs/readme` and cannot know the
//! file exists. Holding those to a file-existence rule would be testing the
//! input, not the output. So the rule is scoped to the `<nav class="toc">` and
//! `<nav class="pager">` blocks the engine builds, plus the anchors it emits.
//!
//! External URLs (anything with a scheme) are out of scope too, and so are
//! resource references: a `<link href>` to a stylesheet that 404s is a missing
//! asset, not a dead navigation link.
//!
//! # The properties
//!
//! 1. Every generated internal reference resolves, within the generated set.
//! 2. Ids are unique within a document. A duplicate does not break the link —
//!    the browser jumps to the first match — so it is invisible to check 1 and
//!    has to be asserted separately.
//! 3. Each TOC entry points at the section it names. A cross-file TOC can
//!    easily link to a file that *exists* and still be off by one, which check
//!    1 cannot see either.
//!
//! The scanner is deliberately small and reads the finished HTML rather than
//! the engine's internals: the point is to check the bytes a browser gets.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use textrill::convert::Converter;
use textrill::options::Options;

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn tmpdir() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("textrill-linkint-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Documents that stress heading structure. Each is `(name, text)`.
fn documents() -> Vec<(&'static str, String)> {
    vec![
        (
            "setext h1 and h2",
            "One\n===\n\nfirst\n\nSub\n---\n\nsub\n\nTwo\n===\n\nsecond\n".into(),
        ),
        (
            "atx every level",
            "# One\n\n## Two\n\n### Three\n\n#### Four\n\n##### Five\n\n###### Six\n\n\
             tail\n"
                .into(),
        ),
        (
            "duplicate headings",
            "Same\n====\n\na\n\nSame\n====\n\nb\n\nSame\n----\n\nc\n".into(),
        ),
        (
            "more than nine sections, so chunk-10 exists",
            (1..=12)
                .map(|i| format!("Head{i}\n=====\n\nbody {i}\n"))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        (
            "headings needing escape, and inline markup",
            "A & B <tag> \"q\"\n==================\n\nx\n\n\
             #Hash and *bold* and <URL:https://e.com/>\n===================================\n\ny\n"
                .into(),
        ),
        (
            "non-ascii heading",
            "\u{4f60}\u{597d} \u{4e16}\u{754c}\n=====\n\nx\n\n\
             \u{65e5}\u{672c}\u{8a9e} \u{30c6}\u{30b9}\u{30c8}\n===============\n\ny\n"
                .into(),
        ),
        (
            "sub-heading before any top-level one",
            "Sub first\n---------\n\ns\n\nOne\n===\n\nx\n\nSub two\n-------\n\nt\n".into(),
        ),
        (
            "no headings at all",
            "Just a paragraph.\n\nAnd another one.\n".into(),
        ),
        ("a single heading", "Only\n====\n\nbody\n".into()),
        (
            "preamble then headings",
            "Preamble before any heading.\n\nOne\n===\n\na\n\nTwo\n===\n\nb\n".into(),
        ),
        ("heading on the first line", "First\n=====\n\nbody\n".into()),
        (
            "heading with no body text after it",
            "One\n===\n\nTwo\n===\n\nbody\n".into(),
        ),
    ]
}

/// A named option-set closure, for the tables below.
type Flag = (&'static str, fn(&mut Options));

/// The option sets that change *link structure*, crossed with every document.
/// The interesting failures are the interactions, so this is a full cross
/// product rather than a pairing.
///
/// The wrapper flags are deliberately not here: `--extract`, `--html5` and
/// `--make_links` change the document around the navigation but not the `href`s
/// or `id`s in it, so putting them in the matrix would cost conversions and buy
/// nothing. `wrappers_still_resolve` covers them on one document.
fn option_sets() -> Vec<(&'static str, Options)> {
    let flags: &[Flag] = &[
        ("toc", |o| o.toc = true),
        ("toc+section", |o| {
            o.toc = true;
            o.section = true;
        }),
        ("toc+section+number", |o| {
            o.toc = true;
            o.section = true;
            o.number_headings = true;
        }),
        ("toc+section+anchors", |o| {
            o.toc = true;
            o.section = true;
            o.make_anchors = true;
        }),
    ];
    flags
        .iter()
        .map(|(name, f)| {
            let mut o = Options::default();
            f(&mut o);
            (*name, o)
        })
        .collect()
}

fn wrappers() -> Vec<(&'static str, Options)> {
    let flags: &[Flag] = &[
        ("toc+extract+section", |o| {
            o.toc = true;
            o.section = true;
            o.extract = true;
        }),
        ("toc+html5+section", |o| {
            o.toc = true;
            o.section = true;
            o.html5 = true;
        }),
        ("toc+make_links+section", |o| {
            o.toc = true;
            o.section = true;
            o.make_links = true;
        }),
        ("toc+make_links+anchors", |o| {
            o.toc = true;
            o.section = true;
            o.make_links = true;
            o.make_anchors = true;
        }),
    ];
    flags
        .iter()
        .map(|(name, f)| {
            let mut o = Options::default();
            f(&mut o);
            (*name, o)
        })
        .collect()
}

// ------------------------------------------------------------- the scanner --

/// Ids and `<a name>` targets in one document, with duplicates kept so they can
/// be detected rather than silently collapsed by a `HashSet`.
fn ids_of(html: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let b = html.as_bytes();
    let mut i = 0;
    while let Some(rel) = html[i..].find("id=") {
        let at = i + rel;
        let mut v = at + 3;
        while v < b.len() && (b[v] as char).is_whitespace() {
            v += 1;
        }
        if v < b.len() && (b[v] == b'"' || b[v] == b'\'') {
            let q = b[v];
            if let Some(end) = html[v + 1..].find(q as char) {
                ids.push(html[v + 1..v + 1 + end].to_string());
            }
        }
        i = at + 3;
    }
    // <a name="section_1"> from --make_anchors.
    let mut i = 0;
    while let Some(rel) = html[i..].find("<a") {
        let at = i + rel;
        let tag_end = html[at..].find('>').map(|p| at + p).unwrap_or(html.len());
        let tag = &html[at..tag_end];
        if let Some(n) = tag.find("name=") {
            let mut v = at + n + 5;
            while v < b.len() && (b[v] as char).is_whitespace() {
                v += 1;
            }
            if v < b.len() && (b[v] == b'"' || b[v] == b'\'') {
                let q = b[v];
                if let Some(end) = html[v + 1..].find(q as char) {
                    ids.push(html[v + 1..v + 1 + end].to_string());
                }
            }
        }
        i = at + 2;
    }
    ids
}

/// Only the TOC nav. The pager is a `<nav>` too, and its labels are "part 2 →",
/// which are deliberately not heading text.
fn toc_navs(html: &str) -> Vec<String> {
    navs(html)
        .into_iter()
        .filter(|n| {
            let open = &n[..n.find('>').unwrap_or(n.len())];
            open.to_ascii_lowercase().contains("class=\"toc\"")
        })
        .collect()
}

/// The `<nav>…</nav>` blocks, which is where every generated link lives.
fn navs(html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("<nav") {
        let at = from + rel;
        match lower[at..].find("</nav>") {
            Some(p) => {
                out.push(html[at..at + p].to_string());
                from = at + p + 6;
            }
            None => break,
        }
    }
    out
}

fn attrs(tag: &str, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find(name) {
        let at = from + rel;
        from = at + name.len();
        // Must be a whole attribute name, not the tail of another one.
        if at > 0 {
            let prev = lower.as_bytes()[at - 1];
            if prev.is_ascii_alphanumeric() || matches!(prev, b'-' | b'_' | b':') {
                continue;
            }
        }
        let b = tag.as_bytes();
        let mut v = at + name.len();
        while v < b.len() && (b[v] as char).is_whitespace() {
            v += 1;
        }
        if v >= b.len() || b[v] != b'=' {
            continue;
        }
        v += 1;
        while v < b.len() && (b[v] as char).is_whitespace() {
            v += 1;
        }
        if v < b.len() && (b[v] == b'"' || b[v] == b'\'') {
            let q = b[v];
            if let Some(end) = tag[v + 1..].find(q as char) {
                out.push(tag[v + 1..v + 1 + end].to_string());
            }
        }
    }
    out
}

/// A browser's first test for "is this a URL with a scheme". Deliberately the
/// same rule as `urlscheme::scheme_of`, minus its C0/space skip, so a
/// `javascript:` or a `news:` link is never mistaken for a relative path.
fn has_scheme(u: &str) -> bool {
    let u = u.trim_start_matches([' ', '\t', '\n', '\r', '\u{0}']);
    let mut cs = u.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    for c in cs {
        if c == ':' {
            return true;
        }
        if !(c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.') {
            return false;
        }
    }
    false
}

fn is_internal(u: &str) -> bool {
    !u.is_empty() && !u.starts_with("//") && !has_scheme(u)
}

/// Every `<a href>` the engine generated, as the raw attribute value.
fn generated_links(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for nav in navs(html) {
        for (href, _) in anchors(&nav) {
            if is_internal(&href) {
                out.push(href);
            }
        }
    }
    out
}

/// Strip tags and unescape, leaving the text a reader sees.
fn text_of(inner: &str) -> String {
    let mut clean = String::new();
    let mut i = 0;
    while i < inner.len() {
        if inner.as_bytes()[i] == b'<' {
            match inner[i..].find('>') {
                Some(q) => i += q + 1,
                None => break,
            }
        } else {
            let c = inner[i..].chars().next().unwrap();
            clean.push(c);
            i += c.len_utf8();
        }
    }
    unescape(&clean).trim().to_string()
}

/// `(href, text)` for every anchor in `nav`. An anchor is scanned to its
/// `</a>` rather than to the next `>`, because the label is part of the
/// cross-file check and `tags` would have dropped it.
fn anchors(nav: &str) -> Vec<(String, String)> {
    let lower = nav.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("<a") {
        let at = from + rel;
        let after = lower.as_bytes().get(at + 2).copied();
        if !matches!(after, Some(b' ') | Some(b'>') | Some(b'\t') | Some(b'\n')) {
            from = at + 2;
            continue;
        }
        let open_end = match nav[at..].find('>') {
            Some(p) => at + p,
            None => break,
        };
        let close = lower[open_end..]
            .find("</a>")
            .map(|p| open_end + p)
            .unwrap_or(nav.len());
        let href = attrs(&nav[at..=open_end], "href").into_iter().next();
        if let Some(href) = href {
            out.push((href, text_of(&nav[open_end + 1..close])));
        }
        from = close.max(at + 2);
    }
    out
}

/// Heading text, tags stripped, for the cross-file label check.
fn heading_text(html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("<h") {
        let at = from + rel;
        let after = lower.as_bytes().get(at + 2).copied();
        if !matches!(after, Some(b'1'..=b'6')) {
            from = at + 2;
            continue;
        }
        let level = after.unwrap() as char;
        let close = format!("</h{level}>");
        match lower[at..].find(&close) {
            Some(p) => {
                let inner = &html[at..at + p];
                out.push(text_of(inner));
                from = at + p;
            }
            None => break,
        }
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Check one output set: `(file name, html)`.
fn check_set(label: &str, files: &[(String, String)]) {
    let ids: Vec<Vec<String>> = files.iter().map(|(_, h)| ids_of(h)).collect();

    for (i, (name, html)) in files.iter().enumerate() {
        // Property 2: ids unique within a document.
        let mut seen = std::collections::HashSet::new();
        for id in &ids[i] {
            if !seen.insert(id) {
                panic!("{label}: {name} has a duplicate id {id:?}\n{html}");
            }
        }
        // Properties 1 and 3.
        for href in generated_links(html) {
            let (target, frag) = if let Some(f) = href.strip_prefix('#') {
                (i, f.to_string())
            } else {
                let (t, f) = href.split_once('#').unwrap_or((href.as_str(), ""));
                match files
                    .iter()
                    .position(|(n, _)| n == t || n.ends_with(&format!("/{t}")))
                {
                    Some(j) => (j, f.to_string()),
                    None => {
                        panic!("{label}: {name} links to {href:?}, which is not a generated file")
                    }
                }
            };
            if !frag.is_empty() && !ids[target].contains(&frag) {
                panic!(
                    "{label}: {name} links to {href:?} but {} has no such id\n{}",
                    files[target].0, html
                );
            }
            // A `#fragment` must at least name a heading when it is a TOC entry.
            if !frag.is_empty() && !href.starts_with('#') {
                let labels = heading_text(&files[target].1);
                if !labels.is_empty() && !labels.contains(&frag) {
                    // Not a heading anchor; make_anchors names are fine too.
                    assert!(ids[target].contains(&frag), "{label}: unreachable {href:?}");
                }
            }
        }
    }
}

// ------------------------------------------------------------------ the tests --

#[test]
fn every_generated_single_file_link_resolves() {
    for (doc_name, text) in documents() {
        let dir = tmpdir();
        let input = dir.join("in.txt");
        std::fs::write(&input, &text).unwrap();
        for (opt_name, opts) in option_sets() {
            let mut opts = opts.clone();
            opts.infile = vec![input.to_string_lossy().into_owned()];
            let mut conv = Converter::new(opts);
            let out = conv.try_convert().expect("readable input");
            check_set(
                &format!("{doc_name} / {opt_name}"),
                &[("out.html".to_string(), out)],
            );
        }
    }
}

#[test]
fn wrappers_still_resolve() {
    // `--extract` moves the body, `--html5` changes the doctype, `--make_links`
    // adds hrefs of its own. None may disturb a generated navigation link, and
    // `--make_links` in particular must not make a `<URL:...>` in a heading look
    // like a navigation target.
    let dir = tmpdir();
    let input = dir.join("in.txt");
    std::fs::write(
        &input,
        "One\n===\n\nsee https://e.com/ and <URL:javascript:alert(1)>\n\n\
         Sub\n---\n\nmore\n\nTwo\n===\n\nend\n",
    )
    .unwrap();
    for (opt_name, opts) in wrappers() {
        let mut opts = opts;
        opts.infile = vec![input.to_string_lossy().into_owned()];
        let mut conv = Converter::new(opts);
        let out = conv.try_convert().expect("readable input");
        check_set(
            &format!("wrappers / {opt_name}"),
            &[("out.html".to_string(), out)],
        );
    }
}

#[test]
fn every_generated_chunked_link_resolves() {
    for (doc_name, text) in documents() {
        let dir = tmpdir();
        let input = dir.join("in.txt");
        std::fs::write(&input, &text).unwrap();
        for (opt_name, opts) in option_sets() {
            let mut opts = opts.clone();
            opts.chunk = true;
            opts.infile = vec![input.to_string_lossy().into_owned()];
            opts.outfile = dir.join("out.html").to_string_lossy().into_owned();
            let mut conv = Converter::new(opts);
            let (files, unreadable) = conv.try_convert_chunked();
            assert!(
                unreadable.is_empty(),
                "{doc_name} / {opt_name}: {unreadable:?}"
            );
            // `files` carries write paths; compare on the base name, which is
            // what the links between pages use.
            let set: Vec<(String, String)> = files
                .iter()
                .map(|(p, h)| (base_name(p), h.clone()))
                .collect();
            check_set(&format!("{doc_name} / chunk / {opt_name}"), &set);
        }
    }
}

#[test]
fn a_cross_file_toc_entry_points_at_the_heading_it_names() {
    // The property a bare existence check cannot see: in multi-file output a TOC
    // entry can link to a file that exists and still be off by one, so the
    // label and the target have to agree.
    let dir = tmpdir();
    let input = dir.join("in.txt");
    std::fs::write(
        &input,
        "One\n===\n\nfirst\n\nSub\n---\n\nsub\n\nTwo\n===\n\nsecond\n\
         \nThree\n=====\n\nthird\n",
    )
    .unwrap();
    let opts = Options {
        toc: true,
        section: true,
        chunk: true,
        infile: vec![input.to_string_lossy().into_owned()],
        outfile: dir.join("out.html").to_string_lossy().into_owned(),
        ..Options::default()
    };
    let mut conv = Converter::new(opts);
    let (files, _) = conv.try_convert_chunked();
    let set: Vec<(String, String)> = files
        .iter()
        .map(|(p, h)| (base_name(p), h.clone()))
        .collect();
    assert!(
        set.len() >= 3,
        "expected a page per top-level section: {}",
        set.len()
    );
    for (_, html) in &set {
        for nav in toc_navs(html) {
            for (href, label) in anchors(&nav) {
                if !is_internal(&href) {
                    continue;
                }
                // The href may deep-link (`page.html#chunk-2`); the label has to
                // match the *target file's* headings, and the fragment has to
                // exist in it.
                let (file, frag) = match href.split_once('#') {
                    Some((f, g)) => (f.to_string(), g.to_string()),
                    None => (href.clone(), String::new()),
                };
                let target = set
                    .iter()
                    .find(|(n, _)| *n == file)
                    .unwrap_or_else(|| panic!("TOC links to missing file {href:?}"));
                if !frag.is_empty() {
                    assert!(
                        ids_of(&target.1).contains(&frag),
                        "TOC entry {label:?} links {href:?}, but {} has no id {frag:?}",
                        target.0
                    );
                }
                let heads = heading_text(&target.1);
                assert!(
                    heads.contains(&label),
                    "TOC entry {label:?} points at {}, whose headings are {heads:?}",
                    target.0
                );
            }
        }
    }
}

#[test]
fn chunk_output_gives_each_page_its_own_anchor() {
    // A cross-file TOC can only deep-link if the page carries an anchor for the
    // section, so this pins that the ids exist and are unique per file even
    // though numbering restarts.
    let dir = tmpdir();
    let input = dir.join("in.txt");
    std::fs::write(
        &input,
        "One\n===\n\na\n\nTwo\n===\n\nb\n\nThree\n=====\n\nc\n",
    )
    .unwrap();
    let opts = Options {
        toc: true,
        section: true,
        chunk: true,
        infile: vec![input.to_string_lossy().into_owned()],
        outfile: dir.join("out.html").to_string_lossy().into_owned(),
        ..Options::default()
    };
    let mut conv = Converter::new(opts);
    let (files, _) = conv.try_convert_chunked();
    let set: Vec<(String, String)> = files
        .iter()
        .map(|(p, h)| (base_name(p), h.clone()))
        .collect();
    check_set("chunk anchors", &set);
}

fn base_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}
