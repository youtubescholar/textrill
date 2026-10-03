# textrill-gui

A small Qt front end for [textrill](../textrill), the Rust reimplementation
of the Perl module `HTML::TextToHTML` 3.0.

> **This front end is scheduled to be rewritten, 2026-10-01.** The project
> settled that the deliverable is a *single Flatpak artifact*, and this
> package needs a Python runtime plus PySide6 at run time, so it does not meet
> that. The rewrite moves the shell to Rust + Qt. The converter in
> `../textrill` is **not** being rewritten — it is the part that is finished
> and byte-verified against Perl, and a Rust GUI calls into it directly instead
> of across the pyo3 boundary this package uses.
>
> The plan is **Phase 6** in [`../REMEDIATION-PLAN.md`](../REMEDIATION-PLAN.md),
> and `tests/test_gui.py` is treated as its acceptance criteria rather than as
> code to translate. The 30 shell tests in it are the specification of what the
> Rust GUI has to do. Nothing here is a dead end: the tests, the encoding rules
> and the concurrency contract all carry over. Settled since then: the CLI stays, this package is kept runnable
> alongside the new one until it passes the ported suite, and the artifact ships as
> a Flatpak. Still open before the port starts is Qt6 vs GTK4, which Flatpak has
> reduced to a choice of runtime.

Type or open text on the left, watch the HTML appear on the right, and adjust
the conversion options in the panel on the far right.  Every option, its
default, its help text and its aliases come from the converter itself, so the
form cannot drift away from the library.

## Installing

The GUI needs the `textrill` extension module, which is built from this
repository with [maturin](https://www.maturin.rs/):

```sh
cd ../textrill
python -m venv .venv
. .venv/bin/activate
pip install maturin
maturin develop --release
```

Then install the GUI next to it:

```sh
cd ../textrill-gui
pip install -e .
```

## Running

```sh
textrill-gui                    # empty editor
textrill-gui notes.txt          # open a file
textrill-gui --no-xhtml --tables sample.txt
python -m textrill_gui          # without installing the entry point
```

Files are read as UTF-8, falling back to Latin-1, which is what the converter
does, so the editor shows exactly the characters that will be converted.

## Using the window

| | |
|---|---|
| **Editor** | the text to convert; tab and drag-and-drop work |
| **Preview** | the rendered result, with working links |
| **HTML** | the generated source, for copying or saving |
| **Options** | every conversion option, grouped and filterable |

Conversions run on a background thread, so typing in a large document stays
responsive.  The `auto` switch in the toolbar turns live conversion on and
off; with it off, use **Convert now** (<kbd>Ctrl</kbd>+<kbd>Return</kbd>).
The status bar reports how long the last conversion took.

## Saving

The text you are converting and the HTML you produce are two different
documents, so they are saved by two different commands:

| Command | Writes | Shortcut |
|---|---|---|
| **Save** / **Save As…** | the generated HTML, always as `.html` | <kbd>Ctrl</kbd>+<kbd>S</kbd> |
| **Save text…** | the text in the editor, back to the file it came from | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> |

**Save As…** suggests a sibling `.html` next to the source, and refuses to write
over the text file that is being converted.  The window title tracks both
kinds of pending work: `*` means the text has unsaved edits, `+` means the
HTML on screen has not been written to disk yet.  Saving the HTML does not
clear `*`, because your edits to the text are still unsaved.

The `auto` switch, the option values and the window geometry are remembered
between sessions.

## Layout of the code

| File | What it does |
|---|---|
| `textrill_gui/app.py` | argument parsing and startup |
| `textrill_gui/mainwindow.py` | the window, its actions and its shortcuts |
| `textrill_gui/optionspanel.py` | the options form, built from `option_specs()` |
| `textrill_gui/worker.py` | conversions on a `QThreadPool` |
| `textrill_gui/files.py` | reading and writing text files |
| `tests/test_gui.py` | head-less tests |

## Tests

The tests need no display; they select the offscreen Qt platform themselves.
They also point `XDG_CONFIG_HOME` at a fresh temporary directory, so they never
read or write your real settings.

```sh
QT_QPA_PLATFORM=offscreen python -m unittest discover -s tests
```

One test compares the converter's output against the upstream
`good_table-border.html` and skips itself unless the reference files are
available. With them set it runs, and it passes:

```sh
T2H_TFILES=/path/to/txt2html-3.0/tfiles \
  QT_QPA_PLATFORM=offscreen python -m unittest discover -s tests
```

The reference is one `make ref` away from the top of the repository, and CI
runs the suite this way on every push — a skipped test is otherwise
indistinguishable from a passing one.

## Licence

Released under the GNU General Public License, version 3 or later; see
`LICENSE`.
