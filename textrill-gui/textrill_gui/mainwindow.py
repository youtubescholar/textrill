# txt2html GUI — a Qt front end for the txt2html converter.
#
# Copyright (C) 2026 the txt2html-rs authors.
#
# This program is free software: you can redistribute it and/or modify it
# under the terms of the GNU General Public License as published by the Free
# Software Foundation, either version 3 of the License, or (at your option)
# any later version.  See the LICENSE file for the full text.

"""The main window: text on the left, HTML on the right, options alongside."""

from __future__ import annotations

import os
from typing import Optional

from PySide6.QtCore import QCoreApplication, QSettings, Qt, QTimer, Slot
from PySide6.QtGui import QAction, QFont, QKeySequence, QTextCursor
from PySide6.QtWidgets import (
    QApplication,
    QCheckBox,
    QFileDialog,
    QLabel,
    QMainWindow,
    QMessageBox,
    QPlainTextEdit,
    QSplitter,
    QTabWidget,
    QTextBrowser,
    QToolBar,
    QWidget,
)

import txt2html

from .files import read_text_file, write_text_file
from .optionspanel import OptionsPanel
from .worker import Converter

#: How long to wait after the last keystroke before converting, in ms.
AUTO_CONVERT_DELAY = 300

#: Encoding offered when saving the converted document.
HTML_FILTER = "HTML files (*.html *.htm);;All files (*)"
TEXT_FILTER = "Text files (*.txt *.text);;All files (*)"

SAMPLE_TEXT = """\
EXAMPLE HEADER
=============

This is a paragraph with *emphasis*, #strong# text and a link to
https://example.org/ in it.  Paragraphs, lists, tables and preformatted
text are all recognised:

1. first item
2. second item
    - a nested bullet
    - another one

+--------+--------+
| Food   | Qty    |
+--------+--------+
| Bread  | 1      |
| Milk   | 1      |
| Oranges| 3      |
| Apples | 6      |
+--------+--------+

Turn on "make tables" in the options panel to convert that block into a
real table.
"""


class MainWindow(QMainWindow):
    """The application window."""

    def __init__(self, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("txt2html")
        self.resize(1100, 720)
        self.setAcceptDrops(True)

        # An explicit organization name lets the tests keep their settings in
        # a throw-away location instead of the user's real configuration.
        org = QCoreApplication.organizationName() or "txt2html-gui"
        app = QCoreApplication.applicationName() or "txt2html-gui"
        self.settings = QSettings(org, app)
        # path: the text file that was opened; output_path: where the HTML
        # was last written.  They are deliberately different files.
        self.path: Optional[str] = None
        self.output_path: Optional[str] = None
        # The encoding the source file was decoded with, so "Save text" writes
        # the same bytes back. Defaults to UTF-8 for text that never came from a
        # file. Reset with the file on load, not carried over: a new file's
        # encoding says nothing about the old one.
        self.encoding: str = "utf-8"
        # dirty tracks edits to the *text*; output_stale tracks HTML that has
        # not been written to output_path yet.  Saving the HTML must never
        # make unsaved text edits look saved.
        self.dirty = False
        self.output_stale = False
        # the exact HTML last written to output_path, so re-converting the
        # same text does not keep claiming there is unsaved output
        self._saved_html: Optional[str] = None
        self._latest = 0
        self._completed = 0

        self.converter = Converter(self)

        self._build_widgets()
        self._build_actions()
        self._restore_settings()

        self.converter.sink.finished.connect(self._on_converted)
        self.options.changed.connect(self.schedule_convert)
        self.editor.textChanged.connect(self._on_text_changed)

        self.auto_checkbox.toggled.connect(self._on_auto_toggled)
        self._on_auto_toggled(True)

        self.convert_now()
        QTimer.singleShot(0, self.editor.setFocus)

    # ------------------------------------------------------------------ ui

    def _build_widgets(self) -> None:
        self.editor = QPlainTextEdit()
        self.editor.setPlaceholderText("Type or drop a text file here…")
        self.editor.setFont(QFont("monospace", 11))
        self.editor.setLineWrapMode(QPlainTextEdit.NoWrap)
        self.editor.setTabStopDistance(4 * self.editor.fontMetrics().horizontalAdvance(" "))

        self.preview = QTextBrowser()
        self.preview.setOpenExternalLinks(True)

        self.html_view = QPlainTextEdit()
        self.html_view.setReadOnly(True)
        self.html_view.setFont(QFont("monospace", 10))
        self.html_view.setLineWrapMode(QPlainTextEdit.NoWrap)

        self.tabs = QTabWidget()
        self.tabs.addTab(self.preview, "Preview")
        self.tabs.addTab(self.html_view, "HTML")

        self.options = OptionsPanel()

        splitter = QSplitter(Qt.Horizontal)
        splitter.addWidget(self.editor)
        splitter.addWidget(self.tabs)
        splitter.setStretchFactor(0, 1)
        splitter.setStretchFactor(1, 1)
        splitter.setSizes([520, 580])

        self.splitter = QSplitter(Qt.Horizontal)
        self.splitter.addWidget(splitter)
        self.splitter.addWidget(self.options)
        self.splitter.setStretchFactor(0, 1)
        self.splitter.setStretchFactor(1, 0)
        self.splitter.setSizes([820, 300])
        self.setCentralWidget(self.splitter)

        self.status = self.statusBar()
        self.status_label = QLabel("ready")
        self.status.addPermanentWidget(self.status_label)

        self.timer = QTimer(self)
        self.timer.setSingleShot(True)
        self.timer.setInterval(AUTO_CONVERT_DELAY)
        self.timer.timeout.connect(self.convert_now)

    def _build_actions(self) -> None:
        menu = self.menuBar()

        file_menu = menu.addMenu("&File")
        self.act_new = QAction("&New", self)
        self.act_new.setShortcut(QKeySequence.StandardKey.New)
        self.act_new.triggered.connect(self.new_document)
        file_menu.addAction(self.act_new)

        self.act_open = QAction("&Open…", self)
        self.act_open.setShortcut(QKeySequence.StandardKey.Open)
        self.act_open.triggered.connect(self.open_document)
        file_menu.addAction(self.act_open)

        self.act_save = QAction("&Save", self)
        self.act_save.setShortcut(QKeySequence.StandardKey.Save)
        self.act_save.triggered.connect(self.save_document)
        file_menu.addAction(self.act_save)

        self.act_save_as = QAction("Save &As…", self)
        self.act_save_as.setShortcut(QKeySequence.StandardKey.SaveAs)
        self.act_save_as.triggered.connect(self.save_document_as)
        file_menu.addAction(self.act_save_as)

        # Save and Save As write the *HTML*.  The text being converted is a
        # separate document and needs its own way out, or edits to it would be
        # lost on exit.
        self.act_save_text = QAction("Save &text…", self)
        self.act_save_text.setShortcut(QKeySequence("Ctrl+Shift+S"))
        self.act_save_text.triggered.connect(self.save_text)
        file_menu.addAction(self.act_save_text)

        file_menu.addSeparator()
        self.act_sample = QAction("Load &example", self)
        self.act_sample.triggered.connect(self.load_sample)
        file_menu.addAction(self.act_sample)

        file_menu.addSeparator()
        self.act_quit = QAction("&Quit", self)
        self.act_quit.setShortcut(QKeySequence.StandardKey.Quit)
        self.act_quit.triggered.connect(self.close)
        file_menu.addAction(self.act_quit)

        edit_menu = menu.addMenu("&Edit")
        self.act_copy_html = QAction("Copy &HTML", self)
        self.act_copy_html.setShortcut(QKeySequence.StandardKey.Copy)
        self.act_copy_html.triggered.connect(self.copy_html)
        edit_menu.addAction(self.act_copy_html)
        self.act_convert = QAction("&Convert now", self)
        self.act_convert.setShortcut("Ctrl+Return")
        self.act_convert.triggered.connect(self.convert_now)
        edit_menu.addAction(self.act_convert)
        self.act_reset = QAction("&Reset options", self)
        self.act_reset.triggered.connect(self.options.reset_all)
        edit_menu.addAction(self.act_reset)

        view_menu = menu.addMenu("&View")
        self.act_toggle_options = view_menu.addAction("Show &options")
        self.act_toggle_options.setCheckable(True)
        self.act_toggle_options.setChecked(True)
        self.act_toggle_options.triggered.connect(self.options.setVisible)

        help_menu = menu.addMenu("&Help")
        act_about = QAction("&About", self)
        act_about.triggered.connect(self.about)
        help_menu.addAction(act_about)
        act_about_qt = QAction("About &Qt", self)
        act_about_qt.triggered.connect(lambda: QApplication.aboutQt())
        help_menu.addAction(act_about_qt)

        toolbar = QToolBar("Main", self)
        toolbar.setObjectName("main-toolbar")
        toolbar.setMovable(False)
        for action in (self.act_new, self.act_open, self.act_save, self.act_sample):
            toolbar.addAction(action)
        toolbar.addSeparator()
        toolbar.addAction(self.act_convert)
        self.auto_checkbox = QCheckBox("auto")
        self.auto_checkbox.setToolTip("Convert as you type")
        # on by default: a live preview is the point of the window.  A
        # previous session's choice, if any, overrides this below.
        self.auto_checkbox.setChecked(True)
        toolbar.addWidget(self.auto_checkbox)
        toolbar.addSeparator()
        toolbar.addAction(self.act_toggle_options)
        self.addToolBar(toolbar)

    # ------------------------------------------------------------ behaviour

    @Slot()
    def _on_text_changed(self) -> None:
        self.dirty = True
        self._update_title()
        if self.auto_checkbox.isChecked():
            self.schedule_convert()

    @Slot()
    def _on_auto_toggled(self, enabled: bool) -> None:
        if enabled:
            self.schedule_convert()
        else:
            self.timer.stop()

    def schedule_convert(self) -> None:
        """Queue a conversion, coalescing bursts of changes."""
        if self.auto_checkbox.isChecked() or not self._latest:
            self.timer.start()
        else:
            self.convert_now()

    @Slot()
    def convert_now(self) -> None:
        self.timer.stop()
        text = self.editor.toPlainText()
        self._latest = self.converter.convert(text, self.options.values())
        self.statusBar().showMessage("converting…")

    @Slot(int, str, float, str)
    def _on_converted(self, generation: int, html: str, seconds: float, error: str) -> None:
        if generation < self._latest:
            return  # a newer conversion is already under way
        self._completed = generation
        if error:
            self.preview.setHtml(
                f"<p><b>Conversion failed:</b></p><pre>{_escape(error)}</pre>"
            )
            self.html_view.setPlainText(error)
            self.status_label.setText("error")
            self.statusBar().showMessage("conversion failed", 5000)
            return
        self.preview.setHtml(html)
        self.html_view.setPlainText(html)
        cursor = self.html_view.textCursor()
        cursor.movePosition(QTextCursor.Start)
        self.html_view.setTextCursor(cursor)
        lines = html.count("\n") + 1 if html else 0
        self.status_label.setText(f"{lines} lines · {seconds * 1000:.0f} ms")
        # Fresh HTML means whatever is on disk no longer matches it.
        if html != self._saved_html:
            self.output_stale = True
            self._update_title()
        self.statusBar().clearMessage()

    # ------------------------------------------------------------- commands

    @Slot()
    def new_document(self) -> None:
        if not self._maybe_save():
            return
        self.editor.clear()
        self.path = None
        self.output_path = None
        self._saved_html = None
        self.output_stale = False
        self.dirty = False
        self._update_title()
        self.convert_now()

    @Slot()
    def open_document(self) -> None:
        if not self._maybe_save():
            return
        start = self.path or os.getcwd()
        name, _ = QFileDialog.getOpenFileName(
            self, "Open text file", start, TEXT_FILTER
        )
        if not name:
            return
        self.load_file(name)

    def load_file(self, name: str) -> None:
        try:
            text, self.encoding = read_text_file(name)
        except OSError as exc:
            QMessageBox.warning(self, "Cannot open", str(exc))
            return
        self.editor.setPlainText(text)
        self.path = name
        # a newly opened file has no saved HTML yet
        self.output_path = None
        self._saved_html = None
        self.output_stale = False
        self.dirty = False
        self._update_title()
        self.convert_now()

    @Slot()
    def load_sample(self) -> None:
        if not self._maybe_save():
            return
        self.editor.setPlainText(SAMPLE_TEXT)
        self.path = None
        self.output_path = None
        self._saved_html = None
        self.output_stale = False
        self.dirty = True
        self._update_title()
        self.convert_now()

    @Slot()
    def save_text(self) -> None:
        """Write the plain text back, the way any editor would."""
        name = self.path
        if not name:
            name, _ = QFileDialog.getSaveFileName(
                self, "Save text", os.path.join(os.getcwd(), "untitled.txt"), TEXT_FILTER
            )
            if not name:
                return
        try:
            # Back to the encoding the file was read with. Decoding as Latin-1
            # and writing UTF-8 would transcode the file behind the user's back,
            # and for CP1252 that is lossy: 0x80-0x9F are punctuation there and
            # control characters in Latin-1.
            write_text_file(name, self.editor.toPlainText(), self.encoding)
        except OSError as exc:
            QMessageBox.warning(self, "Cannot save", str(exc))
            return
        self.path = name
        self.dirty = False
        self._update_title()
        self.statusBar().showMessage(f"saved text to {name}", 4000)

    @Slot()
    def save_document(self) -> None:
        """Save the HTML, never over the text file that was opened."""
        if not self.output_path:
            self.save_document_as()
            return
        self._save(self.output_path)

    @Slot()
    def save_document_as(self) -> None:
        # The suggestion is the *output* name: proposing the source .txt back
        # would invite overwriting the very file being converted.
        suggestion = self.output_path or self.path
        if suggestion:
            suggestion = os.path.splitext(suggestion)[0] + ".html"
        else:
            suggestion = os.path.join(os.getcwd(), "converted.html")
        name, _ = QFileDialog.getSaveFileName(
            self, "Save HTML", suggestion, HTML_FILTER
        )
        if name:
            self._save(name)

    def _save(self, name: str) -> None:
        # Guard against clobbering the file that is being converted *before*
        # touching the disk: the source must stay intact even when a path is
        # typed into the dialog by hand.
        if self.path and os.path.abspath(name) == os.path.abspath(self.path):
            QMessageBox.warning(
                self,
                "Cannot save",
                f"{name} is the file you are converting.\n"
                "Choose a different name, or use File → Save As.",
            )
            return
        try:
            # The generated HTML is UTF-8 whatever the source file was: it is
            # text this program produced, not a transcription of the input. Note
            # that it declares no charset yet, which is P7.2's job.
            write_text_file(name, self.html_view.toPlainText(), "utf-8")
        except OSError as exc:
            QMessageBox.warning(self, "Cannot save", str(exc))
            return
        self.output_path = name
        self._saved_html = self.html_view.toPlainText()
        # Saving the generated HTML does not make edits to the *source* text
        # go away, so the text keeps its own dirty flag.
        self.output_stale = False
        self._update_title()
        self.statusBar().showMessage(f"saved {name}", 4000)

    @Slot()
    def copy_html(self) -> None:
        clipboard = QApplication.clipboard()
        if clipboard is not None:
            clipboard.setText(self.html_view.toPlainText())
            self.statusBar().showMessage("HTML copied to the clipboard", 3000)

    @Slot()
    def about(self) -> None:
        QMessageBox.about(
            self,
            "About txt2html",
            f"<h3>txt2html {txt2html.version()}</h3>"
            "<p>A Rust port of <b>HTML::TextToHTML</b> 3.0, with a Qt front end.</p>"
            "<p>The conversion is byte-identical to the original Perl module.</p>"
            "<p>Released under the GNU General Public License, version 3 or later.</p>",
        )

    # --------------------------------------------------------------- window

    def _maybe_save(self) -> bool:
        if not self.dirty:
            return True
        answer = QMessageBox.question(
            self,
            "Unsaved changes",
            "The text has unsaved changes.  Save them?",
            QMessageBox.StandardButton.Save
            | QMessageBox.StandardButton.Discard
            | QMessageBox.StandardButton.Cancel,
        )
        if answer == QMessageBox.StandardButton.Cancel:
            return False
        if answer == QMessageBox.StandardButton.Save:
            # This must write the *text*, which is what the prompt is about;
            # save_document would only write the HTML and leave the flag set.
            self.save_text()
            return not self.dirty
        return True

    def _update_title(self) -> None:
        name = os.path.basename(self.path) if self.path else "untitled"
        # "*" marks unsaved text edits, "+" marks HTML not yet written out.
        mark = ("*" if self.dirty else "") + ("+" if self.output_stale else "")
        self.setWindowTitle(f"{name}{mark} — txt2html")

    def _restore_settings(self) -> None:
        geometry = self.settings.value("geometry")
        if geometry is not None:
            self.restoreGeometry(geometry)
        state = self.settings.value("window-state")
        if state is not None:
            self.restoreState(state)
        # A stored value wins in both directions; a fresh install keeps the "on"
        # default that _build_actions set.  QSettings returns a real bool for a
        # value we stored as one, and the string "true" for a value written by
        # hand into the INI file, so accept both spellings.
        auto = self.settings.value("auto")
        if auto is not None:
            self.auto_checkbox.setChecked(_as_bool(auto))
        values = self.settings.value("options")
        if isinstance(values, str) and values:
            import json

            try:
                self.options.set_values(json.loads(values))
            except (ValueError, TypeError):
                pass

    def _store_settings(self) -> None:
        import json

        self.settings.setValue("geometry", self.saveGeometry())
        self.settings.setValue("window-state", self.saveState())
        self.settings.setValue("auto", self.auto_checkbox.isChecked())
        self.settings.setValue("options", json.dumps(self.options.values()))

    def closeEvent(self, event) -> None:  # noqa: N802 (Qt naming)
        if not self._maybe_save():
            event.ignore()
            return
        self.timer.stop()
        # let running conversions finish before the window disappears
        self.converter.wait(5000)
        self._store_settings()
        event.accept()

    # drag and drop

    def dragEnterEvent(self, event) -> None:  # noqa: N802 (Qt naming)
        if event.mimeData().hasUrls():
            event.acceptProposedAction()

    def dropEvent(self, event) -> None:  # noqa: N802 (Qt naming)
        for url in event.mimeData().urls():
            if url.isLocalFile():
                self.load_file(url.toLocalFile())
                break


def _as_bool(value) -> bool:
    """Read a QSettings value that may be a bool, a number, or a string."""
    if isinstance(value, str):
        return value.strip().lower() in ("true", "1", "yes", "on")
    return bool(value)


def _escape(text: str) -> str:
    return (
        text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
    )
