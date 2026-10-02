// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Writing text back out, in the encoding it was read with.
//!
//! This is the save path, and it is the reason this module exists. Until now
//! the engine could decode but not encode: `files.py` grew a Python encoder so
//! that a file opened and saved without edits would come back byte-for-byte
//! identical. The Phase 6 rewrite deletes `files.py`, so the encoder has to
//! live in the engine or the rewrite loses that guarantee.
//!
//! The rule it exists to protect: **opening a file must not change it.** A
//! CP1252 file containing `0x93` is a curly quote. Written back as UTF-8 it
//! becomes `c2 93`, two C1 control characters, and the user has silently
//! corrupted their document by doing nothing to it.

use crate::options::{Encoding, SingleByte};

/// Why a character could not be written.
///
/// Distinct from a plain error because the remedy differs: an unrepresentable
/// character is something the *user* has to decide about (change the text, or
/// choose a different encoding), while an unknown encoding name is a bug in the
/// caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// The text contains a character this encoding cannot hold.
    ///
    /// Carries the character and its offset so a front end can underline it in
    /// the editor rather than saying "unsupported character" and leaving the
    /// user to find it.
    Unrepresentable { ch: char, at: usize },
    /// The encoding name is not one this engine writes.
    UnknownEncoding(String),
    /// A surrogate or a code point above U+10FFFF cannot be encoded at all,
    /// which in Rust cannot reach `str` but can reach a lone surrogate arriving
    /// from a decoded file.
    NotAChar(u32),
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::Unrepresentable { ch, at } => write!(
                f,
                "character {ch:?} at position {at} cannot be written in this encoding"
            ),
            EncodeError::UnknownEncoding(name) => {
                write!(f, "unknown encoding `{name}`")
            }
            EncodeError::NotAChar(cp) => {
                write!(f, "U+{cp:04X} is not a character")
            }
        }
    }
}

impl std::error::Error for EncodeError {}

/// The byte-order mark for a wide encoding, if it has one.
///
/// Separate from [`bom`] in `convert` because that one *detects* a mark on input
/// and this one *writes* one on output, and they must agree about UTF-32LE:
/// its mark starts with UTF-16LE's, so a lookup that picked the shorter one
/// would emit a UTF-16LE mark for a UTF-32LE file.
fn bom_for(encoding: Encoding) -> Option<&'static [u8]> {
    Some(match encoding {
        Encoding::Utf32Le => &[0xFF, 0xFE, 0x00, 0x00],
        Encoding::Utf32Be => &[0x00, 0x00, 0xFE, 0xFF],
        Encoding::Utf16Le => &[0xFF, 0xFE],
        Encoding::Utf16Be => &[0xFE, 0xFF],
        _ => return None,
    })
}

/// The single byte that represents `ch` in `enc`, if there is one.
///
/// The reverse of `convert::single_byte_char`. Built by scanning the same
/// `ENCODING_TABLES` rather than by keeping a second table, because a second
/// table is exactly the kind of copy that drifts -- and a drifted encoder
/// produces a save that corrupts a file, which is worse than a missing feature.
fn single_byte_of(enc: SingleByte, ch: char) -> Option<u8> {
    if (ch as u32) < 0x80 {
        return Some(ch as u8);
    }
    let table = crate::convert::encoding_table(enc);
    for (i, &cp) in table.iter().enumerate() {
        // A `0` entry is an undefined slot. `single_byte_char` decodes those as
        // Latin-1, so encoding them back as Latin-1 is what round-trips; leaving
        // them unencodable would mean a file containing 0x81 cannot be saved at
        // all, which is the corruption this module exists to prevent.
        let effective = if cp == 0 { 0x80 + i as u32 } else { cp };
        if effective == ch as u32 {
            return Some(0x80 + i as u8);
        }
    }
    None
}

/// Encode `text` under `encoding`.
///
/// `Encoding::Auto` is **not** accepted: "auto" describes a decision made on
/// input, and there is nothing to detect on output. A caller that wants UTF-8
/// says so. Returning `UnknownEncoding` rather than silently defaulting is the
/// point -- a save that quietly wrote UTF-8 to a CP1252 file is precisely the
/// bug this module was written to end.
///
/// Wide encodings are written *with* a byte-order mark even when the input had
/// none. That asymmetry is deliberate and matches the Python behaviour: the
/// mark is a declaration that costs two bytes, tools that read UTF-16 expect
/// it, and "this file had no BOM" is state the GUI has nowhere to keep.
pub fn encode(text: &str, encoding: Encoding) -> Result<Vec<u8>, EncodeError> {
    match encoding {
        Encoding::Auto => Err(EncodeError::UnknownEncoding("auto".into())),
        Encoding::Utf8 => Ok(text.as_bytes().to_vec()),
        Encoding::Latin1
        | Encoding::Cp1252
        | Encoding::Cp1251
        | Encoding::Cp1253
        | Encoding::Koi8R => {
            let sb = encoding.single_byte().expect("matched above");
            let mut out = Vec::with_capacity(text.len());
            for (at, ch) in text.char_indices() {
                match single_byte_of(sb, ch) {
                    Some(b) => out.push(b),
                    None => return Err(EncodeError::Unrepresentable { ch, at }),
                }
            }
            Ok(out)
        }
        wide => {
            let mut out = Vec::with_capacity(text.len() * 2 + 4);
            out.extend_from_slice(bom_for(wide).unwrap_or_default());
            let units = wide.units_per_char();
            // Surrogate pairs are a UTF-16 concern specifically. UTF-32 holds
            // whole code points, so applying the same split there would write an
            // astral character as two invalid code units -- which decodes back
            // as mojibake, the very corruption this module prevents.
            let utf16 = matches!(wide, Encoding::Utf16Le | Encoding::Utf16Be);
            let _ = units;
            for ch in text.chars() {
                let cp = ch as u32;
                if utf16 && cp > 0xFFFF {
                    // UTF-16 cannot hold an astral character, so it needs a
                    // surrogate pair. Getting this wrong writes two Latin-1
                    // characters instead of one emoji, which is the same
                    // corruption class as the CP1252 bug.
                    let v = cp - 0x1_0000;
                    push_unit(&mut out, wide, 0xD800 + (v >> 10));
                    push_unit(&mut out, wide, 0xDC00 + (v & 0x3FF));
                } else {
                    push_unit(&mut out, wide, cp);
                }
            }
            Ok(out)
        }
    }
}

/// Append one code unit to `out`, in the byte order `encoding` names.
///
/// UTF-32 takes the full 32 bits; UTF-16 takes 16. They are separate arms
/// rather than one arm with a width parameter because the truncation for UTF-16
/// is only ever correct for values that already fit -- the astral case is split
/// into a surrogate pair by the caller before it gets here.
fn push_unit(out: &mut Vec<u8>, encoding: Encoding, unit: u32) {
    match encoding {
        Encoding::Utf16Le => out.extend_from_slice(&(unit as u16).to_le_bytes()),
        Encoding::Utf16Be => out.extend_from_slice(&(unit as u16).to_be_bytes()),
        Encoding::Utf32Le => out.extend_from_slice(&unit.to_le_bytes()),
        Encoding::Utf32Be => out.extend_from_slice(&unit.to_be_bytes()),
        _ => unreachable!("push_unit is only called for wide encodings"),
    }
}

/// Write `text` to `path`, encoded under `encoding`.
///
/// Fails if the parent directory does not exist, and does **not** create it.
///
/// A missing directory used to be created with `mkdir -p` in Python, on the
/// theory that a save should always succeed. It should not: this is called from
/// a save dialog, which is exactly where a mistyped path happens, and one typo
/// like `newtree/a/b/c/out.html` then invented five directories the user never
/// asked for and has to clean up by hand. An actionable error beats a stray
/// tree.
pub fn write_with(
    path: &std::path::Path,
    text: &str,
    encoding: Encoding,
) -> Result<(), WriteError> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(dir) = dir {
        if !dir.is_dir() {
            return Err(WriteError::NoSuchDirectory(dir.to_path_buf()));
        }
    }
    let bytes = encode(text, encoding).map_err(WriteError::Encode)?;
    std::fs::write(path, bytes).map_err(|e| WriteError::Io(path.to_path_buf(), e))
}

/// Why a write failed.
#[derive(Debug)]
pub enum WriteError {
    /// The directory the file was to be written into does not exist.
    NoSuchDirectory(std::path::PathBuf),
    /// The text could not be represented in the requested encoding.
    Encode(EncodeError),
    /// The filesystem refused the write.
    Io(std::path::PathBuf, std::io::Error),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::NoSuchDirectory(d) => {
                write!(f, "no such directory: {}", d.display())
            }
            WriteError::Encode(e) => write!(f, "{e}"),
            WriteError::Io(p, e) => write!(f, "{}: {e}", p.display()),
        }
    }
}

impl std::error::Error for WriteError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::{decode_bytes_with, Resolved};

    fn round(text: &str, enc: Encoding) -> String {
        let bytes = encode(text, enc).expect("encodable");
        decode_bytes_with(&bytes, enc).0
    }

    #[test]
    fn utf8_round_trips() {
        assert_eq!(round("héllo\n", Encoding::Utf8), "héllo\n");
    }

    #[test]
    fn cp1252_round_trips_punctuation() {
        // The bytes the whole module exists for: 0x93/0x94/0x96/0x97 are
        // punctuation in CP1252 and C1 controls in Latin-1. The decoded
        // characters are U+2014 and friends, NOT U+0097: a decoder that returned
        // the Latin-1 reading is the original P7 defect.
        let text = "Café \u{2014} naïve \u{201c}quotes\u{201d}";
        let bytes = encode(text, Encoding::Cp1252).unwrap();
        assert!(bytes.contains(&0x97), "em dash should be one byte 0x97");
        assert!(bytes.contains(&0x93), "left quote should be 0x93");
        assert_eq!(decode_bytes_with(&bytes, Encoding::Cp1252).0, text);
    }

    #[test]
    fn a_round_trip_preserves_the_original_bytes() {
        // The A5 property: read then write leaves a CP1252 file untouched.
        let original: &[u8] = b"Caf\xe9 \x97 na\xefve \x93quotes\x94\r\nsecond\r\n";
        let (text, resolved) = decode_bytes_with(original, Encoding::Auto);
        assert_eq!(resolved, Resolved::Single(SingleByte::Cp1252));
        assert_eq!(encode(&text, resolved.encoding()).unwrap(), original);
    }

    #[test]
    fn undefined_cp1252_slots_round_trip_as_latin1() {
        for b in [0x81u8, 0x8D, 0x8F, 0x90, 0x9D] {
            let (text, _) = decode_bytes_with(&[b], Encoding::Cp1252);
            assert_eq!(
                encode(&text, Encoding::Cp1252).unwrap(),
                vec![b],
                "byte {b:#04x} must survive a save"
            );
        }
    }

    #[test]
    fn an_unrepresentable_character_is_an_error_not_a_substitution() {
        // U+0416 is Cyrillic, which CP1252 cannot hold. Substituting '?' would
        // silently corrupt the document; this must be reported so a front end
        // can offer the user a choice.
        // U+041F is CYRILLIC CAPITAL LETTER PE, the first character here.
        let err = encode("Привет", Encoding::Cp1252).unwrap_err();
        assert_eq!(
            err,
            EncodeError::Unrepresentable {
                ch: '\u{041f}',
                at: 0
            }
        );
    }

    #[test]
    fn the_error_carries_the_offset_so_an_editor_can_point_at_it() {
        // "ok then " is 8 bytes of ASCII, so the first Cyrillic character
        // starts at byte offset 8.
        let err = encode("ok then Привет", Encoding::Cp1252).unwrap_err();
        match err {
            EncodeError::Unrepresentable { ch, at } => {
                assert_eq!(ch, '\u{041f}');
                assert_eq!(at, 8, "offset should be a byte index");
            }
            other => panic!("wrong error: {other:?}"),
        }
    }

    #[test]
    fn auto_is_rejected_on_output() {
        assert!(matches!(
            encode("x", Encoding::Auto),
            Err(EncodeError::UnknownEncoding(_))
        ));
    }

    #[test]
    fn wide_encodings_are_written_with_a_bom() {
        for (enc, mark) in [
            (Encoding::Utf16Le, vec![0xFFu8, 0xFE]),
            (Encoding::Utf16Be, vec![0xFE, 0xFF]),
        ] {
            let bytes = encode("hi", enc).unwrap();
            assert_eq!(&bytes[..mark.len()], &mark[..], "BOM for {enc:?}");
        }
        // UTF-32LE's mark starts with UTF-16LE's; picking the short one would
        // emit a mark that makes the file decode as UTF-16.
        let bytes = encode("hi", Encoding::Utf32Le).unwrap();
        assert_eq!(&bytes[..4], &[0xFF, 0xFE, 0x00, 0x00]);
    }

    #[test]
    fn utf16_round_trips_ascii_and_punctuation() {
        for enc in [Encoding::Utf16Le, Encoding::Utf16Be] {
            assert_eq!(round("Café \u{2014} x", enc), "Café \u{2014} x");
        }
    }

    #[test]
    fn utf16_encodes_astral_characters_as_surrogate_pairs() {
        // Written as two UTF-16 code units, not as two Latin-1 characters.
        let bytes = encode("\u{1F600}", Encoding::Utf16Le).unwrap();
        assert_eq!(bytes.len(), 2 + 4);
        assert_eq!(round("\u{1F600}", Encoding::Utf16Le), "\u{1F600}");
    }

    #[test]
    fn utf32_round_trips_an_astral_character_directly() {
        assert_eq!(round("\u{1F600}", Encoding::Utf32Le), "\u{1F600}");
        let bytes = encode("\u{1F600}", Encoding::Utf32Le).unwrap();
        assert_eq!(bytes.len(), 4 + 4);
    }

    #[test]
    fn every_supported_encoding_round_trips_its_own_alphabet() {
        // Guards against a table entry that decodes but does not encode back.
        let cases = [
            (Encoding::Latin1, "café naïve"),
            (Encoding::Cp1251, "Привет"),
            (Encoding::Cp1253, "καλημέρα"),
            (Encoding::Koi8R, "Привет"),
        ];
        for (enc, text) in cases {
            assert_eq!(round(text, enc), text, "{enc:?} did not round trip");
        }
    }
}
