# Document register

Written 2026-10-06. Replaces the register written earlier the same day, which
described a project organised around parity with a Perl reference. This project
is now organised around what the Rust tool should do on its own.

Every document gets exactly one status, decided by a test rather than a feeling:

| status | test | consequence |
|---|---|---|
| **Authoritative** | Code, tests, or CI are checked against it. | Changing behaviour means changing this document in the same change. |
| **Background** | It records a decision or evidence; nothing is checked against it. | Read it for *why*. Never cite it for *what to do*. |
| **Archived** | Superseded, or describes code that no longer exists. | Lives in `legacy-archive/`. Never read as current. |

A document with no status is the failure mode this file exists to end.

---

## The register

### Authoritative

| Document | What it is |
|---|---|
| [`docs/PLAN.md`](docs/PLAN.md) | **What we do next.** Every item traces to a measurement in one of the two documents below. |
| [`docs/CAPABILITIES.md`](docs/CAPABILITIES.md) | What textrill does, measured by running it. Defects, strengths, and the ten-clause output contract. |
| [`docs/LANDSCAPE.md`](docs/LANDSCAPE.md) | Who else does this, and the three-part niche. Grounded in `research/pandoc/` and the installed pandoc. |
| [`docs/OFFERING.md`](docs/OFFERING.md) | How the tool is offered against pandoc: the claim, its measurement (`make proof`), what must be built first, and what pandoc does better. |
| [`docs/PACKAGING.md`](docs/PACKAGING.md) | The Flatpak manifest, its two deliberate blockers, and how to clear them. |
| `textrill/README.md` | The user-facing contract. The only document a user reads. |
| `textrill/tests/corpus/README.md` | How the differential harness decides pass/fail — and how to prove it can fail. |
| `DOCS.md` (this file) | Statuses and reading order. |

### Background

| Document | What it is |
|---|---|
| `docs/RUST-GUI-FINDINGS.md` | The `egui` decision and the evidence for it: why `cxx-qt` failed, the dependency measurements, the licence and privacy audit, the "verify the published crate version, not the docs" rule. §8's Phase 6 to-do list is finished. |
| `docs/template-research/FINDINGS.md` | Reusable designs from a deleted prior attempt. Its §3, "Hazards found in our own engine", is still the sharpest description of what the tool does to plain text before you add anything else that touches it. |
| [`docs/ORACLE-ARCHAEOLOGY.md`](docs/ORACLE-ARCHAEOLOGY.md) | For every bug the project found in itself, which oracle found it and whether a reference-free oracle would have. Concludes the Perl differential's unique contribution was three frozen semantic cases, and frames Perl as a bootstrap compiler rather than a gate. |
| [`docs/DEVLOG.md`](docs/DEVLOG.md) | The verbatim development log, oldest first, back to the txt2html-rs import. Commit messages now point here instead of carrying walls of body text. Read it for *what happened when*; PLAN/CAPABILITIES are *why* and *what now*. |
| `lexicon` — none exists yet | A naming document would help: `textrill` vs `txt2html` vs `Textrill`, and which is which. `docs/PLAN.md` Phase 1 is the place it would come from. |

### Archived — `legacy-archive/`

| Path | What it was |
|---|---|
| `REMEDIATION-PLAN.md` | 3 653 lines, built by `level-gate.py` to reconstruct the project at four points in its life. Asked "what does Perl do that we do not?", and its 65-option inventory existed to produce a list of gaps. Kept because it records *why* decisions went the way they did, including the ones that turned out wrong. |
| `ADVERSARIAL-FINDINGS.md` | An attack pass that judged the port "against the Perl module as the specification". Findings A1–A12 are all implemented and verified, so it is evidence about a finished job. |
| `TOOL-SURVEY.md` | The 2026-09-29 feature-gap survey. Archived because its matrix went *false*: it reported TOC, HTML5, rc files, `charset` and a stylesheet as absent, and all five exist. Its upstream TOC disclaimer and its option-abbreviation rationale are still cited by live documents. |
| `SURFACE.md` | The frozen GUI/engine contract the native GUI was ported from, moved up from `textrill-gui/` on 2026-10-08. A specification, and specifications outlive implementations. |

Removed from the archive on 2026-10-08, recoverable from git history:
`level-gate.py` (the plan's generator, nothing used it), the retired
`textrill-gui/` implementation, the `python-bindings-*` / `python/` pyo3 layer,
and `quicknote1.txt` (an agent transcript, cited nowhere in the tree). See
`legacy-archive/README.md`.

`legacy-archive/README.md` is the archive's own index.

### Deliberately not documents

- `ref/` — the Perl reference. Ignored by git, derived by `make ref` from the
  tracked `research/txt2html-3.0.tar.gz`. **Kept, and this is not a
  half-measure.** It is the oracle for the parity tier of the harness and is
  available for historical artifact investigation. It is *not* what found
  A1–A12: the reference-free attack pass did. What it alone found are the silent
  semantic divergences a reference-free oracle cannot judge — E3, `delim_retry`,
  the non-ASCII delimiter predicate — see `docs/ORACLE-ARCHAEOLOGY.md`. It is an
  oracle now, not a specification. Phase 8 (`docs/PLAN.md`) plans to retire it
  as a gate once reference-free acceptance exists.
- `research/` — source archives for the offline studies: the pandoc 3.12
  release (+ stripped) tarballs and the txt2html reference archives. Tracked so
  the studies re-run offline; a tree extracted from them is ignored.
- `stubs/` — the single canonical Perl stub that `make ref` installs into the
  derived `ref/` tree, kept reviewable in git.
- `examples/` — real documents, not prose about documents. Eight licence-clean
  texts with a register and pinned counts in `examples/README.md`.
- `packaging/` — the manifest and its generator target.
- `textrill/`, `textrill-gui-rs/` — the code.

---

## Reading order, by task

**"I want to change what the tool outputs."**
1. `docs/CAPABILITIES.md` §4 — what is weak, with reproductions.
2. `docs/CAPABILITIES.md` §6 — the output contract you are changing.
3. `docs/PLAN.md` — whether the change is already planned.

**"I want to work on templates."**
1. `docs/LANDSCAPE.md` §3 — the gap, measured against pandoc.
2. `docs/PLAN.md` Phase 4 — what is proposed, and why the 7-slot design stays.

**"I want to explain what we offer against pandoc."**
1. `docs/OFFERING.md` §2 — the claim, and the four things not to say.
2. `docs/OFFERING.md` §3 — the measurement, re-run with `make proof`.
3. `docs/OFFERING.md` §5 — what has to be built first, and in what order.

**"I want to change how it is tested."**
1. `textrill/tests/corpus/README.md`, including § "Six ways this reported
   success wrongly" — with the two sabotage commands that prove the harness can
   fail.
2. `CONTRIBUTING.md` house rule 2 — when a harness change needs proving.
3. `docs/PLAN.md` § Standing rule — the same rule, scoped to the plan's own gates.

**"I want to understand a decision."**
1. `docs/PLAN.md` — every item links to its measurement.
2. `legacy-archive/REMEDIATION-PLAN.md` — the long form, including the parts
   that turned out wrong.

**"I want to work on the GUI."**
1. `docs/RUST-GUI-FINDINGS.md` §4–6 and §7.
2. `legacy-archive/SURFACE.md` for the frozen behavioural contract.

---

## Decisions

1. ~~**The GitHub owner.**~~ **Resolved** 2026-10-08. The repo is
   https://github.com/youtubescholar/textrill; both `Cargo.toml` `repository`
   fields and the Flatpak app-id `io.github.youtubescholar.Textrill` follow
   from it (`docs/PACKAGING.md`).
2. **`flatpak-cargo-generator` or `cargo vendor`.** `docs/PACKAGING.md` §
   "Undecided" argues for vendoring; not decided. This is the remaining
   blocker on a buildable Flatpak.

## What was read

To produce the two studies, on 2026-10-06: `textrill --help` and the engine
source for the capability claims; `research/pandoc/pandoc-3.12-full.tar.gz` and
`/usr/bin/pandoc` 3.1.3 for the landscape; `examples/homer.txt` converted by
both tools for the comparison; `textrill/LICENSE` for the licensing position;
`legacy-archive/TOOL-SURVEY.md` for the category survey and the upstream TOC
disclaimer; `legacy-archive/REMEDIATION-PLAN.md` § "Compatibility policy" for
the three-tier rule that first recorded the reference being wrong on UTF-8.

To produce `docs/OFFERING.md`, the same day: the §3 numbers re-measured and
then pinned in `textrill/tests/positioning.py`; `pandoc --list-input-formats`
and `--list-output-formats` for the format counts; a small indented-list
fixture converted by both tools for the soft-wrap example; and the configuration
inventory in §1 taken from `textrill --help`, the engine's option sources and
`textrill-gui-rs/src/settings.rs`.
