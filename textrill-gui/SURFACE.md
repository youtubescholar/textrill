# The GUI/engine surface, frozen

This is §6.3 step 1 of `REMEDIATION-PLAN.md`: the boundary the rewritten GUI is
allowed to use, written down **before** any of it is ported, so the port is a
transcription rather than a re-derivation from memory.

The rule this file encodes: **the GUI may not invent engine behaviour.** Every
capability listed here already exists in the Rust engine and is byte-verified
against the Perl reference by the differential corpus. Nothing here is a new
feature, and adding to this list means going through `REMEDIATION-PLAN.md`, not
through the port.

Figures below were read off the build, not estimated: `option_specs()` returns
**54 options** — 20 `bool`, 18 `str`, 11 `int`, 4 `str_array`, 1 `table_type`.

## 1. The five calls the GUI makes today

`textrill/python/textrill/__init__.py` wraps six `#[pyfunction]`s. The rewritten
GUI calls the Rust items directly; this table is the mapping.

| Python (today) | Rust (after) | Notes |
|---|---|---|
| `textrill.convert(text, options)` | `convert::Converter::new(opts).convert_text(text)` | The live-preview path |
| `textrill.convert_file(path, options)` | `convert::read_with(path, opts.encoding)` then the above | Records the resolved encoding for save |
| `textrill.process_chunk(text, options)` | `Converter::process_chunk(text, true, true)` | Single-paragraph path |
| `textrill.file_encoding()` | process-global `LAST_FILE_ENCODING` | **See §5 — this is the one wart** |
| `textrill.option_specs()` | `cli::SPECS` + `options::numeric_range` | Builds the options panel |
| `textrill.version()` | `env!("CARGO_PKG_VERSION")` | About box |

`options_from_dict` has no direct successor, because a Rust GUI builds a typed
`Options` rather than a `HashMap<String, Value>`. Its one piece of real logic —
a list value *replaces* the default rather than appending to it — is carried into
`OptionsPanel::options()` as an explicit `.clear()` before assignment, and the
four affected fields (`custom_heading_regexp`, `infile`, `instring`,
`links_dictionaries`) are listed in the source today so the port does not have to
rediscover them.

`validate()` is called after building options, not at conversion time. The pyo3
layer does it in `options_from_dict` "because that is the one place all three
entry points build their options"; in Rust each front end builds its own
`Options`, so **each front end must call `validate()` itself**. The CLI does it in
`deal_with_options`; the GUI must too, or a bad numeric range reaches the engine
as a panic instead of a reportable error.

## 2. The options panel

`optionspanel.py` (503 lines) exists to turn `option_specs()` output into widgets.
In Rust it is deleted rather than ported, because `cli::SPECS` is readable
directly and cannot drift from the engine.

Per option the panel gets: canonical name, aliases (`spec.names[1..]`), kind,
default (`cli::get_value`), numeric range (`options::numeric_range`, `(low,
high)` or unbounded), help string.

The guarantee worth stating, because it is the reason for deleting 491 lines: **a
Rust options panel cannot offer a value the engine rejects.** The range comes
from the same table the engine validates against. The Python version had to be
*trusted* to preserve that; the Rust version cannot break it.

Kinds map to widgets:

| kind | widget | count |
|---|---|---|
| `bool` | checkbox, plus a `no_` negation path | 20 |
| `int` | spin box, range from `numeric_range` | 11 |
| `str` | line edit | 18 |
| `str_array` | multi-line text, one entry per line | 4 |
| `table_type` | four checkboxes, `align`/`pgsql`/`border`/`delim` | 1 |

## 3. The concurrency contract — carried verbatim in intent

`worker.py:75` solves a real problem and is **not** to be rediscovered. The
design, as written there:

1. **Generation counter.** Each request increments a counter and tags the job with
   it. A result arriving with a stale generation is dropped, which is what stops
   the preview flickering with older output.
2. **Queue-drop, not just result-drop.** Waiting jobs are cleared at every
   keystroke. A keystroke burst queues faster than the pool drains, and each
   queued job holds its own copy of the document — **a backlog is a memory
   backlog.** A 300 ms debounce alone does not bound memory.
3. **Bounded in-flight work.** A started job cannot be recalled, so at most
   `max_threads` (2) conversions run at once and the remainder is dropped rather
   than accumulated.
4. **Result emission is in a `finally`.** No failure path — including a panic,
   which in Python is a `BaseException` — may leave the window showing
   "converting…" forever. The Rust equivalent is emitting the completion signal on
   every path, including a poisoned worker thread.
5. **The sink outlives the window.** The completion sink is deliberately not
   parented to it, so a worker finishing during shutdown can still deliver.

A panic is explicitly caught and reported as *"The converter stopped on invalid
input"* rather than crashing the front end. In Rust this is `catch_unwind`; it is
worth keeping, because the engine's panic paths are exactly the inherited-hang
class of bug (P5).

## 4. Files, and the encoding rule that must not be duplicated

`files.py` (297 lines) exists partly to re-implement `convert.rs`'s encoding
policy in Python. **The rewrite deletes that second implementation** — it is
the duplicate-encoding-rule item in §6.2, and the reason P7 was a prerequisite.

The one behaviour that must survive is the save path: `file_encoding()` reports
what `read_with` resolved, so **Save text…** writes a CP1252 file back as CP1252
instead of silently transcoding it to UTF-8.

## 5. Open items, recorded rather than resolved here

- **`file_encoding` is a process-global** (`python.rs`, `LAST_FILE_ENCODING`),
  with a comment explaining why a `thread_local` would be wrong under a GUI. It
  is correct for a single-document front end and wrong for a two-document one.
  The rewrite should return the encoding from the read instead of stashing it
  globally — the comment's reasoning about the UI thread vs worker thread is
  about *where it is read*, not about global state being good. Flagged for the
  port, not fixed here.
- **`process_chunk` has no caller in the GUI**, confirmed by grep across
  `textrill_gui/` and `tests/`. Do not port it unless something starts needing
  it.
- **The encoder now exists in the engine** (`src/encode.rs`), written while
  porting `FileTests`. `files.py`'s Python encoder is therefore deletable
  without losing the round-trip guarantee, and §6.2's "duplicated rule" count
  should be read as including it. Two defects were found and fixed in the
  process — see the plan's §6.3 step 2.
- **Toolkit is unresolved.** Only Qt **5.15** is installed on this machine; Qt6
  and GTK4 are both absent, and no Qt/GTK crates are vendored. Qt5 availability
  makes an incremental build possible now, but the deliverable is a Flatpak and
  Flathub ships the **Qt 6** runtime (`org.kde.Platform`/`org.freedesktop.Platform`
  both track Qt6), so building on Qt5 now risks a port. This is the Phase 6
  toolkit decision and it is still open.

## 6. Test disposition (`test_gui.py`, 1123 lines)

| class | count | fate |
|---|---|---|
| `ConverterTests` | 6 | **delete** — pure engine, already covered by cargo |
| `FileTests` | 20 | **done** — ten save-path tests now live in `tests/encodingtest.rs`; the rest were already covered by the decode-side tests there |
| `BacklogTests` | 3 | **port** — acceptance criteria |
| `GuiTests` | 29 | **port** — acceptance criteria |

32 of the 58 are the port's acceptance bar. Counts are read off `test_gui.py` by
the actual `def test_` count, and the total agrees with the 58 the GUI suite runs;
the plan's older "10 / 27 / 46" figures were stale. Both GUIs stay runnable until they
pass, which is what makes the comparison meaningful instead of a rewrite from
memory.