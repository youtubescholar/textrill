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

use textrill::chars::demoronize_char;
use textrill::convert::Converter;
use textrill::convert::{read_any_file_with_encoding, Resolved};
use textrill::options::Options;
use textrill::options::{Encoding, SingleByte};

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

    assert_eq!(resolved, Resolved::Single(SingleByte::Cp1252));
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

    assert_eq!(
        resolved,
        Resolved::Single(SingleByte::Cp1252),
        "0xE9 is not valid UTF-8"
    );
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
        assert_eq!(resolved, Resolved::Single(SingleByte::Cp1252));
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
    let out = conv.convert();
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
    let _ = conv.try_convert().unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        conv.resolved_encoding(),
        Resolved::Single(SingleByte::Cp1252)
    );
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
    let out = conv.convert();
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
        textrill::convert::read_with(path.to_str().unwrap(), textrill::options::Encoding::Auto)
            .unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(resolved, Resolved::Single(SingleByte::Cp1252));
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
        textrill::convert::read_with(path.to_str().unwrap(), textrill::options::Encoding::Cp1252)
            .unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(resolved, Resolved::Single(SingleByte::Cp1252));
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
        textrill::convert::read_with(path.to_str().unwrap(), textrill::options::Encoding::Utf8)
            .unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(resolved, Resolved::Utf8);
    assert!(text.contains('\u{fffd}'), "{text:?}");
}

#[test]
fn encoding_names_round_trip() {
    use textrill::options::Encoding;
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
    let out = conv.convert();
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
    let out = conv.convert();
    assert!(out.contains("<meta charset=\"utf-8\"/>"), "{out:?}");
    // Each meta needs its own line. The generator string is spelled from the
    // crate version rather than written out: this test is about the newline
    // between the two metas, not about which generator is named, and a literal
    // here went stale the moment P1.1 renamed it -- which is how the test
    // started failing for a reason that had nothing to do with encodings. The
    // value itself is asserted in tests/provenance.rs.
    let gen = format!(
        "content=\"textrill v{}\"/>\n<meta charset",
        env!("CARGO_PKG_VERSION")
    );
    assert!(out.contains(&gen), "each meta needs its own line: {out:?}");
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
    let out = conv.convert();
    assert!(out.contains("<META CHARSET=\"utf-8\">"), "{out:?}");
}

// ---------------------------------------------------------------------------
// P7.4 -- detection order: BOM, then NUL structure, then UTF-8, then CP1252.
//
// The tests above cover the single-byte fallback, which was the easy half of
// the problem. These cover the half that was actually broken: a UTF-16 file
// whose code units are all ASCII is *valid UTF-8*, so the old probe accepted
// it, and the file reached the output with a NUL between every character
// (`H\0e\0l\0l\0o\0`). Anything that can distinguish "valid UTF-8" from
// "UTF-16LE ASCII" has to look at the NUL pattern, never at UTF-8 validity.
// ---------------------------------------------------------------------------

/// Encode `s` as UTF-16/32 with the given BOM prefix, or none.
fn wide(text: &str, enc: &str, bom: bool) -> Vec<u8> {
    let (body, mark) = match enc {
        "utf-16le" => (
            text.encode_utf16()
                .flat_map(|u| u.to_le_bytes())
                .collect::<Vec<u8>>(),
            &[0xFF, 0xFE][..],
        ),
        "utf-16be" => (
            text.encode_utf16()
                .flat_map(|u| u.to_be_bytes())
                .collect::<Vec<u8>>(),
            &[0xFE, 0xFF][..],
        ),
        "utf-32le" => (
            text.chars()
                .flat_map(|c| (c as u32).to_le_bytes())
                .collect::<Vec<u8>>(),
            &[0xFF, 0xFE, 0x00, 0x00][..],
        ),
        _ => (
            text.chars()
                .flat_map(|c| (c as u32).to_be_bytes())
                .collect::<Vec<u8>>(),
            &[0x00, 0x00, 0xFE, 0xFF][..],
        ),
    };
    if bom {
        mark.iter().chain(body.iter()).copied().collect()
    } else {
        body
    }
}

const PROSE: &str = "Hello world.\nThis is plain ASCII prose.\n";

#[test]
fn bomless_utf16_ascii_is_not_mistaken_for_utf8() {
    // The regression that motivated P7.4. Both files are valid UTF-8; the only
    // thing separating them from a real UTF-8 document is where the NULs sit.
    for (enc, name) in [("utf-16le", "utf-16le"), ("utf-16be", "utf-16be")] {
        let bytes = wide(PROSE, enc, false);
        assert!(
            std::str::from_utf8(&bytes).is_ok(),
            "{enc} of ASCII must be valid UTF-8, or this test proves nothing"
        );
        let path = fixture("nb", &bytes);
        let (text, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(text, PROSE, "{enc} without a BOM");
        assert_eq!(resolved.name(), name);
        assert!(
            !text.contains('\0'),
            "a NUL between every character is the failure this test exists for"
        );
    }
}

#[test]
fn a_bom_is_honoured_rather_than_decoded_as_cp1252() {
    // `FF FE` is a guarantee, not a decode error. Before P7.4 it failed the
    // UTF-8 probe and fell through to CP1252, where those bytes have no meaning
    // at all, so the text came out as `&yuml;&thorn;Hello`.
    for enc in ["utf-16le", "utf-16be", "utf-32le", "utf-32be"] {
        let bytes = wide(PROSE, enc, true);
        let path = fixture("bom", &bytes);
        let (text, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(text, PROSE, "{enc} with a BOM: {text:?}");
        assert_eq!(resolved.name(), enc);
    }
}

#[test]
fn a_utf8_bom_is_kept_because_it_is_content_not_metadata() {
    // Asymmetric with the wide encodings on purpose. A UTF-8 BOM is a
    // signature the reader may skip, but the reference keeps it and so does
    // the GUI's display, so dropping it here would change every byte of an
    // otherwise ASCII-identical document.
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(PROSE.as_bytes());
    let path = fixture("u8bom", &bytes);
    let (text, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(text, format!("\u{FEFF}{PROSE}"));
    assert_eq!(resolved.name(), "utf-8");
}

#[test]
fn utf32le_is_not_read_as_utf16le() {
    // UTF-32LE's BOM begins with UTF-16LE's. Checking the short mark first
    // produces pairs of Latin-1 characters -- a wrong answer that still looks
    // like text, which is the most expensive kind to notice by eye.
    let bytes = wide("abcd\u{1F600}ef", "utf-32le", true);
    let path = fixture("u32", &bytes);
    let (text, _) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(text, "abcd\u{1F600}ef");
}

#[test]
fn the_nul_heuristic_does_not_fire_on_ordinary_text() {
    // A UTF-8 file containing a few NULs, a binary blob, and a doc with no
    // NULs at all must all stay UTF-8. A detector that fires on all of these
    // would be worse than no detector.
    let cases: &[&[u8]] = &[
        b"plain ascii prose with no nul bytes at all\n",
        "héllo wörld\n".as_bytes(),
        b"one nul\0in the middle of a line\n",
        b"\x00\x00\x00 short and mostly nul\n",
    ];
    for (i, bytes) in cases.iter().enumerate() {
        let path = fixture(&format!("neg{i}"), bytes);
        let (_, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(resolved.name(), "utf-8", "case {i} was misdetected");
    }
}

#[test]
fn bomless_utf16_of_non_latin_text_is_detectable_only_when_named() {
    // The documented limit, pinned so it cannot be forgotten. Cyrillic in
    // UTF-16LE is `04 xx` per unit: no NULs, so the structural evidence simply
    // is not there. `--encoding utf-16le` recovers it, which is exactly why the
    // option exists and why P7.4 did not try to guess harder.
    // What decides detection is not the script but the *density* of
    // ASCII-range units, since spaces and punctuation are what supply the
    // NULs. Measured, rather than assumed:
    //
    //   "Привет, мир!"  4 NULs in 24 bytes  -> sniff fires, decodes correctly
    //   "Привет"        0 NULs in 12 bytes  -> nothing to detect, reads as UTF-8
    //
    // The second is the honest limit, and it is why `--encoding` exists. P7.4
    // deliberately does not guess harder here: a text file that is pure
    // Cyrillic in BOM-less UTF-16 is indistinguishable from UTF-8 bytes by any
    // rule short of a statistical model, and a confident wrong answer is worse
    // than one the user can fix with a flag.
    let ambiguous = "Привет";
    let bytes = wide(ambiguous, "utf-16le", false);
    assert_eq!(
        bytes.iter().filter(|&&b| b == 0).count(),
        0,
        "pure Cyrillic supplies no NULs at all"
    );
    let path = fixture("nb-cyr", &bytes);
    let (auto_text, auto_resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
    let (named, _) =
        textrill::convert::read_with(path.to_str().unwrap(), Encoding::Utf16Le).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(named, ambiguous, "the flag must still recover it");
    assert_eq!(auto_resolved.name(), "utf-8");
    assert_ne!(auto_text, ambiguous);

    // And the denser case is detected, so the limit is not "Cyrillic".
    let denser = "Привет, мир!";
    let bytes = wide(denser, "utf-16le", false);
    let path = fixture("nb-cyr2", &bytes);
    let (text, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(text, denser);
    assert_eq!(resolved.name(), "utf-16le");
}

#[test]
fn the_nul_threshold_is_pinned_to_measured_text() {
    // `src/convert.rs` documents the 1/8 threshold against measured NUL
    // fractions. This makes the decision a deliberate one: these are the same
    // samples, so changing the threshold to "feel" right breaks here and the
    // measurement has to be redone rather than re-guessed.
    //
    // Note the Russian and Greek rows. An earlier 2/3 threshold passed the
    // English ones and failed these, which would have left the most
    // interesting input -- the one that is not ASCII and so is not trivially
    // detectable -- broken while the easy cases looked fine.
    let detected: &[(&str, &str)] = &[
        (
            "english prose",
            "The quick brown fox jumps over the lazy dog.\n",
        ),
        ("russian prose", "Привет, мир! Это обычный русский текст.\n"),
        (
            "greek prose",
            "Γειά σου Κόσμε! Αυτό είναι ελληνικό κείμενο.\n",
        ),
    ];
    for (name, text) in detected {
        for enc in ["utf-16le", "utf-16be"] {
            let bytes = wide(text, enc, false);
            let path = fixture("thr", &bytes);
            let (got, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
            let _ = std::fs::remove_file(&path);
            assert_eq!(got.as_str(), *text, "{name} as {enc}");
            assert_eq!(resolved.name(), enc);
        }
    }

    let untouched: &[(&str, &[u8])] = &[
        (
            "ascii document",
            b"This is a plain ASCII document with no unusual bytes.\n",
        ),
        (
            "html",
            b"<html><body><p>Hello</p>\n<ul><li>a</li></ul>\n</body></html>\n",
        ),
        (
            "raw byte range",
            &[0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        ),
    ];
    for (name, bytes) in untouched {
        let path = fixture("neg", bytes);
        let (_, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(
            matches!(resolved, Resolved::Utf8 | Resolved::Single(_)),
            "{name} must not be decoded as UTF-16"
        );
    }
}

#[test]
fn a_lone_surrogate_becomes_a_replacement_not_a_panic() {
    // `char::from_u32` rejects surrogates. Substituting U+FFFD matches every
    // other decoder; panicking would turn one bad file into a crash.
    // Long enough for the NUL sniff to engage, with a lone surrogate in place of
    // the first character: 0xD800 little-endian is 00 D8.
    let mut bytes = wide("abc def ghi jkl", "utf-16le", false);
    bytes[0..2].copy_from_slice(&[0x00, 0xD8]);
    let path = fixture("surrogate", &bytes);
    let (text, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(resolved.name(), "utf-16le");
    assert_eq!(text, "\u{FFFD}bc def ghi jkl");
}

#[test]
fn a_truncated_final_code_unit_is_dropped_not_padded() {
    // Odd-length UTF-16 happens. Inventing a character from half a unit would
    // be a guess; dropping the fragment is what a decoder should do.
    let mut bytes = wide("abc def ghi", "utf-16le", false);
    bytes.push(0x41);
    let path = fixture("trunc", &bytes);
    let (text, resolved) = read_any_file_with_encoding(path.to_str().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(resolved.name(), "utf-16le");
    assert_eq!(text, "abc def ghi");
}

// ---------------------------------------------------------------------------
// Explicit overrides: the escape hatch for what detection cannot do.
// ---------------------------------------------------------------------------

#[test]
fn naming_the_encoding_recovers_what_detection_cannot() {
    // Russian in CP1251 and in KOI8-R disagree about almost every high byte, so
    // guessing wrong between them is not a small error -- and detection cannot
    // help, because both are valid CP1252 byte sequences. This is the case that
    // makes `--encoding` worth having at all.
    const TEXT_CYR: &str = "Привет, мир!\n";
    const TEXT_GR: &str = "Γειά σου Κόσμε!\n";
    // CP1251 and CP1253 both have a gap where the reference implementation
    // emitted an entity for every non-ASCII byte, so these are the exact
    // byte-for-byte shapes of the old failure.
    let cases: &[(&str, &[u8], &str)] = &[
        (
            "cp1251",
            &[
                0xcf, 0xf0, 0xe8, 0xe2, 0xe5, 0xf2, 0x2c, 0x20, 0xec, 0xe8, 0xf0, 0x21, 0x0a,
            ],
            TEXT_CYR,
        ),
        (
            "koi8-r",
            &[
                0xf0, 0xd2, 0xc9, 0xd7, 0xc5, 0xd4, 0x2c, 0x20, 0xcd, 0xc9, 0xd2, 0x21, 0x0a,
            ],
            TEXT_CYR,
        ),
        (
            "cp1253",
            &[
                0xc3, 0xe5, 0xe9, 0xdc, 0x20, 0xf3, 0xef, 0xf5, 0x20, 0xca, 0xfc, 0xf3, 0xec, 0xe5,
                0x21, 0x0a,
            ],
            TEXT_GR,
        ),
    ];
    for (name, bytes, want) in cases {
        // Auto guesses CP1252 and is wrong for all three -- the pre-P7.4
        // behaviour, kept visible rather than deleted.
        let (auto, auto_resolved) = textrill::convert::decode_bytes_with(bytes, Encoding::Auto);
        assert_eq!(
            auto_resolved.name(),
            "cp1252",
            "{name} should not be auto-detected"
        );
        assert_ne!(auto, *want, "{name} must not decode correctly by luck");

        let (got, resolved) =
            textrill::convert::decode_bytes_with(bytes, Encoding::parse(name).unwrap());
        assert_eq!(got, *want, "--encoding {name}");
        assert_eq!(resolved.name(), *name);
    }
}

#[test]
fn an_explicitly_named_utf8_file_is_not_reinterpreted_as_cp1252() {
    // The mirror image of P7.1's CP1252 case: a file that *is* valid UTF-8 but
    // was meant to be read as something else. Auto calls it UTF-8 (correct
    // here), and naming cp1252 gets the Latin-1 reading the user asked for.
    const TEXT: &str = "Привет, мир!\n";
    let utf8 = TEXT.as_bytes();
    assert!(std::str::from_utf8(utf8).is_ok());
    let (auto, resolved) = textrill::convert::decode_bytes_with(utf8, Encoding::Auto);
    assert_eq!(auto, TEXT);
    assert_eq!(resolved.name(), "utf-8");
    // Every byte of that UTF-8 is a legal CP1252 byte, which is exactly why
    // "valid UTF-8" cannot be the only question asked.
    let (as_cp1252, _) = textrill::convert::decode_bytes_with(utf8, Encoding::Cp1252);
    assert_eq!(
        as_cp1252.chars().count(),
        utf8.len(),
        "one byte in, one char out"
    );
    assert!(
        !as_cp1252
            .chars()
            .any(|c| ('\u{0400}'..='\u{04FF}').contains(&c)),
        "reading UTF-8 bytes as CP1252 must not yield Cyrillic: {as_cp1252:?}"
    );
}

// GENERATED by tests/gen_encoding_tables.py -- do not edit by hand.
// The reference the hand-written tables in src/convert.rs are checked
// against, byte for byte.
const EXPECTED_HIGH: &[(&str, &[u32; 128])] = &[
    (
        "iso8859_1",
        &[
            0x0080, 0x0081, 0x0082, 0x0083, 0x0084, 0x0085, 0x0086, 0x0087, 0x0088, 0x0089, 0x008a,
            0x008b, 0x008c, 0x008d, 0x008e, 0x008f, 0x0090, 0x0091, 0x0092, 0x0093, 0x0094, 0x0095,
            0x0096, 0x0097, 0x0098, 0x0099, 0x009a, 0x009b, 0x009c, 0x009d, 0x009e, 0x009f, 0x00a0,
            0x00a1, 0x00a2, 0x00a3, 0x00a4, 0x00a5, 0x00a6, 0x00a7, 0x00a8, 0x00a9, 0x00aa, 0x00ab,
            0x00ac, 0x00ad, 0x00ae, 0x00af, 0x00b0, 0x00b1, 0x00b2, 0x00b3, 0x00b4, 0x00b5, 0x00b6,
            0x00b7, 0x00b8, 0x00b9, 0x00ba, 0x00bb, 0x00bc, 0x00bd, 0x00be, 0x00bf, 0x00c0, 0x00c1,
            0x00c2, 0x00c3, 0x00c4, 0x00c5, 0x00c6, 0x00c7, 0x00c8, 0x00c9, 0x00ca, 0x00cb, 0x00cc,
            0x00cd, 0x00ce, 0x00cf, 0x00d0, 0x00d1, 0x00d2, 0x00d3, 0x00d4, 0x00d5, 0x00d6, 0x00d7,
            0x00d8, 0x00d9, 0x00da, 0x00db, 0x00dc, 0x00dd, 0x00de, 0x00df, 0x00e0, 0x00e1, 0x00e2,
            0x00e3, 0x00e4, 0x00e5, 0x00e6, 0x00e7, 0x00e8, 0x00e9, 0x00ea, 0x00eb, 0x00ec, 0x00ed,
            0x00ee, 0x00ef, 0x00f0, 0x00f1, 0x00f2, 0x00f3, 0x00f4, 0x00f5, 0x00f6, 0x00f7, 0x00f8,
            0x00f9, 0x00fa, 0x00fb, 0x00fc, 0x00fd, 0x00fe, 0x00ff,
        ],
    ),
    (
        "cp1252",
        &[
            0x20ac, 0, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160,
            0x2039, 0x0152, 0, 0x017d, 0, 0, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013,
            0x2014, 0x02dc, 0x2122, 0x0161, 0x203a, 0x0153, 0, 0x017e, 0x0178, 0x00a0, 0x00a1,
            0x00a2, 0x00a3, 0x00a4, 0x00a5, 0x00a6, 0x00a7, 0x00a8, 0x00a9, 0x00aa, 0x00ab, 0x00ac,
            0x00ad, 0x00ae, 0x00af, 0x00b0, 0x00b1, 0x00b2, 0x00b3, 0x00b4, 0x00b5, 0x00b6, 0x00b7,
            0x00b8, 0x00b9, 0x00ba, 0x00bb, 0x00bc, 0x00bd, 0x00be, 0x00bf, 0x00c0, 0x00c1, 0x00c2,
            0x00c3, 0x00c4, 0x00c5, 0x00c6, 0x00c7, 0x00c8, 0x00c9, 0x00ca, 0x00cb, 0x00cc, 0x00cd,
            0x00ce, 0x00cf, 0x00d0, 0x00d1, 0x00d2, 0x00d3, 0x00d4, 0x00d5, 0x00d6, 0x00d7, 0x00d8,
            0x00d9, 0x00da, 0x00db, 0x00dc, 0x00dd, 0x00de, 0x00df, 0x00e0, 0x00e1, 0x00e2, 0x00e3,
            0x00e4, 0x00e5, 0x00e6, 0x00e7, 0x00e8, 0x00e9, 0x00ea, 0x00eb, 0x00ec, 0x00ed, 0x00ee,
            0x00ef, 0x00f0, 0x00f1, 0x00f2, 0x00f3, 0x00f4, 0x00f5, 0x00f6, 0x00f7, 0x00f8, 0x00f9,
            0x00fa, 0x00fb, 0x00fc, 0x00fd, 0x00fe, 0x00ff,
        ],
    ),
    (
        "cp1251",
        &[
            0x0402, 0x0403, 0x201a, 0x0453, 0x201e, 0x2026, 0x2020, 0x2021, 0x20ac, 0x2030, 0x0409,
            0x2039, 0x040a, 0x040c, 0x040b, 0x040f, 0x0452, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022,
            0x2013, 0x2014, 0, 0x2122, 0x0459, 0x203a, 0x045a, 0x045c, 0x045b, 0x045f, 0x00a0,
            0x040e, 0x045e, 0x0408, 0x00a4, 0x0490, 0x00a6, 0x00a7, 0x0401, 0x00a9, 0x0404, 0x00ab,
            0x00ac, 0x00ad, 0x00ae, 0x0407, 0x00b0, 0x00b1, 0x0406, 0x0456, 0x0491, 0x00b5, 0x00b6,
            0x00b7, 0x0451, 0x2116, 0x0454, 0x00bb, 0x0458, 0x0405, 0x0455, 0x0457, 0x0410, 0x0411,
            0x0412, 0x0413, 0x0414, 0x0415, 0x0416, 0x0417, 0x0418, 0x0419, 0x041a, 0x041b, 0x041c,
            0x041d, 0x041e, 0x041f, 0x0420, 0x0421, 0x0422, 0x0423, 0x0424, 0x0425, 0x0426, 0x0427,
            0x0428, 0x0429, 0x042a, 0x042b, 0x042c, 0x042d, 0x042e, 0x042f, 0x0430, 0x0431, 0x0432,
            0x0433, 0x0434, 0x0435, 0x0436, 0x0437, 0x0438, 0x0439, 0x043a, 0x043b, 0x043c, 0x043d,
            0x043e, 0x043f, 0x0440, 0x0441, 0x0442, 0x0443, 0x0444, 0x0445, 0x0446, 0x0447, 0x0448,
            0x0449, 0x044a, 0x044b, 0x044c, 0x044d, 0x044e, 0x044f,
        ],
    ),
    (
        "cp1253",
        &[
            0x20ac, 0, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0, 0x2030, 0, 0x2039, 0, 0,
            0, 0, 0, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0, 0x2122, 0, 0x203a,
            0, 0, 0, 0, 0x00a0, 0x0385, 0x0386, 0x00a3, 0x00a4, 0x00a5, 0x00a6, 0x00a7, 0x00a8,
            0x00a9, 0, 0x00ab, 0x00ac, 0x00ad, 0x00ae, 0x2015, 0x00b0, 0x00b1, 0x00b2, 0x00b3,
            0x0384, 0x00b5, 0x00b6, 0x00b7, 0x0388, 0x0389, 0x038a, 0x00bb, 0x038c, 0x00bd, 0x038e,
            0x038f, 0x0390, 0x0391, 0x0392, 0x0393, 0x0394, 0x0395, 0x0396, 0x0397, 0x0398, 0x0399,
            0x039a, 0x039b, 0x039c, 0x039d, 0x039e, 0x039f, 0x03a0, 0x03a1, 0, 0x03a3, 0x03a4,
            0x03a5, 0x03a6, 0x03a7, 0x03a8, 0x03a9, 0x03aa, 0x03ab, 0x03ac, 0x03ad, 0x03ae, 0x03af,
            0x03b0, 0x03b1, 0x03b2, 0x03b3, 0x03b4, 0x03b5, 0x03b6, 0x03b7, 0x03b8, 0x03b9, 0x03ba,
            0x03bb, 0x03bc, 0x03bd, 0x03be, 0x03bf, 0x03c0, 0x03c1, 0x03c2, 0x03c3, 0x03c4, 0x03c5,
            0x03c6, 0x03c7, 0x03c8, 0x03c9, 0x03ca, 0x03cb, 0x03cc, 0x03cd, 0x03ce, 0,
        ],
    ),
    (
        "koi8_r",
        &[
            0x2500, 0x2502, 0x250c, 0x2510, 0x2514, 0x2518, 0x251c, 0x2524, 0x252c, 0x2534, 0x253c,
            0x2580, 0x2584, 0x2588, 0x258c, 0x2590, 0x2591, 0x2592, 0x2593, 0x2320, 0x25a0, 0x2219,
            0x221a, 0x2248, 0x2264, 0x2265, 0x00a0, 0x2321, 0x00b0, 0x00b2, 0x00b7, 0x00f7, 0x2550,
            0x2551, 0x2552, 0x0451, 0x2553, 0x2554, 0x2555, 0x2556, 0x2557, 0x2558, 0x2559, 0x255a,
            0x255b, 0x255c, 0x255d, 0x255e, 0x255f, 0x2560, 0x2561, 0x0401, 0x2562, 0x2563, 0x2564,
            0x2565, 0x2566, 0x2567, 0x2568, 0x2569, 0x256a, 0x256b, 0x256c, 0x00a9, 0x044e, 0x0430,
            0x0431, 0x0446, 0x0434, 0x0435, 0x0444, 0x0433, 0x0445, 0x0438, 0x0439, 0x043a, 0x043b,
            0x043c, 0x043d, 0x043e, 0x043f, 0x044f, 0x0440, 0x0441, 0x0442, 0x0443, 0x0436, 0x0432,
            0x044c, 0x044b, 0x0437, 0x0448, 0x044d, 0x0449, 0x0447, 0x044a, 0x042e, 0x0410, 0x0411,
            0x0426, 0x0414, 0x0415, 0x0424, 0x0413, 0x0425, 0x0418, 0x0419, 0x041a, 0x041b, 0x041c,
            0x041d, 0x041e, 0x041f, 0x042f, 0x0420, 0x0421, 0x0422, 0x0423, 0x0416, 0x0412, 0x042c,
            0x042b, 0x0417, 0x0428, 0x042d, 0x0429, 0x0427, 0x042a,
        ],
    ),
];

#[test]
fn every_table_entry_matches_python() {
    // The tables in src/convert.rs are generated from Python's codec tables,
    // but a generator that is wrong is still just a wrong table. This is the
    // cross-check: all 128 high bytes of all 5 encodings, against the same
    // reference the generator used.
    for (codec, expected) in EXPECTED_HIGH {
        let encoding = Encoding::parse(codec).unwrap_or_else(|e| panic!("{codec}: {e}"));
        for (i, &want) in expected.iter().enumerate() {
            let byte = 0x80 + i as u8;
            let (text, _) = textrill::convert::decode_bytes_with(&[byte], encoding);
            let got = text.chars().next().unwrap() as u32;
            // 0 means "undefined in this encoding"; the decoder is documented
            // to keep the Latin-1 reading, which is byte-as-char.
            let want = if want == 0 { byte as u32 } else { want };
            assert_eq!(
                got, want,
                "{codec} byte 0x{byte:02x}: expected U+{want:04X}, got U+{got:04X}"
            );
        }
    }
}

#[test]
fn undefined_slots_are_exactly_the_documented_ones() {
    // The docs above the tables in src/convert.rs quote these counts ("CP1252
    // leaves 5 of 128 undefined, CP1251 leaves 1, CP1253 leaves 17, KOI8-R
    // leaves none"). A table edit that changes them makes that prose false, and
    // the prose is the only explanation a user of an undefined byte gets.
    let counts = |codec: &str| -> usize {
        EXPECTED_HIGH
            .iter()
            .find(|(name, _)| *name == codec)
            .map(|(_, table)| table.iter().filter(|&&v| v == 0).count())
            .unwrap_or_else(|| panic!("no generated reference for {codec}"))
    };
    assert_eq!(counts("cp1252"), 5);
    assert_eq!(counts("cp1251"), 1);
    assert_eq!(counts("cp1253"), 17);
    assert_eq!(counts("koi8_r"), 0);
    assert_eq!(counts("iso8859_1"), 0);

    // CP1251's single reserved byte is the Kazakh letter Ғ in most
    // implementations; the point is that it is exactly one and it is 0x98.
    let cp1251 = EXPECTED_HIGH
        .iter()
        .find(|(name, _)| *name == "cp1251")
        .unwrap()
        .1;
    let undefined: Vec<u8> = (0..128)
        .filter(|&i| cp1251[i] == 0)
        .map(|i| 0x80 + i as u8)
        .collect();
    assert_eq!(undefined, vec![0x98]);
}

#[test]
fn latin1_is_no_longer_an_alias_for_cp1252() {
    // Before P7.4 `--encoding latin-1` meant CP1252. That was not a harmless
    // spelling: a Latin-1 file's 0x93 is a C1 control, not a left double quote,
    // and the user asking for Latin-1 was asking for the C1 control.
    let bytes = [0x93u8, b'a', 0x94, b' ', 0x97];
    let (l1, _) = textrill::convert::decode_bytes_with(&bytes, Encoding::Latin1);
    let (cp, _) = textrill::convert::decode_bytes_with(&bytes, Encoding::Cp1252);
    assert_eq!(l1, "\u{93}a\u{94} \u{97}");
    assert_eq!(cp, "\u{201c}a\u{201d} \u{2014}");
}

#[test]
fn resolved_encoding_ranks_a_wide_decode_above_a_legacy_guess() {
    // When one run mixes inputs, one encoding gets reported. UTF-16 should win
    // over CP1252: it means the bytes needed structural detection to be
    // understood at all, so it is the more surprising thing that happened.
    let path = fixture("mixed", &wide("mixed content here\n", "utf-16le", false));
    let mut conv = Converter::new(Options::default());
    conv.opts.infile = vec![path.to_str().unwrap().to_string()];
    conv.opts.default_link_dict = String::new();
    let _ = conv.try_convert().unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(conv.resolved_encoding().name(), "utf-16le");
}

#[test]
fn every_encoding_name_round_trips_through_parse() {
    for name in [
        "utf-8",
        "cp1252",
        "iso-8859-1",
        "cp1251",
        "cp1253",
        "koi8-r",
        "utf-16le",
        "utf-16be",
        "utf-32le",
        "utf-32be",
    ] {
        let parsed = Encoding::parse(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(parsed.name(), name, "{name} did not round-trip");
    }
}

#[test]
fn a_named_wide_encoding_still_strips_a_bom() {
    // Naming an encoding is naming the *encoding*, not the absence of a mark.
    // Leaving FF FE in would emit U+FEFF as the document's first character.
    for encoding in [Encoding::Utf16Le, Encoding::Utf16Be, Encoding::Utf32Le] {
        let enc_name = encoding.name();
        let bytes = wide("bom and all\n", enc_name, true);
        let (text, _) = textrill::convert::decode_bytes_with(&bytes, encoding);
        assert_eq!(text, "bom and all\n", "{enc_name}");
        assert!(!text.starts_with('\u{FEFF}'), "{enc_name} leaked its BOM");
    }
}

// --- The save path -----------------------------------------------------------
//
// Phase 6 step 2. These are `FileTests` from `textrill-gui/tests/test_gui.py`,
// moved here because the rule under test is now the engine's: the encode side
// lives in `convert::encode`, and `files.py` is scheduled for deletion. A test
// that guards a Python helper is work waiting to be thrown away.

use textrill::convert::decode_bytes_with;
use textrill::encode::{encode, write_with, EncodeError, WriteError};

/// Read-then-write with no edits must leave the bytes alone. This is the
/// property the whole encode module exists for.
#[test]
fn an_untouched_file_round_trips_byte_for_byte() {
    let cases: &[(&[u8], Encoding)] = &[
        (b"He said \x93hi\x94 -- \x97dash\x97.\n", Encoding::Auto),
        (b"Caf\xe9 na\xefve\n", Encoding::Auto),
        ("plain ascii\n".as_bytes(), Encoding::Auto),
        // Undefined CP1252 slots must survive too, or an obscure file cannot be
        // saved at all.
        (b"a\x81b\x8dc\x8fd\x90e\x9df\n", Encoding::Auto),
    ];
    for (original, enc) in cases {
        let (text, resolved) = decode_bytes_with(original, *enc);
        assert_eq!(
            encode(&text, resolved.encoding()).unwrap(),
            *original,
            "a save with no edits changed the bytes"
        );
    }
}

/// The five bytes CP1252 leaves undefined must not break a save. Python's
/// `cp1252` codec raises on them, which is why the engine carries its own table.
#[test]
fn every_undefined_cp1252_slot_survives_a_save() {
    for b in [0x81u8, 0x8D, 0x8F, 0x90, 0x9D] {
        let raw = [b'a', b, b'b'];
        let (text, resolved) = decode_bytes_with(&raw, Encoding::Auto);
        assert_eq!(resolved.name(), "cp1252", "0x{b:02X}");
        assert_eq!(text.chars().count(), 3, "0x{b:02X}");
        assert_eq!(
            encode(&text, resolved.encoding()).unwrap(),
            raw,
            "0x{b:02X}"
        );
    }
}

/// A BOM is metadata: it must not survive into the text a front end edits, and
/// a save writes it back rather than dropping it.
#[test]
fn a_bom_is_not_content_but_is_written_back() {
    let mut raw = vec![0xFF, 0xFE];
    for u in "Hello world.\n".encode_utf16() {
        raw.extend_from_slice(&u.to_le_bytes());
    }
    let (text, resolved) = decode_bytes_with(&raw, Encoding::Auto);
    assert_eq!(resolved.name(), "utf-16le");
    assert!(!text.contains('\u{feff}'), "the mark reached the text");
    assert_eq!(encode(&text, resolved.encoding()).unwrap(), raw);
}

/// A character the encoding cannot hold must be reported, never substituted.
/// A `?` would silently alter the user's document on save.
#[test]
fn an_unrepresentable_character_is_reported_not_substituted() {
    let err = encode("Привет", Encoding::Cp1252).unwrap_err();
    match err {
        EncodeError::Unrepresentable { ch, at } => {
            assert_eq!(ch, '\u{041f}');
            assert_eq!(at, 0, "the offset lets an editor underline the character");
        }
        other => panic!("expected Unrepresentable, got {other:?}"),
    }
    // The same text is fine in an encoding that can hold it.
    assert!(encode("Привет", Encoding::Cp1251).is_ok());
}

/// `auto` describes a decision made on input; there is nothing to detect on
/// output, so it is refused rather than quietly defaulted to UTF-8.
#[test]
fn auto_is_refused_on_output() {
    assert!(matches!(
        encode("x", Encoding::Auto),
        Err(EncodeError::UnknownEncoding(_))
    ));
}

/// UTF-32LE's mark starts with UTF-16LE's. Writing the short one would produce
/// a file that decodes as UTF-16 -- text-shaped, so wrong in a way that
/// survives a glance.
#[test]
fn wide_encodings_are_written_with_the_right_bom() {
    let u32le = encode("hi", Encoding::Utf32Le).unwrap();
    assert_eq!(&u32le[..4], &[0xFF, 0xFE, 0x00, 0x00]);
    let u16le = encode("hi", Encoding::Utf16Le).unwrap();
    assert_eq!(&u16le[..2], &[0xFF, 0xFE]);
    // And the round trip agrees with the decoder.
    let (text, resolved) = decode_bytes_with(&u32le, Encoding::Utf32Le);
    assert_eq!(text, "hi");
    assert_eq!(encode(&text, resolved.encoding()).unwrap(), u32le);
}

/// A save into a directory that does not exist must fail, and must not invent
/// the directory. One typo used to create five of them.
#[test]
fn writing_to_a_missing_directory_fails_and_creates_nothing() {
    let tmp = std::env::temp_dir().join("textrill-save-test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let target = tmp.join("newtree/a/b/out.html");

    let err = write_with(&target, "<p>x</p>", Encoding::Utf8).unwrap_err();
    assert!(
        matches!(err, WriteError::NoSuchDirectory(_)),
        "expected NoSuchDirectory, got {err:?}"
    );
    assert!(
        !tmp.join("newtree").exists(),
        "the directory was created anyway"
    );
    assert_eq!(
        std::fs::read_dir(&tmp).unwrap().count(),
        0,
        "nothing left behind"
    );
}

/// Dropping the mkdir must not stop a save into a directory that exists.
#[test]
fn writing_into_an_existing_directory_works() {
    let tmp = std::env::temp_dir().join("textrill-save-test-ok");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let target = tmp.join("out.html");

    write_with(&target, "<p>x</p>", Encoding::Utf8).unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "<p>x</p>");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// A UTF-16 file holding an emoji must decode to the emoji. Two U+FFFD in its
/// place is the failure this guards: surrogate code units are half a character,
/// and feeding them to `char::from_u32` one at a time replaces every character
/// outside the BMP.
#[test]
fn an_astral_character_survives_a_utf16_round_trip() {
    for enc in [Encoding::Utf16Le, Encoding::Utf16Be] {
        let (text, resolved) = {
            let bytes = encode("\u{1F600} ok", enc).unwrap();
            decode_bytes_with(&bytes, enc)
        };
        assert_eq!(text, "\u{1F600} ok", "{enc:?}");
        assert!(
            !text.contains('\u{fffd}'),
            "{enc:?} produced a replacement char"
        );
        let again = encode(&text, resolved.encoding()).unwrap();
        let (_, r2) = decode_bytes_with(&again, enc);
        assert_eq!(r2.encoding(), resolved.encoding());
    }
    // UTF-32 holds whole code points, so it must NOT surrogate-pair them.
    let bytes = encode("\u{1F600}", Encoding::Utf32Le).unwrap();
    assert_eq!(bytes.len(), 4 + 4, "BOM plus one 4-byte code point");
    assert_eq!(decode_bytes_with(&bytes, Encoding::Utf32Le).0, "\u{1F600}");
}

/// A lone surrogate is a malformed file, not half of a character, so it becomes
/// U+FFFD and the following unit still decodes on its own.
#[test]
fn a_lone_surrogate_is_a_replacement_not_half_a_character() {
    let raw = b"\xff\xfe\x3d\xd8\x41\x00"; // BOM, high surrogate, 'A'
    let (text, _) = decode_bytes_with(raw, Encoding::Auto);
    assert_eq!(text, "\u{FFFD}A");
}
