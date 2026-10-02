# textrill-gui — a Qt front end for textrill.
#
# Copyright (C) 2026 the textrill authors.
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
import subprocess
import sys
import tempfile
import time
import unittest
from contextlib import contextmanager
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
# Keep the tests out of the user's real configuration file.  This is assigned
# rather than defaulted: a stale XDG_CONFIG_HOME exported by an earlier command
# would otherwise leak a previous run's settings into these tests.
os.environ["XDG_CONFIG_HOME"] = tempfile.mkdtemp(prefix="t2h-gui-test-")

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import textrill  # noqa: E402
from textrill_gui.app import build_application  # noqa: E402
from textrill_gui.files import decode_bytes, read_text_file, write_text_file  # noqa: E402
from textrill_gui.optionspanel import TABLE_TYPES  # noqa: E402
from textrill_gui.worker import Converter  # noqa: E402
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
        html = textrill.convert("hello *world*", {"extract": True})
        self.assertIn("<p>hello <em>world</em></p>", html)

    def test_xhtml_flag_changes_tags(self):
        html = textrill.convert("hi\n", {"xhtml": False})
        self.assertIn("<HTML>", html)
        html = textrill.convert("hi\n", {"xhtml": True})
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
        self.assertIn("<table", textrill.convert(text, base))
        self.assertNotIn(
            "<table",
            textrill.convert(text, dict(base, table_type={"BORDER": False})),
        )
        # the same table as a string spelling of the flags
        self.assertIn(
            "<table",
            textrill.convert(text, dict(base, table_type="BORDER=1 DELIM=1")),
        )

    def test_table_type_matches_the_reference(self):
        source = os.environ.get("T2H_TFILES")
        if not source:
            self.skipTest("T2H_TFILES is not set")
        text, _ = read_text_file(os.path.join(source, "table-border.txt"))
        golden, _ = read_text_file(os.path.join(source, "good_table-border.html"))
        self.assertEqual(
            textrill.convert(text, {"make_tables": True, "extract": True}), golden
        )

    def test_option_specs_cover_the_whole_table(self):
        specs = textrill.option_specs()
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
        specs = textrill.option_specs()
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
                    textrill.convert("x\n", options),
                    textrill.convert("x\n", {name: default}),
                    f"alias {alias} does not mean {name}",
                )


class FileTests(unittest.TestCase):
    def test_latin1_fallback(self):
        self.assertEqual(decode_bytes(b"caf\xe9"), "café")
        self.assertEqual(decode_bytes("café".encode()), "café")

    def test_round_trip(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "note.txt"
            write_text_file(path, "héllo\n")
            self.assertEqual(read_text_file(path)[0], "héllo\n")

    # ------------------------------------------------- A7: a save that cannot
    # happen should be an error, not a new filesystem layout.

    def test_a_missing_directory_is_an_error_and_creates_nothing(self):
        """One typo used to invent a whole tree.

        `write_text_file` called `mkdir(parents=True)`, so saving to
        `newtree/a/b/c/out.html` created five directories that the user never
        asked for and has to clean up by hand.
        """
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "newtree" / "a" / "b" / "out.html"
            with self.assertRaises(OSError):
                write_text_file(target, "<p>x</p>")
            self.assertFalse(
                (Path(tmp) / "newtree").exists(),
                "the directories were created anyway",
            )
            self.assertEqual(list(Path(tmp).iterdir()), [], "nothing should be left behind")

    def test_an_existing_directory_is_used(self):
        """Dropping the mkdir must not stop a save into a directory that exists."""
        with tempfile.TemporaryDirectory() as tmp:
            existing = Path(tmp) / "out"
            existing.mkdir()
            target = existing / "out.html"
            write_text_file(target, "<p>x</p>")
            self.assertEqual(target.read_text(encoding="utf-8"), "<p>x</p>")

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
            self.assertEqual(encoding, "cp1252")

            # What the window does with it.
            write_text_file(path, text, encoding)
            self.assertEqual(
                path.read_bytes(),
                original,
                "opening and saving a CP1252 file changed its bytes",
            )

    def test_a_cp1252_smart_quote_is_shown_as_punctuation_not_a_control(self):
        """P7.1: the editor has to agree with the converter.

        Both used to decode this file the same way and still disagreed about
        what it contained. The converter's demoronize table is keyed on the
        CP1252 code points, so a Latin-1 decode put U+0093 in the document where
        the table looked for U+201C, every substitution silently did nothing,
        and the C1 control character reached the output as UTF-8 `c2 93` -- an
        invisible character in the file the user was editing and a different
        invisible character in the preview.
        """
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "smart.txt"
            path.write_bytes(b"He said \x93hi\x94 -- \x97dash\x97.\n")
            text, encoding = read_text_file(path)
            self.assertEqual(encoding, "cp1252")
            self.assertIn("\u201chi\u201d", text)
            self.assertIn("\u2014", text)
            for control in ("\x93", "\x94", "\x97"):
                self.assertNotIn(
                    control, text, f"C1 control {control!r} reached the editor"
                )
            # And the bytes still survive a save.
            write_text_file(path, text, encoding)
            self.assertEqual(path.read_bytes(), b"He said \x93hi\x94 -- \x97dash\x97.\n")

    def test_cp1252_undefined_bytes_are_readable_and_survive_a_save(self):
        """The five bytes CP1252 leaves undefined must not break the editor.

        Python's `cp1252` codec raises on them, which is why `files.py` carries
        its own table. If this ever stops holding, opening an obscure file
        becomes an exception instead of a document.
        """
        with tempfile.TemporaryDirectory() as tmp:
            for b in (0x81, 0x8D, 0x8F, 0x90, 0x9D):
                raw = bytes([ord("a"), b, ord("b")])
                path = Path(tmp) / f"u{b:02x}.txt"
                path.write_bytes(raw)
                text, encoding = read_text_file(path)
                self.assertEqual(encoding, "cp1252", f"0x{b:02X}")
                self.assertEqual(len(text), 3, f"0x{b:02X}")
                write_text_file(path, text, encoding)
                self.assertEqual(path.read_bytes(), raw, f"0x{b:02X}")

    def test_a_bom_is_a_declaration_and_utf16_is_decoded_as_such(self):
        """`FF FE` means UTF-16LE. Treating it as a decode error is P7.4's bug.

        The pre-P7.4 editor decoded it as CP1252, where those bytes have no
        meaning, so the document opened as `\u00ff\u00feHello` -- mojibake in the
        editor for a file the converter would have read correctly.
        """
        with tempfile.TemporaryDirectory() as tmp:
            for name, data, expected in [
                ("le.txt", b"\xff\xfe" + "Hello world.\n".encode("utf-16-le"), "utf-16le"),
                ("be.txt", b"\xfe\xff" + "Hello world.\n".encode("utf-16-be"), "utf-16be"),
                ("u32.txt", b"\xff\xfe\x00\x00" + "Hello.\n".encode("utf-32-le"), "utf-32le"),
            ]:
                path = Path(tmp) / name
                path.write_bytes(data)
                text, encoding = read_text_file(path)
                self.assertEqual(encoding, expected, name)
                self.assertEqual(text, "Hello world.\n" if "u32" not in name else "Hello.\n")
                self.assertNotIn("\ufeff", text, f"{name}: the mark is metadata")

    def test_bomless_utf16_of_ascii_is_not_shown_with_a_nul_between_letters(self):
        """The failure that made UTF-16 worth handling: `H\0e\0l\0l\0o\0`.

        This file is *valid UTF-8* -- every byte is below 0x80 -- so the editor
        accepted it and displayed a document with control characters threaded
        through it, and the converter produced the same thing. Only the position
        of the NULs distinguishes it, which is why detection looks there.
        """
        raw = "Hello world.\nThis is plain ASCII prose.\n".encode("utf-16-le")
        self.assertEqual(raw.decode("utf-8"), raw.decode("utf-8"))  # trivially valid
        with tempfile.TemporaryDirectory() as tmp:
            for enc, expected in (("utf-16-le", "utf-16le"), ("utf-16-be", "utf-16be")):
                path = Path(tmp) / f"{expected}.txt"
                path.write_bytes("Hello world.\nThis is prose.\n".encode(enc))
                text, encoding = read_text_file(path)
                self.assertEqual(encoding, expected)
                self.assertEqual(text, "Hello world.\nThis is prose.\n")
                self.assertNotIn("\x00", text)

    def test_a_utf32le_file_is_not_read_as_utf16le(self):
        """UTF-32LE's mark starts with UTF-16LE's, so order decides.

        Checked the short way round, the file decodes as pairs of Latin-1
        characters: still text-shaped, so wrong in a way that survives a glance.
        """
        raw = b"\xff\xfe\x00\x00" + "abcd\U0001F600ef".encode("utf-32-le")
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "u32.txt"
            path.write_bytes(raw)
            text, encoding = read_text_file(path)
            self.assertEqual(encoding, "utf-32le")
            self.assertEqual(text, "abcd\U0001F600ef")

    def test_naming_the_encoding_recovers_what_detection_cannot(self):
        """Russian in CP1251 or KOI8-R is not detectable, so it must be askable.

        CP1251 and KOI8-R disagree about nearly every high byte and both are
        valid CP1252, so `auto` picks CP1252 and is wrong for both. There is no
        evidence in the bytes to do better; the user knowing is the only source.
        """
        text = "Привет, мир!\n"
        with tempfile.TemporaryDirectory() as tmp:
            for codec, name in (("cp1251", "cp1251"), ("koi8-r", "koi8-r")):
                path = Path(tmp) / f"{name}.txt"
                path.write_bytes(text.encode(codec))
                _auto, auto_encoding = read_text_file(path)
                self.assertEqual(auto_encoding, "cp1252", name)
                self.assertNotEqual(decode_bytes(path.read_bytes(), name), "Привет, мир!")
                got, encoding = read_text_file(path, encoding=name)
                self.assertEqual(got, text, name)
                self.assertEqual(encoding, name)

    def test_latin1_is_no_longer_an_alias_for_cp1252(self):
        """`latin-1` used to mean CP1252 here.

        Not a harmless spelling: a Latin-1 file's `0x93` is a C1 control, and a
        user asking for Latin-1 was asking for the C1 control, not for a quote.
        """
        raw = b"\x93quoted\x94\n"
        self.assertEqual(decode_bytes(raw, "latin-1"), "\u0093quoted\u0094\n")
        self.assertEqual(decode_bytes(raw, "cp1252"), "\u201cquoted\u201d\n")

    def test_the_editor_tables_match_the_codecs_the_engine_generated_from(self):
        """`files.py` is a second implementation of an engine rule, and it drifts.

        The engine's tables in `src/convert.rs` were generated from Python's own
        codecs by `tests/gen_encoding_tables.py`, and the Rust test
        `every_table_entry_matches_python` checks them against the same
        reference. So both sides agree with Python by construction, and this
        asserts the GUI half of that: all 128 high bytes of all five encodings.

        Only a real disagreement can come from the one thing the shared
        reference does not express -- a byte the encoding leaves undefined, which
        each side must fall back to the Latin-1 reading for.
        """
        for encoding in ("iso-8859-1", "cp1252", "cp1251", "cp1253", "koi8-r"):
            for byte in range(0x80, 0x100):
                raw = bytes([byte])
                try:
                    want = raw.decode(encoding)
                except UnicodeDecodeError:
                    want = chr(byte)  # undefined: Latin-1 C1, as the engine does
                got = decode_bytes(raw, encoding)
                self.assertEqual(
                    got, want, f"{encoding} 0x{byte:02x}: {got!r} != {want!r}"
                )

    def test_the_engine_and_the_editor_read_a_utf16_file_the_same_way(self):
        """End-to-end parity, without parsing the engine's HTML.

        Comparing decoded text against the converter's output would require
        unwrapping markup to get back to something comparable, which is a second
        source of disagreement. Instead the engine is asked to convert the file
        and the *decoded words* are looked for in its output, while the editor's
        decode is asserted against the same expected string. Both sides are then
        pinned to one expectation, which is the property that matters: a user
        editing one document and previewing another.

        Skipped rather than failed when the engine is not built, so the GUI
        suite stays runnable on its own.
        """
        engine = (
            Path(__file__).resolve().parents[2]
            / "textrill" / "target" / "debug" / "textrill"
        )
        if not engine.exists():
            self.skipTest("engine binary not built; run `cargo build` in textrill/")

        expected = "Hello world.\nThis is plain prose.\n"
        with tempfile.TemporaryDirectory() as tmp:
            for name, data, encoding in [
                ("le", "Hello world.\nThis is plain prose.\n".encode("utf-16-le"), "utf-16le"),
                ("bele", b"\xff\xfe" + "Hello world.\nThis is plain prose.\n".encode("utf-16-le"), "utf-16le"),
                ("be", "Hello world.\nThis is plain prose.\n".encode("utf-16-be"), "utf-16be"),
                ("u32", b"\xff\xfe\x00\x00" + expected.encode("utf-32-le"), "utf-32le"),
            ]:
                path = Path(tmp) / f"{name}.txt"
                path.write_bytes(data)

                text, got_encoding = read_text_file(path)
                self.assertEqual(got_encoding, encoding, name)
                self.assertEqual(text, expected, f"editor, {name}")

                out = subprocess.run(
                    [str(engine), "--default_link_dict", "", str(path)],
                    capture_output=True,
                )
                html = out.stdout.decode("utf-8")
                self.assertIn("Hello world.", html, f"engine, {name}")
                self.assertNotIn("\x00", html, f"engine left NULs, {name}")

    def test_saving_a_character_cp1252_cannot_hold_is_an_error(self):
        """The user typed something the file's encoding has no byte for.

        Writing U+FFFD and losing their text is the worse outcome, and saying so
        lets them save as UTF-8 instead.
        """
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "cjk.txt"
            path.write_bytes(b"na\xefve\n")
            _text, encoding = read_text_file(path)
            with self.assertRaises(UnicodeEncodeError):
                write_text_file(path, "na\u9580ve", encoding)

    def test_encoding_is_reported_for_each_kind_of_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            for name, data, expected in [
                ("utf8.txt", "naïve 門牌\n".encode("utf-8"), "utf-8"),
                ("bom.txt", b"\xef\xbb\xbfhello\n", "utf-8"),
                # Not valid UTF-8, so the single-byte fallback (P7.1).
                ("cp1252.txt", b"caf\xe9\n", "cp1252"),
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
            self.assertEqual(encoding, "cp1252")

            out = Path(tmp) / "out.html"
            write_text_file(out, "<p>naïve 門牌</p>", "utf-8")
            self.assertEqual(out.read_bytes(), "<p>naïve 門牌</p>".encode("utf-8"))


class BacklogTests(unittest.TestCase):
    """A6: a new conversion should make the queued ones pointless.

    The window already discards a stale *result*; without cancelling, it went on
    doing the stale *work*. Each queued job holds its own copy of the document,
    so a queue that grows while the user types is a queue that grows in memory
    too.
    """

    #: Big enough that one conversion outlasts the 300 ms debounce, so a burst of
    #: edits really does pile up. Measured at about 0.36 s.
    SLOW = ("word " * (500 * 1024 // 5)) + "\n"

    @classmethod
    def setUpClass(cls):
        # A QThreadPool needs an application object for cross-thread signals.
        #
        # The window is kept on the class, not bound to a local as it was. A
        # `_window` local drops the last Python reference the moment setUpClass
        # returns, and the MainWindow is collected while `cls.app` still lives.
        # The C++ object then goes with it, and anything Python still holding --
        # here the converter's QThreadPool, which outlives the class -- reports
        # "Internal C++ object (QPlainTextEdit) already deleted" at interpreter
        # shutdown. It did not fail a test, which is exactly why it survived: a
        # shutdown-ordering defect that only ever printed to stderr.
        cls.app, cls.window, _ = build_application([])

    def _settle(self, conv, ran, timeout=120.0, quiet=0.5):
        """Wait for the pool to go idle, delivering results as the app really does.

        `QThreadPool.waitForDone` blocks the main thread, and the results are
        delivered by queued signals on that same thread -- so a test that waits
        with it sees nothing and proves nothing. This spins the event loop
        instead, and stops only after the pool has been idle for `quiet` seconds,
        which also covers the window between `start` and the thread spinning up.
        """
        deadline = time.perf_counter() + timeout
        last_change = time.perf_counter()
        seen = 0
        while time.perf_counter() < deadline:
            self.app.processEvents()
            if len(ran) != seen:
                seen = len(ran)
                last_change = time.perf_counter()
            elif conv.pool.activeThreadCount() == 0 and time.perf_counter() - last_change > quiet:
                return
            time.sleep(0.005)
        self.fail(f"the pool never went idle within {timeout}s")

    def test_a_burst_of_conversions_does_not_queue_them_all(self):
        conv = Converter(max_threads=1)
        ran = []
        conv.sink.finished.connect(lambda gen, html, secs, err: ran.append(gen))

        queued = 8
        for _ in range(queued):
            conv.convert(self.SLOW, {})
        self._settle(conv, ran)

        # One job may already be on the thread and one may be waiting behind it;
        # the rest were dropped when the next one was queued. Without the fix all
        # eight run, and the suite gets several seconds slower as well as failing.
        self.assertLessEqual(
            len(ran), 2,
            f"{len(ran)} of {queued} queued conversions ran; the queue is not being dropped",
        )

    def test_the_newest_generation_is_the_one_that_survives(self):
        conv = Converter(max_threads=1)
        latest = [0]
        conv.sink.finished.connect(
            lambda gen, html, secs, err: latest.__setitem__(0, max(latest[0], gen))
        )
        for _ in range(6):
            latest[0] = conv.convert(self.SLOW, {})
        self._settle(conv, latest)
        self.assertEqual(
            latest[0], 6,
            "the last generation queued is the one whose result matters",
        )

    def test_the_pool_returns_to_idle(self):
        conv = Converter(max_threads=2)
        ran = []
        conv.sink.finished.connect(lambda gen, html, secs, err: ran.append(gen))
        for _ in range(4):
            conv.convert(self.SLOW, {})
        self._settle(conv, ran)
        self.assertEqual(conv.pool.activeThreadCount(), 0)


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
        from textrill_gui.optionspanel import load_specs

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

        The fault is injected, and that is a deliberate change. This test used to
        provoke a real engine panic with a `custom_heading_regexp` that does not
        compile, then with `tab_width=0` before A3 made that a clean validation
        error. P22 has now closed the regexp route too, so there is no documented
        defect left to provoke -- and the thing under test is the *handler*, not
        the defect. Reaching for a new crashing input each time a fix lands meant
        this test kept verifying whatever happened to be broken that week, and
        would have gone away silently with the last one.
        """
        real_convert = textrill.convert

        def panicking_convert(text, options):
            raise textrill.PanicException("injected for the A4 handler test")

        self.window.editor.setPlainText("hello\n\tworld\n")
        textrill.convert = panicking_convert
        try:
            html = drain(self.app, self.window)
        finally:
            textrill.convert = real_convert
            self.window.options.reset_all()
        self.assertNotEqual(
            html.strip(), "", "the preview went blank with nothing to show for it"
        )
        self.assertIn("stopped on invalid input", html)
        self.assertEqual(self.window.status_label.text(), "error")
        # and the window is usable again rather than wedged
        self.assertGreaterEqual(self.window._completed, self.window._latest)

    def test_an_invalid_pattern_is_a_clean_message_not_a_panic(self):
        """P22: a pattern that does not compile is a message, not a crash.

        This is the same input that used to drive the panic above. The engine now
        validates the pattern before converting, so the user gets a diagnostic
        naming the option and the pattern instead of "this is a bug in txt2html"
        and a dead preview. The wording matters: the old message told the user
        their input was probably at fault, when the input was fine and the port
        was not.
        """
        self.window.editor.setPlainText("hello\n\tworld\n")
        self.window.options.set_value("custom_heading_regexp", ["a("])
        try:
            html = drain(self.app, self.window)
        finally:
            self.window.options.reset_all()
        self.assertEqual(self.window.status_label.text(), "error")
        self.assertIn("custom_heading_regexp", html)
        self.assertIn("invalid regular expression", html)
        # The pattern is quoted back, so the user can see which one was rejected
        # when they supplied several.
        self.assertIn("a(", html)
        self.assertNotIn(
            "stopped on invalid input",
            html,
            "reported as an engine panic; P22 should make this a validation error",
        )
        self.assertGreaterEqual(self.window._completed, self.window._latest)

    def test_panic_exception_is_importable_and_is_a_base_exception(self):
        """The class has to be nameable, or catching it precisely is impossible.

        pyo3 puts PanicException in `pyo3_runtime`, which is not importable, so
        without the re-export in python.rs no caller could write
        `except textrill.PanicException`.
        """
        self.assertTrue(hasattr(textrill, "PanicException"))
        self.assertFalse(
            issubclass(textrill.PanicException, Exception),
            "if this ever becomes an Exception the test stops proving anything",
        )
        self.assertTrue(issubclass(textrill.PanicException, BaseException))

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
        from textrill_gui.mainwindow import SAMPLE_TEXT

        html = textrill.convert(SAMPLE_TEXT, {"extract": True, "make_tables": True})
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
            self.assertEqual(self.window.encoding, "cp1252")
            # The editor shows punctuation, not control codes (P7.1).
            #
            # This asserted the opposite, and the comment above it was a
            # careful defence of the wrong answer: it said 0x97 is a C1 control
            # "and the file must keep saying so". True of Latin-1, and
            # irrelevant, because the converter decodes CP1252 -- so the editor
            # was showing one document while the preview showed another. The
            # bytes still round-trip either way; the disagreement was in what
            # the user was looking at.
            shown = self.window.editor.toPlainText()
            self.assertIn("\u2014", shown)
            self.assertIn("\u201cquotes\u201d", shown)
            self.assertNotIn("\u0097", shown)

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
            self.assertEqual(self.window.encoding, "cp1252")

            self.window.load_file(str(second))
            drain(self.app, self.window)
            self.assertEqual(self.window.encoding, "utf-8")

    def test_saving_to_a_mistyped_path_reports_it_and_creates_nothing(self):
        """A7 through the window, where a mistyped path actually happens.

        The save dialog is the one place a path is typed by hand, so it is the
        one place that must not paper over a missing directory.
        """
        with tempfile.TemporaryDirectory() as tmp:
            self.window.editor.setPlainText("a chapter\n")
            drain(self.app, self.window)

            before = sorted(p.name for p in Path(tmp).iterdir())
            proposed = str(Path(tmp) / "newtree" / "a" / "b" / "out.html")

            with self._no_dialogs() as shown:
                # Give the window a file to save, at the mistyped path.
                self.window.path = proposed
                self.window.save_text()

            self.assertIn("warning", shown, "the user was not told the save failed")
            self.assertFalse(
                (Path(tmp) / "newtree").exists(),
                "the window invented the directory tree",
            )
            self.assertEqual(sorted(p.name for p in Path(tmp).iterdir()), before)

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

            # In the directory that exists: this test is about the second save
            # reusing the name it was given, not about creating directories,
            # which A7 deliberately stopped doing. An earlier version used
            # `tmp/out/book.html` and relied on write_text_file making `out`.
            target = Path(tmp) / "book.html"
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

    def test_the_gui_declares_the_charset_it_writes(self):
        """The plan said the GUI turns `meta_charset` on. It did not.

        The engine defaults it off so no golden moves, which is right for a
        byte-compatible CLI and wrong for a panel whose consumer is a browser
        reading a file this program wrote. Without a declaration the browser
        guesses, and a document containing e.g. Cyrillic can render as mojibake
        for want of two bytes of markup.
        """
        self.assertTrue(self.window.options.values()["meta_charset"])
        # And it survives a reset and a settings round trip, since both go
        # through values().
        self.window.options.reset_all()
        self.assertTrue(self.window.options.values()["meta_charset"])

    def test_generated_html_from_the_panel_declares_utf8(self):
        """End to end, because forcing the option is only worth anything if the
        conversion honours it."""
        html = textrill.convert("Привет, мир!\n", self.window.options.values())
        self.assertIn("charset", html)
        self.assertIn("utf-8", html.lower())

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
