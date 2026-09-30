//! Character handling: HTML escaping, demoronizing, and Latin-1 entities.

/// Escape `&`, `<` and `>`, mirroring `escape()` in HTML::TextToHTML.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '>' => out.push_str("&gt;"),
            '<' => out.push_str("&lt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Microsoft "smart character" bytes -> plain characters.
/// Operates on the common Unicode code points produced when such files are
/// read as UTF-8 (these are the UTF-8 encodings of CP1252 code points),
/// mirroring `demoronize_char`.
pub fn demoronize_char(s: &mut String) {
    let replacements: &[(char, &str)] = &[
        ('\u{201a}', ","),   // \x82
        ('\u{201e}', ",,"),  // \x84
        ('\u{2026}', "..."), // \x85
        ('\u{02c6}', "^"),   // \x88
        ('\u{2039}', "<"),   // \x8B
        ('\u{0152}', "Oe"),  // \x8C
        ('\u{2018}', "`"),   // \x91
        ('\u{2019}', "'"),   // \x92
        ('\u{201c}', "\""),  // \x93
        ('\u{201d}', "\""),  // \x94
        ('\u{2022}', "*"),   // \x95
        ('\u{2013}', "-"),   // \x96
        ('\u{2014}', "--"),  // \x97
        ('\u{203a}', ">"),   // \x9B
        ('\u{0153}', "oe"),  // \x9C
    ];
    for (from, to) in replacements {
        *s = s.replace(*from, to);
    }
}

/// Convert a few Microsoft "smart" bytes into HTML code, mirroring
/// `demoronize_code`.
pub fn demoronize_code(s: &str) -> String {
    s.replace('\u{0192}', "<em>f</em>") // \x83
        .replace('\u{02dc}', "<sup>~</sup>") // \x98
        .replace('\u{2122}', "<sup>TM</sup>") // \x99
}

/// Map a Latin-1 character to its HTML entity name, if any.
pub fn char_to_entity(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{00a1}' => "&iexcl;",
        '\u{00a2}' => "&cent;",
        '\u{00a3}' => "&pound;",
        '\u{00a4}' => "&curren;",
        '\u{00a5}' => "&yen;",
        '\u{00a6}' => "&brvbar;",
        '\u{00a7}' => "&sect;",
        '\u{00a8}' => "&uml;",
        '\u{00a9}' => "&copy;",
        '\u{00aa}' => "&ordf;",
        '\u{00ab}' => "&laquo;",
        '\u{00ac}' => "&not;",
        '\u{00ad}' => "&shy;",
        '\u{00ae}' => "&reg;",
        '\u{00af}' => "&hibar;",
        '\u{00b0}' => "&deg;",
        '\u{00b1}' => "&plusmn;",
        '\u{00b2}' => "&sup2;",
        '\u{00b3}' => "&sup3;",
        '\u{00b4}' => "&acute;",
        '\u{00b5}' => "&micro;",
        '\u{00b6}' => "&para;",
        '\u{00b7}' => "&middot;",
        '\u{00b8}' => "&cedil;",
        '\u{00b9}' => "&sup1;",
        '\u{00ba}' => "&ordm;",
        '\u{00bb}' => "&raquo;",
        '\u{00bc}' => "&frac14;",
        '\u{00bd}' => "&frac12;",
        '\u{00be}' => "&frac34;",
        '\u{00bf}' => "&iquest;",
        '\u{00c0}' => "&Agrave;",
        '\u{00c1}' => "&Aacute;",
        '\u{00c2}' => "&Acirc;",
        '\u{00c3}' => "&Atilde;",
        '\u{00c4}' => "&Auml;",
        '\u{00c5}' => "&Aring;",
        '\u{00c6}' => "&AElig;",
        '\u{00c7}' => "&Ccedil;",
        '\u{00c8}' => "&Egrave;",
        '\u{00c9}' => "&Eacute;",
        '\u{00ca}' => "&Ecirc;",
        '\u{00cb}' => "&Euml;",
        '\u{00cc}' => "&Igrave;",
        '\u{00cd}' => "&Iacute;",
        '\u{00ce}' => "&Icirc;",
        '\u{00cf}' => "&Iuml;",
        '\u{00d0}' => "&ETH;",
        '\u{00d1}' => "&Ntilde;",
        '\u{00d2}' => "&Ograve;",
        '\u{00d3}' => "&Oacute;",
        '\u{00d4}' => "&Ocirc;",
        '\u{00d5}' => "&Otilde;",
        '\u{00d6}' => "&Ouml;",
        '\u{00d7}' => "&times;",
        '\u{00d8}' => "&Oslash;",
        '\u{00d9}' => "&Ugrave;",
        '\u{00da}' => "&Uacute;",
        '\u{00db}' => "&Ucirc;",
        '\u{00dc}' => "&Uuml;",
        '\u{00dd}' => "&Yacute;",
        '\u{00de}' => "&THORN;",
        '\u{00df}' => "&szlig;",
        '\u{00e0}' => "&agrave;",
        '\u{00e1}' => "&aacute;",
        '\u{00e2}' => "&acirc;",
        '\u{00e3}' => "&atilde;",
        '\u{00e4}' => "&auml;",
        '\u{00e5}' => "&aring;",
        '\u{00e6}' => "&aelig;",
        '\u{00e7}' => "&ccedil;",
        '\u{00e8}' => "&egrave;",
        '\u{00e9}' => "&eacute;",
        '\u{00ea}' => "&ecirc;",
        '\u{00eb}' => "&euml;",
        '\u{00ec}' => "&igrave;",
        '\u{00ed}' => "&iacute;",
        '\u{00ee}' => "&icirc;",
        '\u{00ef}' => "&iuml;",
        '\u{00f0}' => "&eth;",
        '\u{00f1}' => "&ntilde;",
        '\u{00f2}' => "&ograve;",
        '\u{00f3}' => "&oacute;",
        '\u{00f4}' => "&ocirc;",
        '\u{00f5}' => "&otilde;",
        '\u{00f6}' => "&ouml;",
        '\u{00f7}' => "&divide;",
        '\u{00f8}' => "&oslash;",
        '\u{00f9}' => "&ugrave;",
        '\u{00fa}' => "&uacute;",
        '\u{00fb}' => "&ucirc;",
        '\u{00fc}' => "&uuml;",
        '\u{00fd}' => "&yacute;",
        '\u{00fe}' => "&thorn;",
        '\u{00ff}' => "&yuml;",
        _ => return None,
    })
}

/// Convert bytes interpreted as Latin-1 into a Rust String (UTF-8).
///
/// This is a lossless byte<->char round-trip so that arbitrary input bytes
/// (plain ASCII, Latin-1, or raw UTF-8 bytes) map onto Unicode code points in
/// exactly the way the Perl module treats input "characters".
pub fn latin1_to_string(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        s.push(b as char);
    }
    s
}

/// Replace non-ASCII Latin-1 characters with their HTML entities,
/// leaving ASCII and everything else untouched.
pub fn entities(para: &str) -> String {
    let mut out = String::with_capacity(para.len());
    for c in para.chars() {
        match char_to_entity(c) {
            Some(e) => out.push_str(e),
            None => out.push(c),
        }
    }
    out
}
