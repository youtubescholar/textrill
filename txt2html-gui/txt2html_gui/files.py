# txt2html GUI — a Qt front end for the txt2html converter.
#
# Copyright (C) 2026 the txt2html-rs authors.
#
# This program is free software: you can redistribute it and/or modify it
# under the terms of the GNU General Public License as published by the Free
# Software Foundation, either version 3 of the License, or (at your option)
# any later version.  See the LICENSE file for the full text.

"""Reading and writing text files the way the converter does.

The converter reads bytes and only then decides on an encoding: UTF-8 if the
bytes are valid UTF-8, **CP1252** otherwise, so that 8-bit characters survive.
The editor has to show the same characters, and it uses the same rule — it has
to, because this file is a second implementation of an engine rule and a second
implementation is free to disagree, and until Phase 6 this one did.

Reading reports *which* encoding was used, because a file that was decoded as
CP1252 has to be written back as CP1252. Otherwise saving a file the user only
opened silently transcodes it.

The fallback is CP1252 rather than Latin-1 (P7.1), and that is the whole point
of this file having been wrong until now. Latin-1 and CP1252 differ only on
`0x80`-`0x9F`: Latin-1 calls those C1 control characters, CP1252 calls them the
curly quotes, dashes and ellipsis that every file written on Windows actually
contains. Decoding as Latin-1 made the editor show `\x93` for a left double
quote — an invisible control character — while the converter showed `"`, because
`demoronize_char` is keyed on the CP1252 code points. The user was editing one
document and previewing another.

Python's own `cp1252` codec is not usable here directly: it *rejects* the five
bytes CP1252 leaves undefined (`0x81`, `0x8D`, `0x8F`, `0x90`, `0x9D`), and the
engine keeps those as Latin-1 control characters so the decode stays total. The
tables below are written out rather than imported so the two implementations
can be diffed against each other.
"""

from __future__ import annotations

from pathlib import Path

#: The encoding used when the bytes are not valid UTF-8.
FALLBACK_ENCODING = "cp1252"

#: CP1252's `0x80`-`0x9F` range, which is where it parts company with Latin-1.
#: Kept as a written-out table rather than delegated to Python's `cp1252` codec
#: for the reason in the module docstring: that codec rejects five of these
#: bytes, and the engine does not.
_CP1252_HIGH = {
    0x80: "\u20ac", 0x82: "\u201a", 0x83: "\u0192", 0x84: "\u201e",
    0x85: "\u2026", 0x86: "\u2020", 0x87: "\u2021", 0x88: "\u02c6",
    0x89: "\u2030", 0x8a: "\u0160", 0x8b: "\u2039", 0x8c: "\u0152",
    0x8e: "\u017d", 0x91: "\u2018", 0x92: "\u2019", 0x93: "\u201c",
    0x94: "\u201d", 0x95: "\u2022", 0x96: "\u2013", 0x97: "\u2014",
    0x98: "\u02dc", 0x99: "\u2122", 0x9a: "\u0161", 0x9b: "\u203a",
    0x9c: "\u0153", 0x9e: "\u017e", 0x9f: "\u0178",
}
# 0x81, 0x8D, 0x8F, 0x90 and 0x9D are undefined in CP1252 and absent from the
# table above, so they fall through to Latin-1 and stay C1 control characters --
# which is what the engine does, and what a browser does.


def _decode_cp1252(data: bytes) -> str:
    return "".join(_CP1252_HIGH.get(b, chr(b)) for b in data)


def _encode_cp1252(text: str) -> bytes:
    reverse = {c: b for b, c in _CP1252_HIGH.items()}
    out = bytearray()
    for ch in text:
        if ch in reverse:
            out.append(reverse[ch])
        elif ord(ch) < 0x100:
            out.append(ord(ch))
        else:
            # Reachable only if the user typed something the file's encoding
            # cannot hold. Say so rather than writing a replacement character and
            # losing their text.
            raise UnicodeEncodeError(
                "cp1252", text, text.index(ch), text.index(ch) + 1,
                f"U+{ord(ch):04X} has no CP1252 byte; save as UTF-8 instead",
            )
    return bytes(out)


def detect_encoding(data: bytes) -> str:
    """The encoding `decode_bytes` will use, as a name.

    This is the converter's rule, not Python's: try UTF-8 and fall back to the
    single-byte encoding, which maps every byte, rather than guessing from a
    locale or a declared charset that might be a lie.
    """
    try:
        data.decode("utf-8")
    except UnicodeDecodeError:
        return FALLBACK_ENCODING
    return "utf-8"


def decode_bytes(data: bytes) -> str:
    """Decode text the way the converter does: UTF-8, else CP1252."""
    if detect_encoding(data) == "utf-8":
        return data.decode("utf-8")
    return _decode_cp1252(data)


def encode_bytes(text: str, encoding: str) -> bytes:
    """Encode text for `write_text_file`, routing CP1252 to our own table."""
    if encoding == "cp1252":
        return _encode_cp1252(text)
    return text.encode(encoding)


def read_text_file(path) -> tuple[str, str]:
    """Read a file, returning its text and the encoding it was decoded with.

    The encoding is returned rather than discarded so that a save can write the
    same bytes back. A byte-order mark is deliberately *not* stripped: the
    converter does not strip it either, and it shows up in the converted output,
    so stripping it here would make the preview differ from the saved file.
    """
    data = Path(path).read_bytes()
    return decode_bytes(data), detect_encoding(data)


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
