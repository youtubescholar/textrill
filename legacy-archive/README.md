# legacy-archive

Retired code and retired documents, kept for reference. **Nothing here is built,
tested, linted, or packaged.** It is not part of `make verify`, `make test`, or
CI. `DOCS.md` at the repository root is the register of what is current.

## What is here, and why

| path | what it is |
|---|---|
| `REMEDIATION-PLAN.md` | The long-form reconstruction of the project at four points in its life: the tiered parity policy, the Phase 0b gate-integrity findings, and the decisions including the ones that turned out wrong. |
| `ADVERSARIAL-FINDINGS.md` | The attack pass that judged the port against the Perl module; findings A1–A12, all implemented and verified. |
| `TOOL-SURVEY.md` | The 2026-09-29 feature-gap survey. Archived because its matrix went stale — it still called the port `txt2html-rs` and reported TOC, HTML5 output, rc files, `charset` and a built-in stylesheet as missing. Its upstream TOC disclaimer is still cited by live documents. |
| `SURFACE.md` | The frozen GUI/engine surface that the native `egui` GUI was ported from (moved here from `textrill-gui/` on 2026-10-08). A specification, and specifications outlive implementations. |
| `quicknote1.txt` | Agent residue from the abandoned prior attempt — a transcript of a session reasoning about its own todo list. Concrete evidence for the note in `docs/template-research/FINDINGS.md` that the prior project's process apparatus became its failure mode. |

Two different things are archived here, and the distinction matters.
`REMEDIATION-PLAN.md`, `ADVERSARIAL-FINDINGS.md` and `TOOL-SURVEY.md` are
retired **documents**: the code they describe is still running, but their
claims about it are no longer current. Nothing here is ever read as current.

## Removed on 2026-10-08

`level-gate.py`, the Python + PySide6 GUI implementation (`textrill-gui/`), and
the pyo3 bindings plus Python wrapper (`python-bindings-*`, `python/`) were
deleted outright rather than kept archived: nothing references them, they made
up most of the archive's bulk, and a fresh clone does not need a dead GUI and a
dead generator. All of it is recoverable from git history (`git log --follow
-- <path>`), and the register below records why each existed. `SURFACE.md` was
kept because live documents cite it.

## Why the GUI was retired

The native GUI reproduces the Python front end's behaviour byte-for-byte where
it matters. The side-by-side differential run (2026-10-03) showed:

- the Python suite passed 58 tests (1 skipped) and the native suite 60/60;
- both windows ran side by side under Xvfb on the same fixtures;
- across 10 encodings the two file layers decoded to identical text, detected
  the same encoding, and wrote identical round-trip bytes.

The duplicated encoding rule — the one reason the Python GUI had to be kept in
step with the engine — is gone. See `REMEDIATION-PLAN.md` Phase 6 and
`docs/RUST-GUI-FINDINGS.md`.

## Restoring it

The GUI is recoverable from git history (`git log --follow -- <path>`), and the
Python bindings can be rebuilt by restoring `python.rs`, its `mod python`
declaration in `textrill/src/lib.rs`, and the `pyo3` optional dependency plus
`extension-module` feature in `textrill/Cargo.toml`. Do not reintroduce a second
front end without a reason: the pyo3 layer was deleted to remove a language
boundary the native GUI does not need.