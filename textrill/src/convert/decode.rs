use crate::options::{Encoding, SingleByte};

/// How [`read_any_file`] resolved a file's bytes. `Single` records which
/// single-byte decoding the `Auto` path fell back to, so a caller can name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    /// The bytes were valid UTF-8 and were decoded as such.
    Utf8,
    /// A single-byte encoding, by guess (`Auto`) or because it was named.
    Single(SingleByte),
    /// UTF-16 or UTF-32, little- or big-endian.
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
    pub(super) fn from_encoding(encoding: Encoding) -> Option<Self> {
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

    /// Rank for choosing which encoding to report when several inputs resolve
    /// differently: UTF-8 (0) < single-byte guess (1) < UTF-16/32 (2).
    pub(super) fn notice_rank(self) -> u8 {
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
/// Valid UTF-8 is decoded as UTF-8; otherwise the fallback is **CP1252**, not
/// Latin-1, whose `0x80`-`0x9F` range holds the typographic punctuation
/// [`crate::chars::demoronize_char`] acts on. CP1252's undefined bytes (`0x81`,
/// `0x8D`, `0x8F`, `0x90`, `0x9D`) are left as their Latin-1 control character.
pub fn read_any_file_with_encoding(path: &str) -> Option<(String, Resolved)> {
    read_with(path, Encoding::Auto)
}

/// [`read_any_file_with_encoding`], but honouring an explicit `--encoding`. An
/// explicit `Utf8`/`Cp1252` settles the case `Auto` cannot: a short CP1252
/// document whose bytes happen to be valid UTF-8.
pub fn read_with(path: &str, encoding: Encoding) -> Option<(String, Resolved)> {
    let bytes = std::fs::read(path).ok()?;
    Some(decode_bytes_with(&bytes, encoding))
}

/// Decode `bytes` under `encoding`. Split out from [`read_with`] so the GUI can
/// reuse it without a file and so the detection order is directly testable.
pub fn decode_bytes_with(bytes: &[u8], encoding: Encoding) -> (String, Resolved) {
    match encoding {
        Encoding::Auto => detect(bytes),
        // Lossy on purpose: bad bytes become visible U+FFFD rather than aborting.
        Encoding::Utf8 => (String::from_utf8_lossy(bytes).into_owned(), Resolved::Utf8),
        wide @ (Encoding::Utf16Le | Encoding::Utf16Be | Encoding::Utf32Le | Encoding::Utf32Be) => {
            // A named encoding still has to cope with a BOM, or U+FEFF would
            // become the document's first character.
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
pub(super) fn detect(bytes: &[u8]) -> (String, Resolved) {
    if let Some((encoding, skip)) = bom(bytes) {
        // A UTF-8 BOM is kept in the text; the UTF-16/32 marks are skipped.
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

/// The byte-order mark, if present, as the encoding it declares and its length
/// in bytes. A BOM outranks every heuristic, so it is checked first.
pub(super) fn bom(bytes: &[u8]) -> Option<(Encoding, usize)> {
    // UTF-32LE's BOM starts with UTF-16LE's, so the longer mark is tested first.
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
/// UTF-16LE ASCII prose is valid UTF-8, so a validity check must not run first;
/// UTF-16 has a NUL every second byte in one alignment and nothing else this port
/// decodes does. UTF-32 is deliberately not inferred: without a BOM it is
/// ambiguous, so it is read as UTF-16 rather than risk a confident wrong guess.
pub(super) fn sniff_utf16_or_32(bytes: &[u8]) -> Option<Encoding> {
    pub(super) const WINDOW: usize = 4096;
    let sample = &bytes[..bytes.len().min(WINDOW)];
    if sample.len() < 8 {
        return None;
    }
    // Counted from the bytes rather than `len / 2`: an odd-length sample puts one
    // more byte on the even side, which would loosen the threshold.
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
    // Threshold 1/8, measured: the NUL fraction runs 0.15-0.25 for real
    // non-ASCII UTF-16 prose and 1.00 for ASCII, against a worst UTF-8 negative
    // of 0.04; 2/3 would catch only ASCII. The `* 4` factor rejects scattered
    // NULs. BOM-less UTF-16 with almost no ASCII is undetectable, which is what
    // `--encoding utf-16le` is for.
    if nulls_even * 8 > even && nulls_even > nulls_odd * 4 {
        Some(Encoding::Utf16Be)
    } else if nulls_odd * 8 > odd && nulls_odd > nulls_even * 4 {
        Some(Encoding::Utf16Le)
    } else {
        None
    }
}

/// Decode UTF-16/32 code units into text, joining UTF-16 surrogate pairs.
///
/// A lone surrogate is not a character: `char::from_u32` rejects it and U+FFFD
/// is substituted. Units are paired first, or every astral character would
/// become two U+FFFD.
pub(super) fn units_to_string(units: Vec<u32>, wide: Wide) -> String {
    if wide != Wide::Utf16Le && wide != Wide::Utf16Be {
        // UTF-32 holds whole code points, so there is nothing to pair; values
        // above U+10FFFF are still not characters.
        return units
            .into_iter()
            .map(|u| char::from_u32(u).unwrap_or('\u{FFFD}'))
            .collect();
    }
    pub(super) const HIGH: std::ops::RangeInclusive<u32> = 0xD800..=0xDBFF;
    pub(super) const LOW: std::ops::RangeInclusive<u32> = 0xDC00..=0xDFFF;
    let mut out = String::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        if HIGH.contains(&u) {
            // A high surrogate followed by a low one is one character; anything
            // else is malformed, so the surrogate becomes U+FFFD.
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

pub(super) fn decode_utf16_or_32(
    bytes: &[u8],
    encoding: Encoding,
    skip: usize,
) -> (String, Resolved) {
    let wide = Wide::from_encoding(encoding)
        .unwrap_or_else(|| unreachable!("decode_utf16_or_32 is only reached for UTF-16/32"));
    let body = bytes.get(skip..).unwrap_or_default();
    // `as_chunks` rather than `chunks_exact` for the lint, but a trailing
    // fragment shorter than one code unit is *dropped*, not zero-padded.
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

pub(super) static LATIN1_HIGH: [u32; 128] = [
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

pub(super) static CP1252_HIGH: [u32; 128] = [
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

pub(super) static CP1251_HIGH: [u32; 128] = [
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

pub(super) static CP1253_HIGH: [u32; 128] = [
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

pub(super) static KOI8R_HIGH: [u32; 128] = [
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
pub(super) static ENCODING_TABLES: &[(SingleByte, &[u32; 128])] = &[
    (SingleByte::Latin1, &LATIN1_HIGH),
    (SingleByte::Cp1252, &CP1252_HIGH),
    (SingleByte::Cp1251, &CP1251_HIGH),
    (SingleByte::Cp1253, &CP1253_HIGH),
    (SingleByte::Koi8R, &KOI8R_HIGH),
];

pub(super) fn cp1252_char(b: u8) -> char {
    single_byte_char(SingleByte::Cp1252, b)
}

/// The high half of `enc`, indexed by `byte - 0x80`. Public so [`crate::encode`]
/// can build the reverse mapping from the same table rather than a second copy.
pub fn encoding_table(enc: SingleByte) -> &'static [u32; 128] {
    ENCODING_TABLES
        .iter()
        .find(|(e, _)| *e == enc)
        .map(|(_, t)| *t)
        .unwrap_or_else(|| unreachable!("every SingleByte has a table"))
}

pub(super) fn single_byte_char(enc: SingleByte, b: u8) -> char {
    if b < 0x80 {
        return b as char;
    }
    let cp = encoding_table(enc)[(b - 0x80) as usize];
    if cp == 0 {
        // Undefined here: keep the Latin-1 reading rather than a panic or a
        // replacement glyph.
        b as char
    } else {
        char::from_u32(cp).unwrap_or(b as char)
    }
}

/// Read a text file for conversion, discarding which encoding was resolved.
pub fn read_any_file(path: &str) -> Option<String> {
    read_any_file_with_encoding(path).map(|(text, _)| text)
}

/// [`read_any_file`], honouring an explicit `--encoding`; append/prepend files
/// obey the same rule rather than being re-probed.
pub fn read_file_with(path: &str, encoding: Encoding) -> Option<String> {
    read_with(path, encoding).map(|(text, _)| text)
}

/// Chop trailing whitespace and a DOS CR, i.e. Perl's `s/[ \t]*\x0D$//`.
///
/// Not a regex: `links.rs` rewrites `$` to the lookahead `(?=\n?$)`, which moves
/// the pattern onto fancy-regex's backtracker, where a large paragraph exhausts
/// the step budget and panics. It is anchored at the end, so string surgery
/// suffices, and Perl's `$` also matches before one trailing newline.
pub(super) fn chop_trailing_cr(s: &str) -> String {
    let (body, had_nl) = match s.strip_suffix('\n') {
        Some(b) => (b, true),
        None => (s, false),
    };
    if !body.ends_with('\r') {
        return s.to_string(); // no match, so the paragraph is left alone
    }
    // The `[ \t]*` sits *before* the CR, so the CR is removed first and the
    // spaces and tabs trimmed afterwards; trimming first leaves a stray CR.
    let trimmed = body[..body.len() - 1].trim_end_matches([' ', '\t']);
    let mut out = String::with_capacity(trimmed.len() + 1);
    out.push_str(trimmed);
    if had_nl {
        out.push('\n');
    }
    out
}

/// Chop leading whitespace and a DOS CR, i.e. Perl's `s/^[ \t]*\x0D//`; see
/// [`chop_trailing_cr`] for why this is not a regex. Leading whitespace goes only
/// when a CR follows it, or every indented paragraph would be corrupted.
pub(super) fn chop_leading_cr(s: &str) -> String {
    match s.trim_start_matches([' ', '\t']).strip_prefix('\r') {
        Some(rest) => rest.to_string(),
        None => s.to_string(),
    }
}
