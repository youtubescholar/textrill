# txt2html GUI — a Qt front end for the txt2html converter.
#
# Copyright (C) 2026 the txt2html-rs authors.
#
# This program is free software: you can redistribute it and/or modify it
# under the terms of the GNU General Public License as published by the Free
# Software Foundation, either version 3 of the License, or (at your option)
# any later version.  See the LICENSE file for the full text.

"""Head-less smoke tests.

Run with ``python -m unittest discover -s tests`` from this directory.  They
need no display: the offscreen Qt platform is selected before Qt starts.
"""

import os
import sys
import tempfile
import unittest
from contextlib import contextmanager
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
# Keep the tests out of the user's real configuration file.  This is assigned
# rather than defaulted: a stale XDG_CONFIG_HOME exported by an earlier command
# would otherwise leak a previous run's settings into these tests.
os.environ["XDG_CONFIG_HOME"] = tempfile.mkdtemp(prefix="t2h-gui-test-")

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import txt2html  # noqa: E402
from txt2html_gui.app import build_application  # noqa: E402
from txt2html_gui.files import decode_bytes, read_text_file, write_text_file  # noqa: E402
from txt2html_gui.optionspanel import TABLE_TYPES  # noqa: E402
from PySide6.QtCore import QEventLoop, QTimer  # noqa: E402
from PySide6.QtWidgets import QFileDialog, QMessageBox  # noqa: E402


def drain(app, window, timeout_ms: int = 20000) -> str:
    """Run the event loop until the newest conversion has been applied."""
    window.convert_now()
    generation = window._latest
    loop = QEventLoop()
    deadline = [False]

    def check() -> None:
        if window._completed >= generation:
            deadline[0] = True
            loop.quit()

    poll = QTimer()
    poll.setInterval(10)
    poll.timeout.connect(check)
    poll.start()
    QTimer.singleShot(timeout_ms, loop.quit)
    loop.exec()
    poll.stop()
    if not deadline[0]:
        raise AssertionError("conversion did not finish in time")
    return window.html_view.toPlainText()


class ConverterTests(unittest.TestCase):
    def test_simple_conversion(self):
        html = txt2html.convert("hello *world*", {"extract": True})
        self.assertIn("<p>hello <em>world</em></p>", html)

    def test_xhtml_flag_changes_tags(self):
        html = txt2html.convert("hi\n", {"xhtml": False})
        self.assertIn("<HTML>", html)
        html = txt2html.convert("hi\n", {"xhtml": True})
        self.assertIn("<html", html)

    def test_table_type_flags(self):
        # a border table: "+---+" rules with a closing "(N rows)" line
        text = (
            "+--------+------+\n"
            "| Schema | Name |\n"
            "+--------+------+\n"
            "| public | foo  |\n"
            "| public | bar  |\n"
            "+--------+------+\n"
            "\n(2 rows)\n"
        )
        base = {"make_tables": True, "extract": True}
        self.assertIn("<table", txt2html.convert(text, base))
        self.assertNotIn(
            "<table",
            txt2html.convert(text, dict(base, table_type={"BORDER": False})),
        )
        # the same table as a string spelling of the flags
        self.assertIn(
            "<table",
            txt2html.convert(text, dict(base, table_type="BORDER=1 DELIM=1")),
        )

    def test_table_type_matches_the_reference(self):
        source = os.environ.get("T2H_TFILES")
        if not source:
            self.skipTest("T2H_TFILES is not set")
        text = read_text_file(os.path.join(source, "table-border.txt"))
        golden = read_text_file(os.path.join(source, "good_table-border.html"))
        self.assertEqual(
            txt2html.convert(text, {"make_tables": True, "extract": True}), golden
        )

    def test_option_specs_cover_the_whole_table(self):
        specs = txt2html.option_specs()
        self.assertGreater(len(specs), 40)
        names = set()
        for name, aliases, kind, default, help_text in specs:
            names.add(name)
            self.assertIn(kind, ("bool", "int", "str", "str_array", "table_type"))
            self.assertIsInstance(default, str)
            self.assertTrue(help_text)
        # "escape_HTML_chars" is mixed case upstream; everything else is lower
        self.assertEqual(
            [n for n, *_ in specs if n != n.lower()], ["escape_HTML_chars"]
        )

    def test_aliases_are_usable_and_unique(self):
        specs = txt2html.option_specs()
        names = {s[0] for s in specs}
        seen = set()
        for name, aliases, kind, default, _ in specs:
            for alias in aliases:
                # an alias must not hide a real option, and must not clash
                # with another alias: Getopt::Long would have to pick one
                self.assertNotIn(alias, names, f"{alias} hides a real option")
                self.assertNotIn(alias, seen, f"{alias} is defined twice")
                seen.add(alias)
                # the alias must behave like the option it stands for
                options = {alias: default}
                self.assertEqual(
                    txt2html.convert("x\n", options),
                    txt2html.convert("x\n", {name: default}),
                    f"alias {alias} does not mean {name}",
                )


class FileTests(unittest.TestCase):
    def test_latin1_fallback(self):
        self.assertEqual(decode_bytes(b"caf\xe9"), "café")
        self.assertEqual(decode_bytes("café".encode()), "café")

    def test_round_trip(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "sub" / "note.txt"
            write_text_file(path, "héllo\n")
            self.assertEqual(read_text_file(path), "héllo\n")


class GuiTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app, cls.window, _ = build_application([])

    @classmethod
    def tearDownClass(cls):
        # no modal dialogs: mark the document clean and close directly
        cls.window.dirty = False
        cls.window.close()
        cls.window.converter.wait(5000)

    def setUp(self):
        # The window is shared by the whole class, so a test that edits the
        # text would otherwise leave the next one staring at a modal
        # "unsaved changes" dialog.  Start every test from a clean document.
        self.window.dirty = False
        self.window.output_stale = False
        self.window._saved_html = None
        self.window.path = None
        self.window.output_path = None
        self.window.settings.remove("auto")
        self.window._restore_settings()

    def test_conversion_appears(self):
        self.window.editor.setPlainText("hello *world*\n")
        html = drain(self.app, self.window)
        self.assertIn("<em>world</em>", html)
        self.assertIn("hello", self.window.preview.toPlainText())

    def test_engine_panic_is_reported_instead_of_hanging(self):
        """A4: a panic in the engine must not leave the window waiting.

        pyo3's PanicException inherits BaseException rather than Exception, so
        the old `except Exception` in the worker did not catch it: no result was
        emitted, the status bar stayed on "converting…", and the user got a dead
        preview with no message. This asserts a message actually appears.

        tab_width=0 divides by zero in the tab expander, so the engine panics.
        """
        self.window.editor.setPlainText("hello\n\tworld\n")
        self.window.options.set_value("tab_width", 0)
        try:
            html = drain(self.app, self.window)
        finally:
            self.window.options.reset_all()
        self.assertNotEqual(
            html.strip(), "", "the preview went blank with nothing to show for it"
        )
        self.assertIn("stopped on invalid input", html)
        self.assertEqual(self.window.status_label.text(), "error")
        # and the window is usable again rather than wedged
        self.assertGreaterEqual(self.window._completed, self.window._latest)

    def test_panic_exception_is_importable_and_is_a_base_exception(self):
        """The class has to be nameable, or catching it precisely is impossible.

        pyo3 puts PanicException in `pyo3_runtime`, which is not importable, so
        without the re-export in python.rs no caller could write
        `except txt2html.PanicException`.
        """
        self.assertTrue(hasattr(txt2html, "PanicException"))
        self.assertFalse(
            issubclass(txt2html.PanicException, Exception),
            "if this ever becomes an Exception the test stops proving anything",
        )
        self.assertTrue(issubclass(txt2html.PanicException, BaseException))

    def test_option_change_is_picked_up(self):
        self.window.editor.setPlainText("hi\n")
        drain(self.app, self.window)
        self.window.options.set_value("xhtml", False)
        html = drain(self.app, self.window)
        self.assertIn("<HTML>", html)
        self.window.options.reset_all()
        self.assertTrue(self.window.options.values()["xhtml"])

    def test_table_switches_exist(self):
        values = self.window.options.values()
        self.assertEqual(set(values["table_type"]), set(TABLE_TYPES))
        self.assertTrue(values["table_type"]["BORDER"])

    def test_auto_convert_is_on_by_default(self):
        # A fresh install must convert as you type; a fresh QCheckBox starts
        # unchecked, and the window used to leave it that way.
        self.assertTrue(self.window.auto_checkbox.isChecked())
        self.window.editor.setPlainText("typed text\n")
        self.assertIn("typed text", drain(self.app, self.window))

    def test_typing_while_auto_is_off_does_not_convert(self):
        window = self.window
        was = window.auto_checkbox.isChecked()
        window.auto_checkbox.setChecked(False)
        try:
            window.editor.setPlainText("held back\n")
            loop = QEventLoop()
            QTimer.singleShot(1200, loop.quit)
            loop.exec()
            self.assertNotIn("held back", window.html_view.toPlainText())
        finally:
            window.auto_checkbox.setChecked(was)

    def test_sample_text_exercises_the_converter(self):
        from txt2html_gui.mainwindow import SAMPLE_TEXT

        html = txt2html.convert(SAMPLE_TEXT, {"extract": True, "make_tables": True})
        for what, fragment in [
            ("emphasis", "<em>emphasis</em>"),
            ("strong", "<strong>strong</strong>"),
            ("link", "example.org"),
            ("table", "<table"),
            ("heading", "EXAMPLE"),
            ("list", "<ol"),
        ]:
            self.assertIn(fragment, html, f"the sample lost its {what}")

    def test_save_as_does_not_propose_overwriting_the_source(self):
        # Opening a .txt and hitting Save As used to suggest the .txt back,
        # which reads as "overwrite the file you are converting?".
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "book.txt"
            source.write_text("a chapter\n", encoding="utf-8")
            self.window.load_file(str(source))
            drain(self.app, self.window)

            proposed = self._capture_save_dialog(self.window.save_document_as)
            self.assertTrue(proposed.endswith(".html"), proposed)
            self.assertNotEqual(proposed, str(source))

    def test_save_with_nothing_saved_never_writes_the_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "book.txt"
            source.write_text("a chapter\n", encoding="utf-8")
            self.window.load_file(str(source))
            drain(self.app, self.window)
            before = source.read_bytes()

            # cancel the dialog that Save opens when nothing has been saved
            self._capture_save_dialog(self.window.save_document)
            self.assertEqual(source.read_bytes(), before, "source was overwritten")
            self.assertIsNone(self.window.output_path)

    def test_saving_twice_reuses_the_chosen_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "book.txt"
            source.write_text("a chapter\n", encoding="utf-8")
            self.window.load_file(str(source))
            html = drain(self.app, self.window)

            target = Path(tmp) / "out" / "book.html"
            self.window._save(str(target))
            self.assertEqual(target.read_text(encoding="utf-8"), html)
            self.assertEqual(self.window.output_path, str(target))

            # a second Ctrl+S writes the same file, no dialog
            self.window.html_view.setPlainText("edited\n")
            self.window._save(str(target))
            self.assertEqual(target.read_text(encoding="utf-8"), "edited\n")

    def test_opening_another_file_forgets_the_previous_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "one.txt"
            first.write_text("one\n", encoding="utf-8")
            second = Path(tmp) / "two.txt"
            second.write_text("two\n", encoding="utf-8")
            self.window.load_file(str(first))
            drain(self.app, self.window)
            self.window._save(str(Path(tmp) / "one.html"))
            self.assertIsNotNone(self.window.output_path)
            self.window.load_file(str(second))
            self.assertIsNone(self.window.output_path)

    def test_saving_straight_to_the_source_path_is_refused(self):
        # The guard used to run *after* write_text_file, so a hand-typed path
        # could still clobber the file being converted.
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "book.txt"
            source.write_text("keep me\n", encoding="utf-8")
            self.window.load_file(str(source))
            drain(self.app, self.window)

            with self._no_dialogs() as shown:
                self.window._save(str(source))
            self.assertTrue(shown, "the user should have been warned")
            self.assertEqual(source.read_text(encoding="utf-8"), "keep me\n")
            self.assertIsNone(self.window.output_path)

    def test_saving_html_does_not_clear_unsaved_text_edits(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "book.txt"
            source.write_text("original\n", encoding="utf-8")
            self.window.load_file(str(source))
            drain(self.app, self.window)

            self.window.editor.setPlainText("edited in the editor\n")
            self.assertTrue(self.window.dirty)

            self.window._save(str(Path(tmp) / "book.html"))
            self.assertTrue(
                self.window.dirty,
                "saving the HTML must not claim the text edits are saved",
            )

    def test_save_text_writes_the_editor_contents(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "book.txt"
            source.write_text("original\n", encoding="utf-8")
            self.window.load_file(str(source))
            drain(self.app, self.window)

            self.window.editor.setPlainText("edited in the editor\n")
            self.window.save_text()
            self.assertEqual(source.read_text(encoding="utf-8"), "edited in the editor\n")
            self.assertFalse(self.window.dirty)

    def test_reconverting_the_same_text_does_not_claim_unsaved_output(self):
        self.window.load_sample()
        drain(self.app, self.window)
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "out.html"
            self.window._save(str(target))
            self.assertFalse(self.window.output_stale)

            # an identical reconversion should not re-dirty the output
            self.window.convert_now()
            drain(self.app, self.window)
            self.assertFalse(self.window.output_stale)

    def test_auto_convert_setting_survives_a_restart(self):
        # QSettings gives back a real bool for a value stored as one, not the
        # string "true", so a naive == "true" test silently turned auto off.
        original = self.window.auto_checkbox.isChecked()
        self.addCleanup(self._restore_auto, original)
        for stored, expected in ((True, True), (False, False)):
            self.window.settings.setValue("auto", stored)
            self.window.settings.sync()
            self.window._restore_settings()
            self.assertIs(
                self.window.auto_checkbox.isChecked(), expected, f"stored {stored!r}"
            )
            self.assertIsNotNone(self.window.settings.value("auto"))

    def test_auto_convert_setting_survives_a_hand_edited_config(self):
        original = self.window.auto_checkbox.isChecked()
        self.addCleanup(self._restore_auto, original)
        self.window.settings.setValue("auto", "true")
        self.window.settings.sync()
        self.window._restore_settings()
        self.assertTrue(self.window.auto_checkbox.isChecked())

    def _restore_auto(self, checked: bool) -> None:
        """Put the shared window's auto state back for the next test."""
        self.window.auto_checkbox.setChecked(checked)

    @contextmanager
    def _no_dialogs(self):
        """Swallow modal message boxes, recording that one was shown."""
        shown = []
        original_warning = QMessageBox.warning
        original_question = QMessageBox.question
        original_critical = QMessageBox.critical
        original_info = QMessageBox.information

        def record(name, result=None):
            def stub(*args, **kwargs):
                shown.append(name)
                return result

            return stub

        QMessageBox.warning = staticmethod(record("warning"))
        QMessageBox.critical = staticmethod(record("critical"))
        QMessageBox.information = staticmethod(record("information"))
        QMessageBox.question = staticmethod(
            record("question", QMessageBox.StandardButton.Save)
        )
        try:
            yield shown
        finally:
            QMessageBox.warning = original_warning
            QMessageBox.critical = original_critical
            QMessageBox.information = original_info
            QMessageBox.question = original_question

    @staticmethod
    def _capture_save_dialog(action) -> str:
        """Run a Save As action with the dialog stubbed out."""
        seen = {}
        original = QFileDialog.getSaveFileName

        def spy(parent, caption, start, filt, *args):
            seen["start"] = start
            return ("", "")

        QFileDialog.getSaveFileName = staticmethod(spy)
        try:
            action()
        finally:
            QFileDialog.getSaveFileName = original
        return seen["start"]

    def test_load_file_and_save(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "in.txt"
            source.write_text("Title\n=====\n\nbody text\n", encoding="utf-8")
            self.window.load_file(str(source))
            html = drain(self.app, self.window)
            self.assertIn("body text", html)
            self.assertIn("Title", self.window.preview.toPlainText())
            target = Path(tmp) / "out.html"
            self.window._save(str(target))
            self.assertEqual(target.read_text(encoding="utf-8"), html)

    def test_reset_restores_defaults(self):
        self.window.options.set_value("bold_delimiter", "^")
        self.assertEqual(self.window.options.values()["bold_delimiter"], "^")
        self.window.options.reset_all()
        self.assertEqual(self.window.options.values()["bold_delimiter"], "#")

    def test_filter_hides_rows(self):
        panel = self.window.options
        self.assertTrue(panel.matches("unhyphenation"))
        panel.filter_box.setText("unhyphenation")
        self.assertTrue(panel.matches("unhyphenation"))
        self.assertFalse(panel.matches("tab_width"))
        panel.clear_filter()
        self.assertTrue(panel.matches("tab_width"))

    def test_filter_never_hides_everything_at_once(self):
        panel = self.window.options
        panel.filter_box.setText("zzzz-no-such-option")
        for name in panel.rows:
            self.assertFalse(panel.matches(name), name)
        panel.clear_filter()
        for name in panel.rows:
            self.assertTrue(panel.matches(name), name)


if __name__ == "__main__":
    unittest.main()
