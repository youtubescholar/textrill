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
bytes are valid UTF-8, Latin-1 otherwise, so that 8-bit characters survive.
The editor has to show the same characters, and it uses the same rule.

Reading reports *which* encoding was used, because a file that was decoded as
Latin-1 has to be written back as Latin-1. Otherwise saving a file the user only
opened silently transcodes it, and for a CP1252 file that is lossy rather than
merely untidy: the 0x80-0x9F bytes are Windows punctuation and an em dash, which
Latin-1 turns into C1 control characters, and re-encoding those as UTF-8
produces `\xc2\x97` where the file said `\x97`.
"""

from __future__ import annotations

from pathlib import Path

#: Encodings tried when opening a file, in order.
FALLBACK_ENCODING = "latin-1"


def detect_encoding(data: bytes) -> str:
    """The encoding `decode_bytes` will use, as a name.

    This is the converter's rule, not Python's: try UTF-8 and fall back to
    Latin-1, which maps every byte, rather than guessing from a locale or a
    declared charset that might be a lie.
    """
    try:
        data.decode("utf-8")
    except UnicodeDecodeError:
        return FALLBACK_ENCODING
    return "utf-8"


def decode_bytes(data: bytes) -> str:
    """Decode text the way the converter does: UTF-8, else Latin-1."""
    return data.decode(detect_encoding(data))


def read_text_file(path) -> tuple[str, str]:
    """Read a file, returning its text and the encoding it was decoded with.

    The encoding is returned rather than discarded so that a save can write the
    same bytes back. A byte-order mark is deliberately *not* stripped: the
    converter does not strip it either, and it shows up in the converted output,
    so stripping it here would make the preview differ from the saved file.
    """
    data = Path(path).read_bytes()
    encoding = detect_encoding(data)
    return data.decode(encoding), encoding


def write_text_file(path, text: str, encoding: str = "utf-8") -> None:
    """Write text, creating parent directories that are missing."""
    target = Path(path)
    if target.parent and not target.parent.exists():
        target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text, encoding=encoding)
