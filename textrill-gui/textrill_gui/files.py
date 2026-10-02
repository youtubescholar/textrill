# textrill-gui — a Qt front end for textrill.
#
# Copyright (C) 2026 the textrill authors.
#
# This program is free software: you can redistribute it and/or modify it
# under the terms of the GNU General Public License as published by the Free
# Software Foundation, either version 3 of the License, or (at your option)
# any later version.  See the LICENSE file for the full text.

"""Reading and writing text files the way the converter does.

This is a second implementation of the engine's input-encoding rules, and until
Phase 6 it disagreed with the engine in both directions at different times. It
exists because the editor has to show the same characters the converter will
convert; if the two differ, the user edits one document and previews another.

The rule, in the order the engine applies it (`src/convert.rs`, `Encoding::Auto`):

1. **A byte-order mark.** A declaration by the writer, so it outranks every
   heuristic. `FF FE` is a guarantee of UTF-16LE, not a decode error — treating
   it as garbage is how a BOM'd UTF-16 file used to arrive as `&yuml;&thorn;Hello`.
2. **The NUL pattern of UTF-16**, for files with no BOM. UTF-16LE containing
   only ASCII is *also* valid UTF-8, so a UTF-8 validity check cannot
   distinguish it and must not run first; a NUL on a consistent byte alignment
   can. Measured threshold, see `NUL_RATIO`.
3. **UTF-8 validity.**
4. **CP1252**, which is a guess, not evidence. It is right for Western European
   text and wrong for Cyrillic, Greek and Turkish, which is why those encodings
   are selectable by name.

A UTF-8 BOM is step 1 but is deliberately *kept* in the text: everything after
it is UTF-8 either way, and the reference does not strip it, so stripping here
would make the preview differ from the saved file. The wide-encoding BOMs are
metadata and are consumed.

Why the tables are built from Python's codecs rather than written out, which is
a change from P7.1: there are now five single-byte encodings and 128 high bytes
each, and a hand-transcribed table is a mojibake bug that shows up for one
character in one encoding — the hardest kind to notice. The stdlib gives the
same answers the Rust tables were generated from (`tests/gen_encoding_tables.py`
generates those from these same codecs), so the two implementations agree by
construction instead of by proofreading.

The one thing Python's codecs cannot express is an *undefined* byte: `cp1252`
rejects `0x81`, `0x8D`, `0x8F`, `0x90`, `0x9D`, and CP1253 rejects 17 slots. The
engine keeps those as Latin-1 C1 controls so the decode stays total, so the
tables detect them (U+FFFD under `errors="replace"` is unambiguous here, because
none of these encodings can encode U+FFFD) and fall back to `chr(byte)`.
"""

from __future__ import annotations

from pathlib import Path

#: The encoding used when the bytes are neither UTF-8 nor any other evidence.
#: A guess, and the weakest link in the chain above.
FALLBACK_ENCODING = "cp1252"

#: Single-byte encodings the user can name, as the engine spells them. These
#: names are also valid Python codec names, which is not a coincidence but does
#: save a translation table.
SINGLE_BYTE_ENCODINGS = ("iso-8859-1", "cp1252", "cp1251", "cp1253", "koi8-r")

#: Multi-byte encodings, mapped to the codec that handles them. `utf-16le` is not
#: itself a Python codec name (`utf-16-le` is), hence the mapping rather than a
#: passthrough.
WIDE_ENCODINGS = {
    "utf-16le": "utf-16-le",
    "utf-16be": "utf-16-be",
    "utf-32le": "utf-32-le",
    "utf-32be": "utf-32-be",
}

#: The NUL fraction, on a consistent byte alignment, above which bytes are read
#: as UTF-16. Measured, not guessed — see the table in `src/convert.rs`:
#:
#:   UTF-16LE, ASCII prose      1.00      ASCII document      0.00
#:   UTF-16LE, Russian prose    0.21      UTF-8 with a NUL    0.04
#:   UTF-16LE, Greek prose      0.20      HTML                0.00
#:   UTF-16LE, dense Cyrillic   0.15      raw byte range      0.01
#:
#: An earlier 2/3 threshold passed the ASCII rows and failed the Russian and
#: Greek ones, which would have left exactly the interesting input broken while
#: the easy cases looked fine. 1/8 leaves a 3x margin above the worst negative.
#: The cross-alignment factor rejects files that merely contain scattered NULs.
NUL_RATIO = 8

#: Byte-order marks, longest first. UTF-32LE's mark starts with UTF-16LE's, so
#: order matters: checked the short way round, a UTF-32LE file decodes as
#: UTF-16LE and comes out as pairs of Latin-1 characters, which is a wrong answer
#: that still looks like text.
_BOMS = (
    (b"\xff\xfe\x00\x00", "utf-32le"),
    (b"\x00\x00\xfe\xff", "utf-32be"),
    (b"\xff\xfe", "utf-16le"),
    (b"\xfe\xff", "utf-16be"),
)

_HIGH_TABLES: dict[str, dict[int, str]] = {}


def _high_table(encoding: str) -> dict[int, str]:
    """Map `0x80`-`0xFF` to characters, as the engine's tables do.

    Bytes the encoding leaves undefined are absent from the mapping and fall
    through to `chr(byte)`, i.e. the Latin-1 C1 reading.
    """
    table = _HIGH_TABLES.get(encoding)
    if table is None:
        table = {}
        for byte in range(0x80, 0x100):
            decoded = bytes([byte]).decode(encoding, errors="replace")
            if decoded != "�":
                table[byte] = decoded
        _HIGH_TABLES[encoding] = table
    return table


def _decode_single_byte(data: bytes, encoding: str) -> str:
    table = _high_table(encoding)
    return "".join(table.get(b, chr(b)) for b in data)


def _encode_single_byte(text: str, encoding: str) -> bytes:
    """Encode via the same table, so a decode/encode round trip is lossless.

    A character the encoding cannot hold is an error rather than a replacement,
    because this is reached from a save dialog: silently substituting would
    destroy the user's text, and telling them lets them save as UTF-8 instead.
    """
    reverse = {char: byte for byte, char in _high_table(encoding).items()}
    out = bytearray()
    for index, ch in enumerate(text):
        if ch in reverse:
            out.append(reverse[ch])
        elif ord(ch) < 0x100:
            # Identity, deliberately. This is what makes an undefined byte
            # survive a decode/encode round trip: `0x81` decodes to U+0081 (see
            # `_high_table`), and asking Python whether CP1252 "defines" U+0081
            # would say no and refuse to write it back -- losing the byte on a
            # file the user only opened. Latin-1 covers the whole low range, so
            # every C1 control round-trips.
            out.append(ord(ch))
        else:
            raise UnicodeEncodeError(
                encoding, text, index, index + 1,
                f"U+{ord(ch):04X} has no {encoding} byte; save as UTF-8 instead",
            )
    return bytes(out)


def _bom(data: bytes) -> tuple[str, int] | None:
    """The declared encoding and mark length, or None."""
    for mark, encoding in _BOMS:
        if data.startswith(mark):
            return encoding, len(mark)
    if data.startswith(b"\xef\xbb\xbf"):
        return "utf-8", 3
    return None


def _sniff_utf16(data: bytes) -> str | None:
    """Infer UTF-16 from where the NULs fall. See `NUL_RATIO`."""
    sample = data[:4096]
    if len(sample) < 8:
        return None
    even, odd = sample[0::2], sample[1::2]
    # Budgets are counted from the bytes, not from len // 2: a truncated final
    # code unit leaves the alignments uneven, and a budget that silently
    # overcounted one side would loosen the very threshold this relies on.
    for name, side, other in (("utf-16be", even, odd), ("utf-16le", odd, even)):
        nulls, total = side.count(0), len(side)
        if nulls * NUL_RATIO > total and nulls > other.count(0) * 4:
            return name
    return None


def detect_encoding(data: bytes, encoding: str = "auto") -> str:
    """The encoding `decode_bytes` will use, as a name.

    This is the converter's rule, not Python's: use the evidence the file
    carries, and fall back to a single-byte encoding that maps every byte, rather
    than trusting a locale or a declared charset that might be a lie.

    Passing an explicit `encoding` returns it unchanged, which is the escape
    hatch for what detection cannot do — see the module docstring.
    """
    if encoding != "auto":
        return _canonical(encoding)
    mark = _bom(data)
    if mark is not None:
        declared, _ = mark
        if declared != "utf-8":  # a UTF-8 mark stays in the text
            return declared
        return "utf-8"
    sniffed = _sniff_utf16(data)
    if sniffed is not None:
        return sniffed
    try:
        data.decode("utf-8")
    except UnicodeDecodeError:
        return FALLBACK_ENCODING
    return "utf-8"


def _canonical(encoding: str) -> str:
    """Accept the spellings the CLI accepts, return the canonical name."""
    name = encoding.strip().lower().replace("_", "-")
    aliases = {
        "latin-1": "iso-8859-1",
        "latin1": "iso-8859-1",
        "iso8859-1": "iso-8859-1",
        "iso88591": "iso-8859-1",
        "8859-1": "iso-8859-1",
        "windows-1252": "cp1252",
        "windows-1251": "cp1251",
        "windows-1253": "cp1253",
        "utf8": "utf-8",
        "utf16": "utf-16le",
        "utf16le": "utf-16le",
        "utf16be": "utf-16be",
        "utf-16": "utf-16le",
        "utf-16-le": "utf-16le",
        "utf-16-be": "utf-16be",
        "utf32": "utf-32le",
        "utf32le": "utf-32le",
        "utf32be": "utf-32be",
        "utf-32": "utf-32le",
        "utf-32-le": "utf-32le",
        "utf-32-be": "utf-32be",
        "koi8": "koi8-r",
        "koi8r": "koi8-r",
        "detect": "auto",
    }
    return aliases.get(name, name)


def decode_bytes(data: bytes, encoding: str = "auto") -> str:
    """Decode text the way the converter does: evidence first, CP1252 last."""
    resolved = detect_encoding(data, encoding)
    if resolved in WIDE_ENCODINGS:
        mark = _bom(data)
        skip = mark[1] if mark is not None and mark[0] == resolved else 0
        codec = WIDE_ENCODINGS[resolved]
        text = data[skip:].decode(codec, errors="replace")
        return text.lstrip("﻿") if skip == 0 else text
    if resolved in SINGLE_BYTE_ENCODINGS:
        return _decode_single_byte(data, resolved)
    return data.decode("utf-8", errors="replace")


def encode_bytes(text: str, encoding: str = "utf-8") -> bytes:
    """Encode text for `write_text_file`, matching `decode_bytes`.

    A wide encoding is written *with* a byte-order mark, even if the file was
    read without one. That is the only asymmetry here, and it is deliberate: the
    mark is a declaration that costs two bytes, tools that read UTF-16 expect
    it, and the alternative is carrying "this file had no BOM" as extra state
    that the GUI's `self.encoding` string has nowhere to put.
    """
    resolved = _canonical(encoding)
    if resolved in WIDE_ENCODINGS:
        codec = WIDE_ENCODINGS[resolved]
        mark = next(m for m, name in _BOMS if name == resolved)
        return mark + text.encode(codec)
    if resolved in SINGLE_BYTE_ENCODINGS:
        return _encode_single_byte(text, resolved)
    return text.encode(resolved)


def read_text_file(path, encoding: str = "auto") -> tuple[str, str]:
    """Read a file, returning its text and the encoding it was decoded with.

    The encoding is returned rather than discarded so that a save can write the
    same bytes back. Otherwise saving a file the user only opened silently
    transcodes it.

    `encoding` overrides detection, so a user who knows their file is CP1251 can
    say so; the engine's `--encoding` and this parameter are the same decision,
    and until Phase 6 this is the only place the GUI is not guessing.
    """
    data = Path(path).read_bytes()
    resolved = detect_encoding(data, encoding)
    return decode_bytes(data, resolved), resolved


def write_text_file(path, text: str, encoding: str = "utf-8") -> None:
    """Write text, failing if the directory it names does not exist.

    A missing parent directory used to be created with `mkdir(parents=True)`, on
    the theory that a save should always succeed. It should not: this is called
    from a save dialog, which is exactly where a mistyped path happens, and one
    typo like `newtree/a/b/c/out.html` then invented five directories. The user
    is told the directory does not exist, which is actionable; a stray tree in
    the filesystem is not.
    """
    target = Path(path)
    target.write_bytes(encode_bytes(text, encoding))