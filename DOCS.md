# Document register

Written 2026-10-06. This file exists because the project had accumulated seven
instruction documents with no stated status, and every session began by
re-deriving which of them were still true. That re-derivation had been done
three times in three days and disagreed each time.

The list of documents read to produce this register is in
[§ What was read](#what-was-read).

## The rule

Every document in this repository gets exactly one status. The status is decided
by a test, not a feeling:

| status | test | consequence |
|---|---|---|
| **Authoritative** | Code, tests, or CI are checked against it. | Changing behaviour means changing this document in the same change. |
| **Background** | It records a decision, a measurement, or evidence, and nothing is checked against it. | Read it to understand *why*. Never cite it as *what to do*. |
| **Archived** | It is superseded, or it describes code that no longer exists. | Lives in `legacy-archive/`. Never read as current. |

A document with no status is the failure mode this file exists to end. If you
add a document, give it a status in the table below.

## The register

| Document | Status | Verdict |
|---|---|---|
| `textrill/README.md` | Authoritative | The user-facing contract. Shipped, and the only document a user reads. |
| `textrill/tests/corpus/README.md` | Authoritative | How the differential harness decides pass/fail, and — importantly — how to prove it can fail. |
| `REMEDIATION-PLAN.md` | Authoritative, **but see § The problem with the plan** | 222 KB, 3 653 lines. Referenced by `Makefile`, `tests/proptest.py`, and the Flatpak manifest, so it cannot move. |
| `RUST-GUI-FINDINGS.md` | Background | The `egui` decision and the evidence for it. §8 "Consequences for Phase 6" is now done and reads as a to-do list that is finished. Keep for the Qt/cxx-qt evidence and §7 sequencing rules. |
| `template-research1/FINDINGS.md` | Background | Reusable designs from a deleted prior attempt, plus §3 "Hazards found in our own engine" — which is still the sharpest description of what the tool does to plain text. |
| `ADVERSARIAL-FINDINGS.md` | Background, **closed** | A completed attack pass. Every finding A1–A12 is implemented and verified. Its scope note already says it judged the port "against the Perl module as the specification", which is the assumption now being retired — but the findings themselves were real bugs and remain the evidence for the engine's correctness. |
| `TOOL-SURVEY.md` | **Archived** → `legacy-archive/TOOL-SURVEY.md` | A 2026-09-29 draft whose feature matrix is now factually wrong: it still calls the port `txt2html-rs`, and reports TOC, HTML5 output, rc files, `charset` and a stylesheet as absent. All five exist. Its recommendations (P11 rc files, TOC, heading numbering, built-in stylesheet) are implemented. It also contains the reason option abbreviation is deliberately not supported, which is why it is archived rather than deleted. |
| `DOCS.md` (this file) | Authoritative | Statuses and reading order. |

Also present and deliberately **not** documents: `Homer.txt` (a real 37 KB
unmarked document used as a test input, added 2026-10-06),
`quicknote1.txt` (unrelated, untracked), and the `ref/`, `packaging/`,
`textrill/`, `textrill-gui-rs/` source trees.

## Reading order, by task

**"I want to change what the tool outputs."**
1. `textrill/README.md` — the option surface as shipped.
2. `REMEDIATION-PLAN.md` § "Compatibility policy" — the three-tier rule that
   decides whether Perl is the oracle for what you are about to change. It is at
   line 442, not at the top, which is the problem described below.
3. `template-research1/FINDINGS.md` §3 — what the engine already does to plain
   text before you add anything else that touches it.

**"I want to change how it is tested."**
1. `textrill/tests/corpus/README.md` — including § "Six ways this reported
   success wrongly". Break the gate on purpose before trusting it.
2. `REMEDIATION-PLAN.md` § "How little of the input space any oracle actually
   reaches" — the measurement that says how little the oracle covers.

**"I want to work on the GUI."**
1. `RUST-GUI-FINDINGS.md` §4–6 — the toolkit decision, the dependency
   measurements, and the licence/privacy audit.
2. `legacy-archive/textrill-gui/SURFACE.md` — the frozen behavioural contract
   the native GUI was ported from. Archived, but it is a *specification*, and
   specifications outlive their implementations.

**"I want to know why something was decided."**
1. `REMEDIATION-PLAN.md` § "How to complete a task" and the per-item rationale.
2. `legacy-archive/README.md` — what was retired and how to restore it.

**"I am looking for a feature gap."** — Nothing here. See § The problem with the
plan. Feature gaps are no longer being enumerated against upstream; the
question is now what this tool should do that it does not yet do.

## The problem with the plan

`REMEDIATION-PLAN.md` is 3 653 lines and 222 KB, and it serves two purposes that
are incompatible:

1. **A historical reconstruction.** It is generated by a level-gating script
   that rebuilds the document as it stood at four successive points in the
   project's life. That mechanism is genuinely good — it records what was known
   at each stage, including the things that were wrong, which is how the
   "we believe every green figure before 2026-10-01 is weaker than it appears"
   warning became visible at all.
2. **A forward-looking working plan.** What to do next.

These fight. The evidence is specific:

- The document's title is *txt2html — remediation plan*. Every phase is framed as
  closing a gap against Perl, which is the frame we are retiring.
- The rule that actually governs the work — the three-tier compatibility policy,
  which says match Perl where Perl is right and **must be better** where it is
  wrong — sits at line 442, after the status block, a progress reconciliation,
  and a "how to complete a task" section.
- 222 KB is not a document anyone reads. It is a document everyone greps, and the
  parts needed while working are the parts buried deepest.
- The 65-option / 121-name inventory exists to answer *"what does Perl have that
  we do not?"*. Under the new direction that is the wrong question, and the
  answer it produced — a list of gaps — is what has kept pulling the work back
  toward mimicry.

The Homer.txt run on 2026-10-06 is the clearest single argument. That file is
37 KB of real prose with an obvious human structure — *PREFACE TO FIRST EDITION*,
*BOOK I.* … *BOOK XXIV.* — and no txt2tags markup whatsoever. `--chunk`
therefore produces a single 38 KB page. Under the parity framing this reads as
"is that what Perl does too?", which is a question with a known and irrelevant
answer. Under the Rust-native framing it reads as "the tool cannot see structure
that a human sees instantly, and that is the actual problem" — which is a
question worth the project's remaining effort.

**This is unresolved and needs a decision.** See the end of this file.

## What was read

Every document below was read in full on 2026-10-06, except where noted, to
produce this register.

| Read | What it was used for |
|---|---|
| `TOOL-SURVEY.md` (308 lines) | Feature matrix, to check which recommendations were still outstanding. Found all stale → archive. |
| `RUST-GUI-FINDINGS.md` (423 lines) | Toolkit decision status, whether §8 was still open. |
| `template-research1/FINDINGS.md` (93 lines) | §3 hazards; §4 states the parity assumption now being retired. |
| `ADVERSARIAL-FINDINGS.md` | Structure and scope note only (headings + preamble), plus §4 and §7. Full read of all 12 findings not required: the plan's addendum already tracks each one to done. |
| `legacy-archive/README.md` (37 lines) | Existing archive convention — code and its spec, retired together, restorable from git. |
| `textrill/README.md` | § Status and § Relationship to upstream, to classify as the user-facing contract. Headings only elsewhere. |
| `textrill/tests/corpus/README.md` | Headings plus the "a gate that has never been observed failing is not a gate" rule. |
| `REMEDIATION-PLAN.md` (3 653 lines) | Not read in full. § "Compatibility policy" read in full, plus the status block, the "how to complete a task" section, the Phase 6 status, the oracle-coverage addendum, and all 93 headings. |

Not read, and why: `ref/txt2html-3.0/README.md` and `CONTRIBUTING.md` are
upstream's, not ours; the Perl reference itself was read only at the named lines
when a claim needed checking.

## Open decision

Does `REMEDIATION-PLAN.md` get **updated in place** — new governing section at
the top, parity-framed phases marked superseded, the inventory reclassified — or
does the level-gated reconstruction get **archived whole** and replaced by a
short forward-looking document that says what the tool should do next?

The trade: archiving loses the single-place record that links a decision to the
evidence that forced it, and the reconstruction mechanism cannot reproduce a
document that no longer exists in that shape. Updating in place keeps one file
that is 222 KB and will grow.

Not decided here. Until it is, the compatibility policy at line 442 is the
governing rule, because it is the only part of the document that already says
what we now want to say.
