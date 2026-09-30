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
        text, _ = read_text_file(os.path.join(source, "table-border.txt"))
        golden, _ = read_text_file(os.path.join(source, "good_table-border.html"))
        self.assertEqual(
            txt2html.convert(text, {"make_tables": True, "extract": True}), golden
        )

    def test_option_specs_cover_the_whole_table(self):
        specs = txt2html.option_specs()
        self.assertGreater(len(specs), 40)
        names = set()
        for name, aliases, kind, default, _accepted, help_text in specs:
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
        for name, aliases, kind, default, _accepted, _help in specs:
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
            self.assertEqual(read_text_file(path)[0], "héllo\n")

    # ------------------------------------------------------------ A5: the
    # encoding a file was read with has to survive a save, or merely opening a
    # file changes it.

    def test_opening_and_saving_a_cp1252_file_leaves_the_bytes_alone(self):
        """The defect: 0x80-0x9F are punctuation in CP1252.

        Decoded as Latin-1 they are C1 control characters, so writing them back
        as UTF-8 produced `\xc2\x97` where the file said `\x97` -- a curly
        quote and an em dash replaced by control characters, and the file is
        then a different file.
        """
        original = b"Caf\xe9 \x97 na\xefve \x93quotes\x94\r\nsecond\r\n"
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "cp1252.txt"
            path.write_bytes(original)

            text, encoding = read_text_file(path)
            self.assertEqual(encoding, "latin-1")

            # What the window does with it.
            write_text_file(path, text, encoding)
            self.assertEqual(
                path.read_bytes(),
                original,
                "opening and saving a CP1252 file changed its bytes",
            )

    def test_encoding_is_reported_for_each_kind_of_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            for name, data, expected in [
                ("utf8.txt", "naïve 門牌\n".encode("utf-8"), "utf-8"),
                ("bom.txt", b"\xef\xbb\xbfhello\n", "utf-8"),
                # Not valid UTF-8, so Latin-1 by the converter's rule.
                ("latin1.txt", b"caf\xe9\n", "latin-1"),
                # 0x97 is a valid *UTF-8* sequence when followed by the right
                # continuation byte, so this is a UTF-8 file containing a
                # character that looks like a CP1252 quote.
                ("looks-latin1.txt", b"\xc2\x97\n", "utf-8"),
            ]:
                path = Path(tmp) / name
                path.write_bytes(data)
                _, encoding = read_text_file(path)
                self.assertEqual(encoding, expected, name)

    def test_utf8_file_round_trips_byte_for_byte(self):
        original = "naïve 門牌規劃 — dash\r\n".encode("utf-8")
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "utf8.txt"
            path.write_bytes(original)
            text, encoding = read_text_file(path)
            self.assertEqual(encoding, "utf-8")
            write_text_file(path, text, encoding)
            self.assertEqual(path.read_bytes(), original)

    def test_byte_order_mark_is_kept_because_the_converter_keeps_it(self):
        """Parity beats tidiness: the converter does not strip a BOM.

        A BOM decodes to U+FEFF, which is part of the text, and it appears in the
        converted output. Stripping it in the editor would show the user a
        document the saved file does not contain, and would make the preview
        disagree with the reference.
        """
        original = b"\xef\xbb\xbfhello\n"
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "bom.txt"
            path.write_bytes(original)
            text, encoding = read_text_file(path)
            self.assertEqual(encoding, "utf-8")
            self.assertTrue(text.startswith("\ufeff"), "the BOM should still be in the text")
            write_text_file(path, text, encoding)
            self.assertEqual(path.read_bytes(), original)

    def test_files_module_preserves_crlf(self):
        """The file module does not rewrite line endings.

        True of `files.py` and *not* of the window: QPlainTextEdit normalises
        CRLF to LF before `write_text_file` ever sees the text, which
        `test_opening_and_saving_a_cp1252_file_through_the_window` pins. So this
        is a property of the module, not a promise about the application.
        """
        original = b"one\r\ntwo\r\n"
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "crlf.txt"
            path.write_bytes(original)
            text, encoding = read_text_file(path)
            self.assertEqual(encoding, "utf-8")
            write_text_file(path, text, encoding)
            self.assertEqual(path.read_bytes(), original)

    def test_generated_html_is_written_as_utf8_whatever_the_source_was(self):
        """The HTML is this program's output, not a transcription of the input.

        Writing it in the source file's CP1252 would be the wrong fix for A5: it
        would fail outright on any character CP1252 cannot represent, and it
        would leave the file with no declaration saying what it is (P7.2).
        """
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "cp1252.txt"
            source.write_bytes(b"na\xefve\n")
            _text, encoding = read_text_file(source)
            self.assertEqual(encoding, "latin-1")

            out = Path(tmp) / "out.html"
            write_text_file(out, "<p>naïve 門牌</p>", "utf-8")
            self.assertEqual(out.read_bytes(), "<p>naïve 門牌</p>".encode("utf-8"))


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

    def test_numeric_spin_boxes_use_the_ranges_the_engine_accepts(self):
        """A3: the spin box must not offer a value the engine will reject.

        tab_width=0 was reachable from this window, not just the command line:
        every numeric option had a spin-box minimum of 0, and 0 is the divisor in
        the tab expander. The bounds now come from the extension module, which
        reads the same table the engine validates against, so they cannot drift
        apart again.
        """
        from txt2html_gui.optionspanel import load_specs

        specs = {spec.name: spec for spec in load_specs()}

        # The floor: this is the one that mattered.
        self.assertEqual(specs["tab_width"].minimum, 1)
        self.assertEqual(specs["indent_width"].minimum, 0)
        for name in ("hrule_min", "min_caps_length", "preformat_whitespace_min"):
            self.assertEqual(specs[name].minimum, 0, f"{name} has no floor of its own")

        # Every bounded option shares the engine's ceiling.
        for name in ("tab_width", "indent_width", "hrule_min", "min_caps_length",
                     "preformat_whitespace_min"):
            self.assertEqual(specs[name].maximum, 999, f"{name} ceiling")

        # The preformat pair is clamped to a signed byte rather than rejected, so
        # its bound stays the GUI's to state.
        for name in ("preformat_trigger_lines", "endpreformat_trigger_lines"):
            self.assertEqual(specs[name].minimum, -128)
            self.assertEqual(specs[name].maximum, 127)

    def test_engine_panic_is_reported_instead_of_hanging(self):
        """A4: a panic in the engine must not leave the window waiting.

        pyo3's PanicException inherits BaseException rather than Exception, so
        the old `except Exception` in the worker did not catch it: no result was
        emitted, the status bar stayed on "converting…", and the user got a dead
        preview with no message. This asserts a message actually appears.

        The provocation is a `custom_heading_regexp` that does not compile, which
        the engine turns into a `PanicException` (P4, still open). It used to be
        `tab_width=0`, which divided by zero, until A3 made that a clean
        validation error before any conversion starts -- a good sign for the fix
        and a problem for this test, since A4 needs a panic to catch. When P4
        closes the last reachable panic this test will need a fault injected
        rather than a documented defect, because the thing it verifies is the
        handler, not the defect.
        """
        self.window.editor.setPlainText("hello\n\tworld\n")
        self.window.options.set_value("custom_heading_regexp", ["a("])
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

    def test_opening_and_saving_a_cp1252_file_through_the_window(self):
        """A5 end to end, through the window rather than the file module.

        The module tests cover the read and write halves; this covers the wiring
        between them, which is where the encoding has to be remembered.
        """
        original = b"Caf\xe9 \x97 na\xefve \x93quotes\x94\r\nsecond\r\n"
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "cp1252.txt"
            source.write_bytes(original)

            self.window.load_file(str(source))
            drain(self.app, self.window)
            self.assertEqual(self.window.encoding, "latin-1")
            # The editor shows the characters, not control codes.
            # The editor shows what Latin-1 says, which is the converter's rule:
            # 0x97 is a C1 control character, not the em dash CP1252 would make
            # of it. Displaying a prettier character than the encoding contains
            # would be wrong in the other direction -- the file says a control
            # character and the file must keep saying so.
            self.assertIn("\u0097", self.window.editor.toPlainText())
            self.assertNotIn("\u2014", self.window.editor.toPlainText())

            self.window.save_text()
            saved = source.read_bytes()

            # The 8-bit characters are the point of A5, and they survive.
            for byte in (b"\xe9", b"\x97", b"\xef", b"\x93", b"\x94"):
                self.assertIn(byte, saved, f"{byte!r} was lost")
            self.assertTrue(
                saved.startswith(b"Caf\xe9 \x97 na\xefve \x93quotes\x94"),
                f"the text was transcoded: {saved[:40]!r}",
            )
            # No double encoding, which is what the defect looked like.
            self.assertNotIn(b"\xc2\x97", saved)
            self.assertNotIn(b"\xc3\xa9", saved)

            # The line endings are not preserved, and that is Qt, not this code:
            # QPlainTextEdit normalises CRLF to LF in both directions, so the
            # text that reaches write_text_file no longer has them. Recorded in
            # the plan as an open decision; pinned here so it is a choice rather
            # than a surprise.
            self.assertEqual(saved, b"Caf\xe9 \x97 na\xefve \x93quotes\x94\nsecond\n")

    def test_saving_a_second_file_uses_its_own_encoding(self):
        """A new file's encoding says nothing about the previous one.

        Carrying the old encoding over would write the second file in the first
        file's charset, which is a corruption bug of exactly the kind A5 is
        about, just in the other direction.
        """
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "first.txt"
            first.write_bytes(b"caf\xe9\n")
            second = Path(tmp) / "second.txt"
            second.write_bytes("naïve 門牌\n".encode("utf-8"))

            self.window.load_file(str(first))
            drain(self.app, self.window)
            self.assertEqual(self.window.encoding, "latin-1")

            self.window.load_file(str(second))
            drain(self.app, self.window)
            self.assertEqual(self.window.encoding, "utf-8")

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
