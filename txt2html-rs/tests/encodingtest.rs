//! P7.1 — the encoding rules `read_any_file` and `demoronize` actually
//! implement, pinned.
//!
//! The point of this file is the `0x80`-`0x9F` range. Everything the port did
//! before P7.1 passed on it by accident: no fixture had a byte in that range,
//! so the CP1252 decode was never exercised, and `demoronize` never fired on
//! a file it was supposed to. A test that cannot fail is not a test, so these
//! cases are built from raw bytes rather than from Rust string literals — a
//! literal `"\u{201c}"` in the test would pass under either decoder and prove
//! nothing.

use txt2html::chars::demoronize_char;
use txt2html::convert::Converter;
use txt2html::convert::{read_any_file_with_encoding, Resolved};
use txt2html::options::Options;

use std::io::Write;

/// Write `bytes` to a uniquely named temp file and hand back the path.
fn fixture(tag: &str, bytes: &[u8]) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("txt2html-p7-{}-{tag}.txt", std::process::id()));
    let mut f = std::fs::File::create(&path).expect("create fixture");
    f.write_all(bytes).expect("write fixture");
    path
}

#[test]
fn cp1252_range_decodes_to_cp1252_not_latin1() {
    // The four bytes in the plan's measurement: left/right double quote and
    // en/em dash. Latin-1 would make these U+0093/U+0094/U+0096/U+0097, which
    // are C1 controls and which the demoronize table does not contain.
    let raw = b"He said \x93hello\x94 and \x96dash\x97.\n";
    let path = fixture("quotes", raw);
    let (text, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(resolved, Resolved::Cp1252);
    assert!(
        text.contains('\u{201c}') && text.contains('\u{201d}'),
        "CP1252 0x93/0x94 must decode to U+201C/U+201D, got {text:?}"
    );
    assert!(
        !text.contains('\u{0093}') && !text.contains('\u{0094}'),
        "no C1 control characters may survive; got {text:?}"
    );
}

#[test]
fn latin1_range_is_unchanged() {
    // The range where the two encodings agree, and the only range Latin-1
    // defines. If this fails, the fallback has become something other than
    // "one byte, one code point" and every Latin-1 fixture is suspect.
    let raw = b"caf\xe9 na\xefve \xfcber \xdf\n";
    let path = fixture("latin1", raw);
    let (text, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(resolved, Resolved::Cp1252, "0xE9 is not valid UTF-8");
    assert_eq!(text, "café naïve über ß\n");
}

#[test]
fn undefined_cp1252_slots_stay_as_latin1() {
    // 0x81, 0x8D, 0x8F, 0x90 and 0x9D are undefined in CP1252. The decode must
    // be total: it may not panic, and it may not invent a glyph. Browsers keep
    // the control character, so the port does too.
    for b in [0x81u8, 0x8D, 0x8F, 0x90, 0x9D] {
        let raw = vec![b'a', b, b'b'];
        let path = fixture("undef", &raw);
        let (text, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(resolved, Resolved::Cp1252);
        assert_eq!(
            text.chars().nth(1).unwrap() as u32,
            b as u32,
            "0x{b:02X} is undefined in CP1252 and must stay U+{b:04X}"
        );
    }
}

#[test]
fn valid_utf8_is_reported_as_utf8() {
    // The 0xA0-0xFF agreement means a UTF-8 file and a CP1252 file can contain
    // the same *rendered* text. The report has to distinguish them or P7.3's
    // status line is a coin flip.
    let path = fixture("utf8", "caf\u{e9} \u{201c}hi\u{201d}\n".as_bytes());
    let (text, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(resolved, Resolved::Utf8);
    assert_eq!(text, "café \u{201c}hi\u{201d}\n");
}

#[test]
fn ascii_is_utf8_not_cp1252() {
    // An all-ASCII file is valid UTF-8. Reporting it as CP1252 would be the
    // kind of harmless-looking wrong answer that becomes a real bug the first
    // time someone branches on it.
    let path = fixture("ascii", b"plain ascii\n");
    let (_, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(resolved, Resolved::Utf8);
}

#[test]
fn demoronize_fires_on_a_decoded_cp1252_file() {
    // The regression this whole item is about, as an end-to-end assertion.
    // Before the fix this produced C1 control characters in the HTML.
    let raw = b"He said \x93hello\x94 and \x96dash\x97.\n";
    let path = fixture("e2e", raw);
    let mut conv = Converter::new(Options::default());
    conv.opts.infile = vec![path.to_str().unwrap().to_string()];
    conv.opts.default_link_dict = String::new();
    let out = conv.txt2html();
    let _ = std::fs::remove_file(&path);

    assert!(
        !out.contains('\u{0093}') && !out.contains('\u{0097}'),
        "output must not contain C1 controls: {out:?}"
    );
    assert!(
        out.contains("He said \"hello\" and -dash--."),
        "demoronize should have substituted the CP1252 punctuation: {out:?}"
    );
}

#[test]
fn demoronize_table_is_reachable_at_all() {
    // A direct check that the table keys on the code points the decoder now
    // produces. This is the link in the chain that was broken: the table was
    // correct, and unreachable.
    let mut s = "\u{201c}hi\u{201d} \u{2013} \u{2014} \u{2019}s".to_string();
    demoronize_char(&mut s);
    assert_eq!(s, "\"hi\" - -- 's");
}

#[test]
fn utf8_input_with_cp1252_range_bytes_is_untouched_by_the_fallback() {
    // The lib.rs wording said the difference showed up "only for UTF-8 input
    // containing characters whose encoding has a byte in 0x80-0x9F". A valid
    // UTF-8 file must never reach the fallback, so its U+201C survives as
    // U+201C until demoronize asks for it -- not decoded twice, not mangled.
    let path = fixture("utf8smart", "\u{201c}hi\u{201d}\n".as_bytes());
    let (text, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(resolved, Resolved::Utf8);
    assert_eq!(text, "\u{201c}hi\u{201d}\n");
}

#[test]
fn resolved_encoding_reports_cp1252_after_conversion() {
    // P7.3's reporting, end to end: the caller can find out what happened.
    let path = fixture("report", b"a \x93b\n");
    let mut conv = Converter::new(Options::default());
    conv.opts.infile = vec![path.to_str().unwrap().to_string()];
    conv.opts.default_link_dict = String::new();
    let _ = conv.try_txt2html().unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(conv.resolved_encoding(), Resolved::Cp1252);
    assert_eq!(conv.resolved_encoding().name(), "cp1252");
}

#[test]
fn wide_chars_survive_an_aligned_table() {
    // P7.1's second case. byte_slice cuts cells by byte offset, so this only
    // holds when every row is the same byte length -- which the fixture
    // guarantees by construction, the 3-byte character included. Before the
    // test existed, nothing in the corpus had a wide character in a table, so
    // this path was never checked at all.
    let raw = b"+---------+---------+\n|abc      | def     |\n+---------+---------+\n|\xe6\x97\xa5      | ghi     |\n+---------+---------+\n";
    for line in raw.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
        assert_eq!(line.len(), 21, "every row must be 21 bytes: {line:?}");
    }
    let path = fixture("cjk", raw);
    let mut conv = Converter::new(Options::default());
    conv.opts.infile = vec![path.to_str().unwrap().to_string()];
    conv.opts.make_tables = true;
    conv.opts.default_link_dict = String::new();
    let out = conv.txt2html();
    let _ = std::fs::remove_file(&path);

    assert!(
        out.contains("<td>\u{65e5}</td>"),
        "CJK cell must survive: {out:?}"
    );
    assert!(
        !out.contains("&aelig;") && !out.contains("&not;"),
        "no mangled Latin-1 entities: {out:?}"
    );
}

// --- P7.3: the explicit --encoding option ---------------------------------

#[test]
fn encoding_auto_is_the_default_and_probes() {
    let raw = b"caf\xe9 \x93q\x94\n";
    let path = fixture("enc-auto", raw);
    let (text, resolved) =
        txt2html::convert::read_with(path.to_str().unwrap(), txt2html::options::Encoding::Auto)
            .unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(resolved, Resolved::Cp1252);
    assert!(text.contains('\u{201c}'), "{text:?}");
}

#[test]
fn encoding_cp1252_decodes_even_when_the_bytes_are_valid_utf8() {
    // The case the probe cannot settle. These bytes are valid UTF-8, so
    // Encoding::Auto calls the file UTF-8; decoding it as CP1252 instead is
    // what the user asked for by passing the option, and it is the only way to
    // read a short CP1252 document that happens to be valid UTF-8.
    let path = fixture("enc-cp", b"caf\xc3\xa9\n");
    let (text, resolved) =
        txt2html::convert::read_with(path.to_str().unwrap(), txt2html::options::Encoding::Cp1252)
            .unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(resolved, Resolved::Cp1252);
    // c3 a9 is U+00C3 U+00A9 in CP1252 -- "A-acute", "C-cedilla" -- which is
    // mojibake when read as one encoding and written as another, and is
    // exactly what a user forcing this option on valid UTF-8 is asking for.
    // The first draft of this test asserted the UTF-8 reading here and failed,
    // which is the useful reminder that "these bytes are valid UTF-8" says
    // nothing about which encoding the file was written in.
    assert_eq!(text, "caf\u{c3}\u{a9}\n");
}

#[test]
fn encoding_utf8_is_lossy_and_says_so() {
    // Forcing UTF-8 on invalid bytes substitutes U+FFFD rather than aborting.
    // A converter that exits 1 over one bad byte is not a converter.
    let path = fixture("enc-utf8", b"a \xe9 b\n");
    let (text, resolved) =
        txt2html::convert::read_with(path.to_str().unwrap(), txt2html::options::Encoding::Utf8)
            .unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(resolved, Resolved::Utf8);
    assert!(text.contains('\u{fffd}'), "{text:?}");
}

#[test]
fn encoding_names_round_trip() {
    use txt2html::options::Encoding;
    for e in [Encoding::Auto, Encoding::Utf8, Encoding::Cp1252] {
        assert_eq!(Encoding::parse(e.name()).unwrap(), e);
    }
    // Spellings a user would plausibly type.
    assert_eq!(Encoding::parse("UTF8").unwrap(), Encoding::Utf8);
    assert_eq!(Encoding::parse("windows-1252").unwrap(), Encoding::Cp1252);
    assert!(Encoding::parse("shift_jis").is_err());
}

// --- P7.4: the charset declaration ----------------------------------------

#[test]
fn meta_charset_is_off_by_default() {
    // The compatibility reason, asserted: byte-identical output from the
    // reference is a stated goal, and the reference emits no charset.
    let mut conv = Converter::new(Options::default());
    assert!(!conv.opts.meta_charset);
    conv.opts.instring = vec!["hi".to_string()];
    let out = conv.txt2html();
    assert!(
        !out.contains("charset"),
        "default output must not gain a meta: {out:?}"
    );
}

#[test]
fn meta_charset_emits_one_per_line_when_on() {
    let mut conv = Converter::new(Options::default());
    conv.opts.meta_charset = true;
    conv.opts.xhtml = true;
    conv.opts.instring = vec!["hi".to_string()];
    let out = conv.txt2html();
    assert!(out.contains("<meta charset=\"utf-8\"/>"), "{out:?}");
    assert!(
        out.contains("content=\"HTML::TextToHTML v3.0\"/>\n<meta charset"),
        "each meta needs its own line: {out:?}"
    );
}

#[test]
fn meta_charset_follows_lower_case_tags() {
    // The document's tag case is an option, so the new tag has to follow it or
    // the head is half upper-case and half lower-case.
    let mut conv = Converter::new(Options::default());
    conv.opts.meta_charset = true;
    conv.opts.xhtml = false;
    conv.opts.lower_case_tags = false;
    conv.opts.instring = vec!["hi".to_string()];
    let out = conv.txt2html();
    assert!(out.contains("<META CHARSET=\"utf-8\">"), "{out:?}");
}
