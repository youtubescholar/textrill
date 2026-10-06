# legacy-archive

Retired code and retired documents, kept for reference. **Nothing here is built,
tested, linted, or packaged.** It is not part of `make verify`, `make test`, or
CI. `DOCS.md` at the repository root is the register of what is current.

## What is here, and why

| path | what it was |
|---|---|
| `textrill-gui/` | The Python + PySide6 front end (`textrill_gui`, 1,365 lines) and its 58-test suite. It was the GUI until the native `egui` rewrite in `textrill-gui-rs/` passed the ported acceptance suite. |
| `textrill-gui/SURFACE.md` | The frozen GUI/engine surface used to drive the port. Still the written record of the behavioural contract. |
| `python-bindings-python.rs` | The pyo3 bindings (`textrill/src/python.rs`), exposed as `textrill._native`. They existed only to cross into Python; the native GUI links the engine directly, so nothing needs them. |
| `python-bindings-pyproject.toml` | The maturin build config for the extension (`textrill/pyproject.toml`). |
| `python/` | The thin Python wrapper package (`textrill/python/textrill/__init__.py`). |
| `TOOL-SURVEY.md` | The 2026-09-29 feature-gap survey. Archived 2026-10-06 because its matrix had gone stale — it still called the port `txt2html-rs` and reported TOC, HTML5 output, rc files, `charset` and a built-in stylesheet as missing, all of which now exist. A *document*, retired for the same reason as the code above: it no longer describes what exists. |

Two different things are archived here, and the distinction matters. The Python
GUI is retired **code**: it is gone from every build and test path.
`TOOL-SURVEY.md` is a retired **document**: the code it describes is still
running, but the document's claims about that code are no longer true. Neither
is ever read as current.

## Why it was retired

The native GUI reproduces the Python front end's behaviour byte-for-byte where
it matters. The side-by-side differential run (2026-10-03) showed:

- the Python suite passed 58 tests (1 skipped) and the native suite 60/60;
- both windows ran side by side under Xvfb on the same fixtures;
- across 10 encodings the two file layers decoded to identical text, detected
  the same encoding, and wrote identical round-trip bytes.

The duplicated encoding rule — the one reason the Python GUI had to be kept in
step with the engine — is gone. See `REMEDIATION-PLAN.md` Phase 6 and
`RUST-GUI-FINDINGS.md`.

## Restoring it

The GUI is recoverable from git history (`git log --follow -- <path>`), and the
Python bindings can be rebuilt by restoring `python.rs`, its `mod python`
declaration in `textrill/src/lib.rs`, and the `pyo3` optional dependency plus
`extension-module` feature in `textrill/Cargo.toml`. Do not reintroduce a second
front end without a reason: the pyo3 layer was deleted to remove a language
boundary the native GUI does not need.
