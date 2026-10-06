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
| [`docs/LANDSCAPE.md`](docs/LANDSCAPE.md) | Who else does this, and the three-part niche. Grounded in `competition-files/` and the installed pandoc. |
| [`docs/PACKAGING.md`](docs/PACKAGING.md) | The Flatpak manifest, its two deliberate blockers, and how to clear them. |
| `textrill/README.md` | The user-facing contract. The only document a user reads. |
| `textrill/tests/corpus/README.md` | How the differential harness decides pass/fail — and how to prove it can fail. |
| `DOCS.md` (this file) | Statuses and reading order. |

### Background

| Document | What it is |
|---|---|
| `RUST-GUI-FINDINGS.md` | The `egui` decision and the evidence for it: why `cxx-qt` failed, the dependency measurements, the licence and privacy audit, the "verify the published crate version, not the docs" rule. §8's Phase 6 to-do list is finished. |
| `template-research1/FINDINGS.md` | Reusable designs from a deleted prior attempt. Its §3, "Hazards found in our own engine", is still the sharpest description of what the tool does to plain text before you add anything else that touches it. |
| `lexicon` — none exists yet | A naming document would help: `textrill` vs `txt2html` vs `Textrill`, and which is which. `docs/PLAN.md` Phase 1 is the place it would come from. |

### Archived — `legacy-archive/`

| Path | What it was |
|---|---|
| `REMEDIATION-PLAN.md` | 3 653 lines, built by `level-gate.py` to reconstruct the project at four points in its life. Asked "what does Perl do that we do not?", and its 65-option inventory existed to produce a list of gaps. Kept because it records *why* decisions went the way they did, including the ones that turned out wrong. |
| `level-gate.py` | The script that generated the plan above. Same reason. Nothing uses it now. |
| `ADVERSARIAL-FINDINGS.md` | An attack pass that judged the port "against the Perl module as the specification". Findings A1–A12 are all implemented and verified, so it is evidence about a finished job. |
| `TOOL-SURVEY.md` | The 2026-09-29 feature-gap survey. Archived because its matrix went *false*: it reported TOC, HTML5, rc files, `charset` and a stylesheet as absent, and all five exist. Its upstream TOC disclaimer and its option-abbreviation rationale are still cited by live documents. |
| `textrill-gui/` | The retired Python + PySide6 front end, 1 365 lines and its 58-test suite. |
| `textrill-gui/SURFACE.md` | The frozen GUI/engine contract the native GUI was ported from. A specification, and specifications outlive implementations. |
| `quicknote1.txt` | Agent residue from the abandoned prior attempt — a transcript of a session reasoning about its own todo list. The concrete evidence for the note in `template-research1/FINDINGS.md` that the prior project's process apparatus became its failure mode. |
| `python*` | The pyo3 bindings and the thin Python wrapper, retired with the GUI. |

`legacy-archive/README.md` is the archive's own index.

### Deliberately not documents

- `ref/` — the Perl reference. Ignored by git, derived by `make ref` from the
  tracked `txt2html-3.0.tar.gz`. **Kept, and this is not a half-measure.** It is
  the oracle for the parity tier of the harness — the only thing that found
  A1–A12 — and it is available for historical artifact investigation. It is an
  oracle now, not a specification.
- `competition-files/` — the pandoc 3.12 source, tracked as tarballs so the
  studies can be re-run offline. The extracted tree is ignored.
- `examples/` — real documents, not prose about documents. `homer.txt` for now.
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

**"I want to change how it is tested."**
1. `textrill/tests/corpus/README.md`, including § "Six ways this reported
   success wrongly". Break the gate on purpose before trusting it.
2. `docs/PLAN.md` § Standing rule — the same rule, and why it exists.

**"I want to understand a decision."**
1. `docs/PLAN.md` — every item links to its measurement.
2. `legacy-archive/REMEDIATION-PLAN.md` — the long form, including the parts
   that turned out wrong.

**"I want to work on the GUI."**
1. `RUST-GUI-FINDINGS.md` §4–6 and §7.
2. `legacy-archive/textrill-gui/SURFACE.md` for the frozen behavioural contract.

---

## Two open decisions

1. **The GitHub owner.** Blocks the app-id, both `Cargo.toml` `repository` TODOs,
   and packaging. Everything else can proceed without it.
2. **`flatpak-cargo-generator` or `cargo vendor`.** `docs/PACKAGING.md` §
   "Undecided" argues for vendoring; not decided.

## What was read

To produce the two studies, on 2026-10-06: `textrill --help` and the engine
source for the capability claims; `competition-files/pandoc-3.12.tar.gz` and
`/usr/bin/pandoc` 3.1.3 for the landscape; `examples/homer.txt` converted by
both tools for the comparison; `textrill/LICENSE` for the licensing position;
`legacy-archive/TOOL-SURVEY.md` for the category survey and the upstream TOC
disclaimer; `legacy-archive/REMEDIATION-PLAN.md` § "Compatibility policy" for
the three-tier rule that first recorded the reference being wrong on UTF-8.
