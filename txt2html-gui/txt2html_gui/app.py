# txt2html GUI — a Qt front end for the txt2html converter.
#
# Copyright (C) 2026 the txt2html-rs authors.
#
# This program is free software: you can redistribute it and/or modify it
# under the terms of the GNU General Public License as published by the Free
# Software Foundation, either version 3 of the License, or (at your option)
# any later version.  See the LICENSE file for the full text.

"""Starting the GUI."""

from __future__ import annotations

import argparse
import sys
from typing import List, Optional

from PySide6.QtWidgets import QApplication

from .mainwindow import MainWindow


def parse_args(argv: Optional[List[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="txt2html-gui",
        description="Convert plain text to HTML, with a live preview.",
    )
    parser.add_argument(
        "file", nargs="?", help="text file to open (UTF-8 or Latin-1)"
    )
    parser.add_argument(
        "--xhtml",
        dest="xhtml",
        action="store_true",
        default=None,
        help="produce XHTML, the converter's default",
    )
    parser.add_argument(
        "--no-xhtml",
        dest="xhtml",
        action="store_false",
        help="produce HTML 4 instead",
    )
    parser.add_argument(
        "--tables", action="store_true", help="start with table recognition on"
    )
    parser.add_argument(
        "--version", action="store_true", help="print the version and exit"
    )
    return parser.parse_args(argv)


def build_application(argv: Optional[List[str]] = None):
    """Create the QApplication and the main window, without showing anything.

    The tests use this to drive the window head-less.
    """
    args = parse_args(argv)
    app = QApplication.instance() or QApplication(sys.argv[:1])
    app.setApplicationName("txt2html")
    app.setApplicationDisplayName("txt2html")
    app.setOrganizationName("txt2html-gui")
    window = MainWindow()
    if args.file:
        window.load_file(args.file)
    if args.xhtml is not None:
        window.options.set_value("xhtml", args.xhtml)
    if args.tables:
        window.options.set_value("make_tables", True)
    if args.xhtml is not None or args.tables:
        window.convert_now()
    return app, window, args


def main(argv: Optional[List[str]] = None) -> int:
    app, window, args = build_application(argv)
    if args.version:
        import txt2html

        print(f"txt2html-gui for txt2html {txt2html.version()}")
        return 0
    window.show()
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
