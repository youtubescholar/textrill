//! `--report`: what the conversion recovered, on standard error.
//!
//! The flag is off by default, writes nothing to the output, and must agree
//! with what someone counting the produced file would get — the same numbers
//! `make examples` prints. So the tests either recount the output with a
//! deliberately different implementation and compare, or pin the numbers that
//! the example goldens and `make examples` already hold (the inference counts
//! plus the byte size of `examples/homer.txt`).

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

/// A document that exercises every count: a heading, an indented block
/// (`<pre>`, which a naive `<p` prefix would miscount as a paragraph), a
/// capitalised line and a short line (both `<br>` and `<strong>`).
const SAMPLE: &str = "ALL CAPS ON THIS LINE\n\nIntro paragraph.\n\nSection One\n===========\n\nfirst *ital* and #bold# here\n\n     preformatted block\n     longer line\n\ntiny line\n\nTail paragraph.\n";

const HOMER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/homer.txt");

/// The five keys of the report line, in the order `make examples` prints them.
const KEYS: [&str; 5] = ["bytes", "headings", "paragraphs", "strong", "br"];

#[derive(Debug, PartialEq, Eq)]
struct Counts {
    bytes: usize,
    headings: usize,
    paragraphs: usize,
    strong: usize,
    br: usize,
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(args: &[&str]) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_textrill"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn textrill");
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn run_with_stdin(args: &[&str], input: &[u8]) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_textrill"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn textrill");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input)
        .expect("write stdin");
    let out = child.wait_with_output().unwrap();
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn tmpdir() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("textrill-p50-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The one report line, as `key=value` pairs in the documented order.
fn report_of(r: &Run) -> Counts {
    let mut lines = r
        .stderr
        .lines()
        .filter(|l| l.starts_with("textrill: report "));
    let line = lines
        .next()
        .unwrap_or_else(|| panic!("no report line on stderr: {:?}", r.stderr));
    assert!(
        lines.next().is_none(),
        "more than one report line: {:?}",
        r.stderr
    );
    let rest = &line["textrill: report ".len()..];
    let parts: Vec<&str> = rest.split_whitespace().collect();
    assert_eq!(
        parts.len(),
        5,
        "report must name exactly the five counts, got {rest:?}"
    );
    let mut c = Counts {
        bytes: 0,
        headings: 0,
        paragraphs: 0,
        strong: 0,
        br: 0,
    };
    let vals = [
        &mut c.bytes,
        &mut c.headings,
        &mut c.paragraphs,
        &mut c.strong,
        &mut c.br,
    ];
    for (i, part) in parts.iter().enumerate() {
        let (k, v) = part
            .split_once('=')
            .unwrap_or_else(|| panic!("not key=value: {part:?}"));
        assert_eq!(
            k, KEYS[i],
            "the keys must be in the order make examples prints them"
        );
        *vals[i] = v
            .parse()
            .unwrap_or_else(|_| panic!("unparseable count {v:?}"));
    }
    c
}

/// What `grep` would get, written as a second implementation on purpose so that
/// a counter which agrees with itself is not the whole test. Lowercases first
/// and uses `matches` for the prefix counts, where the implementation scans
/// bytes and reads the tag name; two code paths that share nothing.
fn recount(html: &str) -> Counts {
    let lower = html.to_ascii_lowercase();
    let strong = lower.matches("<strong").count();
    let br = lower.matches("<br").count();
    let mut headings = 0;
    let mut paragraphs = 0;
    for (i, _) in lower.match_indices('<') {
        let mut rest = lower[i + 1..].chars();
        match rest.next() {
            // `<h1>` .. `<h6>`, and `<hr>` is not one.
            Some('h') if matches!(rest.next(), Some('1'..='6')) => headings += 1,
            // `<pre>` must not count: it is not a paragraph, it is a block.
            Some('p') => match rest.next() {
                Some('>') => paragraphs += 1,
                Some(c) if c.is_whitespace() => paragraphs += 1,
                _ => {}
            },
            _ => {}
        }
    }
    Counts {
        bytes: html.len(),
        headings,
        paragraphs,
        strong,
        br,
    }
}

/// The report is off by default: stderr carries nothing about counts, and a run
/// without the flag must be indistinguishable from one before the option
/// existed.
#[test]
fn the_report_is_off_by_default() {
    let r = run(&["--infile", HOMER]);
    assert_eq!(r.code, 0);
    assert!(
        !r.stderr.contains("report"),
        "a bare run wrote a report: {:?}",
        r.stderr
    );
    assert!(r.stdout.contains("<p>"), "sanity: the document converted");
}

/// With the flag, the counts describe the produced output, not the engine's
/// belief about it. `SAMPLE` carries a `<pre>` block, a heading, a
/// capitalised line and a short line, so a counter with the wrong rule for any
/// one of them disagrees here.
#[test]
fn the_counts_are_what_is_in_the_output() {
    let stdout = run_with_stdin(&["--instring", SAMPLE, "--report"], SAMPLE.as_bytes());
    assert_eq!(stdout.code, 0, "stderr: {}", stdout.stderr);
    assert_eq!(
        report_of(&stdout),
        recount(&stdout.stdout),
        "the report disagreed with a count of the output"
    );
}

/// The case the rule exists for, isolated: an indented block is `<pre>`, not a
/// paragraph, so counting `<p` as a prefix overcounts by one per block.
#[test]
fn an_indented_block_is_not_a_paragraph() {
    let src = "Text before.\n\n     pre\n     pre\n\nText after.\n";
    let r = run_with_stdin(&["--report"], src.as_bytes());
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(
        r.stdout.contains("<pre>"),
        "the sample was supposed to produce a <pre> block"
    );
    let c = report_of(&r);
    assert_eq!((c.paragraphs, c.headings, c.strong, c.br), (2, 0, 0, 0));
    assert_eq!(c, recount(&r.stdout));
}

/// A paragraph carrying an attribute (mailmode's header) is still counted; the
/// rule looks at what follows `<p`, not what preceded it.
#[test]
fn a_paragraph_with_an_attribute_is_counted() {
    let src = "From: a@example.com\nSubject: hi\n\nBody line.\n";
    let r = run_with_stdin(&["--mailmode", "--report"], src.as_bytes());
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(
        r.stdout.contains("<p class="),
        "mailmode was supposed to emit an attributed paragraph"
    );
    let c = report_of(&r);
    assert_eq!(c, recount(&r.stdout));
}

/// The report reads `--no-html5`'s upper-case tags as well as the HTML5
/// lower-case ones, or it would report the serialisation rather than the
/// structure.
#[test]
fn the_report_reads_upper_case_tags() {
    let src = "ALL CAPS LINE\n\nHeading\n=======\n";
    let r = run_with_stdin(&["--no-html5", "--report"], src.as_bytes());
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(
        r.stdout.contains("<H1>"),
        "the sample was supposed to emit an upper-case heading"
    );
    let c = report_of(&r);
    assert_eq!((c.headings, c.strong), (1, 1));
}

/// Fine-grained: the report must change nothing but stderr, on stdout and on
/// the file path, and the report must never reach stdout even when stdout is
/// the output.
#[test]
fn the_report_changes_nothing_but_stderr() {
    let dir = tmpdir();
    let input = dir.join("in.txt");
    std::fs::write(&input, SAMPLE).unwrap();
    let path = input.to_string_lossy().into_owned();

    let plain = run(&[&path]);
    let reported = run(&[&path, "--report"]);
    assert_eq!(plain.code, 0);
    assert_eq!(reported.code, 0);
    assert_eq!(
        reported.stdout, plain.stdout,
        "--report wrote to the output"
    );
    assert!(
        !reported.stdout.contains("report"),
        "the report reached standard output"
    );

    let plain_file = dir.join("plain.html");
    let reported_file = dir.join("reported.html");
    let _ = std::fs::remove_file(&plain_file);
    let _ = std::fs::remove_file(&reported_file);
    let a = run(&["--infile", &path, "--outfile", plain_file.to_str().unwrap()]);
    let b = run(&[
        "--infile",
        &path,
        "--outfile",
        reported_file.to_str().unwrap(),
        "--report",
    ]);
    assert_eq!(a.code, 0);
    assert_eq!(b.code, 0);
    assert_eq!(
        std::fs::read(&plain_file).unwrap(),
        std::fs::read(&reported_file).unwrap(),
        "--report changed the file output"
    );
}

/// The number `make examples` prints for `examples/homer.txt`, asserted here so
/// that the test and `make examples` hold the same claim.
#[test]
fn homer_reports_the_numbers_make_examples_prints() {
    let r = run(&["--infile", HOMER, "--report"]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let c = report_of(&r);
    assert_eq!(
        c,
        Counts {
            bytes: 38487,
            headings: 0,
            paragraphs: 64,
            strong: 39,
            br: 34,
        }
    );
    // And it really is what the file holds, not just what make examples claims.
    assert_eq!(c, recount(&r.stdout));
}

/// The rest of `examples/`: the CC0/PD documents, pinned the
/// same way as homer so that `make examples` and `make verify` agree on every
/// corpus file. The counts come from the report of the produced output, and the
/// recount-of-output assertion keeps the instrument from agreeing with itself.
#[test]
fn the_example_corpus_reports_its_counts() {
    let corpus: [(&str, Counts); 7] = [
        (
            "blake.txt",
            Counts {
                bytes: 16913,
                headings: 0,
                paragraphs: 466,
                strong: 26,
                br: 10,
            },
        ),
        (
            "calli.txt",
            Counts {
                bytes: 123439,
                headings: 0,
                paragraphs: 2814,
                strong: 164,
                br: 3,
            },
        ),
        (
            "erya.txt",
            Counts {
                bytes: 51314,
                headings: 0,
                paragraphs: 41,
                strong: 0,
                br: 1028,
            },
        ),
        (
            "gelbenhuegel.txt",
            Counts {
                bytes: 44494,
                headings: 16,
                paragraphs: 191,
                strong: 1,
                br: 334,
            },
        ),
        (
            "mohe_zhiguan_vol001.txt",
            Counts {
                bytes: 48443,
                headings: 0,
                paragraphs: 1,
                strong: 0,
                br: 7,
            },
        ),
        (
            "septuagint_swete_genesis.txt",
            Counts {
                bytes: 378864,
                headings: 0,
                paragraphs: 50,
                strong: 0,
                br: 50,
            },
        ),
        (
            "talmud.txt",
            Counts {
                bytes: 13611,
                headings: 0,
                paragraphs: 78,
                strong: 0,
                br: 3,
            },
        ),
    ];
    for (name, expected) in corpus {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/").to_owned() + name;
        let r = run(&["--infile", &path, "--report"]);
        assert_eq!(r.code, 0, "{name}: {}", r.stderr);
        let c = report_of(&r);
        assert_eq!(c, expected, "{name} regressed from its recorded counts");
        assert_eq!(
            c,
            recount(&r.stdout),
            "{name}: the report lied about the output"
        );
    }
}

/// The refusal is the same class the other whole-body passes share: a
/// streaming run never assembles the document, so there is nothing to count,
/// and a report of zeros would be a lie about the input.
#[test]
fn stream_refuses_the_report() {
    let r = run_with_stdin(&["--stream", "--report"], SAMPLE.as_bytes());
    assert_eq!(r.code, 1);
    assert!(
        r.stderr.contains("--report is not valid with --stream"),
        "unhelpful refusal: {:?}",
        r.stderr
    );
    // Without --report the same input still streams cleanly, so the refusal is
    // not spilling onto the legitimate pairing.
    let ok = run_with_stdin(&["--stream"], SAMPLE.as_bytes());
    assert_eq!(ok.code, 0, "stderr: {}", ok.stderr);
}

/// `--chunk` writes several files, so the report totals the run: one line whose
/// counts are the sum of the files actually written.
#[test]
fn chunk_totals_the_run() {
    let dir = tmpdir();
    let input = dir.join("in.txt");
    std::fs::write(&input, "One\n===\n\nalpha\n\ntiny\n\nTwo\n===\n\nbeta\n").unwrap();
    let out_path = dir.join("out.html");
    let r = run(&[
        "--infile",
        input.to_str().unwrap(),
        "--outfile",
        out_path.to_str().unwrap(),
        "--chunk",
        "--report",
    ]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let c = report_of(&r);
    assert_eq!(c.headings, 2, "two sections should mean two headings");

    let mut total = Counts {
        bytes: 0,
        headings: 0,
        paragraphs: 0,
        strong: 0,
        br: 0,
    };
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "html") {
            let html = std::fs::read_to_string(&path).unwrap();
            let c = recount(&html);
            total.bytes += c.bytes;
            total.headings += c.headings;
            total.paragraphs += c.paragraphs;
            total.strong += c.strong;
            total.br += c.br;
        }
    }
    assert_eq!(c, total, "the aggregated report did not sum the files");
}
