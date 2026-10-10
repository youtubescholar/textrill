//! Opt-in sectioning, TOC and multi-file chunking.
//!
//! All three are off by default, so the first test pins that the default output
//! is untouched. The rest pin what the flags add: sequential `chunk-N` ids,
//! a generated TOC, and one file per top-level section with a pager.
//!
//! These go through a real input file rather than `--instring`: in string mode
//! each element is one paragraph, so a heading can share a line with the
//! preceding `</p>` and never looks like a heading to the post-pass.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use textrill::cli;
use textrill::convert::Converter;
use textrill::options::Options;
use textrill::section;

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn sample() -> String {
    "Intro.\n\nOne\n===\n\nfirst\n\nSub\n---\n\nsub\n\nTwo\n===\n\nsecond\n".to_string()
}

/// Convert `text` through a throwaway file and return the document.
fn render(text: &str, opts: Options) -> String {
    let dir = tmpdir();
    let input = dir.join("in.txt");
    std::fs::write(&input, text).unwrap();
    let mut opts = opts;
    opts.infile = vec![input.to_string_lossy().into_owned()];
    let mut conv = Converter::new(opts);
    conv.try_convert().expect("readable input")
}

fn tmpdir() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("textrill-p52-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn section_toc_and_chunk_are_off_by_default() {
    let out = render(&sample(), Options::default());
    assert!(!out.contains("<article"), "{out:?}");
    assert!(!out.contains("<nav class=\"toc\""), "{out:?}");
    assert!(!out.contains("chunk-"), "{out:?}");
}

#[test]
fn section_wraps_each_heading_with_sequential_ids() {
    let out = render(
        &sample(),
        Options {
            section: true,
            ..Options::default()
        },
    );
    assert!(
        out.contains("<article class=\"section\" id=\"chunk-1\">"),
        "{out:?}"
    );
    assert!(
        out.contains("<article class=\"section\" id=\"chunk-2\">"),
        "{out:?}"
    );
    assert!(
        out.contains("<article class=\"section\" id=\"chunk-3\">"),
        "{out:?}"
    );
    assert!(!out.contains("<nav class=\"toc\""), "{out:?}");
}

#[test]
fn toc_implies_sectioning_and_links_every_section() {
    let out = render(
        &sample(),
        Options {
            toc: true,
            ..Options::default()
        },
    );
    assert!(out.contains("<nav class=\"toc\" id=\"toc\">"), "{out:?}");
    assert!(out.contains("<a href=\"#chunk-1\">One</a>"), "{out:?}");
    assert!(out.contains("<a href=\"#chunk-2\">Sub</a>"), "{out:?}");
    assert!(out.contains("<a href=\"#chunk-3\">Two</a>"), "{out:?}");
}

#[test]
fn a_headingless_document_is_unchanged_by_toc() {
    let text = "just a paragraph\n";
    let plain = render(text, Options::default());
    let with_toc = render(
        text,
        Options {
            toc: true,
            ..Options::default()
        },
    );
    assert_eq!(plain, with_toc);
}

#[test]
fn section_and_chunk_round_trip_through_the_cli() {
    for name in ["section", "toc", "chunk", "number_headings", "stream"] {
        let mut opts = Options::default();
        assert_eq!(cli::get_value(&opts, name).unwrap(), "false");
        cli::set_value(&mut opts, name, "1").unwrap();
        assert_eq!(cli::get_value(&opts, name).unwrap(), "true");
        cli::set_value(&mut opts, name, "no-1").unwrap();
        assert_eq!(cli::get_value(&opts, name).unwrap(), "false");
    }
}

#[test]
fn chunk_splits_at_top_level_sections() {
    let dir = tmpdir();
    let input = dir.join("book.txt");
    std::fs::write(&input, sample()).unwrap();

    let opts = Options {
        infile: vec![input.to_string_lossy().into_owned()],
        outfile: dir.join("out.html").to_string_lossy().into_owned(),
        chunk: true,
        toc: true,
        ..Options::default()
    };

    let mut conv = Converter::new(opts);
    let (files, unreadable) = conv.try_convert_chunked();
    assert!(unreadable.is_empty());
    assert_eq!(files.len(), 2, "one file per top-level heading");
    assert!(files[0].0.ends_with("out-chunk-01.html"), "{}", files[0].0);
    assert!(files[1].0.ends_with("out-chunk-02.html"), "{}", files[1].0);
    // The TOC cross-links files; the pager moves between them. Sibling links
    // are bare names, not the absolute write path.
    assert!(
        files[0].1.contains("href=\"out-chunk-02.html\""),
        "{}",
        files[0].1
    );
    assert!(files[1].1.contains("rel=\"prev\""), "{}", files[1].1);
    // A subsection stays on its parent's page.
    assert!(files[0].1.contains("Sub"), "{}", files[0].1);
    assert!(!files[1].1.contains("Sub"), "{}", files[1].1);
}

#[test]
fn number_headings_prefixes_hierarchically() {
    let out = render(
        &sample(),
        Options {
            number_headings: true,
            ..Options::default()
        },
    );
    assert!(
        out.contains("<h1>1 <a name=\"section_1\">One</a></h1>"),
        "{out:?}"
    );
    assert!(
        out.contains("<h2>1.1 <a name=\"section_1_1\">Sub</a></h2>"),
        "{out:?}"
    );
    assert!(
        out.contains("<h1>2 <a name=\"section_2\">Two</a></h1>"),
        "{out:?}"
    );
    // Numbering alone does not section.
    assert!(!out.contains("<article"), "{out:?}");
}

#[test]
fn numbered_toc_labels_carry_the_numbers() {
    let out = render(
        &sample(),
        Options {
            number_headings: true,
            toc: true,
            ..Options::default()
        },
    );
    assert!(out.contains("<a href=\"#chunk-1\">1 One</a>"), "{out:?}");
    assert!(out.contains("<a href=\"#chunk-2\">1.1 Sub</a>"), "{out:?}");
    assert!(out.contains("<a href=\"#chunk-3\">2 Two</a>"), "{out:?}");
}

#[test]
fn split_sections_ignores_indented_heading_lookalikes() {
    let body = "<pre>\n  <h1>not a heading</h1>\n</pre>\n<h1>real</h1>\n";
    let (pre, secs) = section::split_sections(body);
    assert!(pre.contains("not a heading"));
    assert_eq!(secs.len(), 1);
    assert_eq!(secs[0].label, "real");
}
