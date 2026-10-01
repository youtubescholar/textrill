# txt2html GUI

A small Qt front end for [txt2html](../txt2html-rs), the Rust port of the Perl
module `HTML::TextToHTML` 3.0.

Type or open text on the left, watch the HTML appear on the right, and adjust
the conversion options in the panel on the far right.  Every option, its
default, its help text and its aliases come from the converter itself, so the
form cannot drift away from the library.

## Installing

The GUI needs the `txt2html` extension module, which is built from this
repository with [maturin](https://www.maturin.rs/):

```sh
cd ../txt2html-rs
python -m venv .venv
. .venv/bin/activate
pip install maturin
maturin develop --release
```

Then install the GUI next to it:

```sh
cd ../txt2html-gui
pip install -e .
```

## Running

```sh
txt2html-gui                    # empty editor
txt2html-gui notes.txt          # open a file
txt2html-gui --no-xhtml --tables sample.txt
python -m txt2html_gui          # without installing the entry point
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
| `txt2html_gui/app.py` | argument parsing and startup |
| `txt2html_gui/mainwindow.py` | the window, its actions and its shortcuts |
| `txt2html_gui/optionspanel.py` | the options form, built from `option_specs()` |
| `txt2html_gui/worker.py` | conversions on a `QThreadPool` |
| `txt2html_gui/files.py` | reading and writing text files |
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
