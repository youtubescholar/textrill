//! The textrill conversion engine.
//!
//! A faithful port of HTML::TextToHTML v3.0's `process_para`,
//! `process_chunk`, `txt2html`, `do_file_start` and the associated helper
//! subroutines.

use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use fancy_regex::Regex;

use crate::chars;
use crate::links::{self, LinkParser};
use crate::options::{Encoding, Options, SingleByte};

/// One or more inputs that could not be opened, and the output built from
/// whatever *was* readable (A9). The caller should write that output if it has
/// anywhere to put it, and exit non-zero regardless.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableInput {
    /// The input paths that could not be opened, in the order given.
    pub unreadable: Vec<String>,
    /// The document produced from the inputs that were readable. Empty when
    /// none of them were.
    pub out: String,
}

/// How [`read_any_file`] resolved a file's bytes, so a caller can report it
/// instead of guessing. `Auto` records which of the two decodings actually
/// happened, because "auto" that silently becomes Latin-1 is indistinguishable
/// from "auto" that silently became UTF-8 — and the difference is what P7.1's
/// output divergence is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    /// The bytes were valid UTF-8 and were decoded as such.
    Utf8,
    /// A single-byte encoding, by guess (`Auto`) or because it was named.
    Single(SingleByte),
    /// UTF-16 or UTF-32, little- or big-endian.
    ///
    /// These are carried separately from [`SingleByte`] rather than folded into
    /// one "other" case so a status bar can say `utf-16le` without the caller
    /// having to re-derive it.
    Wide(Wide),
}

/// A multi-byte encoding, all of which are UTF-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wide {
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
}

impl Wide {
    fn from_encoding(encoding: Encoding) -> Option<Self> {
        match encoding {
            Encoding::Utf16Le => Some(Wide::Utf16Le),
            Encoding::Utf16Be => Some(Wide::Utf16Be),
            Encoding::Utf32Le => Some(Wide::Utf32Le),
            Encoding::Utf32Be => Some(Wide::Utf32Be),
            _ => None,
        }
    }
}

impl Resolved {
    /// A name for status bars, log lines and `--verbose` output.
    pub fn name(self) -> &'static str {
        match self {
            Resolved::Utf8 => "utf-8",
            Resolved::Single(sb) => sb.name(),
            Resolved::Wide(w) => match w {
                Wide::Utf16Le => "utf-16le",
                Wide::Utf16Be => "utf-16be",
                Wide::Utf32Le => "utf-32le",
                Wide::Utf32Be => "utf-32be",
            },
        }
    }

    /// How loudly this result deserves to be reported, when several inputs in
    /// one run resolved differently and only one can be named.
    ///
    /// UTF-8 is the silent default and ranks lowest, so an ASCII file cannot
    /// hijack the report. A single-byte legacy encoding ranks above it, because
    /// that is a *guess* and the user most wants to know about it -- a CP1251
    /// file read as CP1252 is the failure this reporting exists to make visible.
    /// UTF-16/32 outranks both: it means the bytes needed structural detection to
    /// be understood at all, so it is the most surprising thing that could have
    /// happened, and the one most worth naming.
    fn notice_rank(self) -> u8 {
        match self {
            Resolved::Utf8 => 0,
            Resolved::Single(_) => 1,
            Resolved::Wide(_) => 2,
        }
    }

    /// The [`Encoding`] this is, for callers that want to round-trip it back
    /// through the option layer.
    pub fn encoding(self) -> Encoding {
        match self {
            Resolved::Utf8 => Encoding::Utf8,
            Resolved::Single(sb) => sb.encoding(),
            Resolved::Wide(Wide::Utf16Le) => Encoding::Utf16Le,
            Resolved::Wide(Wide::Utf16Be) => Encoding::Utf16Be,
            Resolved::Wide(Wide::Utf32Le) => Encoding::Utf32Le,
            Resolved::Wide(Wide::Utf32Be) => Encoding::Utf32Be,
        }
    }
}

/// Read a text file for conversion, reporting which encoding was used.
///
/// Perl reads raw bytes and keeps 8-bit characters intact; mimic that by
/// decoding UTF-8 when the bytes are valid UTF-8, and falling back to a
/// single-byte encoding otherwise.
///
/// The fallback is **CP1252**, not Latin-1, and this is P7.1's root cause
/// rather than a stylistic choice. The two agree on `0x00`-`0x7F` and on
/// `0xA0`-`0xFF` — which is the entire range Latin-1 defines — and differ
/// only on `0x80`-`0x9F`, where Latin-1 holds C1 control characters and CP1252
/// holds the typographic punctuation every Windows text file actually uses.
///
/// Decoding as Latin-1 made `demoronize` unreachable on exactly the files it
/// exists to serve. [`crate::chars::demoronize_char`] maps U+201C, U+2019,
/// U+2013 and the rest — the CP1252 code points. A Latin-1 decode of the same
/// byte produces U+009C, U+0092, U+0093, none of which are in the table, so
/// every substitution silently did nothing. Measured on a CP1252 file
/// containing `0x93 0x94 0x96 0x97`:
///
/// ```text
/// Perl emits the raw bytes; a browser reading the result as CP1252 shows
///   "hello" and dash -- .
/// Latin-1 fallback emitted UTF-8 C2 93, C2 94, C2 96, C2 97 -- C1 control
///   characters, which render as nothing at all.
/// ```
///
/// CP1252 also defines `0x81`, `0x8D`, `0x8F`, `0x90` and `0x9D` as
/// *undefined*. They are left as the Latin-1 control character, matching what
/// browsers do, because inventing a glyph for them would be a guess and a
/// visible one.
pub fn read_any_file_with_encoding(path: &str) -> Option<(String, Resolved)> {
    read_with(path, Encoding::Auto)
}

/// [`read_any_file_with_encoding`], but honouring an explicit `--encoding`.
///
/// The probe in [`Encoding::Auto`] cannot tell a CP1252 file whose bytes happen
/// to be valid UTF-8 from a genuine UTF-8 file, and that is not a rare edge: a
/// short CP1252 document can be valid UTF-8 by accident. `Utf8` and `Cp1252`
/// exist so the caller can settle it, which is why this is a separate function
/// rather than a parameter bolted onto the other.
pub fn read_with(path: &str, encoding: Encoding) -> Option<(String, Resolved)> {
    let bytes = std::fs::read(path).ok()?;
    Some(decode_bytes_with(&bytes, encoding))
}

/// Decode `bytes` under `encoding`. Split out from [`read_with`] because the
/// GUI needs the same rule without a file, and because it is directly testable
/// that way — the detection order is the substance of this function and it
/// should not only be reachable through the filesystem.
pub fn decode_bytes_with(bytes: &[u8], encoding: Encoding) -> (String, Resolved) {
    match encoding {
        Encoding::Auto => detect(bytes),
        // Lossy on purpose, and only reachable when the caller has declared the
        // encoding. `String::from_utf8_lossy` substitutes U+FFFD rather than
        // failing, because a converter that aborts on one bad byte is not
        // useful; the substitution is visible in the output instead of silent.
        Encoding::Utf8 => (String::from_utf8_lossy(bytes).into_owned(), Resolved::Utf8),
        wide @ (Encoding::Utf16Le | Encoding::Utf16Be | Encoding::Utf32Le | Encoding::Utf32Be) => {
            // An explicitly named encoding still has to cope with a BOM: the
            // user names the *encoding*, not the absence of a mark, and leaving
            // the mark in would emit U+FEFF as the first character of the
            // document.
            let skip = bom(bytes).map_or(0, |(_, n)| n);
            decode_utf16_or_32(bytes, wide, skip)
        }
        other => {
            let sb = other.single_byte().unwrap_or(SingleByte::Cp1252);
            (
                bytes.iter().map(|&b| single_byte_char(sb, b)).collect(),
                Resolved::Single(sb),
            )
        }
    }
}

/// `Encoding::Auto`, in the order the documentation gives: BOM, then NUL
/// structure, then UTF-8 validity, then the single-byte guess.
fn detect(bytes: &[u8]) -> (String, Resolved) {
    if let Some((encoding, skip)) = bom(bytes) {
        // A UTF-8 BOM is not an encoding change -- everything after it is UTF-8
        // either way -- so it is deliberately kept in the text. The UTF-16 and
        // UTF-32 marks are metadata and go.
        if encoding != Encoding::Utf8 {
            return decode_utf16_or_32(bytes, encoding, skip);
        }
    }
    if let Some(encoding) = sniff_utf16_or_32(bytes) {
        return decode_utf16_or_32(bytes, encoding, 0);
    }
    match String::from_utf8(bytes.to_vec()) {
        Ok(s) => (s, Resolved::Utf8),
        Err(e) => (
            e.into_bytes().into_iter().map(cp1252_char).collect(),
            Resolved::Single(SingleByte::Cp1252),
        ),
    }
}

/// The byte-order mark, if there is one, as the encoding it declares and how
/// many bytes it occupies.
///
/// A BOM is a declaration by the writer and outranks every heuristic, which is
/// why it is checked first and why the pre-P7.4 code was wrong to treat `FF FE`
/// as a decode error: those two bytes are a guarantee, not garbage.
fn bom(bytes: &[u8]) -> Option<(Encoding, usize)> {
    // UTF-32LE's BOM starts with UTF-16LE's, so the longer mark must be tested
    // first or every UTF-32LE file with a BOM decodes as UTF-16LE and comes out
    // as pairs of Latin-1 characters -- which is a wrong answer that still looks
    // like text, the most expensive kind.
    for (encoding, mark) in [
        (Encoding::Utf32Le, &[0xFF, 0xFE, 0x00, 0x00][..]),
        (Encoding::Utf32Be, &[0x00, 0x00, 0xFE, 0xFF][..]),
        (Encoding::Utf16Le, &[0xFF, 0xFE][..]),
        (Encoding::Utf16Be, &[0xFE, 0xFF][..]),
        (Encoding::Utf8, &[0xEF, 0xBB, 0xBF][..]),
    ] {
        if bytes.starts_with(mark) {
            return Some((encoding, mark.len()));
        }
    }
    None
}

/// Structural UTF-16 detection for BOM-less files.
///
/// This is a heuristic, and the ordering is the substance of it. UTF-16LE text
/// is *valid UTF-8* whenever every code unit is below `0x80` -- which is ASCII
/// prose, i.e. most documents -- so a UTF-8 validity check cannot distinguish it
/// and must not run first. What UTF-16 does have is a NUL every second byte, in
/// one alignment or the other, and no other encoding this port decodes produces
/// that by accident.
///
/// UTF-32 is deliberately not inferred. Its three NULs per unit are distinctive,
/// but a document with no NULs at all is ambiguous in a way that does not
/// resolve, and a confident wrong guess is worse than the mojibake. UTF-32
/// without a BOM is read as UTF-16, which at least gets the ASCII range right.
fn sniff_utf16_or_32(bytes: &[u8]) -> Option<Encoding> {
    const WINDOW: usize = 4096;
    let sample = &bytes[..bytes.len().min(WINDOW)];
    if sample.len() < 8 {
        return None;
    }
    // Counted from the bytes rather than derived from `len / 2`: an odd-length
    // sample (a truncated final code unit is normal in real files) puts one
    // more byte on the even side, and a budget that silently overcounts one
    // alignment would loosen exactly the threshold that keeps this from firing
    // on ordinary text.
    let nulls_even = sample.iter().step_by(2).filter(|&&b| b == 0).count();
    let nulls_odd = sample
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|&&b| b == 0)
        .count();
    let even = nulls_even + sample.iter().step_by(2).filter(|&&b| b != 0).count();
    let odd = nulls_odd
        + sample
            .iter()
            .skip(1)
            .step_by(2)
            .filter(|&&b| b != 0)
            .count();
    // The threshold is 1/8, which is a measured number rather than a round one.
    // An earlier draft used 2/3, on the assumption that real prose is mostly
    // ASCII and therefore mostly NUL; measuring the NUL fraction per alignment
    // on representative text says otherwise:
    //
    //   UTF-16LE, ASCII prose      1.00     UTF-16LE, Russian prose   0.21
    //   UTF-16LE, Greek prose      0.20     UTF-16LE, dense Cyrillic 0.15
    //   UTF-16BE, ASCII prose      1.00     UTF-16LE, short Russian   0.25
    //
    // Two thirds therefore catches only the English case, which is the case a
    // validity check already gets *wrong* and the easy one to miss. Against it,
    // nothing that ought to stay UTF-8 comes close:
    //
    //   ASCII document   0.00   UTF-8 Russian  0.00   UTF-8 CJK  0.00
    //   UTF-8 with NULs  0.04   HTML          0.00   raw bytes  0.01
    //
    // So 1/8 sits with a 3x margin above the worst negative and below the
    // weakest positive. The cross-alignment factor rejects files that merely
    // contain a few scattered NULs, where the two counts would be similar.
    //
    // The residual limit is real and not hidden by the threshold: BOM-less
    // UTF-16 with very little ASCII in it -- 0.15 is the floor for real prose,
    // and a document of nothing but Cyrillic letters reaches 0 -- is not
    // detectable by any rule short of a statistical model. That is what
    // `--encoding utf-16le` is for.
    if nulls_even * 8 > even && nulls_even > nulls_odd * 4 {
        Some(Encoding::Utf16Be)
    } else if nulls_odd * 8 > odd && nulls_odd > nulls_even * 4 {
        Some(Encoding::Utf16Le)
    } else {
        None
    }
}

/// Decode UTF-16 or UTF-32, skipping `skip` leading bytes (a BOM, normally).
///
/// A lone surrogate is not a character; `char::from_u32` rejects it and U+FFFD
/// is what every other decoder substitutes, so a malformed or truncated file
/// yields visible replacement characters instead of a panic. Trailing bytes
/// that do not make a whole code unit are dropped rather than padded, since the
/// alternative would invent a character from half a unit.
/// Turn code units into text, joining UTF-16 surrogate pairs.
///
/// `char::from_u32` returns `None` for a surrogate code point, because a
/// surrogate is half a character and not one. Feeding UTF-16 units straight to
/// it therefore replaced every astral character with *two* U+FFFD: a file
/// containing a single emoji decoded as a pair of replacement glyphs. That was
/// a real defect, not a theoretical one -- anything outside the BMP is
/// unrepresentable as one UTF-16 unit, so the failure hits all of them.
///
/// A *lone* surrogate is still a replacement character, which is the correct
/// reading of a file that is genuinely malformed rather than one that is merely
/// using the encoding properly.
fn units_to_string(units: Vec<u32>, wide: Wide) -> String {
    if wide != Wide::Utf16Le && wide != Wide::Utf16Be {
        // UTF-32 holds whole code points, so there is nothing to pair. Values
        // above U+10FFFF are still not characters, and `from_u32` says so.
        return units
            .into_iter()
            .map(|u| char::from_u32(u).unwrap_or('\u{FFFD}'))
            .collect();
    }
    const HIGH: std::ops::RangeInclusive<u32> = 0xD800..=0xDBFF;
    const LOW: std::ops::RangeInclusive<u32> = 0xDC00..=0xDFFF;
    let mut out = String::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        if HIGH.contains(&u) {
            // A high surrogate followed by a low one is one character. Anything
            // else -- a high surrogate at the end of the file, or followed by a
            // normal unit -- is malformed, so the surrogate becomes U+FFFD and
            // the next unit is decoded on its own.
            match units.get(i + 1) {
                Some(&next) if LOW.contains(&next) => {
                    let cp = 0x1_0000 + ((u - 0xD800) << 10) + (next - 0xDC00);
                    out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                    i += 2;
                    continue;
                }
                _ => {
                    out.push('\u{FFFD}');
                    i += 1;
                    continue;
                }
            }
        }
        out.push(char::from_u32(u).unwrap_or('\u{FFFD}'));
        i += 1;
    }
    out
}

fn decode_utf16_or_32(bytes: &[u8], encoding: Encoding, skip: usize) -> (String, Resolved) {
    let wide = Wide::from_encoding(encoding)
        .unwrap_or_else(|| unreachable!("decode_utf16_or_32 is only reached for UTF-16/32"));
    let body = bytes.get(skip..).unwrap_or_default();
    // `as_chunks` rather than `chunks_exact` for the lint, but the semantics
    // wanted here are `chunks_exact`'s: a trailing fragment shorter than one code
    // unit is *dropped*, not zero-padded, because padding would invent a
    // character from half a unit.
    let units: Vec<u32> = match wide {
        Wide::Utf16Le => body
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u32::from(u16::from_le_bytes([c[0], c[1]])))
            .collect(),
        Wide::Utf16Be => body
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u32::from(u16::from_be_bytes([c[0], c[1]])))
            .collect(),
        Wide::Utf32Le => body
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
        Wide::Utf32Be => body
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_be_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    };
    (units_to_string(units, wide), Resolved::Wide(wide))
}

// GENERATED by tests/gen_encoding_tables.py -- do not edit by hand.

static LATIN1_HIGH: [u32; 128] = [
    0x0080, 0x0081, 0x0082, 0x0083, 0x0084, 0x0085, 0x0086, 0x0087, 0x0088, 0x0089, 0x008a, 0x008b,
    0x008c, 0x008d, 0x008e, 0x008f, 0x0090, 0x0091, 0x0092, 0x0093, 0x0094, 0x0095, 0x0096, 0x0097,
    0x0098, 0x0099, 0x009a, 0x009b, 0x009c, 0x009d, 0x009e, 0x009f, 0x00a0, 0x00a1, 0x00a2, 0x00a3,
    0x00a4, 0x00a5, 0x00a6, 0x00a7, 0x00a8, 0x00a9, 0x00aa, 0x00ab, 0x00ac, 0x00ad, 0x00ae, 0x00af,
    0x00b0, 0x00b1, 0x00b2, 0x00b3, 0x00b4, 0x00b5, 0x00b6, 0x00b7, 0x00b8, 0x00b9, 0x00ba, 0x00bb,
    0x00bc, 0x00bd, 0x00be, 0x00bf, 0x00c0, 0x00c1, 0x00c2, 0x00c3, 0x00c4, 0x00c5, 0x00c6, 0x00c7,
    0x00c8, 0x00c9, 0x00ca, 0x00cb, 0x00cc, 0x00cd, 0x00ce, 0x00cf, 0x00d0, 0x00d1, 0x00d2, 0x00d3,
    0x00d4, 0x00d5, 0x00d6, 0x00d7, 0x00d8, 0x00d9, 0x00da, 0x00db, 0x00dc, 0x00dd, 0x00de, 0x00df,
    0x00e0, 0x00e1, 0x00e2, 0x00e3, 0x00e4, 0x00e5, 0x00e6, 0x00e7, 0x00e8, 0x00e9, 0x00ea, 0x00eb,
    0x00ec, 0x00ed, 0x00ee, 0x00ef, 0x00f0, 0x00f1, 0x00f2, 0x00f3, 0x00f4, 0x00f5, 0x00f6, 0x00f7,
    0x00f8, 0x00f9, 0x00fa, 0x00fb, 0x00fc, 0x00fd, 0x00fe, 0x00ff,
];

static CP1252_HIGH: [u32; 128] = [
    0x20ac, 0, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160, 0x2039,
    0x0152, 0, 0x017d, 0, 0, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x02dc,
    0x2122, 0x0161, 0x203a, 0x0153, 0, 0x017e, 0x0178, 0x00a0, 0x00a1, 0x00a2, 0x00a3, 0x00a4,
    0x00a5, 0x00a6, 0x00a7, 0x00a8, 0x00a9, 0x00aa, 0x00ab, 0x00ac, 0x00ad, 0x00ae, 0x00af, 0x00b0,
    0x00b1, 0x00b2, 0x00b3, 0x00b4, 0x00b5, 0x00b6, 0x00b7, 0x00b8, 0x00b9, 0x00ba, 0x00bb, 0x00bc,
    0x00bd, 0x00be, 0x00bf, 0x00c0, 0x00c1, 0x00c2, 0x00c3, 0x00c4, 0x00c5, 0x00c6, 0x00c7, 0x00c8,
    0x00c9, 0x00ca, 0x00cb, 0x00cc, 0x00cd, 0x00ce, 0x00cf, 0x00d0, 0x00d1, 0x00d2, 0x00d3, 0x00d4,
    0x00d5, 0x00d6, 0x00d7, 0x00d8, 0x00d9, 0x00da, 0x00db, 0x00dc, 0x00dd, 0x00de, 0x00df, 0x00e0,
    0x00e1, 0x00e2, 0x00e3, 0x00e4, 0x00e5, 0x00e6, 0x00e7, 0x00e8, 0x00e9, 0x00ea, 0x00eb, 0x00ec,
    0x00ed, 0x00ee, 0x00ef, 0x00f0, 0x00f1, 0x00f2, 0x00f3, 0x00f4, 0x00f5, 0x00f6, 0x00f7, 0x00f8,
    0x00f9, 0x00fa, 0x00fb, 0x00fc, 0x00fd, 0x00fe, 0x00ff,
];
// CP1252: undefined at 0x81, 0x8d, 0x8f, 0x90, 0x9d

static CP1251_HIGH: [u32; 128] = [
    0x0402, 0x0403, 0x201a, 0x0453, 0x201e, 0x2026, 0x2020, 0x2021, 0x20ac, 0x2030, 0x0409, 0x2039,
    0x040a, 0x040c, 0x040b, 0x040f, 0x0452, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014,
    0, 0x2122, 0x0459, 0x203a, 0x045a, 0x045c, 0x045b, 0x045f, 0x00a0, 0x040e, 0x045e, 0x0408,
    0x00a4, 0x0490, 0x00a6, 0x00a7, 0x0401, 0x00a9, 0x0404, 0x00ab, 0x00ac, 0x00ad, 0x00ae, 0x0407,
    0x00b0, 0x00b1, 0x0406, 0x0456, 0x0491, 0x00b5, 0x00b6, 0x00b7, 0x0451, 0x2116, 0x0454, 0x00bb,
    0x0458, 0x0405, 0x0455, 0x0457, 0x0410, 0x0411, 0x0412, 0x0413, 0x0414, 0x0415, 0x0416, 0x0417,
    0x0418, 0x0419, 0x041a, 0x041b, 0x041c, 0x041d, 0x041e, 0x041f, 0x0420, 0x0421, 0x0422, 0x0423,
    0x0424, 0x0425, 0x0426, 0x0427, 0x0428, 0x0429, 0x042a, 0x042b, 0x042c, 0x042d, 0x042e, 0x042f,
    0x0430, 0x0431, 0x0432, 0x0433, 0x0434, 0x0435, 0x0436, 0x0437, 0x0438, 0x0439, 0x043a, 0x043b,
    0x043c, 0x043d, 0x043e, 0x043f, 0x0440, 0x0441, 0x0442, 0x0443, 0x0444, 0x0445, 0x0446, 0x0447,
    0x0448, 0x0449, 0x044a, 0x044b, 0x044c, 0x044d, 0x044e, 0x044f,
];
// CP1251: undefined at 0x98

static CP1253_HIGH: [u32; 128] = [
    0x20ac, 0, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0, 0x2030, 0, 0x2039, 0, 0, 0, 0, 0,
    0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0, 0x2122, 0, 0x203a, 0, 0, 0, 0,
    0x00a0, 0x0385, 0x0386, 0x00a3, 0x00a4, 0x00a5, 0x00a6, 0x00a7, 0x00a8, 0x00a9, 0, 0x00ab,
    0x00ac, 0x00ad, 0x00ae, 0x2015, 0x00b0, 0x00b1, 0x00b2, 0x00b3, 0x0384, 0x00b5, 0x00b6, 0x00b7,
    0x0388, 0x0389, 0x038a, 0x00bb, 0x038c, 0x00bd, 0x038e, 0x038f, 0x0390, 0x0391, 0x0392, 0x0393,
    0x0394, 0x0395, 0x0396, 0x0397, 0x0398, 0x0399, 0x039a, 0x039b, 0x039c, 0x039d, 0x039e, 0x039f,
    0x03a0, 0x03a1, 0, 0x03a3, 0x03a4, 0x03a5, 0x03a6, 0x03a7, 0x03a8, 0x03a9, 0x03aa, 0x03ab,
    0x03ac, 0x03ad, 0x03ae, 0x03af, 0x03b0, 0x03b1, 0x03b2, 0x03b3, 0x03b4, 0x03b5, 0x03b6, 0x03b7,
    0x03b8, 0x03b9, 0x03ba, 0x03bb, 0x03bc, 0x03bd, 0x03be, 0x03bf, 0x03c0, 0x03c1, 0x03c2, 0x03c3,
    0x03c4, 0x03c5, 0x03c6, 0x03c7, 0x03c8, 0x03c9, 0x03ca, 0x03cb, 0x03cc, 0x03cd, 0x03ce, 0,
];
// CP1253: undefined at 0x81, 0x88, 0x8a, 0x8c, 0x8d, 0x8e, 0x8f, 0x90, 0x98, 0x9a, 0x9c, 0x9d, 0x9e, 0x9f, 0xaa, 0xd2, 0xff

static KOI8R_HIGH: [u32; 128] = [
    0x2500, 0x2502, 0x250c, 0x2510, 0x2514, 0x2518, 0x251c, 0x2524, 0x252c, 0x2534, 0x253c, 0x2580,
    0x2584, 0x2588, 0x258c, 0x2590, 0x2591, 0x2592, 0x2593, 0x2320, 0x25a0, 0x2219, 0x221a, 0x2248,
    0x2264, 0x2265, 0x00a0, 0x2321, 0x00b0, 0x00b2, 0x00b7, 0x00f7, 0x2550, 0x2551, 0x2552, 0x0451,
    0x2553, 0x2554, 0x2555, 0x2556, 0x2557, 0x2558, 0x2559, 0x255a, 0x255b, 0x255c, 0x255d, 0x255e,
    0x255f, 0x2560, 0x2561, 0x0401, 0x2562, 0x2563, 0x2564, 0x2565, 0x2566, 0x2567, 0x2568, 0x2569,
    0x256a, 0x256b, 0x256c, 0x00a9, 0x044e, 0x0430, 0x0431, 0x0446, 0x0434, 0x0435, 0x0444, 0x0433,
    0x0445, 0x0438, 0x0439, 0x043a, 0x043b, 0x043c, 0x043d, 0x043e, 0x043f, 0x044f, 0x0440, 0x0441,
    0x0442, 0x0443, 0x0436, 0x0432, 0x044c, 0x044b, 0x0437, 0x0448, 0x044d, 0x0449, 0x0447, 0x044a,
    0x042e, 0x0410, 0x0411, 0x0426, 0x0414, 0x0415, 0x0424, 0x0413, 0x0425, 0x0418, 0x0419, 0x041a,
    0x041b, 0x041c, 0x041d, 0x041e, 0x041f, 0x042f, 0x0420, 0x0421, 0x0422, 0x0423, 0x0416, 0x0412,
    0x042c, 0x042b, 0x0417, 0x0428, 0x042d, 0x0429, 0x0427, 0x042a,
];

/// The high half of each single-byte encoding, indexed by `byte - 0x80`.
static ENCODING_TABLES: &[(SingleByte, &[u32; 128])] = &[
    (SingleByte::Latin1, &LATIN1_HIGH),
    (SingleByte::Cp1252, &CP1252_HIGH),
    (SingleByte::Cp1251, &CP1251_HIGH),
    (SingleByte::Cp1253, &CP1253_HIGH),
    (SingleByte::Koi8R, &KOI8R_HIGH),
];

fn cp1252_char(b: u8) -> char {
    single_byte_char(SingleByte::Cp1252, b)
}

/// The high half of `enc`, indexed by `byte - 0x80`.
///
/// Public so [`crate::encode`] can build the *reverse* mapping from the same
/// table rather than keeping a second copy of it. A second table would be free
/// to drift from this one, and a drifted encoder corrupts a file on save, which
/// is worse than not having the feature.
pub fn encoding_table(enc: SingleByte) -> &'static [u32; 128] {
    ENCODING_TABLES
        .iter()
        .find(|(e, _)| *e == enc)
        .map(|(_, t)| *t)
        .unwrap_or_else(|| unreachable!("every SingleByte has a table"))
}

fn single_byte_char(enc: SingleByte, b: u8) -> char {
    if b < 0x80 {
        return b as char;
    }
    let cp = encoding_table(enc)[(b - 0x80) as usize];
    if cp == 0 {
        // Undefined in this encoding: keep the Latin-1 reading so the result is
        // still a character rather than a panic or a replacement glyph.
        b as char
    } else {
        char::from_u32(cp).unwrap_or(b as char)
    }
}

/// Read a text file for conversion, discarding which encoding was resolved.
pub fn read_any_file(path: &str) -> Option<String> {
    read_any_file_with_encoding(path).map(|(text, _)| text)
}

/// [`read_any_file`], honouring an explicit `--encoding`. Used for the
/// `append_file` / `append_head` / `prepend_file` options, which are read
/// during the same run and so should obey the same encoding rule as the input
/// rather than silently re-probing each one.
pub fn read_file_with(path: &str, encoding: Encoding) -> Option<String> {
    read_with(path, encoding).map(|(text, _)| text)
}

/// Chop trailing whitespace and a DOS CR, i.e. what the Perl
/// `s/[ \t]*\x0D$//` did.
///
/// This started life as a regular expression, and it is still written as
/// `[ \t]*\x0D$` in the Perl source. It cannot stay one here: `links.rs`
/// rewrites every `$` into the lookahead `(?=\n?$)`, which takes the pattern
/// off fancy-regex's fast automaton and onto the backtracker, where a large
/// paragraph exhausts the step budget and panics. A 1 MB paragraph with no
/// blank lines is enough.
///
/// It is anchored at the end of the paragraph, so it is just string surgery,
/// and string surgery has no budget to exhaust.
///
/// The subtlety is that Perl's `$` means "end of text, *or* before a single
/// trailing newline", so this cannot be `trim_end()` followed by a check for
/// `\r`: a trailing `\r\n` has to be recognised as well.
fn chop_trailing_cr(s: &str) -> String {
    let (body, had_nl) = match s.strip_suffix('\n') {
        Some(b) => (b, true),
        None => (s, false),
    };
    if !body.ends_with('\r') {
        return s.to_string(); // no match, so the paragraph is left alone
    }
    // The `[ \t]*` sits *before* the CR, so the CR comes off first and the
    // spaces and tabs are trimmed afterwards. Trimming first leaves a stray CR.
    let trimmed = body[..body.len() - 1].trim_end_matches([' ', '\t']);
    let mut out = String::with_capacity(trimmed.len() + 1);
    out.push_str(trimmed);
    if had_nl {
        out.push('\n');
    }
    out
}

/// Chop leading whitespace and a DOS CR, i.e. what the Perl
/// `s/^[ \t]*\x0D//` did. See [`chop_trailing_cr`] for why this is not a
/// regular expression.
///
/// The leading pattern only removes anything when a CR actually follows the run
/// of whitespace. Trimming the leading whitespace unconditionally would
/// corrupt every indented paragraph in the document.
fn chop_leading_cr(s: &str) -> String {
    match s.trim_start_matches([' ', '\t']).strip_prefix('\r') {
        Some(rest) => rest.to_string(),
        None => s.to_string(),
    }
}

// mode bits
pub const NONE: u32 = 0;
pub const LIST: u32 = 1;
pub const HRULE: u32 = 2;
pub const PAR: u32 = 4;
pub const PRE: u32 = 8;
pub const END: u32 = 16;
pub const BREAK: u32 = 32;
pub const HEADER: u32 = 64;
pub const MAILHEADER: u32 = 128;
pub const MAILQUOTE: u32 = 256;
pub const CAPS: u32 = 512;
pub const LINK: u32 = 1024;
pub const PRE_EXPLICIT: u32 = 2048;
pub const TABLE: u32 = 4096;
pub const IND_BREAK: u32 = 8192;
pub const LIST_START: u32 = 16384;
pub const LIST_ITEM: u32 = 32768;

const OL: u8 = 1;
const UL: u8 = 2;
const DL: u8 = 3;

const TAB_ALIGN: u32 = 1;
const TAB_PGSQL: u32 = 2;
const TAB_BORDER: u32 = 3;
const TAB_DELIM: u32 = 4;

const TAG_START: u8 = 1;
const TAG_END: u8 = 2;
const TAG_EMPTY: u8 = 3;

// P1.1. The generator meta is provenance: it names what produced this file.
// textrill produced it, and stating otherwise is a false claim in every
// document the tool writes -- including the ones a reader may inspect years
// later to work out what made them.
//
// This is deliberately NOT the Perl module's name. It is also deliberately not
// an attribution mechanism: the credit for HTML::TextToHTML belongs in
// LICENSE, where it is, permanently and correctly. Provenance and attribution
// are different claims, and conflating them is what put "HTML::TextToHTML" in a
// place it does not belong.
//
// The version comes from the crate rather than being written out, so bumping
// Cargo.toml cannot leave a stale string in every generated document.
const PROG: &str = "textrill";
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn subtract_modes(v: u32, mask: u32) -> u32 {
    (v | mask) - mask
}

fn is_perl_space(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\x0c' || c == '\x0b'
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Split a string into lines the way `split(/^/, $para)` does
/// (each line keeps its trailing newline, except a final un-terminated one).
fn split_lines(s: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        cur.push(c);
        if c == '\n' {
            lines.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

pub struct Converter {
    pub opts: Options,
    links: LinkParser,
    tags: Vec<String>,
    mode: u32,
    listnum: usize,
    list_nice_indent: String,
    list_indent: Vec<usize>,
    list: Vec<u8>,
    list_prefix: Vec<String>,
    number_match: String,
    term_match: String,
    heading_styles: HashMap<String, usize>,
    num_heading_styles: usize,
    heading_count: Vec<usize>,
    non_header_anchor: usize,
    prev_para_action: u32,
    preformat_enabled: bool,
    re_cache: HashMap<String, Regex>,
    print_count: u32,
    /// P7.3. Set by `try_convert`; read back with `resolved_encoding()`.
    resolved: Resolved,
    /// P5.5. The text between `<head>` and `</head>` the last `do_file_start`
    /// emitted, exposed to a template as `{{textrill:head}}`.
    head_inner: String,
    /// P5.5. The final, escaped document title, for `{{textrill:title}}`.
    title_text: String,
    /// P5.5. The template file's contents, read once in [`Converter::new`].
    /// Empty means no template is active.
    template_text: String,
    /// Set when a note or glossary set is unusable, so the caller can report it
    /// and write nothing.
    ///
    /// Conversion cannot return this as an error without changing the signature
    /// the reference and the Python bindings use, and it must not be reported by
    /// printing and carrying on: a document whose citations lost a definition
    /// would otherwise reach disk with a dangling `[1]`.
    pub notes_error: Option<String>,
    /// P5.5. True when `template_text` is a whole-document template rather than
    /// a wrapper fragment.
    document_template: bool,
    /// A11. The scheme policy, built once in [`Converter::new`] so the scrub
    /// does not rebuild it per paragraph. Kept beside `links` because the two
    /// must agree; `links::load_links` is given this same value.
    url_policy: crate::urlscheme::UrlPolicy,
    /// A11. Schemes already reported on standard error, so a document with two
    /// hundred `javascript:` links says so once instead of two hundred times.
    dropped_schemes: Vec<String>,
}

impl Converter {
    pub fn new(mut opts: Options) -> Self {
        opts.deal_with_options();
        let preformat_enabled = opts.endpreformat_trigger_lines != 0 || opts.use_preformat_marker;

        let mut heading_styles = HashMap::new();
        let mut num_heading_styles = 0;
        if opts.use_mosaic_header {
            for s in ["*", "=", "+", "-", "~", "."] {
                num_heading_styles += 1;
                heading_styles.insert(s.to_string(), num_heading_styles);
            }
        }

        // P5.5/S4. Resolve the template once, at construction, so conversion
        // cannot fail on IO. `Options::validate` has already reported a missing
        // or malformed template, or an unknown shipped template, on the
        // command-line path; a library caller that skips validation simply gets
        // no templating when the template is unresolvable, rather than an error
        // part-way through output.
        let (template_text, document_template) = match opts.template_source() {
            Ok((2, _, _)) => (String::new(), false),
            Ok((whole, body, _)) => (body, whole == 1),
            Err(_) => (String::new(), false),
        };

        // A11. One policy, shared with the dictionary loader, so a rule kept at
        // load and a paragraph scrubbed later can never disagree.
        let url_policy = opts.url_policy();
        let links = links::load_links(&opts);
        let number_match_default = if opts.bullets_ordered.is_empty() {
            r"(\d+|[A-Za-z_])".to_string()
        } else {
            format!(r"(\d+|[A-Za-z]|[{0}])", class_body(&opts.bullets_ordered))
        };

        Converter {
            notes_error: None,
            number_match: number_match_default,
            term_match: r"(\w\w+)".to_string(),
            tags: Vec::new(),
            mode: 0,
            listnum: 0,
            list_nice_indent: String::new(),
            list_indent: Vec::new(),
            list: Vec::new(),
            list_prefix: Vec::new(),
            heading_styles,
            num_heading_styles,
            heading_count: Vec::new(),
            non_header_anchor: 0,
            prev_para_action: 0,
            opts,
            links,
            preformat_enabled,
            re_cache: HashMap::new(),
            print_count: 0,
            resolved: Resolved::Utf8,
            head_inner: String::new(),
            title_text: String::new(),
            template_text,
            url_policy,
            dropped_schemes: Vec::new(),
            document_template,
        }
    }

    /// The most compiled patterns [`Converter::re_cache`] will hold (A10).
    ///
    /// Mirrors `links::ascii_re_cached`'s 128, so the two caches in the engine
    /// are bounded the same way. Measured rather than guessed: a document
    /// exercising every construct that reaches `re` -- lists, definition lists,
    /// all four inline delimiters, tables, hrules, preformatted blocks, caps and
    /// short lines -- compiles 19 distinct patterns with default options, and 19
    /// with *every* pattern-varying option set to a distinct value at once
    /// (bullets, bullets_ordered, all three delimiters, hrule_min, both preformat
    /// markers, custom heading patterns). So 128 is roughly 6x the worst
    /// realistic case rather than an arbitrary round number, and the same input
    /// does not grow the cache: these are memoised per *pattern*, not per line.
    const RE_CACHE_MAX: usize = 128;

    /// Insert into the pattern cache, keeping it bounded (A10).
    ///
    /// Clears the whole cache when it is full rather than evicting one entry.
    /// A compiled `Regex` is expensive to build and cheap to keep, and the
    /// working set of any real conversion is small, so clearing occasionally
    /// costs a few recompiles; an LRU would cost a lookup on the hot path that
    /// this function exists to avoid. It cannot change the output either way --
    /// the cache is a pure memo of `pattern -> compiled`, and a miss recompiles
    /// exactly what a hit would have returned.
    fn cache_pattern(&mut self, key: &str, re: Regex) -> &Regex {
        if self.re_cache.len() >= Self::RE_CACHE_MAX {
            self.re_cache.clear();
        }
        self.re_cache.insert(key.to_string(), re);
        // Looked up rather than reached for, because after a `clear()` above
        // this is the only entry but without one it is not, and
        // `values().next()` would then be free to return a *different*
        // pattern's Regex.
        self.re_cache.get(key).expect("just inserted")
    }

    fn re(&mut self, pat: &str) -> &Regex {
        let key = format!("(?s){pat}");
        if !self.re_cache.contains_key(&key) {
            // A pattern the caller supplied is compiled by `Options::validate`
            // before conversion starts and reported as a clean error, so getting
            // here with a bad one means validation was bypassed -- an internal
            // bug, and a panic is the right response to an internal bug. Every
            // other pattern reaching this line is a literal in this file.
            let re = links::try_compile_pattern(pat, false)
                .unwrap_or_else(|e| panic!("bad regex {pat:?}: {e}"));
            return self.cache_pattern(&key, re);
        }
        self.re_cache.get(&key).unwrap()
    }

    fn re_i(&mut self, pat: &str) -> &Regex {
        let key = format!("(?s)(?i){pat}");
        if !self.re_cache.contains_key(&key) {
            let re = links::try_compile_pattern(pat, true)
                .unwrap_or_else(|e| panic!("bad regex {pat:?}: {e}"));
            return self.cache_pattern(&key, re);
        }
        self.re_cache.get(&key).unwrap()
    }

    // ------------------------------------------------------- tags

    fn get_tag(&mut self, in_tag: &str, tag_type: u8, inside_tag: &str) -> String {
        let open_tag = self.tags.last().cloned().unwrap_or_default();
        let mut tag_prefix = String::new();

        if self.opts.xhtml {
            // Two conditions per close, merged with `||` where they close the
            // same tag: a `p` is closed by a nested `p` *or* by a block element
            // that cannot live inside one, and a `li` by a nested `li` *or* by
            // the end of its list. The reference states these as separate
            // branches; identical bodies make that a repetition rather than a
            // distinction, and the corpus is what pins the result.
            if open_tag == "p"
                && ((in_tag == "p" && tag_type != TAG_END)
                    || in_tag.starts_with("hr")
                    || in_tag == "ul"
                    || in_tag == "ol"
                    || in_tag == "dl"
                    || in_tag == "pre"
                    || in_tag == "table"
                    || in_tag.starts_with('h'))
            {
                tag_prefix = self.close_tag("p");
            } else if open_tag == "li"
                && ((in_tag == "li" && tag_type != TAG_END)
                    || ((in_tag == "ul" || in_tag == "ol") && tag_type == TAG_END))
            {
                tag_prefix = self.close_tag("li");
            } else if open_tag == "dt" && in_tag == "dd" && tag_type != TAG_END {
                tag_prefix = self.close_tag("dt");
            } else if open_tag == "dd"
                && ((in_tag == "dt" && tag_type != TAG_END)
                    || (in_tag == "dl" && tag_type == TAG_END))
            {
                tag_prefix = self.close_tag("dd");
            }
        }

        if tag_type == TAG_END {
            let out = self.close_tag(in_tag);
            return if tag_prefix.is_empty() {
                out
            } else {
                format!("{tag_prefix}{out}")
            };
        }

        let mut out_tag = in_tag.to_string();
        if self.opts.lower_case_tags {
            out_tag = out_tag.to_ascii_lowercase();
        } else {
            out_tag = out_tag.to_ascii_uppercase();
        }
        let out;
        if tag_type == TAG_EMPTY {
            if self.opts.xhtml {
                out = format!("<{out_tag}{inside_tag}/>");
            } else {
                out = format!("<{out_tag}{inside_tag}>");
            }
        } else {
            self.tags.push(in_tag.to_string());
            out = format!("<{out_tag}{inside_tag}>");
        }
        if tag_prefix.is_empty() {
            out
        } else {
            format!("{tag_prefix}{out}")
        }
    }

    fn close_tag(&mut self, in_tag: &str) -> String {
        let open_tag = self.tags.pop().unwrap_or_default();
        let in_tag = if in_tag.is_empty() { &open_tag } else { in_tag };
        if !open_tag.is_empty() && open_tag != in_tag {
            self.tags.push(open_tag.clone());
        }
        let mut out_tag = in_tag.to_string();
        if self.opts.lower_case_tags {
            out_tag = out_tag.to_ascii_lowercase();
        } else {
            out_tag = out_tag.to_ascii_uppercase();
        }
        format!("</{out_tag}>")
    }

    // ------------------------------------------------------- simple blocks

    fn hrule(&mut self, lines: &mut [String], actions: &mut [u32], ind: usize) {
        let hrmin = self.opts.hrule_min;
        let pat = format!(r"^\s*([\-_~=*]\s*){{{hrmin},}}$");
        if self.re(&pat).is_match(&lines[ind]).unwrap_or(false) {
            let tag = self.get_tag("hr", TAG_EMPTY, "");
            lines[ind] = format!("{tag}\n");
            actions[ind] |= HRULE;
        } else if lines[ind].contains('\x0c') {
            actions[ind] |= HRULE;
            let tag = self.get_tag("hr", TAG_EMPTY, "");
            lines[ind] = lines[ind].replace('\x0c', &format!("\n{tag}\n"));
        }
    }

    fn shortline(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
        i: usize,
        prev: &mut String,
        prev_action: &mut u32,
        prev_line_len: usize,
    ) {
        let tag = self.get_tag("br", TAG_EMPTY, "");
        if !lines[i].trim().is_empty()
            && !prev.trim().is_empty()
            && prev_line_len < self.opts.short_line_length
            && actions[i] & (END | HEADER | HRULE | LIST | IND_BREAK | PAR) == 0
            && *prev_action & (HEADER | HRULE | BREAK | IND_BREAK) == 0
        {
            let c = prev.pop().unwrap_or('\0');
            prev.push_str(&tag);
            prev.push(c);
            *prev_action |= BREAK;
        }
    }

    fn is_mailheader(&self, rows: &[String]) -> bool {
        let re = links::ascii_re_cached(r"^(?:From:?)|Newsgroups: ");
        if rows.is_empty() {
            return false;
        }
        re.is_match(&rows[0]).unwrap_or(false)
    }

    fn mailheader(&mut self, rows_ref: &mut Vec<String>) {
        let mut rows = rows_ref.clone();
        if self.is_mailheader(rows_ref) {
            self.mode |= MAILHEADER;
            if self.opts.escape_html_chars {
                rows[0] = chars::escape(&rows[0]);
            }
            self.anchor_mail(&mut rows[0]);
            if rows[0].ends_with('\n') {
                rows[0].pop();
            }
            let tag = self.get_tag("p", TAG_START, " class='mail_header'");
            let tag2 = self.get_tag("br", TAG_EMPTY, "");
            rows[0] = format!("<!-- New Message -->\n{tag}{}{tag2}\n", rows[0]);
            // Every row but the last is terminated with a `<br/>`; the last is
            // left for the paragraph that follows it. Splitting the tail apart
            // says that directly, where the index form needed `rlen` to say it.
            if rows.len() > 1 {
                let (final_row, br_rows) = rows[1..].split_last_mut().unwrap();
                for row in br_rows {
                    if self.opts.escape_html_chars {
                        *row = chars::escape(row);
                    }
                    if row.ends_with('\n') {
                        row.pop();
                    }
                    let tag3 = self.get_tag("br", TAG_EMPTY, "");
                    *row = format!("{row}{tag3}\n");
                }
                if self.opts.escape_html_chars {
                    *final_row = chars::escape(final_row);
                }
            }
        }
        *rows_ref = rows;
    }

    fn mailquote(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
        ind: usize,
        prev: &mut String,
        prev_action: &mut u32,
        next: Option<&str>,
    ) {
        let starts_quote = {
            let l = &lines[ind];
            let re1 = links::ascii_re_cached(r"^\w*&gt");
            let re2 = links::ascii_re_cached(r"^[\|:]");
            (re1.is_match(l).unwrap_or(false) || re2.is_match(l).unwrap_or(false))
                && next.is_some()
                && !next.unwrap().trim().is_empty()
        };
        if starts_quote {
            let tag = self.get_tag("br", TAG_EMPTY, "");
            // Perl: s/$/<tag>/ with `$` matching before the trailing newline
            if let Some(stripped) = lines[ind].strip_suffix('\n') {
                lines[ind] = format!("{stripped}{tag}\n");
            } else {
                lines[ind].push_str(&tag);
            }
            actions[ind] |= BREAK | MAILQUOTE;
            if *prev_action & (BREAK | MAILQUOTE) == 0 {
                let tag = self.get_tag("p", TAG_START, " class='quote_mail'");
                prev.push_str(&tag);
                actions[ind] |= PAR;
            }
        }
    }

    // The parameters of the three state-machine dispatchers below are the
    // reference's argument list for the same routine. Threading this state
    // through a struct would hide which state each routine reads and writes,
    // and that mapping is what the differential corpus tests. So the argument
    // counts are what they are on purpose.
    #[allow(clippy::too_many_arguments)]
    fn paragraph(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
        _indents: &mut Vec<usize>,
        ind: usize,
        prev: &mut String,
        prev_action: &mut u32,
        line_indent: usize,
        prev_indent: usize,
        is_fragment: bool,
        line_no: usize,
    ) {
        let par_indent = self.opts.par_indent;
        let cond = !lines[ind].trim().is_empty()
            && subtract_modes(actions[ind], END | MAILQUOTE | CAPS | BREAK) == 0
            && (prev.trim().is_empty()
                || actions[ind] & END != 0
                || line_indent > prev_indent + par_indent)
            && !(is_fragment && line_no == 0);
        if cond {
            if self.opts.indent_par_break
                && !prev.trim().is_empty()
                && actions[ind] & END == 0
                && line_indent > prev_indent + par_indent
            {
                let tag = self.get_tag("br", TAG_EMPTY, "");
                prev.push_str(&tag);
                prev.push_str(&"&nbsp;".repeat(line_indent));
                lines[ind] = strip_leading_spaces(&lines[ind], line_indent);
                *prev_action |= BREAK;
                actions[ind] |= IND_BREAK;
            } else if self.opts.preserve_indent {
                let tag = self.get_tag("p", TAG_START, "");
                prev.push_str(&tag);
                prev.push_str(&"&nbsp;".repeat(line_indent));
                lines[ind] = strip_leading_spaces(&lines[ind], line_indent);
                actions[ind] |= PAR;
            } else {
                let tag = self.get_tag("p", TAG_START, "");
                prev.push_str(&tag);
                actions[ind] |= PAR;
            }
        } else if self.opts.indent_par_break
            && self.mode & (PRE | TABLE | LIST) == 0
            && !prev.trim().is_empty()
            && actions[ind] & END == 0
            && *prev_action & (IND_BREAK | PAR) != 0
            && subtract_modes(actions[ind], END | MAILQUOTE | CAPS) == 0
            && line_indent > par_indent
            && line_indent == prev_indent
        {
            let tag = self.get_tag("br", TAG_EMPTY, "");
            prev.push_str(&tag);
            prev.push_str(&"&nbsp;".repeat(line_indent));
            lines[ind] = strip_leading_spaces(&lines[ind], line_indent);
            *prev_action |= BREAK;
            actions[ind] |= IND_BREAK;
        }
    }

    // ------------------------------------------------------- lists

    fn listprefix(&mut self, line: &str) -> (String, String, String, String) {
        let bullets = class_body(&self.opts.bullets);
        let bullets_full = format!("[{}]", bullets);
        let number_match = self.number_match.clone();
        let term_match = self.term_match.clone();

        let pat_bullet = format!(r"^\s*{}\s+\S", bullets_full);
        let pat_ordered = format!(r"^\s*{number_match}[\.\)\]:]\s+\S");
        let pat_term = format!(r"^\s*{term_match}:$");

        let is_bullet = self.re(&pat_bullet).is_match(line).unwrap_or(false);
        let is_ordered = self.re(&pat_ordered).is_match(line).unwrap_or(false);
        let is_term = self.re(&pat_term).is_match(line).unwrap_or(false);
        if !is_bullet && !is_ordered && !is_term {
            return (String::new(), String::new(), String::new(), String::new());
        }

        let mut term = String::new();
        if let Some(caps) = self.re(&pat_term).captures(line).ok().flatten() {
            if let Some(g) = caps.get(1) {
                term = g.as_str().to_string();
            }
        }
        let mut number = String::new();
        let pat_num = format!(r"^\s*{number_match}\S\s+\S");
        if let Some(caps) = self.re(&pat_num).captures(line).ok().flatten() {
            if let Some(g) = caps.get(1) {
                number = g.as_str().to_string();
            }
        }
        if !self.opts.bullets_ordered.is_empty() {
            let bop = class_body(&self.opts.bullets_ordered);
            let bop_full = format!("[{}]", bop);
            if self.re(&bop_full).is_match(&number).unwrap_or(false) {
                number = "1".to_string();
            }
        }
        // slippery "o" bullet exception (Perl sets $number = 0, which is falsy)
        if self.opts.bullets.contains('o') {
            let re = links::ascii_re_cached(r"^\s*o\s");
            if re.is_match(line).unwrap_or(false) {
                number.clear();
            }
        }

        let rawprefix;
        let prefix = if !term.is_empty() {
            let pat = format!(r"^(\s*{term_match}.)$");
            rawprefix = self
                .re(&pat)
                .captures(line)
                .ok()
                .flatten()
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            rawprefix.replace(&term, "")
        } else if !number.is_empty() {
            // the captured number string is truthy in Perl (even "0")
            let pat = format!(r"^(\s*{number_match}.)");
            rawprefix = self
                .re(&pat)
                .captures(line)
                .ok()
                .flatten()
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            rawprefix.replace(&number, "")
        } else {
            let pat = format!(r"^(\s*{}.)", bullets_full);
            rawprefix = self
                .re(&pat)
                .captures(line)
                .ok()
                .flatten()
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            rawprefix.clone()
        };
        (prefix, number, rawprefix, term)
    }

    #[allow(clippy::too_many_arguments)]
    fn startlist(
        &mut self,
        prefix: &str,
        number: &str,
        _rawprefix: &str,
        term: &str,
        _lines: &mut [String],
        actions: &mut [u32],
        _indents: &mut Vec<usize>,
        ind: usize,
        prev: &mut String,
        total_prefix: &str,
    ) -> bool {
        // The reference indexes these stacks by __listnum (assignment, not
        // push), so entries below listnum stay but are overwritten.
        while self.list_prefix.len() <= self.listnum {
            self.list_prefix.push(String::new());
        }
        self.list_prefix[self.listnum] = prefix.to_string();
        let num_truthy = !number.is_empty();
        let tag;
        if num_truthy {
            if number != "1" && number != "a" && number != "A" {
                return false;
            }
            tag = self.get_tag("ol", TAG_START, "");
            prev.push_str(&format!("{}{tag}\n", self.list_nice_indent));
            while self.list.len() <= self.listnum {
                self.list.push(0);
            }
            self.list[self.listnum] = OL;
        } else if !term.is_empty() {
            tag = self.get_tag("dl", TAG_START, "");
            prev.push_str(&format!("{}{tag}\n", self.list_nice_indent));
            while self.list.len() <= self.listnum {
                self.list.push(0);
            }
            self.list[self.listnum] = DL;
        } else {
            tag = self.get_tag("ul", TAG_START, "");
            prev.push_str(&format!("{}{tag}\n", self.list_nice_indent));
            while self.list.len() <= self.listnum {
                self.list.push(0);
            }
            self.list[self.listnum] = UL;
        }
        let _ = tag;
        while self.list_indent.len() <= self.listnum {
            self.list_indent.push(0);
        }
        self.list_indent[self.listnum] = total_prefix.chars().count();
        self.listnum += 1;
        self.list_nice_indent = " ".repeat(self.listnum * self.opts.indent_width);
        actions[ind] |= LIST;
        actions[ind] |= LIST_START;
        self.mode |= LIST;
        true
    }

    fn endlist(&mut self, num_lists: usize, prev: &mut String, line_action: &mut u32) {
        let mut n = num_lists;
        while n > 0 {
            self.list_nice_indent = " ".repeat((self.listnum - 1) * self.opts.indent_width);
            let lt = self.list.get(self.listnum - 1).copied().unwrap_or(0);

            let tag = if lt == UL {
                self.get_tag("ul", TAG_END, "")
            } else if lt == OL {
                self.get_tag("ol", TAG_END, "")
            } else if lt == DL {
                self.get_tag("dl", TAG_END, "")
            } else {
                String::new()
            };
            prev.push_str(&format!("{}{tag}\n", self.list_nice_indent));
            self.list_indent.pop();
            self.listnum = self.listnum.saturating_sub(1);
            n -= 1;
        }
        *line_action |= END;
        if self.listnum == 0 {
            self.mode ^= LIST & self.mode;
        }
    }

    fn continuelist(&mut self, lines: &mut [String], actions: &mut [u32], ind: usize, term: &str) {
        let list_indent = self.list_nice_indent.clone();
        let lt = self.list.get(self.listnum - 1).copied().unwrap_or(0);
        if lt == UL {
            let bullets_full = format!("[{}]", class_body(&self.opts.bullets));
            let pat = format!(r"^\s*{bullets_full} \s*");
            // compile once and reuse: this runs for every list item, and
            // building the pattern afresh each time dominated large documents
            let re = self.re(&pat);
            let matched = re.is_match(&lines[ind]).unwrap_or(false);
            if matched {
                let tag = self.get_tag("li", TAG_START, "");
                let re = self.re(&pat);
                let replaced = re
                    .replace(&lines[ind], format!("{list_indent}{tag}"))
                    .to_string();
                lines[ind] = replaced;
                actions[ind] |= LIST_ITEM;
            }
        }
        if lt == OL {
            let num_match = self.number_match.clone();
            let pat = format!(r"^\s*{num_match}.\s*");
            let tag = self.get_tag("li", TAG_START, "");
            let re = self.re(&pat);
            let replaced = re
                .replace(&lines[ind], format!("{list_indent}{tag}"))
                .to_string();
            lines[ind] = replaced;
            actions[ind] |= LIST_ITEM;
        }
        if lt == DL && !term.is_empty() {
            let term_match = self.term_match.clone();
            let tag = self.get_tag("dt", TAG_START, "");
            let tag2 = self.get_tag("dt", TAG_END, "");
            let term_clean = term.replace('_', " ");
            let pat = format!(r"^\s*{term_match}.$");
            let re = self.re(&pat);
            let replaced = re
                .replace(&lines[ind], format!("{list_indent}{tag}{term_clean}{tag2}"))
                .to_string();
            lines[ind] = replaced;
            let tag = self.get_tag("dd", TAG_START, "");
            lines[ind].push_str(&tag);
            actions[ind] |= LIST_ITEM;
        }
        actions[ind] |= LIST;
    }

    fn total_prefix(&self, line: &str, term: bool) -> String {
        if term {
            // ^(\s*)term.$  -> leading whitespace only
            let ws: String = line.chars().take_while(|c| is_perl_space(*c)).collect();
            return format!("{ws}{}", " ".repeat(self.opts.indent_width));
        }
        let ch: Vec<char> = line.chars().collect();
        let n = ch.len();
        let mut i = 0;
        while i < n && is_perl_space(ch[i]) {
            i += 1;
        }
        while i < n
            && (is_word_char(ch[i])
                || self.opts.bullets.contains(ch[i])
                || self.opts.bullets_ordered.contains(ch[i]))
        {
            i += 1;
        }
        if i < n {
            i += 1; // the '.' any char
        }
        while i < n && is_perl_space(ch[i]) {
            i += 1;
        }
        ch[..i].iter().collect()
    }

    fn liststuff(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
        indents: &mut Vec<usize>,
        ind: usize,
        prev: &mut String,
        prev_action: &mut u32,
    ) {
        let (prefix, number, rawprefix, term) = self.listprefix(&lines[ind]);
        let _ = rawprefix;

        if prefix.is_empty() {
            if ind > 0 && !prev.trim().is_empty() {
                return;
            }
            if ind == 0 && self.listnum > 0 && indents[ind] == *self.list_indent.last().unwrap() {
                let tag = self.get_tag("p", TAG_START, "");
                prev.push_str(&tag);
                actions[ind] |= PAR;
                return;
            }
            if self.listnum > 0 {
                let nl = self.listnum;
                self.endlist(nl, prev, &mut actions[ind]);
            }
            return;
        }

        let mut prefix_alternate: Option<String> = None;
        if number.chars().count() > 1 {
            prefix_alternate = Some(format!(
                "{}{}",
                " ".repeat(number.chars().count() - 1),
                prefix
            ));
        }

        // walk back to a matching prefix
        let mut i = self.listnum as i64 - 1;
        while i >= 0 && prefix != self.list_prefix[i as usize] {
            if number.chars().count() > 1 {
                if let Some(pa) = &prefix_alternate {
                    if pa == &self.list_prefix[i as usize] {
                        break;
                    }
                }
            }
            i -= 1;
        }

        let total = self.total_prefix(&lines[ind], !term.is_empty());

        let mut islist = true;
        i += 1;
        if i > 0 && i as usize != self.listnum {
            let nl = self.listnum - i as usize;
            self.endlist(nl, prev, &mut actions[ind]);
            islist = false;
        } else if self.listnum == 0 || i as usize != self.listnum {
            if indents[ind] > 0
                || ind == 0
                || (ind > 0 && prev.trim().is_empty())
                || (ind > 0 && *prev_action & (BREAK | HEADER | CAPS) != 0)
            {
                islist = self.startlist(
                    &prefix, &number, "", &term, lines, actions, indents, ind, prev, &total,
                );
            } else {
                return;
            }
        }

        if self.mode & LIST != 0 {
            self.continuelist(lines, actions, ind, &term);
        }
        if islist {
            indents[ind] = total.chars().count();
        }
        let _ = prev_action;
    }

    // ------------------------------------------------------- tables

    fn table_spaces(rows: &[String], para_len: usize) -> String {
        let mut spaces: Vec<u8> = Vec::new();
        let mut min = para_len;
        for row in rows {
            if row.len() < min {
                min = row.len();
            }
            let bytes = row.as_bytes();
            if spaces.is_empty() {
                spaces = bytes.to_vec();
            } else {
                for (i, b) in bytes.iter().enumerate() {
                    if i >= spaces.len() {
                        break;
                    }
                    spaces[i] |= b;
                }
            }
        }
        for b in spaces.iter_mut() {
            if *b != b' ' {
                // 'X' keeps offsets byte==char (the reference uses \xff,
                // which would be multi-byte in a Rust &str).
                *b = b'X';
            }
        }
        spaces.truncate(min);
        spaces.iter().map(|&b| b as char).collect()
    }

    fn is_aligned_table(rows: &[String], para_len: usize) -> bool {
        if rows.len() < 2 {
            return false;
        }
        let spaces = Self::table_spaces(rows, para_len);
        let mut starts: Vec<usize> = Vec::new();
        if !spaces.starts_with(' ') {
            starts.push(0);
        }
        let re = links::ascii_re_cached(r"(?:^| ) +(?=[^ ])");
        for m in re.find_iter(&spaces).flatten() {
            starts.push(m.end());
        }
        rows.len() >= 2 && starts.len() >= 2
    }

    fn table_columns(rows: &[String], para_len: usize) -> (Vec<usize>, Vec<usize>) {
        let spaces = Self::table_spaces(rows, para_len);
        let max = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut starts: Vec<usize> = Vec::new();
        let mut ends: Vec<usize> = Vec::new();
        if !spaces.starts_with(' ') {
            starts.push(0);
        }
        let re = links::ascii_re_cached(r"((?:^| ) +)(?=[^ ])");
        for caps in re.captures_iter(&spaces).flatten() {
            let g = caps.get(1).unwrap();
            ends.push(g.start());
            starts.push(g.end());
        }
        if spaces.starts_with(' ') && !ends.is_empty() {
            ends.remove(0);
        }
        ends.push(max);
        (starts, ends)
    }

    fn make_aligned_table(&mut self, rows: &mut Vec<String>, para_len: usize) -> bool {
        let (starts, ends) = Self::table_columns(rows, para_len);
        if rows.len() < 2 || starts.len() < 2 {
            return false;
        }
        self.mode |= TABLE;

        let align_idx: Vec<usize> = (0..starts.len())
            .map(|col| {
                let width = ends[col] - starts[col];
                let mut count = [0usize; 4];
                for row in rows.iter() {
                    let cell = byte_slice(row, starts[col], width);
                    let a = if cell.starts_with(' ') { 2 } else { 0 };
                    let b = if cell.ends_with(' ') || byte_len(cell) < width {
                        1
                    } else {
                        0
                    };
                    count[a + b] += 1;
                }
                let population = count[1] + count[2] + count[3];
                count
                    .iter()
                    .enumerate()
                    .skip(1)
                    .find(|(_, n)| **n * 2 > population)
                    .map_or(0, |(x, _)| x)
            })
            .collect();

        let mut new_rows: Vec<String> = Vec::new();
        for row in rows.iter() {
            let mut out = String::new();
            out.push_str(&self.get_tag("tr", TAG_START, ""));
            for (col, &sc) in starts.iter().enumerate() {
                let width = ends[col] - sc;
                let mut cell = byte_slice(row, sc, width).to_string();
                cell = cell.trim_matches(' ').to_string();
                if self.opts.escape_html_chars {
                    cell = chars::escape(&cell);
                }
                let inside = if self.opts.xhtml {
                    match align_idx[col] {
                        2 => " style=\"text-align: right;\"".to_string(),
                        3 => " style=\"text-align: center;\"".to_string(),
                        _ => String::new(),
                    }
                } else if self.opts.lower_case_tags {
                    match align_idx[col] {
                        2 => " align=\"right\"".to_string(),
                        3 => " align=\"center\"".to_string(),
                        _ => String::new(),
                    }
                } else {
                    match align_idx[col] {
                        2 => " ALIGN=\"RIGHT\"".to_string(),
                        3 => " ALIGN=\"CENTER\"".to_string(),
                        _ => String::new(),
                    }
                };
                let tag = self.get_tag("td", TAG_START, &inside);
                let tag2 = self.close_tag("td");
                out.push_str(&format!("{tag}{cell}{tag2}"));
            }
            out.push_str(&self.close_tag("tr"));
            new_rows.push(out);
        }

        let tag = if self.opts.xhtml {
            self.get_tag("table", TAG_START, " summary=\"\"")
        } else {
            self.get_tag("table", TAG_START, "")
        };
        new_rows[0] = format!("{tag}\n{}", new_rows[0]);
        let tag = self.close_tag("table");
        let last = new_rows.last_mut().unwrap();
        last.push_str(&format!("\n{tag}"));
        *rows = new_rows;
        true
    }

    fn make_pgsql_table(&mut self, out: &mut Vec<String>) -> bool {
        let mut rows = out.clone();
        let mut caption = String::new();
        if !rows[0].contains('|') {
            let re = links::ascii_re_cached(r"^\s*\w+");
            if re.is_match(&rows[0]).unwrap_or(false) {
                caption = rows.remove(0);
            }
        }
        // split the heading row on \s+|\s+
        let head_row = rows.remove(0);
        let hre = links::ascii_re_cached(r"\s+\|\s+");
        let headings: Vec<String> = hre
            .split(&head_row)
            .filter_map(|f| {
                let f = f.ok()?.trim().to_string();
                if f.is_empty() {
                    None
                } else {
                    Some(f)
                }
            })
            .collect();
        // skip the ----+--- line
        if !rows.is_empty() {
            rows.remove(0);
        }
        // grab the N rows line
        let n_rows = rows.pop().unwrap_or_default();
        // build the table
        let mut tab_lines: Vec<String> = Vec::new();
        let tag = if self.opts.xhtml {
            self.get_tag("table", TAG_START, " border=\"1\" summary=\"\"")
        } else {
            self.get_tag("table", TAG_START, " border=\"1\"")
        };
        tab_lines.push(format!("{tag}\n"));
        if !caption.is_empty() {
            caption = caption.trim().to_string();
            let tag1 = self.get_tag("caption", TAG_START, "");
            let tag2 = self.close_tag("caption");
            tab_lines.push(format!("{tag1}{caption}{tag2}\n"));
        }
        let mut thead = String::new();
        thead.push_str(&self.get_tag("thead", TAG_START, ""));
        thead.push_str(&self.get_tag("tr", TAG_START, ""));
        for col in headings {
            let mut col = col.trim().to_string();
            let _ = &mut col;
            let tag1 = self.get_tag("th", TAG_START, "");
            let tag2 = self.close_tag("th");
            thead.push_str(&format!("{tag1}{col}{tag2}"));
        }
        thead.push_str(&self.close_tag("tr"));
        thead.push_str(&self.close_tag("thead"));
        tab_lines.push(format!("{thead}\n"));
        tab_lines.push(format!("{}\n", self.get_tag("tbody", TAG_START, "")));

        for row in &rows {
            let mut this_row = self.get_tag("tr", TAG_START, "");
            for cell in row.split('|') {
                let mut cell = cell.trim().to_string();
                if self.opts.escape_html_chars {
                    cell = chars::escape(&cell);
                }
                if cell == "0" || cell.is_empty() {
                    cell = "&nbsp;".to_string();
                }
                let tag1 = self.get_tag("td", TAG_START, "");
                let tag2 = self.close_tag("td");
                this_row.push_str(&format!("{tag1}{cell}{tag2}"));
            }
            this_row.push_str(&self.close_tag("tr"));
            tab_lines.push(format!("{this_row}\n"));
        }
        tab_lines.push(format!("{}\n", self.close_tag("tbody")));
        tab_lines.push(format!("{}\n", self.get_tag("table", TAG_END, "")));

        // and add the N rows line
        let ptag = self.get_tag("p", TAG_START, "");
        tab_lines.push(format!("{ptag}{n_rows}\n"));
        if self.opts.xhtml {
            let ptag2 = self.get_tag("p", TAG_END, "");
            let last = tab_lines.last_mut().unwrap();
            if let Some(pos) = last.find('\n') {
                last.insert_str(pos, &ptag2);
            }
        }
        *out = tab_lines;
        true
    }

    fn make_border_table(&mut self, out: &mut Vec<String>) -> bool {
        let mut rows = out.clone();
        let mut caption = String::new();
        if !rows[0].contains('|') {
            let re = links::ascii_re_cached(r"^\s*\w+");
            if re.is_match(&rows[0]).unwrap_or(false) {
                caption = rows.remove(0);
            }
        }
        // skip the +----+---+ line
        rows.remove(0);
        // get the head row and cut off the start and end |
        let mut head_row = rows.remove(0);
        head_row = head_row.trim_start_matches([' ', '\t']).to_string();
        if head_row.starts_with('|') {
            head_row.remove(0);
        }
        if head_row.ends_with('|') {
            head_row.pop();
        }
        let hre = links::ascii_re_cached(r"\s+\|\s+");
        let headings: Vec<String> = hre
            .split(&head_row)
            .filter_map(|f| {
                let f = f.ok()?.trim().to_string();
                if f.is_empty() {
                    None
                } else {
                    Some(f)
                }
            })
            .collect();
        // skip the +----+---+ line
        rows.remove(0);
        // skip the last +----+---+ line
        rows.pop();

        let mut tab_lines: Vec<String> = Vec::new();
        let tag = if self.opts.xhtml {
            self.get_tag("table", TAG_START, " border=\"1\" summary=\"\"")
        } else {
            self.get_tag("table", TAG_START, " border=\"1\"")
        };
        tab_lines.push(format!("{tag}\n"));
        if !caption.is_empty() {
            caption = caption.trim().to_string();
            let tag1 = self.get_tag("caption", TAG_START, "");
            let tag2 = self.close_tag("caption");
            tab_lines.push(format!("{tag1}{caption}{tag2}\n"));
        }
        let mut thead = String::new();
        thead.push_str(&self.get_tag("thead", TAG_START, ""));
        thead.push_str(&self.get_tag("tr", TAG_START, ""));
        for col in headings {
            let col = col.trim().to_string();
            let tag1 = self.get_tag("th", TAG_START, "");
            let tag2 = self.close_tag("th");
            thead.push_str(&format!("{tag1}{col}{tag2}"));
        }
        thead.push_str(&self.close_tag("tr"));
        thead.push_str(&self.close_tag("thead"));
        tab_lines.push(format!("{thead}\n"));
        tab_lines.push(format!("{}\n", self.get_tag("tbody", TAG_START, "")));
        for row in &rows {
            let mut row = row.trim_end_matches('\n').to_string();
            // cut off the start and end |
            row = row.trim_start_matches([' ', '\t']).to_string();
            if row.starts_with('|') {
                row.remove(0);
            }
            if row.ends_with('|') {
                row.pop();
            }
            let mut this_row = self.get_tag("tr", TAG_START, "");
            for cell in row.split('|') {
                let mut cell = cell.trim().to_string();
                if self.opts.escape_html_chars {
                    cell = chars::escape(&cell);
                }
                if cell == "0" || cell.is_empty() {
                    cell = "&nbsp;".to_string();
                }
                let tag1 = self.get_tag("td", TAG_START, "");
                let tag2 = self.close_tag("td");
                this_row.push_str(&format!("{tag1}{cell}{tag2}"));
            }
            this_row.push_str(&self.close_tag("tr"));
            tab_lines.push(format!("{this_row}\n"));
        }
        tab_lines.push(format!("{}\n", self.close_tag("tbody")));
        tab_lines.push(format!("{}\n", self.get_tag("table", TAG_END, "")));
        *out = tab_lines;
        true
    }

    fn make_delim_table(&mut self, out: &mut Vec<String>) -> bool {
        let mut rows = out.clone();
        let mut caption = String::new();
        if !rows[0].contains('|') {
            let re = links::ascii_re_cached(r"^\s*\w+");
            if re.is_match(&rows[0]).unwrap_or(false) {
                caption = rows.remove(0);
            }
        }
        let delim = {
            let re = links::ascii_re_cached(r"^\s*([^A-Za-z0-9])");
            re.captures(&rows[0])
                .ok()
                .flatten()
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().chars().next().unwrap())
        };
        let Some(delim) = delim else { return false };

        let mut tab_lines: Vec<String> = Vec::new();
        let tag = if self.opts.xhtml {
            self.get_tag("table", TAG_START, " border=\"1\" summary=\"\"")
        } else {
            self.get_tag("table", TAG_START, " border=\"1\"")
        };
        tab_lines.push(format!("{tag}\n"));
        if !caption.is_empty() {
            caption = caption.trim().to_string();
            let tag1 = self.get_tag("caption", TAG_START, "");
            let tag2 = self.close_tag("caption");
            tab_lines.push(format!("{tag1}{caption}{tag2}\n"));
        }
        for row in &rows {
            let row = row.trim_end_matches('\n');
            let mut row: Vec<char> = row.chars().collect();
            // cut off leading whitespace and one leading delimiter
            while let Some(&c) = row.first() {
                if c == ' ' || c == '\t' {
                    row.remove(0);
                } else {
                    break;
                }
            }
            if row.first() == Some(&delim) {
                row.remove(0);
            }
            if row.last() == Some(&delim) {
                row.pop();
            }
            let row: String = row.iter().collect();
            let mut this_row = self.get_tag("tr", TAG_START, "");
            for cell in row.split(delim) {
                let mut cell = cell.trim().to_string();
                if self.opts.escape_html_chars {
                    cell = chars::escape(&cell);
                }
                if cell == "0" || cell.is_empty() {
                    cell = "&nbsp;".to_string();
                }
                let tag1 = self.get_tag("td", TAG_START, "");
                let tag2 = self.close_tag("td");
                this_row.push_str(&format!("{tag1}{cell}{tag2}"));
            }
            this_row.push_str(&self.close_tag("tr"));
            tab_lines.push(format!("{this_row}\n"));
        }
        tab_lines.push(format!("{}\n", self.get_tag("table", TAG_END, "")));
        *out = tab_lines;
        true
    }

    fn tablestuff(&mut self, table_type: u32, rows: &mut Vec<String>, para_len: usize) -> bool {
        match table_type {
            TAB_ALIGN => self.make_aligned_table(rows, para_len),
            TAB_PGSQL => self.make_pgsql_table(rows),
            TAB_BORDER => self.make_border_table(rows),
            TAB_DELIM => self.make_delim_table(rows),
            _ => false,
        }
    }

    fn get_table_type(&self, rows: &[String], para_len: usize) -> u32 {
        if self.opts.table_type.delim && is_delim_table(rows) {
            TAB_DELIM
        } else if self.opts.table_type.align && Self::is_aligned_table(rows, para_len) {
            TAB_ALIGN
        } else if self.opts.table_type.pgsql && is_pgsql_table(rows) {
            TAB_PGSQL
        } else if self.opts.table_type.border && is_border_table(rows) {
            TAB_BORDER
        } else {
            0
        }
    }

    // ------------------------------------------------------- preformat

    fn is_preformatted(&mut self, line: &str) -> bool {
        let n = self.opts.preformat_whitespace_min;
        let re1 = format!(r"\s{{{n},}}\S+");
        let re2 = format!(r"\.{{{n},}}\S+");
        self.re(&re1).is_match(line).unwrap_or(false)
            || self.re(&re2).is_match(line).unwrap_or(false)
    }

    fn split_end_explicit_preformat(&mut self, para: &mut String) -> String {
        let mut pre_str = String::new();
        if self.mode & PRE_EXPLICIT != 0 {
            // The reference's test here is *always false*, and that is load
            // bearing. TextToHTML.pm:3868 reads
            //
            //     if (${para_ref} =~ /$pe_mark/io)
            //
            // -- note there is no `$` before `para_ref`. In Perl that is a
            // symbolic reference: the string "para_ref" is treated as the *name*
            // of a variable, so the regex is matched against `$main::para_ref`,
            // a global that nothing in the module ever assigns (it is not
            // `our`-declared either, and the module does not `use strict refs`,
            // so it is simply undef). Undef never matches, so the `if` body is
            // dead code and every call falls through to the comment Perl labels
            // "no end -- the whole thing is preformatted".
            //
            // Adding the missing deref changes the output, which is how this was
            // confirmed rather than assumed. With `${$para_ref}` in place of
            // `${para_ref}`:
            //
            //   printf '<pre>\na\n\n</pre>\n' | textrill --use_preformat_marker
            //   reference      <pre class='quote_explicit'>\na\n&lt;/pre&gt;\n</pre>
            //   with deref     ...&lt;/pre&gt;</pre>\n<p>&lt;/pre&gt;</p>
            //
            // Three consequences, all of which the port has to reproduce:
            //
            // 1. The whole paragraph is emitted as preformatted text, escaped
            //    like ordinary text. So a literal `</pre>` reaches the output as
            //    `&lt;/pre&gt;` -- visible to the reader, and the reason this
            //    diverged.
            // 2. PRE_EXPLICIT is *not* cleared. The block therefore does not end
            //    at the marker: everything after it in the chunk stays inside
            //    the preformatted block, and the `</pre>` that eventually closes
            //    it comes from the document's own tag cleanup at the end.
            // 3. `para` is emptied, so the marker is not reprocessed as a fresh
            //    paragraph.
            //
            // Concretely, for input `<pre>\na\n\n</pre>\nb\n` the reference
            // emits `b` *inside* the pre block:
            //
            //   <pre class='quote_explicit'>\na\n&lt;/pre&gt;\nb\n</pre>
            //
            // The end marker only has any effect at all when the block and its
            // marker are in the *same* paragraph, because then `endpreformat`
            // (a different function, correctly dereferenced) is what ends it.
            // That is why a blank line before the marker is all it takes to
            // reach this path, and why the same document with the blank line
            // removed behaves correctly.
            pre_str = if self.opts.escape_html_chars {
                chars::escape(para)
            } else {
                para.clone()
            };
            *para = String::new();
        }
        pre_str
    }

    fn endpreformat(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
        ind: usize,
        prev: &mut String,
    ) {
        if self.mode & PRE_EXPLICIT != 0 {
            let pe_mark = self.opts.preformat_end_marker.clone();
            let re = self.re_i(&pe_mark);
            if re.is_match(&lines[ind]).unwrap_or(false) {
                if ind == 0 {
                    let tag = self.close_tag("pre");
                    lines[ind] = format!("{tag}\n");
                } else {
                    let tag = self.close_tag("pre");
                    prev.push_str(&format!("{tag}\n"));
                    lines[ind] = String::new();
                }
                self.mode ^= (PRE | PRE_EXPLICIT) & self.mode;
                actions[ind] |= END;
            }
            return;
        }

        let cond = !self.is_preformatted(&lines[ind])
            && (self.opts.endpreformat_trigger_lines == 1
                || (ind + 1 < lines.len() && !self.is_preformatted(&lines[ind + 1]))
                || ind + 1 >= lines.len());
        if cond {
            if ind == 0 {
                let tag = self.close_tag("pre");
                *prev = format!("{tag}\n");
            } else {
                let tag = self.close_tag("pre");
                prev.push_str(&format!("{tag}\n"));
            }
            self.mode ^= PRE & self.mode;
            actions[ind] |= END;
        }
    }

    fn preformat(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
        ind: usize,
        prev: &mut String,
        prev_action: &mut u32,
        next: Option<&str>,
    ) {
        if self.opts.use_preformat_marker {
            let pstart = self.opts.preformat_start_marker.clone();
            let re = self.re_i(&pstart);
            if re.is_match(&lines[ind]).unwrap_or(false) {
                if prev.ends_with("<p>") {
                    prev.truncate(prev.len() - 3);
                    self.tags.pop();
                }
                let tag = self.get_tag("pre", TAG_START, " class='quote_explicit'");
                lines[ind] = format!("{tag}\n");
                self.mode |= PRE | PRE_EXPLICIT;
                actions[ind] |= PRE;
                return;
            }
        }

        if *prev_action & MAILQUOTE == 0
            && actions[ind] & MAILQUOTE == 0
            && (self.opts.preformat_trigger_lines == 0
                || (self.is_preformatted(&lines[ind])
                    && (self.opts.preformat_trigger_lines == 1
                        || (next.is_some() && self.is_preformatted(next.unwrap())))))
        {
            if prev.ends_with("<p>") {
                prev.truncate(prev.len() - 3);
                self.tags.pop();
            }
            let tag = self.get_tag("pre", TAG_START, "");
            lines[ind] = format!("{tag}\n{}", lines[ind]);
            self.mode |= PRE;
            actions[ind] |= PRE;
        }
    }

    // ------------------------------------------------------- headings/anchors

    fn make_new_anchor(&mut self, heading_level: usize) -> String {
        if heading_level == 0 {
            let a = format!("{}", self.non_header_anchor);
            self.non_header_anchor += 1;
            return a;
        }
        let mut anchor = String::from("section");
        if self.heading_count.len() < heading_level {
            self.heading_count.resize(heading_level, 0);
        }
        self.heading_count[heading_level - 1] += 1;
        for i in (heading_level..self.heading_count.len()).rev() {
            self.heading_count[i] = 0;
        }
        for i in 0..heading_level {
            if self.heading_count[i] == 0 {
                self.heading_count[i] = 1;
            }
            anchor.push_str(&format!("_{}", self.heading_count[i]));
        }
        anchor
    }

    fn anchor_mail(&mut self, line_ref: &mut String) {
        if self.opts.make_anchors {
            let anchor = self.make_new_anchor(0);
            // s/([^ ]*)/<a name="$anchor">$1<\/a>/   (first non-space run)
            let re = links::ascii_re_cached(r"[^ ]*");
            if let Some(m) = re.find(&*line_ref).ok().flatten() {
                let inner = m.as_str().to_string();
                let rep = if self.opts.lower_case_tags {
                    format!("<a name=\"{anchor}\">{inner}</a>")
                } else {
                    format!("<A NAME=\"{anchor}\">{inner}</A>")
                };
                line_ref.replace_range(m.start()..m.end(), &rep);
            }
        }
    }

    fn anchor_heading(&mut self, level: usize, line_ref: &mut String) {
        if self.opts.make_anchors {
            let anchor = self.make_new_anchor(level);
            if self.opts.lower_case_tags {
                let re = links::ascii_re_cached(r"(<h\d>)(.*)(</h\d>)");
                *line_ref = re
                    .replace(&*line_ref, format!("$1<a name=\"{anchor}\">$2</a>$3"))
                    .to_string();
            } else {
                let re = links::ascii_re_cached(r"(<H\d>)(.*)(</H\d>)");
                *line_ref = re
                    .replace(&*line_ref, format!("$1<A NAME=\"{anchor}\">$2</A>$3"))
                    .to_string();
            }
        }
    }

    fn is_ul_list_line(&mut self, line: &str) -> bool {
        let (prefix, number, _, _) = self.listprefix(line);
        !prefix.is_empty() && number.is_empty()
    }

    fn heading_level(&mut self, style: &str) -> usize {
        if !self.heading_styles.contains_key(style) {
            self.num_heading_styles += 1;
            self.heading_styles
                .insert(style.to_string(), self.num_heading_styles);
        }
        self.heading_styles[style]
    }

    fn is_heading(&mut self, line: &str, next: Option<&str>) -> bool {
        if line.trim().is_empty() {
            return false;
        }
        if self.is_ul_list_line(line) {
            return false;
        }
        let Some(next) = next else { return false };
        let re = links::ascii_re_cached(r"^\s*[-=*.~+]+\s*$");
        if !re.is_match(next).unwrap_or(false) {
            return false;
        }
        let (hoffset, heading) = match links::ascii_re_cached(r"^(\s*)(.+)$")
            .captures(line)
            .ok()
            .flatten()
        {
            Some(c) => (
                c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
                c.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
            ),
            None => return false,
        };
        let heading_c = links::ascii_re_cached(r"&[^;]+;")
            .replace_all(&heading, "X")
            .to_string();
        let (uoffset, underline) = match links::ascii_re_cached(r"^(\s*)(\S+)\s*$")
            .captures(next)
            .ok()
            .flatten()
        {
            Some(c) => (
                c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
                c.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
            ),
            None => return false,
        };
        let lendiff = (heading_c.chars().count() as i64 - underline.chars().count() as i64).abs();
        let offsetdiff = (hoffset.chars().count() as i64 - uoffset.chars().count() as i64).abs();
        let _ = (heading, offsetdiff);
        if (lendiff as usize <= self.opts.underline_length_tolerance)
            || (offsetdiff as usize <= self.opts.underline_offset_tolerance)
        {
            return true;
        }
        false
    }

    fn heading(&mut self, line_ref: &mut String, next_ref: &mut String) {
        // ($uoffset, $underline) = ${$next_ref} =~ /^(\s*)(\S+)\s*$/
        let underline = {
            let re = links::ascii_re_cached(r"^(\s*)(\S+)\s*$");
            let mut u = String::new();
            if let Some(c) = re.captures(next_ref).ok().flatten() {
                if let Some(g) = c.get(2) {
                    u = g.as_str().to_string();
                }
            }
            u
        };
        let mut style = underline.chars().next().unwrap_or(' ').to_string();
        if self.iscaps(line_ref) {
            style.push('C');
        }
        *next_ref = " ".to_string();
        let level = self.heading_level(&style);
        if self.opts.escape_html_chars {
            *line_ref = chars::escape(line_ref);
        }
        self.tagline(&format!("H{level}"), line_ref);
        self.anchor_heading(level, line_ref);
    }

    fn is_custom_heading(&mut self, line: &str) -> bool {
        for reg in self.opts.custom_heading_regexp.clone() {
            let re = self.re(&reg);
            if re.is_match(line).unwrap_or(false) {
                return true;
            }
        }
        false
    }

    fn custom_heading(&mut self, line_ref: &mut String) {
        for (i, reg) in self
            .opts
            .custom_heading_regexp
            .clone()
            .into_iter()
            .enumerate()
        {
            let re = self.re(&reg);
            if re.is_match(&*line_ref).unwrap_or(false) {
                let level = if self.opts.explicit_headings {
                    i + 1
                } else {
                    self.heading_level(&format!("Cust{i}"))
                };
                if self.opts.escape_html_chars {
                    *line_ref = chars::escape(line_ref);
                }
                let tag_level = format!("H{level}");
                self.tagline(&tag_level, line_ref);
                self.anchor_heading(level, line_ref);
                return;
            }
        }
    }

    fn unhyphenate_para(&mut self, para_ref: &mut String) {
        // s/(\s*)([^\W\d_]*)\-\n(\s*)([^\W\d_]+[\)\}\]\.,:;\'\"\>]*\s*)/$1$2$4\n$3/gs
        let re = self.re(r#"(\s*)([A-Za-z]*)-\n(\s*)([A-Za-z]+['").,:;}>]*\s*)"#);
        *para_ref = re.replace_all_captures(para_ref, |c| {
            let g = |i: usize| -> String {
                c.get(i).map(|m| m.as_str().to_string()).unwrap_or_default()
            };
            format!("{}{}{}\n{}", g(1), g(2), g(4), g(3))
        });
    }

    fn tagline(&mut self, tag: &str, line_ref: &mut String) {
        // Perl chomp: drop a trailing newline (and CR), not any char
        if line_ref.ends_with('\n') {
            line_ref.pop();
        }
        if line_ref.ends_with('\r') {
            line_ref.pop();
        }
        let re = links::ascii_re_cached(r"^\s*(.*)$");
        let caps = re.captures(&*line_ref).ok().flatten().unwrap();
        let body = caps.get(1).unwrap().as_str().to_string();
        let tag1 = self.get_tag(tag, TAG_START, "");
        let tag2 = self.close_tag(tag);
        *line_ref = format!("{tag1}{body}{tag2}\n");
    }

    fn iscaps(&mut self, line: &str) -> bool {
        let min = self.opts.min_caps_length;
        let pat = format!(r"^[^a-z<]*[A-Z]{{{min},}}[^a-z<]*$");
        self.re(&pat).is_match(line).unwrap_or(false)
    }

    fn caps(&mut self, line_ref: &mut String, action: &mut u32) {
        if !self.opts.caps_tag.is_empty() && self.iscaps(line_ref) {
            let tag = self.opts.caps_tag.clone();
            self.tagline(&tag, line_ref);
            *action |= CAPS;
        }
    }

    // ------------------------------------------------------- inline markup

    fn do_delim(&mut self, line_ref: &mut String, _action: &mut u32, delim: &str, tag: &str) {
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");
        if delim == "#" {
            // `#x#` where `x` is a single letter and the surrounding positions
            // are not word boundaries (the `\B` assertions). Pattern kept free
            // of `\B`/lookaround so fancy-regex stays on its linear path;
            // the boundary condition is applied in code instead.
            let re1 = self.re(r"#([A-Za-z])#").clone();
            self.delim_replace(line_ref, &re1, non_boundary('#'), ltag);
            // special treatment of # for the #num case and the #link case
            if line_ref.contains('#') {
                let re2 = self.re(r"#([^\d#][^#]*[^# \t\n])#").clone();
                if !line_ref.contains("<a") && !line_ref.contains("<A") {
                    self.delim_replace(line_ref, &re2, no_list_or_para_tag, ltag);
                } else {
                    *line_ref = self.delim_loop(line_ref, &re2, no_list_or_para_tag, tag);
                }
            }
        } else if delim == "^" {
            let re1 = self.re(r#"\^((?![^^]*(?:<li>|<LI>|<p>|<P>))(\w|["'<>])[^^]*)\^"#);
            if line_ref.contains('^') {
                *line_ref =
                    re1.replace_all_captures(line_ref, |c| ltag(c.get(1).unwrap().as_str()));
            }
            let re2 = self.re(r"\^([A-Za-z])\^").clone();
            self.delim_replace(line_ref, &re2, non_boundary('^'), ltag);
        } else if delim == "_" {
            let re1 = self.re(r"_([A-Za-z])_").clone();
            let r1m = self.delim_replace(line_ref, &re1, non_boundary('_'), ltag);
            if r1m {
                let re2 = self.re(r#"_([^_]+?[A-Za-z0-9"'.?&;:<>])_"#).clone();
                self.delim_replace(line_ref, &re2, not_preceded_by_word_or_underscore, ltag);
            } else if line_ref.contains('_') {
                let re2 = self.re(r#"_([^_]+?[A-Za-z0-9"'.?&;:<>])_"#).clone();
                *line_ref =
                    self.delim_loop(line_ref, &re2, not_preceded_by_word_or_underscore, tag);
            }
        } else if delim.chars().count() == 1 {
            let db = class_body(delim);
            let dch = delim.chars().next().unwrap();
            let re1 = self.re(&format!(r"[{db}]([A-Za-z])[{db}]")).clone();
            self.delim_replace(line_ref, &re1, non_boundary(dch), ltag);
            // `delim ... delim` where the content is one or more non-delimiter
            // characters ending in a "word/punctuation" character. The Perl
            // pattern's leading `(?<!delim)` is applied in code (`accept`)
            // so the regex itself stays free of lookaround (linear).
            let re2 = self
                .re(&format!(
                    r"[{db}]([^{db}]+?[A-Za-z0-9!-/:-@\[-`{{-~&<>])[{db}]"
                ))
                .clone();
            self.delim_replace(line_ref, &re2, not_preceded_by(dch), ltag);
        } else {
            // Perl interpolates `${delim}` straight into these patterns, so a
            // delimiter holding a regex metacharacter (`**` being the
            // obvious one) makes Perl itself raise "Quantifier follows
            // nothing in regex" and drop the substitution. The port used to
            // panic on the same input; escaping makes the pattern mean what
            // it obviously meant. Identical output for any delimiter Perl
            // could actually compile.
            //
            // The `(?<!...)` on the first one is a fixed-width literal
            // assertion, so it moves into `accept` like the others: left in
            // the regex it is a lookbehind, and a paragraph carrying a
            // multi-character delimiter run put it through the backtracking
            // VM for tens of seconds.
            let d = fancy_regex::escape(delim);
            let re1 = self
                .re(&format!(
                    r#"{d}((\w|["'])(\w|[-\s!-/:-@\[-`{{-~])*[^\s]){d}"#
                ))
                .clone();
            if line_ref.contains(delim) {
                self.delim_replace(line_ref, &re1, not_preceded_by_str(d.to_string()), ltag);
            }
            let re2 = self.re(&format!(r"{d}\]([A-Za-z]){d}"));
            *line_ref = re2.replace_all_captures(line_ref, |c| ltag(c.get(1).unwrap().as_str()));
        }
    }

    /// Perl-style `s///g` for a *linear* regex (one with no lookaround, `\B`
    /// or backrefs, so the regex crate stays on its non-backtracking path).
    ///
    /// `captures_from_pos` locates each candidate match; `accept` then applies
    /// whatever condition the original regex expressed with a lookbehind/`\B`
    /// (which fancy-regex would otherwise run through its exploding
    /// backtracking VM). Rejected candidates are kept verbatim and scanning
    /// continues just past them, mirroring how Perl's engine advances.
    ///
    /// Returns whether anything was replaced.
    fn delim_replace(
        &self,
        line_ref: &mut String,
        re: &Regex,
        accept: impl Fn(&str, usize, usize) -> bool,
        replace: impl Fn(&str) -> String,
    ) -> bool {
        let text = std::mem::take(line_ref);
        let mut out = String::with_capacity(text.len());
        // `last` is how much of `text` has already been appended to `out`;
        // `pos` is where the next candidate is searched for.  Rejected
        // candidates are left in place (advancing `pos` just past their first
        // character) so their bytes are flushed unmodified by a later accept
        // or by the final tail.
        let mut last = 0;
        let mut pos = 0;
        let mut changed = false;
        loop {
            match re.captures_from_pos(&text, pos).ok().flatten() {
                None => break,
                Some(caps) => {
                    let m = match caps.get(0) {
                        Some(m) => m,
                        None => break,
                    };
                    if accept(&text, m.start(), m.end()) {
                        out.push_str(&text[last..m.start()]);
                        out.push_str(&replace(caps.get(1).unwrap().as_str()));
                        last = m.end();
                        pos = m.end();
                        changed = true;
                    } else {
                        pos = m.start() + char_len(&text, m.start());
                    }
                }
            }
        }
        out.push_str(&text[last..]);
        *line_ref = out;
        changed
    }

    /// As `delim_replace`, but for a line that contains generated markup:
    /// the replacement is skipped inside link contexts, which is what the
    /// `contains("<a")` split in `do_delim` selects. Left to right, always
    /// consuming at least one character so the loop terminates.
    fn delim_loop(
        &mut self,
        line_ref: &mut String,
        re: &Regex,
        accept: impl Fn(&str, usize, usize) -> bool,
        tag: &str,
    ) -> String {
        let mut line_with_links = String::new();
        loop {
            let cur = line_ref.clone();
            match re.captures(&cur).ok().flatten() {
                None => break,
                Some(caps) => {
                    let m = caps.get(0).unwrap();
                    if !accept(&cur, m.start(), m.end()) {
                        // A rejected candidate is not consumed: the original
                        // regex retries one character along, so a candidate
                        // starting *inside* this one can still match. Skipping
                        // the whole span loses it -- which is how a `#` run
                        // spanning a `</p><p>` boundary used to swallow the
                        // pair that followed. The skipped character is
                        // ordinary text and is emitted verbatim.
                        let keep = m.start() + char_len(&cur, m.start());
                        line_with_links.push_str(&cur[..keep]);
                        *line_ref = cur[keep..].to_string();
                        continue;
                    }
                    let pre = cur[..m.start()].to_string();
                    let mut linkme = cur[m.start()..m.end()].to_string();
                    let post = cur[m.end()..].to_string();
                    line_with_links.push_str(&pre);
                    if !self.links.in_link_context(&linkme, &line_with_links) {
                        let rebuilt = if let Some(c) = re.captures(&linkme).ok().flatten() {
                            if let Some(g) = c.get(1) {
                                format!("<{tag}>{}</{tag}>", g.as_str())
                            } else {
                                linkme.clone()
                            }
                        } else {
                            linkme.clone()
                        };
                        linkme = rebuilt;
                    }
                    line_with_links.push_str(&linkme);
                    *line_ref = post;
                }
            }
        }
        format!("{line_with_links}{line_ref}")
    }

    // ------------------------------------------------------- links + markup

    fn apply_links(&mut self, para_ref: &mut String, para_action: &mut u32) {
        if self.opts.make_links && !self.links.rules.is_empty() {
            self.links.check_dictionary_links(para_ref);
            // A11. The single point where document text and dictionary URLs
            // become an `href`, so it is also the single point that decides
            // whether the scheme is one a browser will act on. Placed here
            // rather than at each construction site so that `links.rs` stays a
            // faithful port and so a producer added later is covered without
            // remembering this rule.
            //
            // Runs over the finished paragraph rather than per substitution, so
            // it costs one scan per paragraph and not one per match -- the same
            // reasoning as the P6 prefilter.
            let mut dropped = Vec::new();
            if let Some(scrubbed) =
                crate::urlscheme::scrub_hrefs(para_ref, &self.url_policy, &mut dropped)
            {
                *para_ref = scrubbed;
                for scheme in dropped {
                    if !self.dropped_schemes.contains(&scheme) {
                        eprintln!(
                            "textrill: dropped a link with the {scheme:?} URL scheme; \
                             the text is kept. Pass --allowed_url_schemes {scheme} to allow it."
                        );
                        self.dropped_schemes.push(scheme);
                    }
                }
            }
        }
        let ls = self.opts.lower_case_tags;
        if !self.opts.bold_delimiter.is_empty() {
            let tag = if ls { "strong" } else { "STRONG" };
            let d = self.opts.bold_delimiter.clone();
            self.do_delim(para_ref, para_action, &d, tag);
        }
        if !self.opts.italic_delimiter.is_empty() {
            let tag = if ls { "em" } else { "EM" };
            let d = self.opts.italic_delimiter.clone();
            self.do_delim(para_ref, para_action, &d, tag);
        }
        if !self.opts.underline_delimiter.is_empty() {
            let tag = if ls { "u" } else { "U" };
            let d = self.opts.underline_delimiter.clone();
            self.do_delim(para_ref, para_action, &d, tag);
        }
        *para_action |= LINK;
    }

    // ------------------------------------------------------- main pipeline

    pub fn process_chunk(&mut self, chunk: &str, close_tags: bool, is_fragment: bool) -> String {
        // Perl `split(/\r?\n\r?\n/, $chunk)` semantics
        let para_strs = split_blank_lines(chunk);
        let mut ret = String::new();
        if para_strs.is_empty() {
            // splitting an empty string yields no elements in Perl
        } else if para_strs.len() == 1 {
            ret += &self.process_para(chunk, close_tags, is_fragment);
        } else {
            let last = para_strs.len() - 1;
            for (i, p) in para_strs.iter().enumerate() {
                let mut p = p.clone();
                if !p.ends_with('\n') {
                    p.push('\n');
                }
                let ct = i == last && close_tags;
                ret += &self.process_para(&p, ct, false);
            }
        }
        ret
    }

    pub fn process_para(&mut self, para: &str, close_tags: bool, is_fragment: bool) -> String {
        let mut para = para.to_string();
        let mut para_action = NONE;

        // tables and mailheaders don't carry over from one para to the next
        if self.mode & TABLE != 0 {
            self.mode ^= TABLE;
        }
        if self.mode & MAILHEADER != 0 {
            self.mode ^= MAILHEADER;
        }

        if self.opts.demoronize {
            chars::demoronize_char(&mut para);
        }

        if !self.opts.link_only {
            // Chop trailing whitespace and DOS CRs
            para = chop_trailing_cr(&para);
            // Chop leading whitespace and DOS CRs
            para = chop_leading_cr(&para);
            para = para.replace('\r', ""); // remove any stray carriage returns
            let para_len = para.chars().count();
            let mut done_lines: Vec<String> = Vec::new();

            // PRE_EXPLICIT may carry over
            if self.mode & PRE_EXPLICIT != 0 {
                let pre_str = self.split_end_explicit_preformat(&mut para);
                if !pre_str.is_empty() {
                    done_lines.push(pre_str);
                }
            }

            // The explicit-preformat continuation above swallows the whole
            // paragraph when the end marker is absent: everything left is
            // verbatim preformatted text, and there is nothing for the
            // paragraph machinery below to do.  `para = done_lines.join("")`
            // lives *inside* that block, so without this the buffered lines
            // were dropped on the floor and only the tags survived:
            //
            //   printf '<pre>\n\nX' | textrill --use_preformat_marker
            //   reference  <pre class='quote_explicit'>\nX\n</pre>
            //   port       <pre class='quote_explicit'>\n\n</pre>     (before)
            //
            // Everything after the first blank line of an explicit quote was
            // lost, since a blank line is what ends a paragraph.
            //
            // This is not a `return`: the continuation text still has to reach
            // the tail of this function, because the reference runs every
            // paragraph through apply_links (TextToHTML.pm:1375) and then the
            // demoronize/entities passes.  Returning early left the
            // bold/italic/underline delimiters unprocessed inside an explicit
            // quote -- "*d*" stayed literal where the reference emitted
            // "<em>d</em>".
            let pre_continuation = para.is_empty();
            if pre_continuation {
                para = done_lines.join("");
                // The same trailing-newline chop the main path applies to a
                // continuing PRE, so the two agree on where paragraphs end
                // inside an explicit quote.
                if self.mode & (LIST | PRE) != 0 {
                    while para.ends_with('\n') {
                        para.pop();
                    }
                }
            }

            if !pre_continuation && !para.is_empty() {
                // split into lines and compute indent/len
                let mut para_lines = split_lines(&para);
                if para_lines.is_empty() {
                    para_lines.push(String::new());
                }
                let mut para_line_len: Vec<usize> = Vec::new();
                let mut para_line_indent: Vec<usize> = Vec::new();
                let mut para_line_action: Vec<u32> = Vec::new();
                for (i, line) in para_lines.iter_mut().enumerate() {
                    // tabs -> spaces
                    while let Some(tab) = line.find('\t') {
                        let tw = self.opts.tab_width;
                        let spaces = " ".repeat(tw - ((tab) % tw));
                        line.replace_range(tab..tab + 1, &spaces);
                    }
                    para_line_len.push(line.chars().count());
                    if line.trim().is_empty() {
                        para_line_indent.push(if i == 0 { 0 } else { para_line_indent[i - 1] });
                    } else {
                        let ws = line.chars().take_while(|c| *c == ' ').count();
                        para_line_indent.push(ws);
                    }
                    para_line_action.push(NONE);
                }

                // structural detection
                let mut is_table = false;
                let mut table_type = 0;
                let mut is_mailheader_para = false;
                let mut is_header = false;
                let mut is_custom_header = false;
                if !self.opts.custom_heading_regexp.is_empty() {
                    is_custom_header = self.is_custom_heading(&para_lines[0]);
                }
                if self.opts.make_tables && para_lines.len() > 1 {
                    table_type = self.get_table_type(&para_lines, para_len);
                    is_table = table_type != 0;
                }
                if !self.opts.explicit_headings && para_lines.len() > 1 && !is_table {
                    is_header = self.is_heading(&para_lines[0], Some(&para_lines[1]));
                }
                if self.opts.mailmode && !is_table && !is_custom_header {
                    is_mailheader_para = self.is_mailheader(&para_lines);
                }

                // end the list if we can end it
                if self.mode & LIST != 0
                    && (is_table || is_mailheader_para || is_header || is_custom_header)
                {
                    let mut list_end = String::new();
                    let mut action = 0;
                    let nl = self.listnum;
                    self.endlist(nl, &mut list_end, &mut action);
                    done_lines.push(list_end);
                    self.prev_para_action |= END;
                }

                // end the PRE if we can end it
                if self.mode & PRE != 0
                    && self.mode & PRE_EXPLICIT == 0
                    && (is_table || is_mailheader_para || !self.is_preformatted(&para_lines[0]))
                    && self.opts.preformat_trigger_lines != 0
                {
                    let tag = self.close_tag("pre");
                    let pre_end = format!("{tag}\n");
                    self.mode ^= PRE & self.mode;
                    done_lines.push(pre_end);
                    self.prev_para_action |= END;
                }

                // keep blank lines inside preformatted text
                if self.mode & PRE != 0 {
                    done_lines.push("\n".to_string());
                }

                // start-of-para structures that eat lines
                if is_custom_header {
                    let header = para_lines.remove(0);
                    para_line_len.remove(0);
                    para_line_indent.remove(0);
                    para_line_action.remove(0);
                    let mut header = header;
                    self.custom_heading(&mut header);
                    done_lines.push(header);
                    self.prev_para_action |= HEADER;
                } else if is_header {
                    let header = para_lines.remove(0);
                    para_line_len.remove(0);
                    para_line_indent.remove(0);
                    para_line_action.remove(0);
                    let underline = para_lines.remove(0);
                    para_line_len.remove(0);
                    para_line_indent.remove(0);
                    para_line_action.remove(0);
                    let mut header = header;
                    let mut underline = underline;
                    self.heading(&mut header, &mut underline);
                    done_lines.push(header);
                    self.prev_para_action |= HEADER;
                }

                // tables
                if self.opts.make_tables
                    && is_table
                    && self.tablestuff(table_type, &mut para_lines, para_len)
                {
                    done_lines.append(&mut para_lines);
                }

                // mailheader
                if is_mailheader_para && self.mode & TABLE == 0 && !para_lines.is_empty() {
                    let mut rows = para_lines.clone();
                    self.mailheader(&mut rows);
                    done_lines.extend(rows);
                    para_lines.clear();
                }

                // process line-by-line
                let mut prev = String::new();
                let mut prev_action = self.prev_para_action;
                let mut idx = 0;
                while idx < para_lines.len() {
                    if idx > 0 {
                        prev = para_lines[idx - 1].clone();
                        prev_action = para_line_action[idx - 1];
                    }
                    self.process_line(
                        &mut para_lines,
                        &mut para_line_action,
                        &mut para_line_indent,
                        &para_line_len,
                        idx,
                        &mut prev,
                        &mut prev_action,
                        is_fragment,
                    );
                    if idx == 0 {
                        if !prev.trim().is_empty() {
                            para_lines[0] = format!("{prev}{}", para_lines[0]);
                        }
                    } else {
                        para_lines[idx - 1] = prev.clone();
                        para_line_action[idx - 1] = prev_action;
                    }
                    idx += 1;
                }
                let last_action = para_line_action.last().copied().unwrap_or(NONE);
                para_action = last_action;
                para_line_action.clear();

                done_lines.extend(para_lines);

                // now put the para back together
                para = done_lines.join("");

                // XHTML: close an open paragraph
                if self.opts.xhtml && self.tags.last().map(|t| t == "p").unwrap_or(false) {
                    para.push_str(&self.close_tag("p"));
                }

                if self.opts.unhyphenation
                    && self
                        .re(r"[A-Za-z]\-\n\s*[A-Za-z]")
                        .is_match(&para)
                        .unwrap_or(false)
                    && (self.mode & (PRE | HEADER | MAILHEADER | TABLE | BREAK)) == 0
                {
                    self.unhyphenate_para(&mut para);
                }
                // chop trailing newlines for continuing lists and PRE
                if self.mode & LIST != 0 || self.mode & PRE != 0 {
                    while para.ends_with('\n') {
                        para.pop();
                    }
                }
            }
        }

        // apply links and bold/italic/underline formatting
        if !para.trim().is_empty() {
            self.apply_links(&mut para, &mut para_action);
        }

        if close_tags && self.mode & LIST != 0 {
            let nl = self.listnum;
            self.endlist(nl, &mut para, &mut para_action);
        }
        if close_tags && self.opts.xhtml {
            while !self.tags.is_empty() {
                para.push_str(&self.close_tag(""));
            }
        }

        // remaining Microsoft character codes -> HTML
        if self.opts.demoronize && !self.opts.eight_bit_clean {
            para = chars::demoronize_code(&para);
        }
        if !self.opts.eight_bit_clean {
            para = chars::entities(&para);
        }

        self.prev_para_action = para_action;
        para
    }

    #[allow(clippy::too_many_arguments)]
    fn process_line(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
        indents: &mut Vec<usize>,
        line_lens: &[usize],
        i: usize,
        prev: &mut String,
        prev_action: &mut u32,
        is_fragment: bool,
    ) {
        if self.opts.escape_html_chars {
            lines[i] = chars::escape(&lines[i]);
        }

        let next = if i + 1 < lines.len() {
            Some(lines[i + 1].clone())
        } else {
            None
        };

        if self.opts.mailmode && self.mode & PRE_EXPLICIT == 0 {
            self.mailquote(lines, actions, i, prev, prev_action, next.as_deref());
        }

        if self.mode & PRE != 0 && self.opts.preformat_trigger_lines != 0 {
            self.endpreformat(lines, actions, i, prev);
        }

        if self.mode & PRE == 0 {
            self.hrule(lines, actions, i);
        }
        if self.mode & PRE == 0 && !lines[i].trim().is_empty() {
            self.liststuff(lines, actions, indents, i, prev, prev_action);
        }
        if actions[i] & (HEADER | LIST) == 0
            && self.mode & (LIST | PRE) == 0
            && self.preformat_enabled
        {
            self.preformat(lines, actions, i, prev, prev_action, next.as_deref());
        }
        if self.mode & PRE == 0 {
            let line_indent = indents[i];
            let prev_indent = if i == 0 { 0 } else { indents[i - 1] };
            self.paragraph(
                lines,
                actions,
                indents,
                i,
                prev,
                prev_action,
                line_indent,
                prev_indent,
                is_fragment,
                i,
            );
        }
        if self.mode & (PRE | LIST) == 0 {
            let prev_line_len = if i == 0 { 0 } else { line_lens[i - 1] };
            self.shortline(lines, actions, i, prev, prev_action, prev_line_len);
        }
        if self.mode & PRE == 0 {
            self.caps(&mut lines[i], &mut actions[i]);
        }
    }

    // ------------------------------------------------------- file start / top-level

    pub fn do_file_start(&mut self, para: &str) -> String {
        let mut out = String::new();
        if !self.opts.extract {
            // split(/\n/, $para), not str::lines(): lines() also strips the \r
            // of a CRLF pair, so a CRLF file would lose it from --titlefirst
            // where the reference keeps it (TextToHTML.pm:5143).
            let first_line = para.split('\n').next().unwrap_or("").to_string();

            if !self.opts.doctype.is_empty() {
                // XHTML is checked first even though HTML5 is the default:
                // tests and front ends flip `opts.xhtml` on a converter that
                // already carries the default `html5: true`, and the doctype
                // that was asked for explicitly must be the one emitted. The
                // CLI mode flags keep the two exclusive anyway (cli.rs).
                if self.opts.xhtml {
                    out.push_str("<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Strict//EN\"\n");
                    out.push_str("\"http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd\">\n");
                    out.push_str(&self.get_tag(
                        "html",
                        TAG_START,
                        " xmlns=\"http://www.w3.org/1999/xhtml\"",
                    ));
                    out.push('\n');
                } else if self.opts.html5 {
                    // P5.1. HTML5 serialisation: the short doctype, and an
                    // <html> element with no namespace. Tag case follows
                    // lower_case_tags as everywhere else; the mode flags set
                    // it, so the default is lower-case HTML5. The charset meta
                    // below is forced on with the doctype: an HTML5 document
                    // without a declared encoding is the exact thing the mode
                    // exists to fix, and a validator warns about it.
                    out.push_str("<!DOCTYPE html>\n");
                    out.push_str(&self.get_tag("html", TAG_START, ""));
                    out.push('\n');
                } else {
                    out.push_str("<!DOCTYPE HTML PUBLIC \"");
                    out.push_str(&self.opts.doctype);
                    out.push_str("\">\n");
                    out.push_str(&self.get_tag("html", TAG_START, ""));
                    out.push('\n');
                }
            }
            self.head_inner = self.build_head(&first_line);
            out.push_str(&self.get_tag("head", TAG_START, ""));
            out.push('\n');
            out.push_str(&self.head_inner);
            out.push_str(&self.close_tag("head"));
            out.push('\n');
            let body_deco = self.opts.body_deco.clone();
            if !body_deco.is_empty() {
                out.push_str(&self.get_tag("body", TAG_START, &body_deco));
            } else {
                out.push_str(&self.get_tag("body", TAG_START, ""));
            }
            out.push('\n');
        }

        if !self.opts.prepend_file.is_empty() {
            if let Some(contents) = read_file_with(&self.opts.prepend_file, self.opts.encoding) {
                out.push_str(&contents);
            }
        }
        out
    }

    /// P5.5. The contents of the document `<head>`: the `<title>`, any
    /// `--append_head` file, the generator and charset metas, and the
    /// stylesheet link.
    ///
    /// Split out of `do_file_start` with no behaviour change: the returned
    /// string is exactly the bytes that used to be written between `<head>` and
    /// `</head>`. It also records the final escaped title in `self.title_text`
    /// so a template can use it as `{{textrill:title}}`.
    fn build_head(&mut self, first_line: &str) -> String {
        let mut out = String::new();

        // A title reaches the document by two routes, and they need
        // different treatment. An explicit `--title` is an option value
        // interpolated into element text with nothing stopping it from
        // closing the tag (A8) -- `--title '</title><script>alert(3)</script>'`
        // was emitted verbatim -- so it is always escaped, and `"` too
        // because an option value can reach an attribute. A `--titlefirst`
        // title is instead lifted out of the document's own first line, so
        // it is already document text, and the reference's rule for document
        // text (`escape_html_chars`) is both correct and sufficient: `<`, `>`
        // and `&` are all that can break out of element text.
        //
        // Escaping both with `escape_attr` would be wrong in a way the fuzzer
        // caught: it ignores `escape_html_chars`, so
        // `--titlefirst --no-escape_HTML_chars` on `a & b` emitted
        // `<title>a &amp; b</title>` where the reference emits
        // `<title>a & b</title>`. That is a Tier 1 divergence introduced by
        // an over-broad fix, so the derived route keeps the flag.
        let title_escaped = if self.opts.titlefirst && self.opts.title.is_empty() {
            // ($tit) = $first_line =~ /^ *(.*)/
            let tit = first_line.trim_start_matches(' ').to_string();
            let tit = tit.trim_end_matches(' ').to_string();
            if self.opts.escape_html_chars {
                self.opts.title = chars::escape(&tit);
            } else {
                self.opts.title = tit;
            }
            // Already escaped above; emitting it again would double escape.
            true
        } else {
            false
        };
        out.push_str(&self.get_tag("title", TAG_START, ""));
        if title_escaped {
            self.title_text = self.opts.title.clone();
        } else {
            self.title_text = chars::escape_attr(&self.opts.title);
        }
        out.push_str(&self.title_text);
        out.push_str(&self.close_tag("title"));
        out.push('\n');

        if self.opts.append_head_available() {
            if let Some(contents) = read_file_with(&self.opts.append_head, self.opts.encoding) {
                out.push_str(&contents);
            }
        }

        if self.opts.lower_case_tags {
            out.push_str(&self.get_tag(
                "meta",
                TAG_EMPTY,
                &format!(" name=\"generator\" content=\"{PROG} v{VERSION}\""),
            ));
        } else {
            out.push_str(&self.get_tag(
                "meta",
                TAG_EMPTY,
                &format!(" NAME=\"generator\" CONTENT=\"{PROG} v{VERSION}\""),
            ));
        }
        // P7.4. Optional, off by default, because the reference emits no
        // charset declaration and byte-identical output is a stated goal of
        // this port -- turning this on by default would move every golden.
        // A GUI turns it on, because there the consumer is a browser that
        // is about to guess, and it will guess wrong for exactly the
        // CP1252-derived input this port decodes.
        // The line break belongs to the element, not to the option: the
        // generator meta above is written without one, and adding this
        // second meta after it means someone has to. When meta_charset is
        // off the push below is the only newline, so the generator keeps
        // the exact single trailing newline it has always had and no golden
        // moves.
        if self.opts.meta_charset || self.opts.html5 {
            out.push('\n');
            out.push_str(&self.get_tag(
                "meta",
                TAG_EMPTY,
                if self.opts.lower_case_tags {
                    " charset=\"utf-8\""
                } else {
                    " CHARSET=\"utf-8\""
                },
            ));
        }
        out.push('\n');
        if !self.opts.style_url.is_empty() {
            let style_url = chars::escape_attr(&self.opts.style_url);
            if self.opts.lower_case_tags {
                out.push_str(&self.get_tag(
                    "link",
                    TAG_EMPTY,
                    &format!(" rel=\"stylesheet\" type=\"text/css\" href=\"{style_url}\""),
                ));
            } else {
                out.push_str(&self.get_tag(
                    "link",
                    TAG_EMPTY,
                    &format!(" REL=\"stylesheet\" TYPE=\"text/css\" HREF=\"{style_url}\""),
                ));
            }
            out.push('\n');
        }
        out
    }

    /// Convert the whole input (could already be pre-split paragraphs)
    /// through the full conversion pipeline.
    ///
    /// A9: an input file that cannot be read is reported to the caller instead
    /// of being skipped. The reference prints `Could not open …` to stderr and
    /// carries on, which means the process exits 0 having written a 0-byte
    /// output file -- a Makefile or CI job reads that as success. This returns
    /// `Err` so the caller can exit non-zero.
    ///
    /// The *output* is unchanged, so no golden moves: a file that cannot be read
    /// contributes nothing either way. When several inputs are given, the ones
    /// that are readable are still converted and the error carries that partial
    /// output, so this differs from the reference only in the exit code.
    pub fn try_convert(&mut self) -> Result<String, UnreadableInput> {
        let mut sources: Vec<String> = Vec::new();
        let source_type;
        let mut unreadable: Vec<String> = Vec::new();
        // P7.3. The most notable encoding seen across the inputs, so
        // `resolved_encoding` can report the one that actually mattered. UTF-8
        // is the floor: an ASCII file is valid UTF-8, so it must not be reported
        // as some legacy encoding merely because another input in the same run
        // was.
        let mut worst = Resolved::Utf8;
        if !self.opts.infile.is_empty() {
            source_type = "file".to_string();
            for f in &self.opts.infile {
                if f == "-" {
                    // stdin provided by caller
                    let mut buf = String::new();
                    use std::io::Read;
                    let _ = std::io::stdin().read_to_string(&mut buf);
                    sources.push(buf);
                } else {
                    match read_with(f, self.opts.encoding) {
                        Some((c, resolved)) => {
                            if resolved.notice_rank() > worst.notice_rank() {
                                worst = resolved;
                            }
                            sources.push(c);
                        }
                        None => {
                            eprintln!("Could not open {f}\n");
                            // Kept going rather than bailing out, so that a
                            // multi-file invocation still converts the files
                            // that *are* readable. What changed is the exit
                            // code, not which files get read.
                            unreadable.push(f.clone());
                        }
                    }
                }
            }
        } else if !self.opts.instring.is_empty() {
            source_type = "string".to_string();
            sources = self.opts.instring.clone();
        } else {
            return Ok(String::new());
        }

        self.resolved = worst;
        let out = self.convert_sources(sources, source_type == "string");
        if unreadable.is_empty() {
            Ok(out)
        } else {
            Err(UnreadableInput { unreadable, out })
        }
    }

    /// The encoding [`Converter::try_convert`] resolved the input with, for a
    /// caller that wants to say so — a status bar, a `--verbose` line, or a GUI
    /// that has to write the text back out in the encoding it came in.
    ///
    /// Only file inputs carry an encoding. `instring` and `process_chunk` hand
    /// over a `str`, which is already decoded, so this reports `Utf8` for them
    /// because UTF-8 is the encoding the port writes.
    pub fn resolved_encoding(&self) -> Resolved {
        self.resolved
    }

    /// As [`Converter::try_convert`], but discarding the unreadable-file error
    /// so the output is always produced. This is the behaviour the reference
    /// has; it is what the Python bindings and the in-process tests use, and it
    /// is kept so that A9 is a change to the CLI's exit code only.
    pub fn convert(&mut self) -> String {
        match self.try_convert() {
            Ok(out) => out,
            Err(e) => e.out,
        }
    }

    /// Convert text held in memory exactly as if it had been read from a
    /// file: paragraph records, the file-start document header, the
    /// append/prepend files and so on. This is what the Python bindings use.
    pub fn convert_text(&mut self, text: &str) -> String {
        self.convert_sources(vec![text.to_string()], false)
    }

    fn convert_sources(&mut self, sources: Vec<String>, string_mode: bool) -> String {
        self.notes_error = None;
        let (start, body, tail) = self.convert_to_parts(sources, string_mode);
        // P5.3. Number before sectioning so the TOC labels carry the numbers.
        let body = if self.opts.number_headings {
            crate::section::number_headings(&body)
        } else {
            body
        };
        // P5.2. Sectioning is a pure post-pass over the body, so the reference
        // path is untouched when all these flags are off. The TOC is kept
        // separate from the sectioned body so a template can place it.
        let (toc, body) = if self.opts.section || self.opts.toc {
            crate::section::sectionize_parts(&body, self.opts.toc)
        } else {
            (String::new(), body)
        };
        // Collect the note sets over the finished body, so definition content
        // is rendered markup rather than source text. On failure the body is
        // left exactly as it was and the error is recorded: the caller writes
        // nothing, so a refused document produces no output at all.
        let (stripped, citations, glossary) = match self.collect_notes(&body) {
            Ok(n) => n,
            Err(e) => {
                self.notes_error = Some(e);
                (String::new(), String::new(), String::new())
            }
        };
        let body = if stripped.is_empty() { body } else { stripped };
        // P5.5. A template replaces the default arrangement. `--body_template`
        // wraps the body inside the engine's own prolog and epilog; a
        // `--document_template` owns the whole page, so neither is emitted.
        if !self.template_text.is_empty() {
            return self.apply_template(&start, &body, &toc, &tail, &citations, &glossary);
        }
        let mut body = body;
        // Without a template there is nowhere to put the lists, so they go at
        // the end of the body -- after the prose, where a reader expects an
        // endnotes section, and inside `--extract`'s output too.
        body.push_str(&citations);
        body.push_str(&glossary);
        let mut out = String::with_capacity(start.len() + toc.len() + body.len() + tail.len());
        out.push_str(&start);
        out.push_str(&toc);
        out.push_str(&body);
        out.push_str(&tail);
        out
    }

    /// Collect and validate the note sets, returning the finished lists.
    ///
    /// Returns `(stripped body, citations HTML, glossary HTML)`. All three are
    /// empty when both modes are off, which is the default and must leave the
    /// body untouched.
    fn collect_notes(&self, body: &str) -> Result<(String, String, String), String> {
        if !self.opts.citations && !self.opts.glossary {
            return Ok((String::new(), String::new(), String::new()));
        }
        let (stripped, notes) =
            crate::notes::collect(body, (self.opts.citations, self.opts.glossary));
        crate::notes::validate(&notes)?;
        Ok((
            stripped,
            notes.render_one(crate::notes::Kind::Citation),
            notes.render_one(crate::notes::Kind::Glossary),
        ))
    }

    /// P5.5. Fill the active template's slots and assemble the page.
    ///
    /// `{{textrill:content}}` is the sectioned body (with the TOC already
    /// separated out), `{{textrill:toc}}` the generated navigation,
    /// `{{textrill:title}}` and `{{textrill:head}}` the escaped title and the
    /// `<head>` contents the engine produced, and `{{textrill:pager}}` empty
    /// (pagers belong to `--chunk`, which is refused with a template).
    fn apply_template(
        &self,
        start: &str,
        body: &str,
        toc: &str,
        tail: &str,
        citations: &str,
        glossary: &str,
    ) -> String {
        // A template that names neither slot would silently drop the notes, so
        // the fallback is the same end-of-body placement the untemplated path
        // uses. Falling back keeps a template working without change when the
        // author has no notes; requiring the slots would not.
        let has_slot = |name: &str| {
            self.template_text
                .contains(&format!("{{{{textrill:{name}}}}}"))
        };
        let mut body = body.to_string();
        if !citations.is_empty() && !has_slot("citations") {
            body.push_str(citations);
        }
        if !glossary.is_empty() && !has_slot("glossary") {
            body.push_str(glossary);
        }
        let mut slots = vec![
            ("content", body.as_str()),
            ("toc", toc),
            ("title", self.title_text.as_str()),
            ("head", self.head_inner.as_str()),
            ("pager", ""),
            ("citations", citations),
            ("glossary", glossary),
        ];
        // P5.5. User parameters extend the fixed set with `var:name` keys, owned by
        // this frame so the borrows stay live through `apply`. The values go in
        // verbatim, exactly as `--var` bound them.
        let var_keys: Vec<String> = self
            .opts
            .vars
            .iter()
            .map(|(name, _)| format!("var:{name}"))
            .collect();
        for (i, (_, value)) in self.opts.vars.iter().enumerate() {
            slots.push((var_keys[i].as_str(), value.as_str()));
        }
        let filled = crate::template::apply(&self.template_text, &slots);
        if self.document_template {
            return filled;
        }
        let mut out = String::with_capacity(start.len() + filled.len() + tail.len());
        out.push_str(start);
        out.push_str(&filled);
        out.push_str(tail);
        out
    }

    /// One top-level page produced by `--chunk`: a full document and the file
    /// name it should be written to.
    ///
    /// The name is suggested by the caller's `--outfile`; this method only
    /// derives it. See [`Converter::try_convert_chunked`].
    fn convert_to_parts(
        &mut self,
        sources: Vec<String>,
        string_mode: bool,
    ) -> (String, String, String) {
        let mut start = String::new();
        let mut body = String::new();
        let mut count = 0;
        for source in &sources {
            if string_mode {
                let mut para = source.clone();
                if let Some(stripped) = para.strip_suffix('\n') {
                    para = stripped.to_string();
                }
                if count == 0 {
                    start.push_str(&self.do_file_start(&para));
                }
                self.links.sect_once_done = vec![false; self.links.rules.len()];
                let p = self.process_chunk(&para, false, false);
                body.push_str(&p);
                body.push('\n');
                self.print_count += 1;
                count += 1;
                continue;
            }
            for rec in paragraph_records(source) {
                let mut para = rec;
                if let Some(stripped) = para.strip_suffix('\n') {
                    para = stripped.to_string();
                }
                if count == 0 {
                    start.push_str(&self.do_file_start(&para));
                }
                self.links.sect_once_done = vec![false; self.links.rules.len()];
                let p = self.process_chunk(&para, false, false);
                body.push_str(&p);
                body.push('\n');
                self.print_count += 1;
                count += 1;
            }
        }

        self.finish_body(&mut body);
        let tail = self.finish_tail();
        (start, body, tail)
    }

    /// The state-dependent close-out appended to the body after the last
    /// paragraph: end an open list or preformatted block, close any remaining
    /// XHTML tags, then append `--append_file`.
    ///
    /// Shared by the buffered path and [`Converter::convert_stream`] so the two
    /// cannot drift.
    fn finish_body(&mut self, body: &mut String) {
        if self.mode & LIST != 0 {
            let nl = self.listnum;
            self.endlist(nl, body, &mut 0);
        }
        if self.mode & PRE != 0 {
            let tag = self.close_tag("pre");
            body.push_str(&tag);
        }
        if self.opts.xhtml && !self.opts.extract && !self.tags.is_empty() {
            let mut open_tag = self.tags.last().cloned().unwrap_or_default();
            while !self.tags.is_empty() && open_tag != "body" && open_tag != "html" {
                body.push_str(&self.close_tag(""));
                open_tag = self.tags.last().cloned().unwrap_or_default();
            }
            body.push('\n');
        }
        if !self.opts.append_file.is_empty() {
            if let Some(contents) = read_file_with(&self.opts.append_file, self.opts.encoding) {
                body.push_str(&contents);
            }
        }
    }

    /// The document footer: the `body` and `html` close tags, unless the output
    /// was extracted or nothing was printed.
    fn finish_tail(&mut self) -> String {
        let mut tail = String::new();
        if self.print_count > 0 && !self.opts.extract {
            tail.push_str(&self.close_tag("body"));
            tail.push('\n');
            tail.push_str(&self.close_tag("html"));
            tail.push('\n');
        }
        tail
    }

    /// P5.4. Convert `input` to `out` one paragraph at a time.
    ///
    /// The engine's only cross-paragraph state lives in `self` (list number,
    /// open tags, section headers, link rules), so feeding the records from a
    /// reader in order produces exactly the bytes [`Converter::convert_text`]
    /// would produce from the same text — while holding only one paragraph at a
    /// time instead of the whole document and its markup. The record boundary
    /// is the reference's `$/ = ""` paragraph mode; see [`ParagraphReader`].
    ///
    /// Input must be valid UTF-8; a UTF-16/UTF-32 byte-order mark or NUL
    /// structure, or any byte sequence that is not valid UTF-8, is an
    /// [`io::ErrorKind::InvalidData`] error rather than a silent
    /// replacement-`char` — because the buffered `Auto` path would have decoded
    /// such a file as CP1252, and quietly emitting something different is the
    /// one thing this path must not do. The caller is responsible for refusing
    /// whole-body post-passes (`--number_headings`, `--section`, `--toc`,
    /// `--chunk`) and `--instring`, which need the assembled body.
    pub fn convert_stream<R: BufRead, W: Write>(
        &mut self,
        mut input: R,
        out: &mut W,
    ) -> io::Result<()> {
        // Refuse a wide encoding up front, before a byte is written. The NUL
        // structure that marks UTF-16 is decodable as UTF-8 (a NUL is U+0000),
        // so per-line validation alone would let a UTF-16 file through with a
        // NUL between every character.
        let head = input.fill_buf()?;
        if let Some((encoding, _)) = bom(head) {
            if encoding != Encoding::Utf8 {
                return Err(not_utf8_error());
            }
        }
        if sniff_utf16_or_32(head).is_some() {
            return Err(not_utf8_error());
        }
        self.resolved = Resolved::Utf8;
        let mut reader = ParagraphReader::new(input);
        let mut rec = String::new();
        let mut first = true;
        while reader.next_record(&mut rec)? {
            let para = rec.strip_suffix('\n').unwrap_or(&rec);
            if first {
                let start = self.do_file_start(para);
                out.write_all(start.as_bytes())?;
                first = false;
            }
            self.links.sect_once_done = vec![false; self.links.rules.len()];
            let body = self.process_chunk(para, false, false);
            out.write_all(body.as_bytes())?;
            out.write_all(b"\n")?;
            self.print_count += 1;
        }
        let mut body = String::new();
        self.finish_body(&mut body);
        out.write_all(body.as_bytes())?;
        let tail = self.finish_tail();
        out.write_all(tail.as_bytes())?;
        Ok(())
    }

    /// Phase 5.2. Convert and split the body into one full document per
    /// top-level section, for `--chunk`.
    ///
    /// "Top-level" is the shallowest heading level present in the body; its
    /// subsections are kept in the same page. Body content before the first
    /// heading becomes the first page. The `(name, html)` pairs are returned in
    /// document order; the caller writes them.
    ///
    /// Only `infile` inputs are chunked. `--chunk` with `--extract`, with
    /// stdout, or with `instring` is a caller error and is reported by the
    /// command line, not here.
    pub fn try_convert_chunked(&mut self) -> (Vec<(String, String)>, Vec<String>) {
        let mut sources: Vec<String> = Vec::new();
        let mut unreadable: Vec<String> = Vec::new();
        let mut worst = Resolved::Utf8;
        for f in &self.opts.infile {
            if f == "-" {
                let mut buf = String::new();
                use std::io::Read;
                let _ = std::io::stdin().read_to_string(&mut buf);
                sources.push(buf);
            } else {
                match read_with(f, self.opts.encoding) {
                    Some((c, resolved)) => {
                        if resolved.notice_rank() > worst.notice_rank() {
                            worst = resolved;
                        }
                        sources.push(c);
                    }
                    None => {
                        eprintln!("Could not open {f}\n");
                        unreadable.push(f.clone());
                    }
                }
            }
        }
        self.resolved = worst;
        let (start, body, tail) = self.convert_to_parts(sources, false);
        let files = self.assemble_chunked(&start, &body, &tail);
        (files, unreadable)
    }

    /// Split an already-built body into pages and assemble each into a full
    /// document. Shared by [`Converter::try_convert_chunked`].
    fn assemble_chunked(&self, start: &str, body: &str, tail: &str) -> Vec<(String, String)> {
        let (preamble, sections) = crate::section::split_sections(body);
        if sections.is_empty() {
            let mut single = String::with_capacity(start.len() + body.len() + tail.len());
            single.push_str(start);
            single.push_str(body);
            single.push_str(tail);
            return vec![(self.chunk_filename(1), single)];
        }
        let top = sections.iter().map(|s| s.level).min().unwrap_or(1);
        let mut pages: Vec<String> = Vec::new();
        // The `chunk-N` id of the top-level section each page holds, in page
        // order. Needed so a cross-file TOC can point *into* a page rather than
        // only at its top, and so `--section` has something to do here.
        let mut page_ids: Vec<String> = Vec::new();
        let mut leading = String::new();
        let mut current: Option<String> = None;
        let mut current_id = String::new();
        for s in &sections {
            if s.level == top {
                if let Some(page) = current.take() {
                    pages.push(page);
                    page_ids.push(std::mem::take(&mut current_id));
                }
                let mut page = std::mem::take(&mut leading);
                page.push_str(&s.html);
                current = Some(page);
                current_id = s.id.clone();
            } else if let Some(page) = current.as_mut() {
                page.push_str(&s.html);
            } else {
                leading.push_str(&s.html);
            }
        }
        if let Some(page) = current {
            pages.push(page);
            page_ids.push(current_id);
        } else if !leading.trim().is_empty() {
            // No top-level section at all, which cannot happen while `top` is the
            // minimum level present, but a page still needs an id to be a target.
            pages.push(leading);
            page_ids.push(String::new());
        }
        if !preamble.is_empty() {
            if let Some(first) = pages.first_mut() {
                first.insert_str(0, &preamble);
            } else {
                pages.push(preamble);
            }
        }
        let names: Vec<String> = (1..=pages.len()).map(|n| self.chunk_filename(n)).collect();
        // Links between sibling files are bare file names, not the (possibly
        // absolute) write path, so the pages stay portable if moved together.
        let links: Vec<String> = (1..=pages.len()).map(|n| self.chunk_basename(n)).collect();
        let toc = if self.opts.toc {
            Some(self.render_page_toc(&sections, top, &links, &page_ids))
        } else {
            None
        };
        pages
            .iter()
            .enumerate()
            .map(|(i, page)| {
                let mut out = String::with_capacity(start.len() + page.len() + tail.len() + 256);
                out.push_str(start);
                if let Some(toc) = &toc {
                    out.push_str(toc);
                }
                // `--section` under `--chunk` used to be silently ignored, because
                // a page *is* one top-level section and there was nothing left to
                // wrap.
                //
                // The wrapper is emitted for `--toc` as well as `--section`, and
                // that is the point: it is the anchor the TOC's own `file#chunk-N`
                // points at. Single-file `--toc` already implies its targets, since
                // `sectionize_parts` wraps whenever either flag is set, and a TOC
                // that can emit a dangling link is worse than a redundant `<article>`.
                if (self.opts.section || self.opts.toc) && !page_ids[i].is_empty() {
                    out.push_str("<article class=\"section\" id=\"");
                    out.push_str(&page_ids[i]);
                    out.push_str("\">\n");
                    out.push_str(page);
                    if !page.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str("</article>\n");
                } else {
                    out.push_str(page);
                }
                out.push_str(&self.render_pager(&links, i));
                out.push_str(tail);
                (names[i].clone(), out)
            })
            .collect()
    }

    /// Base file name for page `n`: `<stem>-chunk-NN.html`.
    fn chunk_basename(&self, n: usize) -> String {
        let path = std::path::Path::new(&self.opts.outfile);
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "output".to_string());
        format!("{stem}-chunk-{n:02}.html")
    }

    /// Write path for page `n`: its base name next to `--outfile`.
    fn chunk_filename(&self, n: usize) -> String {
        let name = self.chunk_basename(n);
        match std::path::Path::new(&self.opts.outfile).parent() {
            Some(dir) if !dir.as_os_str().is_empty() => {
                dir.join(name).to_string_lossy().into_owned()
            }
            _ => name,
        }
    }

    /// TOC across files: each top-level page links to its own file.
    fn render_page_toc(
        &self,
        sections: &[crate::section::Section],
        top: usize,
        names: &[String],
        page_ids: &[String],
    ) -> String {
        let mut out = String::from("<nav class=\"toc\" id=\"toc\">\n<ol class=\"toc-list\">\n");
        let mut page = 0usize;
        for s in sections {
            if s.level == top {
                out.push_str("<li class=\"toc-h");
                out.push_str(&s.level.to_string());
                out.push_str("\"><a href=\"");
                out.push_str(&names[page]);
                // Deep-link into the page. Harmless when the page has no anchor
                // of its own, and the fragment is skipped rather than emitted
                // empty so the href stays a clean relative path.
                if let Some(id) = page_ids.get(page).filter(|id| !id.is_empty()) {
                    out.push('#');
                    out.push_str(id);
                }
                out.push_str("\">");
                out.push_str(&s.label);
                out.push_str("</a></li>\n");
                page += 1;
            }
        }
        out.push_str("</ol>\n</nav>\n");
        out
    }

    /// Prev/next links between chunked pages.
    fn render_pager(&self, names: &[String], current: usize) -> String {
        let mut out = String::from("<nav class=\"pager\" aria-label=\"Parts\">\n");
        if current > 0 {
            out.push_str("<a href=\"");
            out.push_str(&names[current - 1]);
            out.push_str("\" rel=\"prev\">&#8592; part ");
            out.push_str(&(current).to_string());
            out.push_str("</a>\n");
        }
        if current + 1 < names.len() {
            out.push_str("<a href=\"");
            out.push_str(&names[current + 1]);
            out.push_str("\" rel=\"next\">part ");
            out.push_str(&(current + 2).to_string());
            out.push_str(" &#8594;</a>\n");
        }
        out.push_str("</nav>\n");
        out
    }
}

impl Options {
    fn append_head_available(&self) -> bool {
        !self.append_head.is_empty()
    }
}

/// The condition the `\B` assertions in `\B delim ([A-Za-z]) delim \B`
/// place on the characters flanking a match, expressed in code.
///
/// A `\B` position is not a word boundary, i.e. the two sides agree on
/// word-ness. One side is always the delimiter itself, so the neighbouring
/// character must be a word character exactly when the delimiter is; `_` is
/// the delimiter in common use that is a word character, which is why the
/// test asserts `_a_` is *not* turned into markup while ` #a# ` is.
///
/// The neighbours are examined a byte at a time, which is what Perl's `\b`
/// does on the bytes it was given: every byte of a multi-byte character is
/// `>= 0x80` and so is not a word byte. The delimiter's own word-ness has to
/// come from the character, though -- `delim as u8` truncates `é` to `0xE9`,
/// which is not a byte that appears in its UTF-8 encoding.
fn non_boundary(delim: char) -> impl Fn(&str, usize, usize) -> bool {
    let d_word = delim_is_word(delim);
    move |t: &str, s: usize, e: usize| {
        let left_word = s > 0 && is_word_byte(t.as_bytes()[s - 1]);
        let right_word = e < t.len() && is_word_byte(t.as_bytes()[e]);
        left_word == d_word && right_word == d_word
    }
}

/// `\w` (word char) on a **byte**, which is what Perl's `\b` sees when it is
/// handed a byte string: ASCII alphanumeric or underscore. Every byte of a
/// multi-byte character is `>= 0x80` and so is not a word byte, which is why
/// this is the right test for a *neighbour* even when the text is UTF-8.
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The same question about a delimiter, which has to be asked of the
/// character rather than a byte: `d as u8` truncates `é` to `0xE9`, and
/// `0xE9` is not a byte that appears in `é`'s own UTF-8 encoding. No
/// non-ASCII character is ever a word character.
fn delim_is_word(delim: char) -> bool {
    delim.is_ascii() && is_word_byte(delim as u8)
}

/// `(?<![delim])` from the single-character general pattern: the delimiter
/// must not be the character immediately before the match.
///
/// The check is on the preceding *character*. `delim as u8` truncates the code
/// point -- `é` becomes `0xE9`, which is not a byte of its own UTF-8 encoding
/// -- so a byte-wise test never rejects anything, and `ééwordé` came out
/// marked up where Perl leaves it alone.
fn not_preceded_by(delim: char) -> impl Fn(&str, usize, usize) -> bool {
    move |t: &str, s: usize, _e: usize| !t[..s].ends_with(delim)
}

/// `(?<![_A-Za-z0-9])` from the underscore pattern.
fn not_preceded_by_word_or_underscore(t: &str, s: usize, _e: usize) -> bool {
    s == 0 || !(t.as_bytes()[s - 1] == b'_' || is_word_byte(t.as_bytes()[s - 1]))
}

/// `(?<!delim)` from the multi-character pattern, where the delimiter is a
/// literal string and so may be more than one byte long.
fn not_preceded_by_str(delim: String) -> impl Fn(&str, usize, usize) -> bool {
    let d = delim.into_bytes();
    move |t: &str, s: usize, _e: usize| -> bool {
        s < d.len() || t.as_bytes()[s - d.len()..s] != *d
    }
}

/// The bold pattern's `(?![^#]*(?:<li>|<LI>|<P>|<p>))`, applied in code.
///
/// It only ever inspects the match's own interior: every character of the
/// group is `[^#]`, so the `[^#]*` inside the assertion cannot reach past the
/// closing `#` -- it starts one character in, after the group's leading
/// `[^\d#]`, and stops at that `#`.
///
/// Byte-wise on purpose. The tags are ASCII and no UTF-8 continuation byte can
/// be part of one, so this is exactly equivalent -- and unlike slicing a `str`
/// it cannot panic when `s + 2` is not a character boundary, which it need not
/// be in a paragraph of 8-bit characters.
fn no_list_or_para_tag(t: &str, s: usize, e: usize) -> bool {
    none_of(
        t.as_bytes(),
        s + 2,
        e - 1,
        &[b"<li>", b"<LI>", b"<P>", b"<p>"],
    )
}

/// UTF-8 length in bytes of the character starting at `byte_pos` in `text`.
fn char_len(text: &str, byte_pos: usize) -> usize {
    match text[byte_pos..].chars().next() {
        Some(c) => c.len_utf8(),
        None => 1,
    }
}

/// Whether none of `needles` occurs in `hay[from..to]`.
///
/// Byte-wise, so `from`/`to` may fall inside a multi-byte character (which
/// is what a regex match position can do inside a run of 8-bit text) without
/// panicking the way slicing a `str` would. Every caller passes ASCII
/// needles, which no UTF-8 continuation byte can be part of, so the answer
/// is the same as a `str` search.
fn none_of(hay: &[u8], from: usize, to: usize, needles: &[&[u8]]) -> bool {
    let window = &hay[from..to];
    !needles
        .iter()
        .any(|n| window.windows(n.len()).any(|w| w == *n))
}

fn class_body(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            ']' => out.push_str("\\]"),
            '\\' => out.push_str("\\\\"),
            '^' => out.push_str("\\^"),
            '-' => out.push_str("\\-"),
            '[' => out.push_str("\\["),
            _ => out.push(c),
        }
    }
    out
}

/// Compile a (fragment) table regexp with ASCII semantics and dotall.
fn table_re(pat: &str) -> Regex {
    let translated = links::translate_pattern(pat);
    Regex::new(&format!("(?s){translated}")).unwrap()
}

fn is_pgsql_table(rows: &[String]) -> bool {
    // A PGSQL table must have at least 4 rows
    if rows.len() < 4 {
        return false;
    }
    let mut r: Vec<&String> = rows.iter().collect();
    // possible caption
    if !r[0].contains('|') && table_re(r"^\s*\w+").is_match(r[0]).unwrap_or(false) {
        r.remove(0);
    }
    if r.len() < 4 {
        return false;
    }
    if !table_re(r"^\s*\w+\s+\|\s+").is_match(r[0]).unwrap_or(false) {
        return false; // Colname |
    }
    if !table_re(r"^\s*[-]+[+][-]+").is_match(r[1]).unwrap_or(false) {
        return false; // ----+----
    }
    if !table_re(r"^\s*[^|]*\s+\|\s+")
        .is_match(r[2])
        .unwrap_or(false)
    {
        return false; // value |
    }
    if !table_re(r"\(\d+\s+rows\)")
        .is_match(r[r.len() - 1])
        .unwrap_or(false)
    {
        return false; // (N rows)
    }
    true
}

fn is_border_table(rows: &[String]) -> bool {
    // A BORDER table must have at least 5 rows
    if rows.len() < 5 {
        return false;
    }
    let mut r: Vec<&String> = rows.iter().collect();
    if !r[0].contains('|') && table_re(r"^\s*\w+").is_match(r[0]).unwrap_or(false) {
        r.remove(0);
    }
    if r.len() < 5 {
        return false;
    }
    if !table_re(r"^\s*[+][-]+[+][-]+[+][-+]*$")
        .is_match(r[0])
        .unwrap_or(false)
    {
        return false; // +----+----+
    }
    if !table_re(r"^\s*\|\s*\w+\s+\|\s+.*\|$")
        .is_match(r[1])
        .unwrap_or(false)
    {
        return false; // | Colname |
    }
    if !table_re(r"^\s*[+][-]+[+][-]+[+][-+]*$")
        .is_match(r[2])
        .unwrap_or(false)
    {
        return false; // +----+----+
    }
    if !table_re(r"^\s*\|\s*[^|]*\s+\|\s+.*\|$")
        .is_match(r[3])
        .unwrap_or(false)
    {
        return false; // | value |
    }
    if !table_re(r"^\s*[+][-]+[+][-]+[+][-+]*$")
        .is_match(r[r.len() - 1])
        .unwrap_or(false)
    {
        return false; // +----+----+
    }
    true
}

fn is_delim_table(rows: &[String]) -> bool {
    if rows.len() < 2 {
        return false;
    }
    let mut r: Vec<&String> = rows.iter().collect();
    // possible caption
    if !table_re(r"[^\w\s]").is_match(r[0]).unwrap_or(false)
        && table_re(r"^\s*\w+").is_match(r[0]).unwrap_or(false)
    {
        r.remove(0);
    }
    if r.len() < 2 {
        return false;
    }
    // find a possible delimiter
    let delim = if let Some(c) = table_re(r"^\s*([^A-Za-z0-9])")
        .captures(r[0])
        .ok()
        .flatten()
        .and_then(|c| c.get(1))
    {
        // get rid of ^ and [] and \
        let d = c.as_str().replace(['^', '[', ']', '\\'], "");
        if d.is_empty() {
            return false; // no delimiter after all
        }
        d
    } else {
        return false;
    };
    // There needs to be at least three delimiters in the row
    let dc = links::ascii_re(&format!("[{}]", delim));
    let total_num_delims = dc.find_iter(r[0]).flatten().count();
    if total_num_delims < 3 {
        return false;
    }
    // All rows must start and end with the delimiter
    // and have $total_num_delims number of them
    let re_start = table_re(&format!(r"^\s*[{delim}]"));
    let re_end = table_re(&format!(r"[{delim}]\s*$"));
    for row in &r {
        if !re_start.is_match(row).unwrap_or(false) {
            return false;
        }
        if !re_end.is_match(row).unwrap_or(false) {
            return false;
        }
        if dc.find_iter(row).flatten().count() != total_num_delims {
            return false;
        }
    }
    true
}

fn strip_leading_spaces(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut removed = 0;
    let mut idx = 0;
    while idx < chars.len() && removed < n {
        if chars[idx] == ' ' {
            removed += 1;
            idx += 1;
        } else {
            break;
        }
    }
    chars[idx..].iter().collect()
}

fn byte_slice(s: &str, start: usize, len: usize) -> &str {
    let bytes = s.as_bytes();
    let end = (start + len).min(bytes.len());
    std::str::from_utf8(&bytes[start..end]).unwrap_or("")
}

fn byte_len(s: &str) -> usize {
    s.len()
}

/// Split a string into paragraphs the way Perl's paragraph mode (`$/ = ""`)
/// does for LF text: on runs of blank (empty) lines.
/// The error [`Converter::convert_stream`] returns for input that is not UTF-8.
fn not_utf8_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "--stream needs UTF-8 input; use the buffered path for other encodings",
    )
}

/// Yields paragraph records from a reader the way [`paragraph_records`] does
/// from a string: a record ends at the first line that is exactly `"\n"`, blank
/// lines before a record are skipped, repeated blank lines are discarded, and a
/// line holding only whitespace is not blank.
///
/// Unlike [`paragraph_records`], the input is not held whole: each line is
/// validated as UTF-8 on its own. That is safe because a newline byte can never
/// occur inside a UTF-8 multi-byte sequence, so no character is split across a
/// line boundary.
struct ParagraphReader<R> {
    inner: R,
    line: Vec<u8>,
}

impl<R: BufRead> ParagraphReader<R> {
    fn new(inner: R) -> Self {
        ParagraphReader {
            inner,
            line: Vec::new(),
        }
    }

    /// Read the next record into `out`, returning `false` at end of input.
    fn next_record(&mut self, out: &mut String) -> io::Result<bool> {
        out.clear();
        loop {
            self.line.clear();
            if self.inner.read_until(b'\n', &mut self.line)? == 0 {
                return Ok(!out.is_empty());
            }
            let line = std::str::from_utf8(&self.line)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            if out.is_empty() {
                if line == "\n" {
                    continue;
                }
                out.push_str(line);
                continue;
            }
            out.push_str(line);
            if line == "\n" {
                return Ok(true);
            }
        }
    }
}

fn paragraph_records(s: &str) -> Vec<String> {
    // Perl `$/ = ""` paragraph slurp mode: a record ends at the first
    // blank (empty) line, that line's newline included.  Blank lines
    // before a record are skipped, repeated blank lines are discarded,
    // and a trailing blank run yields no record.  Whitespace-only lines
    // are not blank.
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in s.split_inclusive('\n') {
        if cur.is_empty() {
            if line == "\n" {
                continue;
            }
            cur.push_str(line);
            continue;
        }
        cur.push_str(line);
        if line == "\n" {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Does `\r?\n\r?\n` match starting at index `i`?
fn blank_sep_at(chars: &[char], i: usize) -> bool {
    let n = chars.len();
    let mut j = i;
    if j < n && chars[j] == '\r' {
        j += 1;
    }
    if j >= n || chars[j] != '\n' {
        return false;
    }
    j += 1;
    if j < n && chars[j] == '\r' {
        j += 1;
    }
    if j >= n || chars[j] != '\n' {
        return false;
    }
    true
}

/// Perl `split(/\r?\n\r?\n/, $s)`: keep leading and middle empty fields,
/// drop trailing empty fields.
fn split_blank_lines(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut out: Vec<String> = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < n {
        if blank_sep_at(&chars, i) {
            out.push(chars[start..i].iter().collect());
            let mut j = i;
            if chars[j] == '\r' {
                j += 1;
            }
            if chars[j] == '\n' {
                j += 1;
            }
            if j < n && chars[j] == '\r' {
                j += 1;
            }
            if j < n && chars[j] == '\n' {
                j += 1;
            }
            i = j;
            start = i;
        } else {
            i += 1;
        }
    }
    if start < n {
        out.push(chars[start..].iter().collect());
    }
    // Perl's `split` drops *every* trailing empty field, not just a missing
    // one. A string that is nothing but the separator therefore yields no
    // fields at all: `split(/\r?\n\r?\n/, "\r\n\r\n")` is empty, because
    // the only field is empty and it is trailing. The `if start < n` above
    // only covers the absent tail, so the empty field a separator at the very
    // end pushes in the loop was surviving here as a phantom paragraph --
    // which is E3, two stray blank lines from an input ending in a blank line.
    while out.last().is_some_and(|f| f.is_empty()) {
        out.pop();
    }
    out
}

// ---------------------------------------------------------------- helper extension

trait ReplAll {
    fn replace_all_captures(
        &self,
        text: &str,
        f: impl Fn(&fancy_regex::Captures) -> String,
    ) -> String;
}

impl ReplAll for Regex {
    fn replace_all_captures(
        &self,
        text: &str,
        f: impl Fn(&fancy_regex::Captures) -> String,
    ) -> String {
        let mut out = String::new();
        let mut last = 0;
        for m in self.captures_iter(text).flatten() {
            if let Some(full) = m.get(0) {
                out.push_str(&text[last..full.start()]);
                out.push_str(&f(&m));
                last = full.end();
            }
        }
        out.push_str(&text[last..]);
        out
    }
}
#[cfg(test)]
mod cr_chop_tests {
    use super::{chop_leading_cr, chop_trailing_cr, split_blank_lines};

    /// The patterns the two helpers replaced, compiled exactly as the engine
    /// compiled them.
    ///
    /// They have to go through `ascii_re_cached`, not `Regex::new`, because
    /// that is where Perl's `$` is restored: `$` here is rewritten to the
    /// lookahead `(?=\n?$)` so it matches before a trailing newline as Perl's
    /// does. A bare fancy-regex `$` is end-of-text only, so compiling the
    /// pattern directly would have made this test assert the wrong thing --
    /// it did, until the exhaustive pass caught it.
    fn re_trailing() -> &'static fancy_regex::Regex {
        crate::links::ascii_re_cached(r"[ \t]*\x0D$")
    }
    fn re_leading() -> &'static fancy_regex::Regex {
        crate::links::ascii_re_cached(r"^[ \t]*\x0D")
    }

    /// Exhaustive over the alphabet that matters here. The helpers are string
    /// surgery standing in for a regex, and the failure mode is a silent
    /// difference on one awkward input, so this checks all of them rather than
    /// a handful of examples someone thought of.
    #[test]
    fn matches_the_regex_it_replaced() {
        let alphabet = ['a', ' ', '\t', '\r', '\n'];
        let mut inputs: Vec<String> = vec![String::new()];
        // every string of length 0..=4 over the alphabet
        let mut level = inputs.clone();
        for _ in 0..4 {
            let mut next = Vec::new();
            for s in &level {
                for &c in &alphabet {
                    let mut t = s.clone();
                    t.push(c);
                    next.push(t);
                }
            }
            inputs.extend(next.iter().cloned());
            level = next;
        }
        assert_eq!(inputs.len(), 1 + 5 + 25 + 125 + 625);
        for s in &inputs {
            let want_t = re_trailing().replace(s, "").to_string();
            let want_l = re_leading().replace(s, "").to_string();
            assert_eq!(chop_trailing_cr(s), want_t, "trailing, input {s:?}");
            assert_eq!(chop_leading_cr(s), want_l, "leading, input {s:?}");
        }
    }

    /// E3. `split_blank_lines` stands in for `split(/\r?\n\r?\n/, $s)`,
    /// which is why the exhaustive treatment above did not reach it: the two
    /// helpers it covers are the only ones checked against their regex, and
    /// this one -- the one that decides where paragraphs begin and end -- was
    /// not. It is now, over the same alphabet, against the regex itself.
    ///
    /// Perl's trailing-empty-field rule is the part that is easy to get wrong
    /// and impossible to notice by reading: it is a property of `split` rather
    /// than of the pattern, and a hand-written splitter has no reason to know
    /// about it.
    #[test]
    fn split_blank_lines_matches_perl_split() {
        let seps = crate::links::ascii_re_cached(r"\r?\n\r?\n");
        let alphabet = ['a', ' ', '\r', '\n'];
        let mut inputs: Vec<String> = vec![String::new()];
        let mut level = inputs.clone();
        for _ in 0..5 {
            let mut next = Vec::new();
            for s in &level {
                for &c in &alphabet {
                    let mut t = s.clone();
                    t.push(c);
                    next.push(t);
                }
            }
            inputs.extend(next.iter().cloned());
            level = next;
        }
        for s in &inputs {
            // Perl semantics: split on every non-overlapping match, keeping
            // leading and middle empty fields, then drop trailing empties.
            let mut want: Vec<String> = seps
                .split(s)
                .map(|f| {
                    f.unwrap_or_else(|e| panic!("split failed on {s:?}: {e}"))
                        .to_string()
                })
                .collect();
            while want.last().is_some_and(|f| f.is_empty()) {
                want.pop();
            }
            assert_eq!(split_blank_lines(s), want, "input {s:?}");
        }
    }

    /// The E3 repro, pinned at the level it was reported: a document whose only
    /// record ends in a blank line must produce no paragraph from that
    /// separator. It used to produce one empty paragraph, which is where the two
    /// extra blank lines in the body came from.
    #[test]
    fn a_trailing_blank_line_is_not_a_paragraph() {
        // Every expected value below was produced by running
        // `split(/\r?\n\r?\n/, ...)` under Perl, not by reasoning about the
        // pattern. Reasoning got two of them wrong while writing this test,
        // which is why they are transcribed rather than derived.
        assert_eq!(split_blank_lines("\r\n\r\n"), Vec::<String>::new());
        assert_eq!(split_blank_lines("\n\n"), Vec::<String>::new());
        assert_eq!(split_blank_lines("\r\n\r\n\r\n\r\n"), Vec::<String>::new());
        // a separator at the very end leaves no paragraph behind it
        assert_eq!(split_blank_lines("a\r\n\r\n"), vec!["a"]);
        // ... but an empty field is kept whenever it is not the last one
        assert_eq!(split_blank_lines("\r\n\r\na"), vec!["", "a"]);
        assert_eq!(split_blank_lines("a\r\n\r\nb"), vec!["a", "b"]);
        // only one separator fits in `a\r\n\r\n\r\nb`, so the tail is one
        // field rather than an empty one plus a field
        assert_eq!(split_blank_lines("a\r\n\r\n\r\nb"), vec!["a", "\r\nb"]);
        assert_eq!(split_blank_lines("\r\n\r\n\r\n"), vec!["", "\r\n"]);
        assert_eq!(split_blank_lines("a\r\n\r\n\r"), vec!["a", "\r"]);
        assert_eq!(
            split_blank_lines("a\r\n\r\na\r\n\r\nb"),
            vec!["a", "a", "b"]
        );
    }

    #[test]
    fn trailing_chop_handles_crlf() {
        // `$` also matches before one trailing newline, so a DOS line ending
        // has to be recognised. A plain `ends_with('\r')` would miss these.
        assert_eq!(chop_trailing_cr("abc \r"), "abc");
        assert_eq!(chop_trailing_cr("abc \r\n"), "abc\n");
        assert_eq!(chop_trailing_cr("abc\r\n"), "abc\n");
        assert_eq!(chop_trailing_cr("\r"), "");
        assert_eq!(chop_trailing_cr("\r\n"), "\n");
        // no match: left exactly as it was
        assert_eq!(chop_trailing_cr("abc"), "abc");
        assert_eq!(chop_trailing_cr("abc\n"), "abc\n");
        assert_eq!(chop_trailing_cr(" abc "), " abc ");
    }

    #[test]
    fn leading_chop_needs_a_cr() {
        // Trimming leading whitespace unconditionally would destroy the
        // indentation of every paragraph in the document.
        assert_eq!(chop_leading_cr("  \tabc"), "  \tabc");
        assert_eq!(chop_leading_cr("  abc"), "  abc");
        assert_eq!(chop_leading_cr("  \rabc"), "abc");
        assert_eq!(chop_leading_cr("  \t\rabc"), "abc");
        assert_eq!(chop_leading_cr("\rabc"), "abc");
        assert_eq!(chop_leading_cr("  \r"), "");
    }
}

/// Differential test for the linear (`delim_replace`) delimiter substitution.
///
/// The `\B` and `(?<!...)` variants these replace drove fancy-regex's
/// backtracking VM, which explodes (or errors) on paragraphs of ~500 KB --
/// see the `do_delim` comment. The linear form must agree with the original
/// regexes on every input, so this compares them directly. The original
/// patterns run fine on the short strings used here; the explosion only
/// shows up at scale.
#[cfg(test)]
mod delim_linear_tests {
    use super::*;
    use crate::links::ascii_re;
    use crate::options::Options;

    fn conv() -> Converter {
        let mut opts = Options::default();
        opts.deal_with_options();
        Converter::new(opts)
    }

    /// The original `replace_all_captures`: iterate matches with
    /// `captures_iter`, i.e. what the engine used to do everywhere.
    fn orig_replace(re: &fancy_regex::Regex, text: &str, tag: &str) -> String {
        let mut out = String::new();
        let mut last = 0;
        for m in re.captures_iter(text).flatten() {
            if let Some(full) = m.get(0) {
                out.push_str(&text[last..full.start()]);
                out.push_str(&format!("<{tag}>{}</{tag}>", m.get(1).unwrap().as_str()));
                last = full.end();
            }
        }
        out.push_str(&text[last..]);
        out
    }

    pub(crate) fn check_pair(c: &mut Converter, text: &str, delim: char, tag: &str) {
        let db = class_body(&delim.to_string());
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");

        // single alpha char between delims, wrapped in \B assertions
        let re1_orig = ascii_re(&format!(r"\B[{db}]([A-Za-z])[{db}]\B"));
        let re1_lin = ascii_re(&format!(r"[{db}]([A-Za-z])[{db}]"));
        let expect = orig_replace(&re1_orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, &re1_lin, non_boundary(delim), ltag);
        assert_eq!(got, expect, "re1 mismatch for {text:?} delim={delim}");

        // multi-char content ending in a "word/punct" char, leading
        // (?<!delim)
        let re2_orig = ascii_re(&format!(
            r"(?<![{db}])[{db}]([^{db}]+?[A-Za-z0-9!-/:-@\[-`{{-~&<>])[{db}]"
        ));
        let re2_lin = ascii_re(&format!(
            r"[{db}]([^{db}]+?[A-Za-z0-9!-/:-@\[-`{{-~&<>])[{db}]"
        ));
        let expect = orig_replace(&re2_orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, &re2_lin, not_preceded_by(delim), ltag);
        assert_eq!(got, expect, "re2 mismatch for {text:?} delim={delim}");
    }

    #[test]
    fn linear_delim_matches_original_regexes() {
        let cases = [
            "",
            "a",
            "*",
            "**",
            "***",
            "*a*",
            "x*a*",
            "*a*y*",
            " *a* ",
            "**a*",
            "*a**",
            "*a*b*",
            "*a b*",
            "*a!*",
            "a*b*c*",
            "*ab*",
            "*  *",
            "*\t*",
            "*a*b*c*",
            "*a**b*",
            "*a*b**c*",
            "#a#",
            "x#a#",
            "#ab#",
            "**#a#",
            "#a**",
            "#a#b#",
            "_a_",
            "x_a_",
            "_a_b_",
            "__a_",
            "_ab_",
            "_a!_",
            "^a^",
            "x^a^",
            "^a^b^",
            "^^a^",
            "^a^^",
            "a*b_#c^d",
            "*a**",
            "3#4#",
            "_",
            "#",
            "^",
            "*",
            "a",
            " a ",
            "1*2*3",
            "*a*#b#_c_^d^",
            // A non-ASCII delimiter immediately before another one. The
            // assertion is `(?<!é)`, and the byte the code point truncates to
            // -- 0xE9 for `é` -- is not a byte that `é` actually contains, so
            // a byte-wise check rejects nothing and the assertion is
            // vacuous. `ééwordé` is the shape that came back from a customer
            // file marked up as emphasis where Perl leaves it alone.
            "ééwordé",
            "üüxü",
            "ééé",
            "üxüyü",
            "xééyéé",
            "ééwördé",
            "éé X üéü",
        ];
        let mut c = conv();
        for text in cases {
            for &(delim, tag) in DELIMS {
                check_pair(&mut c, text, delim, tag);
            }
        }
    }

    /// The delimiters exercised everywhere below. The last two are not ASCII,
    /// and they are here because the byte-oriented shortcut `delim as u8` is
    /// silently wrong for them: `é` truncates to `0xE9`, which is not a byte of
    /// its own UTF-8 encoding, so a preceding-delimiter test built on it never
    /// rejects anything and `ééwordé` got marked up where Perl leaves it.
    pub(crate) const DELIMS: &[(char, &str)] = &[
        ('*', "em"),
        ('#', "strong"),
        ('_', "u"),
        ('^', "em"),
        ('é', "em"),
        ('ü', "strong"),
    ];

    /// Randomized differential: the alphabet is the union of delimiter chars
    /// and word/punct characters, so the generator keeps producing the shapes
    /// the boundary and lookbehind conditions care about (adjacent delimiters,
    /// single-char content, delimiter right after a word char, ...).
    #[test]
    fn linear_delim_matches_on_random_strings() {
        let alphabet: &[char] = &[
            'a', 'b', 'Z', '0', '9', ' ', '\t', '\n', '!', '.', '-', '_', '#', '*', '^', '<', '>',
            '&', '"', '/', '=', '@', '`', '{', '}', '~',
        ];
        let mut c = conv();
        let mut state = 0x243F_6A88_85A3_08D3u64;
        let mut next = || {
            // xorshift64
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20_000 {
            let len = (next() % 24) as usize;
            let text: String = (0..len)
                .map(|_| alphabet[(next() as usize) % alphabet.len()])
                .collect();
            for &(delim, tag) in DELIMS {
                check_pair(&mut c, &text, delim, tag);
            }
        }
    }
}

/// Same differential check for the two remaining `do_delim` rewrites: the
/// bold `#...#` pattern and the underscore pattern, both of which lost a
/// lookaround to `delim_replace`'s `accept` callback.
#[cfg(test)]
mod delim_wide_linear_tests {
    use super::delim_linear_tests::check_pair;
    use super::*;
    use crate::links::{ascii_re, ascii_re_cached};
    use crate::options::Options;

    fn conv() -> Converter {
        let mut opts = Options::default();
        opts.deal_with_options();
        Converter::new(opts)
    }

    fn orig_replace(re: &fancy_regex::Regex, text: &str, tag: &str) -> String {
        let mut out = String::new();
        let mut last = 0;
        for m in re.captures_iter(text).flatten() {
            if let Some(full) = m.get(0) {
                out.push_str(&text[last..full.start()]);
                out.push_str(&format!("<{tag}>{}</{tag}>", m.get(1).unwrap().as_str()));
                last = full.end();
            }
        }
        out.push_str(&text[last..]);
        out
    }

    fn check_bold(c: &mut Converter, text: &str) {
        let tag = "strong";
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");
        let orig = ascii_re_cached(r"#([^\d#](?![^#]*(?:<li>|<LI>|<P>|<p>))[^#]*[^# \t\n])#");
        let lin = ascii_re_cached(r"#([^\d#][^#]*[^# \t\n])#");
        let no_tag = no_list_or_para_tag;
        let expect = orig_replace(orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, lin, no_tag, ltag);
        assert_eq!(got, expect, "bold re2 mismatch for {text:?}");
    }

    fn check_under(c: &mut Converter, text: &str) {
        let tag = "u";
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");
        let orig = ascii_re_cached(r#"(?<![_A-Za-z0-9])_([^_]+?[A-Za-z0-9"'.?&;:<>])_"#);
        let lin = ascii_re_cached(r#"_([^_]+?[A-Za-z0-9"'.?&;:<>])_"#);
        let not_word = not_preceded_by_word_or_underscore;
        let expect = orig_replace(orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, lin, not_word, ltag);
        assert_eq!(got, expect, "under re2 mismatch for {text:?}");
    }

    #[test]
    fn wide_linear_delim_matches_original_regexes() {
        let cases = [
            "",
            "#",
            "##",
            "###",
            "#a#",
            "x#a#",
            "#a#b#",
            "#ab cd#",
            "#1a#",
            "#a1#",
            "#<p>x#",
            "#x<p>y#",
            "#<li>#",
            "#<LI>x#",
            "#<P>#",
            "# a#",
            "#a #",
            "#a\t#",
            "#a\nb#",
            "##a##",
            "#a# #b#",
            "#<a href=\"#\">#",
            "_",
            "__",
            "_a_",
            "x_a_",
            "_a_b_",
            "__a_",
            "_ab cd_",
            "_a_ _b_",
            "_a b _",
            "_a!_",
            "_a b_",
            "9_a_",
            "a_b_",
            "_a_b_",
            "#x#_y_",
            "_#x#_",
            "#_a_#",
            "***",
            "#a#b#_c_",
            "# #",
            "#<p><p>#",
            "#<li> <P> <p> <LI>#",
            "#\t#",
            "#\n#",
            "#a# #",
            "#5#",
            "# a b #",
        ];
        let mut c = conv();
        for text in cases {
            check_bold(&mut c, text);
            check_under(&mut c, text);
        }
    }

    /// The multi-character delimiter branch, whose leading `(?<!delim)` also
    /// moved into `accept`. `d` is a metachar-free multi-character delimiter
    /// so the original pattern compiles the way it does for every delimiter
    /// Perl can actually handle.
    fn check_multi(c: &mut Converter, text: &str, delim: &str) {
        let tag = "em";
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");
        let d = fancy_regex::escape(delim);
        let orig = ascii_re(&format!(
            r#"(?<!{d}){d}((\w|["'])(\w|[-\s!-/:-@\[-`{{-~])*[^\s]){d}"#
        ));
        let lin = ascii_re(&format!(
            r#"{d}((\w|["'])(\w|[-\s!-/:-@\[-`{{-~])*[^\s]){d}"#
        ));
        let expect = orig_replace(&orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, &lin, not_preceded_by_str(d.to_string()), ltag);
        assert_eq!(got, expect, "multi mismatch for {text:?} delim={delim}");
    }

    /// `delim_loop` has to behave like the regex it replaces when a candidate
    /// is rejected: the engine retries one character along, so a match that
    /// starts *inside* the rejected span must still be found. Regression test
    /// for the `#`/`</p><p>` case the P3 fuzzer found -- the first `#` pair
    /// spans a paragraph boundary and is rejected, and the pair nested in it
    /// has to be picked up on the retry.
    #[test]
    fn wide_delim_retry_finds_match_inside_rejected_span() {
        let cases = [
            "#a</p><p>b#c#",
            "#a<p>b#c#",
            "#<li>x#y#",
            "#<P>x#y#",
            "#<p>#y#",
            "pre #a</p><p>b# mid #c#",
            "#a# #<p>b#c#",
            "_a</p><p>b_c_d_",
            "#<p>only#",
            "#<p>#",
            "##<p>x#y#",
            "#no close at all",
            "_a</p><p>b_c_d_e_",
        ];
        let mut c = conv();
        for text in cases {
            // bold path, including the in-link-context variant that selects
            // `delim_loop` in the first place
            let re = c.re(r"#([^\d#][^#]*[^# \t\n])#").clone();
            let no_tag = no_list_or_para_tag;
            let expect = orig_replace(
                ascii_re_cached(r"#([^\d#](?![^#]*(?:<li>|<LI>|<P>|<p>))[^#]*[^# \t\n])#"),
                text,
                "strong",
            );
            let got = c.delim_loop(&mut text.to_string(), &re, no_tag, "strong");
            assert_eq!(got, expect, "delim_loop bold mismatch for {text:?}");
        }
    }

    /// 8-bit characters: a regex match position can land in the middle of a
    /// multi-byte character, so any code that indexes a `str` by a match
    /// offset has to cope. Regression test for the panic
    /// "start byte index N is not a char boundary" that the bold rewrite hit
    /// on the reference's own `umlauttest` fixture.
    #[test]
    fn linear_delim_handles_multibyte_text() {
        let cases = [
            "ver\u{e4}ndern *ver\u{e4}ndern* zu",
            "#ver\u{e4}ndern# #\u{c4}NDERN# #a\u{fc}b#",
            "_geht_ _ver\u{e4}ndern_ _\u{c4}NDERN_",
            "#a\u{e4}b#<p>c#",
            "#a\u{e4}<p>b#",
            "#a\u{e4}<li>b#",
            "#a\u{20ac}b# #a\u{2192}b# #\u{1f600}#",
            "*\u{e4}* *\u{1f600}x*",
            "_\u{e4}_ _a\u{e4}b_",
            "\u{e4}#b#\u{e4}",
            "#\u{e4}b\u{e4}#",
            "#\u{e4}#",
            "@@\u{e4}b@@",
            "XY\u{e4}XY",
            "@@a\u{e4}@@",
            "XYa\u{e4}XY",
            "#\u{4e2d}\u{6587}#",
            "_\u{4e2d}\u{6587}_",
            "#a#\u{4e2d}#b#",
        ];
        let mut c = conv();
        for text in cases {
            check_bold(&mut c, text);
            check_under(&mut c, text);
            for delim in ["XY", "@@"] {
                check_multi(&mut c, text, delim);
            }
        }
    }

    /// Same, at random: multi-byte characters mixed with delimiters, so
    /// match offsets land off character boundaries often.
    #[test]
    fn linear_delim_handles_multibyte_random() {
        let alphabet: &[char] = &[
            'a', 'b', '0', ' ', '_', '#', '*', '^', 'X', 'Y', '@', '.', '\u{e4}', '\u{fc}',
            '\u{c4}', '\u{20ac}', '\u{4e2d}', '\u{2192}', '\n', '\t',
        ];
        // a delimiter is 0xC3 0xA9, which shares its trailing byte 0xA9 with
        // several other characters, so a byte-wise "is the previous character
        // the delimiter" test gets the wrong answer on exactly these inputs
        const NONASCII: &[(char, &str)] = &[('\u{e9}', "em"), ('\u{fc}', "strong")];
        let mut c = conv();
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20_000 {
            let len = (next() % 24) as usize;
            let text: String = (0..len)
                .map(|_| alphabet[(next() as usize) % alphabet.len()])
                .collect();
            check_bold(&mut c, &text);
            check_under(&mut c, &text);
            for delim in ["XY", "@@"] {
                check_multi(&mut c, &text, delim);
            }
            for (delim, tag) in NONASCII {
                check_pair(&mut c, &text, *delim, tag);
            }
        }
    }

    #[test]
    fn multi_linear_delim_matches_original_regex() {
        let cases = [
            "",
            "XY",
            "XYXY",
            "XYwordXY",
            "aXYwordXYb",
            "XYword XY",
            "XYword ",
            "XYword",
            "XY w XY",
            "XYwordXYXYwordXY",
            "xXYwordXY",
            "XYwordXYx",
            "@@w@@",
            "w@@w@@",
            "a@@b@@",
            "@@ab@@",
            "@@a b@@",
            "@@@@",
            "@@a@@b@@",
            "XY''XY",
            "XY'a'bXY",
            "XY\"x\"XY",
            "XY\"XY",
            "**bold**",
            "XYaXYbXY",
            " XYwordXY ",
            "\nXYwordXY\n",
            "XY\tXY",
            "@@1@@",
            "@@a1@@",
            "@@a!@@",
        ];
        let mut c = conv();
        for text in cases {
            for delim in ["XY", "@@"] {
                check_multi(&mut c, text, delim);
            }
        }
    }

    #[test]
    fn multi_linear_delim_matches_on_random_strings() {
        let alphabet: &[char] = &[
            'a', 'b', 'Z', '0', ' ', '\t', '\n', '!', '.', '-', '@', 'X', 'Y', '*', '"', '\'', '_',
            '#', '^', '<', '>', '&', '/', '=', '`', '{',
        ];
        let mut c = conv();
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20_000 {
            let len = (next() % 28) as usize;
            let text: String = (0..len)
                .map(|_| alphabet[(next() as usize) % alphabet.len()])
                .collect();
            for delim in ["XY", "@@"] {
                check_multi(&mut c, &text, delim);
            }
        }
    }

    #[test]
    fn wide_linear_delim_matches_on_random_strings() {
        let alphabet: &[char] = &[
            'a', 'b', 'Z', '0', '9', ' ', '\t', '\n', '!', '.', '-', '_', '#', '*', '^', '<', '>',
            '&', '"', '/', '=', '@', '`', '{', '}', '~', '?', ';', ':', '\'',
        ];
        let mut c = conv();
        let mut state = 0xB502_6F5A_A966_19E9u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20_000 {
            let len = (next() % 28) as usize;
            let text: String = (0..len)
                .map(|_| alphabet[(next() as usize) % alphabet.len()])
                .collect();
            check_bold(&mut c, &text);
            check_under(&mut c, &text);
        }
    }
}

#[cfg(test)]
mod re_cache_tests {
    use super::*;
    use crate::options::Options;

    /// The cap, restated here so the test does not have to reach into the
    /// engine for a constant. `re_cache_bounded_by_a_measured_maximum` asserts
    /// the two agree, so this cannot drift from the real limit unnoticed.
    const MAX: usize = 128;

    /// Options set so that each one that feeds a pattern gives a *different*
    /// pattern. Every pattern the engine builds from an option is derived from
    /// one of these, so with all of them varied the working set is as large as
    /// one conversion can make it.
    fn max_varied_options() -> Options {
        Options {
            default_link_dict: String::new(),
            bullets: "a_b_".to_string(),
            bullets_ordered: "z".to_string(),
            bold_delimiter: "@".to_string(),
            italic_delimiter: "\u{a3}".to_string(),
            underline_delimiter: "~".to_string(),
            hrule_min: 7,
            preformat_start_marker: "START".to_string(),
            preformat_end_marker: "STOP".to_string(),
            custom_heading_regexp: vec!["^H1".to_string(), "^H2".to_string()],
            ..Options::default()
        }
    }

    /// A document that reaches every construct that compiles a pattern:
    /// ordered and bulleted lists, definition lists, all four inline
    /// delimiters, tables, hrules, preformatted blocks, caps and short lines.
    fn maximal_document(sections: usize) -> String {
        let mut text = String::new();
        for i in 0..sections {
            text.push_str(&format!("--- section {i} ---\n"));
            text.push_str("H1 heading\nH2 heading\n");
            text.push_str("1. ordered\n* bullet\na_b_ x\nTerm: definition\n");
            text.push_str("@b@ \u{a3}i\u{a3} ~u~ #h# ^s^ |v| **w** <<n>>\n");
            text.push_str("-------\n");
            text.push_str("<pre>\npre line\n</pre>\n\n");
            text.push_str("START\npre start\nSTOP\n\n");
        }
        text
    }

    /// A10, part 1: the cache stays inside its cap.
    ///
    /// The growth is reachable, and it is worth being precise about where it
    /// comes from. Every other pattern the engine builds is either a literal in
    /// the source or derived from a single-valued option, so no document can
    /// grow the cache -- the same input twice produces the same entries, and
    /// `maximal_document` below shows a large document does not either. The one
    /// unbounded source is `custom_heading_regexp`, which is a user-supplied
    /// *list*: 500 patterns give 512 entries, because each is compiled and
    /// cached as it is tried.
    ///
    /// So this is insurance against a user option, not a fix for an attack on
    /// untrusted input, and the plan said as much. It is here for symmetry with
    /// `links::ascii_re_cached`, which bounds the same kind of thing at the same
    /// number.
    #[test]
    fn re_cache_bounded_by_a_measured_maximum() {
        let mut c = Converter::new(Options {
            default_link_dict: String::new(),
            // 400 patterns: comfortably past the cap, so the limit is reached
            // rather than approached.
            custom_heading_regexp: (0..400).map(|i| format!("^NOMATCH{i}")).collect(),
            ..Options::default()
        });
        // A line matching none of them, so all 400 are actually tried.
        c.convert_text("NOMATCH399 heading\n\nplain paragraph\n");

        assert!(
            c.re_cache.len() <= MAX,
            "the pattern cache grew to {} entries, past its cap of {MAX}",
            c.re_cache.len()
        );
    }

    /// A10, part 2: the cap is above the real working set, so a normal
    /// conversion never hits it and never pays for a clear.
    ///
    /// This is the test that stops the cap from becoming a performance
    /// regression dressed up as a fix. 19 is measured, not chosen: it is what
    /// this document compiles with defaults and with every pattern-varying
    /// option set to a distinct value at once. If a future change pushed the
    /// working set past 128, the cap would start clearing on real input and this
    /// would fail rather than quietly getting slower.
    #[test]
    fn the_cap_is_above_the_measured_working_set() {
        for opts in [
            Options {
                default_link_dict: String::new(),
                ..Options::default()
            },
            max_varied_options(),
        ] {
            let mut c = Converter::new(opts);
            c.convert_text(&maximal_document(30));
            let n = c.re_cache.len();
            assert!(
                n < MAX,
                "a normal conversion compiled {n} patterns, at or past the cap of \
                 {MAX}: the cap would be clearing on ordinary input"
            );
        }
    }

    /// A10, part 3: bounding the cache cannot change the output.
    ///
    /// The cache is a pure memo of `pattern -> compiled Regex`, so a miss
    /// recompiles exactly what a hit returned and clearing it is invisible --
    /// but "invisible" is an argument, and this is the measurement. The same
    /// document is converted by a converter whose patterns all fit, and by one
    /// holding 400 uncached-compiling heading patterns that force several
    /// clears part-way through, and the two outputs must be identical.
    #[test]
    fn clearing_the_cache_does_not_change_the_output() {
        let text = maximal_document(30);

        let mut small = Converter::new(Options {
            default_link_dict: String::new(),
            ..Options::default()
        });
        let expected = small.convert_text(&text);

        let mut big = Converter::new(Options {
            default_link_dict: String::new(),
            // 400 patterns, none of which match, so the cache is filled and
            // cleared repeatedly while the document is converted.
            custom_heading_regexp: (0..400).map(|i| format!("^NOMATCH{i}")).collect(),
            ..Options::default()
        });
        assert!(
            big.re_cache_len_for_test() < MAX,
            "precondition: the cache starts empty, so the clears happen during \
             the conversion rather than before it"
        );
        let got = big.convert_text(&text);

        assert_eq!(
            got, expected,
            "bounding the pattern cache changed the output"
        );
    }

    impl Converter {
        fn re_cache_len_for_test(&self) -> usize {
            self.re_cache.len()
        }
    }
}

/// P5.4: the streaming path must be byte-for-byte the buffered path for any
/// UTF-8 input, and must refuse input it cannot decode identically.
#[cfg(test)]
mod stream_tests {
    use super::*;
    use crate::options::Options;

    fn opts() -> Options {
        Options {
            default_link_dict: String::new(),
            ..Options::default()
        }
    }

    fn buffered(options: &Options, text: &str) -> String {
        Converter::new(options.clone()).convert_text(text)
    }

    fn streamed(options: &Options, text: &str) -> String {
        let mut out = Vec::new();
        Converter::new(options.clone())
            .convert_stream(text.as_bytes(), &mut out)
            .expect("streaming a UTF-8 string should not fail");
        String::from_utf8(out).unwrap()
    }

    /// Inputs chosen for the record splitter's edges: blank/no blank, leading
    /// and repeated blanks, no trailing newline, CRLF blank separators, and
    /// paragraphs that drive the list/pre/table state machines.
    const BATTERY: &[&str] = &[
        "plain\n\nsecond para\n",
        "# heading\n\nbody *ital* and #bold#\n",
        "line1\nline2\n\n- a\n- b\n\n    pre\n\nend\n",
        "no trailing newline",
        "\n\nleading blanks\n\n\n\ndouble blank\n",
        "one\n\n\n\ntwo\n",
        "setext\n=====\n\nunderlined\n---------\n",
        "a\r\nb\r\n\r\nc\r\n",
        "ALIGN table\nx | y\n\nplain\n",
        "1. one\n2. two\n\n\n    code after list\n",
    ];

    #[test]
    fn stream_matches_buffered_on_every_battery_case() {
        let options = opts();
        for text in BATTERY {
            assert_eq!(
                streamed(&options, text),
                buffered(&options, text),
                "stream and buffered differ on {text:?}"
            );
        }
    }

    #[test]
    fn stream_matches_buffered_under_non_default_options() {
        let mut make_tables = opts();
        make_tables.make_tables = true;
        let mut xhtml = opts();
        xhtml.xhtml = true;
        let mut no_anchors = opts();
        no_anchors.make_anchors = false;
        let mut lower = opts();
        lower.lower_case_tags = true;
        lower.html5 = true;
        let mut extract = opts();
        extract.extract = true;
        let mut hrule = opts();
        hrule.hrule_min = 2;

        for options in [make_tables, xhtml, no_anchors, lower, extract, hrule] {
            for text in BATTERY {
                assert_eq!(
                    streamed(&options, text),
                    buffered(&options, text),
                    "stream and buffered differ under modified options on {text:?}"
                );
            }
        }
    }

    /// A document large enough to cross the record-by-record state machine many
    /// times, so an off-by-one in the loop would show rather than hide.
    #[test]
    fn stream_matches_buffered_on_a_long_document() {
        let mut text = String::new();
        for i in 0..200 {
            text.push_str(&format!("# Section {i}\n\n"));
            text.push_str("Some *italic* and #bold# text.\n\n");
            text.push_str("- one\n- two\n\n    indented\n\n");
        }
        let options = opts();
        assert_eq!(streamed(&options, &text), buffered(&options, &text));
    }

    #[test]
    fn stream_rejects_invalid_utf8_without_writing_output() {
        let mut out = Vec::new();
        let err = Converter::new(opts())
            .convert_stream(&b"ok line\n\xff\xfe bad\n"[..], &mut out)
            .expect_err("invalid UTF-8 must be refused");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(out.is_empty(), "nothing should be written before the error");
    }

    #[test]
    fn stream_rejects_utf16_even_though_ascii_utf16_is_valid_utf8() {
        // 'a','b' in UTF-16LE: the NULs are valid UTF-8, so only the structural
        // check can catch this.
        let mut out = Vec::new();
        let err = Converter::new(opts())
            .convert_stream(&[0xFF, 0xFE, b'a', 0, b'b', 0, b'\n', 0][..], &mut out)
            .expect_err("UTF-16 input must be refused");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(out.is_empty());
    }
}
