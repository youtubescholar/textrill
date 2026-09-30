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
"""

from __future__ import annotations

from pathlib import Path

#: Encodings tried when opening a file, in order.
FALLBACK_ENCODING = "latin-1"


def decode_bytes(data: bytes) -> str:
    """Decode text the way the converter does: UTF-8, else Latin-1."""
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return data.decode(FALLBACK_ENCODING)


def read_text_file(path) -> str:
    """Read a file and return its text, never raising on encoding."""
    return decode_bytes(Path(path).read_bytes())


def write_text_file(path, text: str, encoding: str = "utf-8") -> None:
    """Write text, creating parent directories that are missing."""
    target = Path(path)
    if target.parent and not target.parent.exists():
        target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text, encoding=encoding)
