# textrill-gui — a Qt front end for textrill.
#
# Copyright (C) 2026 the textrill authors.
#
# This program is free software: you can redistribute it and/or modify it
# under the terms of the GNU General Public License as published by the Free
# Software Foundation, either version 3 of the License, or (at your option)
# any later version.  See the LICENSE file for the full text.

"""A Qt front end for :mod:`textrill`.

Run it with ``python -m textrill_gui`` or, once installed, ``textrill-gui``.
"""

__version__ = "3.0.0"

__all__ = ["__version__", "main"]


def main(argv=None):
    """Entry point: start the GUI and return its exit code."""
    from .app import main as _main

    return _main(argv)
