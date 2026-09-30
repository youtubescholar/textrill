# txt2html GUI — a Qt front end for the txt2html converter.
#
# Copyright (C) 2026 the txt2html-rs authors.
#
# This program is free software: you can redistribute it and/or modify it
# under the terms of the GNU General Public License as published by the Free
# Software Foundation, either version 3 of the License, or (at your option)
# any later version.  See the LICENSE file for the full text.

"""A Qt front end for :mod:`txt2html`.

Run it with ``python -m txt2html_gui`` or, once installed, ``txt2html-gui``.
"""

__version__ = "3.0.0"

__all__ = ["__version__", "main"]


def main(argv=None):
    """Entry point: start the GUI and return its exit code."""
    from .app import main as _main

    return _main(argv)
