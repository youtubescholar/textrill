# txt2html — remediation plan

Status: **in progress**, 2026-10-03. Covers
`textrill` (Rust engine + CLI) and `textrill-gui-rs` (native `egui` front end).
The Python bindings and the PySide6 front end are retired to `legacy-archive/`.

Gates at 2026-10-04: corpus **59/59**, goldens **33/33**, **203** engine tests,
**74** native GUI tests, `fmt`/`clippy` clean. (At 2026-10-03: corpus 59/59,
goldens 33/33, 16 000 fuzz cases 0 mismatches, 164 engine tests, 74 GUI tests;
the fuzzer and corpus are unchanged since.) The dated figures elsewhere in this
document (many sections still say `48/48`, `61 cargo tests`, `60/60`, `46/46`)
are the state at the date they were written and are kept as that record, not
corrected in place.

**Progress is recorded in §0.1 below. Done: P1–P6, P7, P8–P11, P12, P13,
P14–P19, P21–P23, P5.1–P5.4, E1–E3, A1, A1b, A2–A10. P6's gap was not the two
prescribed fixes (worth ~0%): it
was a prefilter that silently did nothing for every `\b`-wrapped dictionary
rule, because the translated pattern carries look-around that `regex-syntax`
refuses to parse, so no literal was proven and `fancy_regex`'s backtracking VM
ran on every paragraph. Deriving those literals from the original pattern
recovers all 9 missing rules and makes the link pass ~3× faster, putting Rust
ahead of Perl on the link-dense benchmark; see P6.
P11 is done: `@file`, `~/.txt2htmlrc` and `./.txt2htmlrc` are read, with
`file:line:` diagnostics. Phase 6's GUI is **done**: the native `egui`/`eframe` crate `textrill-gui-rs`
passes its ported 74-test suite and ran side by side with the Python front end on
2026-10-03; the Python/PySide6 front end and the pyo3 bindings it needed are
retired to `legacy-archive/`. The toolkit evidence and packaging analysis are in
`RUST-GUI-FINDINGS.md`; the earlier Qt 6 decision was abandoned after the spike
(`cxx-qt` 0.10 fails to compile any Rust QObject on this toolchain, and Qt's
official `qtbridge` requires Qt 6.10 while this host has 6.4.2). The frozen
behavioural contract is `textrill-gui/SURFACE.md` (archived); its QtWidgets
widget mapping was ported as tests rather than re-drawn for egui.
P13 is answered: the deliverable is a single self-contained artifact, so the GUI
was rewritten in Rust (egui, not Qt) and **the engine is kept** — see P13 and
Phase 6. Every
High and Medium item from the attack pass is closed. The Phase 0b gate audit is
finished: all four defects it found are fixed, the panic divergence it surfaced
(P22) is closed, and P20's alignment guard is in. **Phase 5 feature work is
complete** (P5.1–P5.4). What remains is packaging and the template design below,
which is settled but not yet built; P6's performance gap is closed (see P6).

> **Read Phase 0b before trusting any result in this document.** Checks in
> `make verify` were found on 2026-10-01 to be structurally incapable of reporting
> failure — most seriously `run.sh`, the primary differential gate, which printed
> `PASS=0 FAIL=46` and exited 0. Two of these have been present since the initial
> import, so **every green figure recorded before 2026-10-01 is weaker evidence
> than it appears**. The standing rule is now in Phase 0b: *a gate that has never
> been observed failing is not a gate.*

**The Perl module is the oracle for most of the work below, but on non-ASCII
input it is the defect, not the specification. Read the compatibility policy
before starting anything that touches Unicode, encoding, or resource limits.**

Companion documents: `TOOL-SURVEY.md` (feature-gap survey) and
`ADVERSARIAL-FINDINGS.md` (attack pass).

**The attack pass found ten further items, planned as A1–A10 in the addendum at
the end of this file. Read that before starting work.** Two of them are
reachable with default options from ordinary input, which outranks everything
here in urgency: A1 (any paragraph over ~500 KB panics the engine) and A3
(`tab_width=0` panics, and the value is selectable in the GUI). A2 needs
`--make_tables`, which is off by default, so it is one opt-in away rather than
zero — but P12 has since measured it and found it far smaller than first claimed
(a bounded ~0.3 MB per process, not unbounded growth with input size; see A2).
A3 is the next item by severity. A4 is a correction rather than a new
item — **P4's claim that `worker.py:51` catches engine panics in the GUI is
wrong**, because pyo3's `PanicException` inherits `BaseException`, so it does
not.

The `P` numbering below is unchanged and is referenced from `TOOL-SURVEY.md`; the
addendum deliberately uses `A` numbering so the two never collide.

Everything below was verified by running it, not by reading it. Measurements
are on this machine, Perl 5.38, release build, default options unless noted.

## How to complete a task

Added 2026-10-01, after this document's own process started costing more than the
work. Four rules, in the order they matter.

**1. Classify against the tier table before calling anything a divergence.**
Sections "Compatibility policy" below settle the oracle question. Do not open a new
question that a tier already answers. This is not hypothetical: P22 was written up
as an undecided judgement call — *warn-and-skip versus fail-cleanly* — when
"error handling" is explicitly Tier 2, meaning "may differ, **must be better**".
An hour of "this needs a decision" went to a decision the document had already
made. **When the tier settles it, the item is not open, it is unimplemented.**

**2. A new finding gets one line here, not a new section.** The narrative belongs
in `ADVERSARIAL-FINDINGS.md`, which is a log. This plan is a queue. The growth
pattern that produced a 1 800-line document is the problem: every finding added
prose, and prose makes the *next action* harder to see, which is how a 5-minute
task turns into an afternoon of bookkeeping. If you cannot state a finding in one
line, it is not yet understood well enough to act on.

**3. "Done" means three things, not one:** the code changed, a gate can report the
failure, and this file's status line says so. A change to behaviour with no
demonstrable failing gate is not done, because nothing distinguishes it from a
change that does nothing.

**4. Every gate is proved once by breaking it.** A gate never observed failing is
not a gate — it is a script printing reassuring text. Phase 0b found four that
way, including the primary one. When you add or change a check, the same session
that adds it must watch it fail on a deliberate fault.

### When the oracle is wrong, say so in the finding, not just the fix

The reference is a good oracle and it is still fallible. Where it is
demonstrably wrong, the port diverges deliberately and the divergence is recorded
as Tier 2 with the evidence. Reference behaviour that looks like error handling
deserves a second look before it is copied: in the P22 case the reference's
graceful handling of a bad pattern is not error handling at all, it is an
unguarded regex interpolated per line that happens to survive because Perl warns
instead of dying. "The reference does it" is not a reason.

## 0. Where things stand

Re-measured 2026-10-01 (release build, Perl 5.38, default options). Figures
marked ✓ were run again that day; the rest are carried from 2026-09-30.

| Check | Result |
|---|---|
| `cargo test --release` ✓ | **52/52** pass (14 lib, 4 optionstest, 13 cliexit, 5 linktest, 9 paratest, 4 alloctest, 3 doc) — was 33/33 |
| GUI (Python/PySide6, offscreen) | **58** tests, 1 skipped — was recorded as 46/46; the suite had grown. Retired 2026-10-03, superseded by the 74-test native suite (`textrill-gui-rs`) |
| corpus, clean `RUNDIR` ✓ | **48/48**, 33/33 goldens (was 38/40, 2 false passes — see P1). Now verified to **fail** when it should — see P15. The 48th is `pre_explicit`, which existed in the tree but never ran: `pre_explicit_blank` was declared twice, so A1's case was shadowed and its own assertions were dead (P21) |
| `proptest.py` ✓ | OK — **0 known-open checks**. It used to report 30, all owned by A8, printed rather than silenced; A8 is fixed and the list is empty. Verified to return 1 on failure |
| `alloctest` ✓ | 4/4, at any `--test-threads` (see P12) |
| `cargo fmt --check` ✓ | clean |
| `cargo clippy --release --all-targets -- -D warnings` ✓ | **0 warnings** — 69 cleared (P23), and the target now *fails* on a new one rather than warning. Verified by injection: an added `&mut Vec` parameter errors the build |
| upstream Perl `t/*.t` ✓ | **102/102** assertions pass, 7 functional files (5 release-only files skipped) — the canary for P1 |
| differential fuzz ✓ | **16 000 cases** (8 seeds × 2 000), **16 000 compared**, 0 mismatches, 0 skipped, 0 timeouts — 5m30s wall / 39m18s CPU, concurrent (P19). Replaces the void figure of P18 |
| `make verify` end-to-end ✓ | **OK** — first complete green in the project, on a harness whose failure modes are demonstrated (P19, P21, P23) |
| GitHub Actions CI ✓ | **`.github/workflows/ci.yml`**, 3 jobs on every push (P8): `rust` (fmt, clippy, engine + native GUI tests), `differential` (corpus, goldens, fuzz, upstream canary), and `musl` (static CLI, run through the corpus). Every failure class observed red locally before the workflow was trusted: clippy, corpus, goldens, fuzz, and a stale `MINE` |
| invalid user regexp ✓ | **clean error, exit 1, no output written** (P22) — was a Rust panic and exit 101. Tier 2 divergence, deliberate |
| speed, `big_para` 1.1 MB ✓ | Rust **0.37 s** vs Perl **0.18 s** (~2.1x slower) |
| speed, `big_para_crlf` 0.8 MB ✓ | Rust **0.37 s** vs Perl **0.41 s** — the port is *faster* here |

Both scale probes are byte-identical to the reference.

The original §0 speed row said "2 MB document, Rust 2.94 s vs Perl 1.51 s". That
fixture is no longer in the tree, so the figure cannot be reproduced or refuted
from the repository; the two rows above replace it. The conclusion is unchanged in
direction and slightly worse than recorded: still about 2x slower than Perl on
plain LF input, and *faster* on CRLF input, which the single old figure hid.

The conversion engine is in good shape. The defects that remain are in error
handling, performance, encoding policy and packaging — not, with the exception of
E1–E3, in the transformation logic. **An earlier version of this section claimed
no defect at all lay in the transformation logic. That was wrong**, and §0.1
records why: promoting the fuzzer to a real test found three engine bugs in its
first four runs, one of them silent content loss.

## 0.1 Progress, 2026-09-30, reconciled 2026-10-01

Phase 0 is complete, and the corpus is at **48/48** with **33/33** goldens
byte-identical and the GUI at **46/46**. Fuzzing is 2 000 cases across eight
seeds (16 000 cases), against 700 cases before. The corpus grew from 46 to 47
with A8, which added two cases and made one of them (`opt_injection`) a declared
Tier 2 divergence — see the A8 section for why its verdict is inverted.

E3 landed on 2026-10-01. P13, the last item on the agreed sequence, was answered
the same day: one self-contained artifact, so the GUI is rewritten in Rust + Qt
and the engine is kept. _Refined 2026-10-02: the artifact ships as **Flatpak**,
and Qt is an assumption rather than a decision — both noted in "Licensing and
distribution"._ The rewrite itself is **Phase 6**, and P5, P6 and P11 were
sequenced ahead of it — all three are now **done**. P7 was sequenced there first of all — because the encoding
rule is implemented in both Rust and Python and the two copies disagreed — and
it is now **done**: both copies decode CP1252, the fallback is the one the
converter's own `demoronize` table was written for, and the rewrite inherits one
correct implementation instead of two.

| Item | State | Note |
|---|---|---|
| P1 | done | `RUNDIR` cleared, non-zero exit fails the case, `--system_link_dict` removed from `CLI[]` and kept in `EXTRA[]` |
| P2 | done | regexps corrected; the four custom-header cases now reproduce `tfiles/good_custom-headers.html` |
| P3 | done | `fuzz.py` is seeded, permanent, runnable in CI; `--cases` defaults to 300 |
| harness false-greens | done | durable `PERL5LIB`, reference smoke gate, stale-binary warning |
| E1 | done | `table_type` merged instead of replacing |
| E2 | done | explicit-quote `<pre>` dropped text after a blank line |
| E3 | **done** | `split_blank_lines` drops every trailing empty field, as Perl's `split()` does; checked exhaustively against Perl over 1 365 inputs. `KNOWN_DIVERGENCES` is now empty |
| A1 | **done** | `chop_trailing_cr`/`chop_leading_cr`; the ~500 KB panic and the hang behind it (A1b) |
| A2 | **done** | `ascii_re_cached` now takes `&'static str`, so only fixed literals can be cached and the leak is bounded by the source; verified by `make alloctest` |
| A3 | **done** | five numeric options bounded, four deliberately not, from one table the GUI also reads; the difference is measured |
| A4 | **done** | `PanicException` re-export; GUI worker reports Rust panics and always completes |
| A5 | **done** | a file's encoding is remembered and written back, so a non-UTF-8 save no longer corrupts it. Extended by P7, which found the remembered encoding was Latin-1 — the write-back was faithful and still wrong, because the editor and the converter were reading different documents |
| P7 | **done** | the encoding fallback decoded as Latin-1 while `demoronize_char`'s table is keyed on CP1252, so the substitutions never fired on any real Windows file and C1 controls leaked into the output. Fallback is CP1252; `--encoding` and `--meta_charset` added; GUI and engine now agree. **P7.4 then found the detection *order* was the real defect**: UTF-8 validity was checked first, which discards a BOM as an error and lets BOM-less UTF-16 pass as UTF-8 — ASCII prose in UTF-16LE *is* valid UTF-8, and both implementations emitted a NUL between every letter. Detection is now BOM → NUL alignment → UTF-8 → CP1252, with `--encoding` covering what detection cannot reach; 34 encoding tests, 9 new corpus cases |
| A6 | **done** | superseded conversions are cancelled; 85% of the CPU and 21 MB saved, not the wall-clock win first predicted |
| A7 | **done** | no directories created on save; the `mkdir` was load-bearing for an unrelated test, found by a 30-minute hang |
| non-ASCII delimiter predicate | **done** | `(?<!é)` was vacuous; predicates extracted so the tests exercise production code |
| P12 | **done** | `proptest.py` (5 properties, no Perl oracle) + `alloctest.rs` (counting allocator); wired into `make verify` |
| P12 harness fix | **done** | the allocation budgets raced on a process-global counter, so `cargo test` was intermittently red; serialised, commit `4f48dbd` |
| P14–P15 | **done** | `make fuzz` and `run.sh` could not report failure at all; `run.sh` printed `FAIL=46` and exited 0. Both now gate |
| P16–P17 | **done** | an uncaught `TimeoutExpired` killed a fuzz seed silently; `run.sh <stem>` died on an unbound `GOLDEN_N` after printing PASS |
| P18–P19, P21 | **done** | fuzz figure re-established on the fixed harness (16 000 compared, 0 mismatches); 8 seeds now concurrent, 99 min → 5m30s; and the fuzzer's missing `compared` counter closed, which had let a run that checked nothing exit 0 |
| P20 | **done** | a case wired into one table and not the other never ran, silently — the P2 shape. Now checked by name in both directions, and it aborts the run rather than summarising a subset |
| fuzzer cleanup | **done** | removed `KNOWN_DIVERGENCES` and ~90 lines of matching machinery, plus a dead `PERL_DRIVER`. The "reference refused" skip turned out to be a real false green and is gone |
| P22, and P4 part 2 | **done** | a user regexp that does not compile no longer panics (exit 101, no output): validated up front, then a clean error naming the option, the pattern and the parser's complaint. A `/pattern/` link-dictionary entry took the same route and now does too — reported and skipped, which was the last user-reachable panic |
| P5 | **done** | the inherited `/|.../` hang: an empty-matching dictionary pattern spun the substitution loop forever, in the Perl original too. Rejected at load with a diagnostic, reusing the P4 channel. The criterion is `re.is_match("")` because `translate_pattern` turns `\b` into a zero-width lookaround alternation and a `*` glob is not empty-matching. `-o`/`-s` deliberately unguarded: they substitute once, Perl accepts them, and guarding them would be a Tier 1 divergence |
| P6 | **done** | the link pass was ~2× slower than Perl; the two prescribed fixes were worth ~0% and the prefilter only reached rules already delegating to the linear `regex` engine. The nine rules with no literal were the `\b`-wrapped family: `translate_pattern` turns `\b` into look-around, `regex-syntax` will not parse that, so `required_literal` returned `None` and their literal/`captures()` ran on every paragraph. `add_rule` now falls back to the original pattern (sound: translation rewrites only zero-width anchors and escape classes), recovering all 9. Link-dense 2 MB × 9 901 paras: 3.75 s → 1.27 s (2.95×), byte-identical; Perl 1.96 s, so Rust is ~1.5× faster, from ~1.9× slower |
| P13 | **decided** | single artifact. The GUI is rewritten off Python + PySide6; **the 5,644-line engine is kept**. P5, P6 and P11 were sequenced *ahead* of it and are all now **done**. P7, its stated prerequisite, is **done** — the encoding rule was implemented twice and the copies disagreed; they now agree. Plan in **Phase 6**. Delivery since refined: Flatpak, not a bundled binary, and the toolkit is now decided (`egui`/`eframe`, not Qt) — see "Licensing and distribution" and Phase 6 |
| Phase 6 | **in progress** | the GUI rewrite: 1,365 lines of Python shell out, 32 tests ported as acceptance criteria. §6.3 step 1 done: surface frozen in `textrill-gui/SURFACE.md`. Step 2 done: `FileTests` moved to Rust, which required adding the engine's missing encoder (`src/encode.rs`) and fixed a UTF-16 decoder defect. Steps 3-5 (the shell itself) are done: the native crate `textrill-gui-rs` is in, with the `cli::SPECS`-generated options panel, `worker.py`'s concurrency contract, the document/file/settings model, the full chrome, the unsaved-changes prompt, native file choosers and `app.py`'s command line all ported, plus window geometry/state persistence. The 32 acceptance criteria are covered by the native suite (74 tests). CLI fate **decided** (CLI stays, independently distributable); replace-vs-coexist **decided** (coexist until the ported suite passes, then replace); packaging **decided** (Flatpak, not a bundled binary). Toolkit **decided: `egui`/`eframe`** (pure Rust), after the Qt path was rejected — `cxx-qt` 0.10 does not compile Rust QObjects on this toolchain and `qtbridge` needs Qt 6.10 while the host has 6.4.2. Evidence and packaging analysis in `RUST-GUI-FINDINGS.md`. Step 5 done 2026-10-03: a side-by-side differential run — Python suite 58 tests (1 skipped) against native 60/60, both GUIs launched under Xvfb on the same fixtures, and a 10-encoding file-layer differential that decoded, detected and round-tripped byte-identically. The Python GUI and the pyo3 bindings are retired to `legacy-archive/`; `make gui` and the CI `gui` job are removed |
| licensing | **decided** | engine and CLI stay **GPL-3.0-or-later**; GUI is **GPLv3**. BSD for the CLI was raised and declined as unnecessary — see "Licensing and distribution" |
| toolchain | done | `make verify` gate, `cargo fmt`, `#![forbid(unsafe_code)]`, git with one logical change per commit |

### Agreed sequence, 2026-09-30

Settled after the A1 diff review and the compatibility investigation, so that
the next person does not have to re-derive the ordering:

1. ~~**P12 — the property suite.**~~ **Done.** `tests/proptest.py` asserts no
   data loss, XHTML well-formedness, determinism, reconversion, and panic
   resistance without consulting the reference at all — the reference is the
   oracle for Tier 1 only, and it is wrong on non-ASCII input.
   `tests/alloctest.rs` adds a counting global allocator and the resource
   bounds. Both are wired into `make verify`.
2. ~~**Re-measure A2 with P12 and correct this plan.**~~ **Done**, and the
   correction was large: retention is 3 919 B per distinct pattern and the total
   is bounded near ~0.3 MB per process, so A2 drops from High to Low. The plan's
   1.3 GB figure was an artefact of a direct-call benchmark. Details and
   arithmetic under **A2** above.
3. ~~**A2 — the fix.**~~ **Done**, verified by `make alloctest` rather than by
   output, since the output was always correct. Marginal retention per distinct
   delimiter fell from 3 451 B to 4 B, and output is byte-identical on a
   table-heavy document at the same speed.
4. ~~**A3.**~~ **Done.** Five numeric options are now bounded and four are
   deliberately not; the difference is measured, not assumed, and it does not
   match what this plan originally said.
5. ~~**A5.**~~ **Done.**
6. ~~**A6.**~~ **Done.** The saving is 85% of the CPU and 21 MB, not the
   wall-clock win the plan predicted; see A6.
7. ~~**A7.**~~ **Done.** The `mkdir` was load-bearing for an unrelated test, found
   by a thirty-minute hang rather than a failure. A5–A7 are all closed.
8. ~~**E3.**~~ **Done**, 2026-10-01, commit `ce8f949`. It was a one-character-class
   accounting bug in the paragraph slurper, and it is the reason `KNOWN_DIVERGENCES`
   is now empty rather than merely accurate.
9. ~~**P13 — the packaging decision.**~~ **Decided 2026-10-01:** one
   self-contained artifact, so the GUI moves to Rust + Qt and the engine is
   kept. Plan in **Phase 6**; P5, P6 and P11 were sequenced ahead of it and are
   now all done. Its
   stated prerequisite P7 is **done**: the encoding rule was implemented in two
   languages, the copies disagreed, and the disagreement was a real defect rather
   than a byte difference.

That was the agreed sequence, and items 1–8 are all closed. **P13 is the last of
them.** But see below: before any further feature work, **P19 then P18**,
because the fuzz number this project reports is currently void (P18) and the run
that would establish it takes 99 minutes (P19).

**P19 and P18 are now both done**, and between them they changed the shape of the
backlog: the full fuzz went from 99 minutes to 5m30s, and `make verify` runs
end to end in under 8. The gate is affordable, so it can be run after every change
instead of once a month — which is the only reason the rest of this list gets
shorter rather than longer.

Recommended order from here — **every item below is now done**:

1. ~~**P22**~~ — **Done.** A user regexp that does not compile is rejected with a
   diagnostic naming the option, the pattern and the parser's complaint, instead
   of panicking. A `/pattern/` link-dictionary entry took the same route and now
   does too.
2. ~~**P20**~~ — **Done.** The `CLI[]`/`EXTRA[]` alignment guard closes the last
   known way to add a corpus case that silently never runs.
3. ~~**P13** — the packaging decision.~~ **Decided:** single artifact, GUI
   rewritten in Rust (**egui**/`eframe`, not Qt — see Phase 6), engine kept.
   P5, P6 and P11 landed before the rewrite, and P7, the prerequisite, was done
   first.
4. ~~**A8** — escape `--title` / `--style_url`.~~ **Done.** It was the sole
   owner of the 30 known-open `proptest` checks; there are none left.
5. ~~**P5, P6, P11**~~ — **Done**, in that order. A10 is done too: the pattern
   cache is bounded at a measured 6× the worst realistic working set. Nothing in
   Phases A or B is open.
6. ~~**Phase 6, the GUI rewrite**~~ — **Done 2026-10-03.** The Python/PySide6
   front end is retired to `legacy-archive/`; the native suite is 74 tests.
   P7 was worth doing first on its own terms: the encoding rule was written
   twice, the copies disagreed, and the disagreement was a live defect rather
   than a byte difference — the engine's `demoronize` table had been
   unreachable for its entire existence.

What actually remains is not a defect: **packaging** (the Flatpak manifest is
decided but not yet written) and the **template** work described below, which is
settled but not built. **Phase 5 is closed.**
**HTML5 mode** landed 2026-10-04 as `--html5` (P5.1), **sectioning + TOC +
multi-file chunking** the same day as `--section`/`--toc`/`--chunk` (P5.2, both
output models, design below), **heading numbering** as `--number_headings`
(P5.3), and **streaming** as `--stream` (P5.4). The
**musl CI build** landed 2026-10-03 — a static `x86_64-unknown-linux-musl`
binary, run through the differential corpus in CI (`make musl`, `make
corpus-musl`). See "What remains" near the top and Phase 5.

Two lessons worth carrying to the next item, because both cost time here: a
micro-benchmark of a helper API is not evidence about the tool's exposure to
hostile input, and a measurement harness needs its own validation before its
output is believed. The first produced a three-orders-of-magnitude error in
A2's severity; the second produced a first allocator test that passed because it
generated a document no table was ever detected in, and a second that measured
nothing because the `thread_local` cache was already warm from the first
measurement.

**A third instance of the second lesson arrived on 2026-10-01.** The allocation
budgets in `alloctest.rs` read process-global counters, and libtest runs tests in
parallel, so a sibling test's 100 MB conversion landed inside another test's
measurement window and `cargo test` was intermittently red with a residual that
should have been zero. `make alloctest` passed `--test-threads=1` and so looked
healthy, which is the trap: the suite had a flag that made one entry point
trustworthy and left the other lying. The fix is in commit `4f48dbd` and the
lesson generalises — **a gate that passes for one reason you did not write down
is not a gate.** See P12.

**And a fourth, which is the most expensive of the four.** P14 and P15 found that
`make fuzz` and `run.sh` — two of the three most load-bearing checks in the
repository — had never been capable of failing. Both had been green, repeatedly,
for the life of the project. Reading their output was never going to reveal it,
because their output was correct; only their exit status was wrong. Phase 0b sets
the standing rule: **deliberately break each gate and watch it fail, once.** The
whole audit took about twenty minutes.

Deliberately not doing yet: GitHub Actions (no remote, so it could not be run),
and clippy `-D warnings` (the crate is not clean; the bulk fix deserves its own
changeset and its own full verification rather than riding along with A2).

### The claim above was wrong

The original survey concluded that every defect lay outside the transformation
logic. Promoting the fuzzer to a permanent, seeded test overturned that: three
of the first four bugs it found were in the engine, one of them silent content
loss. "700 cases found nothing" was a statement about the fuzzer's reach, not
about the engine's correctness, and it should not have been generalised. The
engine bug rate was high *per unit of harness trust* — three of four bugs were
invisible before P1 and P2 landed, and a fourth was misreported by the fuzzer
for several iterations (below).

### E1. `--table_type` merged into the defaults instead of replacing them — **done**

`set_table_type` mutated the existing `TableTypeFlags`, so any `--table_type`
argument was unioned with the four defaults. Getopt::Long's `n%` spec builds a
fresh hash from the options actually present, so `--table_type DELIM=0` leaves
only `{DELIM => 0}` — ALIGN, PGSQL and BORDER are absent, not merely off.

Fixed in `src/cli.rs` by resetting the flags once, on the first `--table_type`,
and letting repeated occurrences accumulate — which is what the reference POD
prescribes. Two of the three originally reported divergences were this one bug;
the third-looking "indented block" case was the same leak surfacing as an
apparently unrelated table-detection difference.

`TableTypeFlags` still needs auditing for the `set_value` path, which merges.
If the Python and GUI bindings are meant to match `t/20tfiles.t` semantics,
replacement likely belongs there too. Unresolved.

### E2. Explicit-quote `<pre>` dropped everything after its first blank line — **done**

`split_end_explicit_preformat` buffers the continuation text, but the join back
lived inside a branch that only runs when text remains, so the continuation was
discarded. Silent content loss, reachable from ordinary input with default
options.

Fixed in `src/convert.rs`. The continuation must also reach `apply_links`:
`--use_preformat_marker` with `<pre>\n\n*d*` gives `<em>d</em>`, not `*d*`.

### E3. CR-only lines leave two stray blank lines — **done**

_Landed 2026-10-01, commit `ce8f949`._

Narrow, and not general CR handling: `\r\n`, `\r`, `\n`, `\r\n\r\n`, `\n\n` and
`a\r\n\r\n\r` all agree.

```sh
printf '\r\n\r\n\n' | txt2html --xhtml --make_anchors \
  --preserve_indent --no-use_mosaic_header --no-titlefirst
# before  port  <body>\n\n\n\n\n</body>    5 newlines
# now     port  <body>\n\n\n</body>       3 -- agrees with the reference
```

A paragraph-boundary accounting bug in the CR path. The paragraph slurper split
on `/\r?\n\r?\n/` but trimmed the final trailing empty field *only when text
followed it*, so a trailing empty field left after consuming the whole input
slipped through and became one extra empty paragraph. Perl's `split()` drops
*every* trailing empty field; `split_blank_lines` now does the same.

It is checked exhaustively against Perl rather than by example: every string of
length ≤ 5 over `{a, space, \r, \n}` — 1 365 inputs — plus explicit repros for the
case above.

The `KNOWN_DIVERGENCES` entry that suppressed this is **deleted, not left in
place**. It was keyed to the symptom, so after the fix it would have hidden a
regression of the same bug. `KNOWN_DIVERGENCES` in `tests/corpus/fuzz.py` is now
empty; any future divergence must be diagnosed, and either fixed or re-added with
a precise signature. The `424242` seed stays in `FUZZ_SEEDS` because it reaches
this input shape — it no longer carries a known divergence, and the Makefile
comment saying otherwise is corrected there.

One test was also changed: it asserted the old behaviour was correct, having been
written by reason rather than measured. Its assertions are now Perl-derived ground
truth.

### Two misdiagnoses worth recording

Both were caught by checking output rather than by reasoning, and both are
reasons not to trust the fuzzer's own first-difference report:

- **The fuzzer reported a lost `<h1>`.** Nothing was lost; the two extra blank
  lines shifted every line number after them, and the report landed on a
  heading. Line-deletion minimisation, rather than the character-shrink
  minimiser, found the real trigger. `KNOWN_DIVERGENCES` now matches E3
  *structurally* — "the port's line list is the reference's with two blank
  lines spliced in" — because the reported line text is not stable across
  inputs, and pinning it would have let genuinely different bugs through.
- **The E2 fix was first written as an early `return`.** That fixed the drop and
  silently skipped `apply_links`, turning one bug into a subtler one. The
  reference is now a fall-through to the common tail.

### Harness changes worth knowing about

- `fuzz.py` runs `scripts/txt2html` on both sides, not the embedded module
  driver. The module driver builds a different configuration for the same
  option values and produced false mismatches; the CLI is the real oracle.
- The fuzzer's domain is ASCII-only, and it strips trailing spaces and tabs
  before line endings. Tabs and CRs are retained. Non-ASCII is excluded because
  Perl is byte-oriented while Rust decodes UTF-8/Latin-1 — that gap is P7, and
  excluding it from the fuzzer would hide it rather than fix it, so it is
  documented rather than silently assumed away.
- A doc comment in `src/lib.rs` was being collected as a doctest and failing
  `cargo test`. Prose in doc comments is executable here.

### Regression coverage added

`table_type_replace`, `table_type_named` and `pre_explicit_blank` are pinned in
`tests/corpus/cases.sh`, with inputs under `tests/corpus/inputs/` referenced by
absolute path. `table_type_named` is a deliberate positive control, so
`table_type_replace` cannot pass by having switched table detection off
wholesale.

A1 added `huge_paragraph`, `huge_paragraph_crlf` and `delim_retry` (the last
from `fuzz.py` seed 99; see A1b). All three are differential against fresh
Perl. The delimiter rewrite is additionally covered by unit tests that compare
the new code path against the *original* patterns directly —
`delim_linear_tests` and `delim_wide_linear_tests`, 9 tests over roughly 200 000
randomised inputs including multi-byte characters. That is the only coverage
that would have caught the three wrong rewrites below, none of which a
byte-comparison test against Perl could explain on its own.

## Compatibility policy — match Perl where Perl is right, and not where it is wrong

Added 2026-09-30, after reviewing the A1 diff. Everything above treats the Perl
module as the specification. That is the right *oracle* for nearly all of it, and
it is how A1, A1b, E1, E2 and the fuzzer's retry bug were found. It is the wrong
*specification* for one large input class, and that was invisible until the
non-ASCII predicate bug in the A1 diff forced the question.

### The finding

On non-ASCII input Perl is the defect, and the reference's own expected output
proves it. `ref/txt2html-3.0/tfiles/utf8.txt` contains `門牌號碼規劃`:

| producer | output for that line |
|---|---|
| `tfiles/good_utf8.html` — the author's expected output | `<p>$ echo 門牌號碼規劃` |
| the port | `<p>$ echo 門牌號碼規劃` — identical to the golden |
| Perl 5.38, 3.0 as shipped | `&eacute;` then raw `96 80`, a spurious `<sup>TM</sup>`, and a truncated `&cent` |

Perl decodes each UTF-8 byte as Latin-1 and entity-escapes the result; byte `0x99`
additionally trips the CP1252 smart-quote path, so it injects `<sup>TM</sup>` into
the middle of a mojibake run. A second probe — a CJK setext heading — gives Perl
`&aelig;&yen;&not;…` plus an `<em>f</em>` where the source contains no markup.

So byte-parity on this class would require us to reproduce corruption and inject
spurious markup. Parity is not merely unhelpful here; as a specification it is
actively harmful, and it would have reported the port's correct behaviour as a bug.

This settles the direction for **genuine UTF-8 input only**. It does not settle
P7, and P7's cases run the other way in places: for a CP1252 file the port emits
C1 control characters where Perl passes the bytes through for a Latin-1 browser,
and under `eight_bit_clean` the port emits `CafÃ©` where Perl is right. Those are
Tier 2 as well, but there the *port* is the one at fault. So Tier 2 means
"establish which side is right, per input class" — not "the port is always
right".

### Three tiers

| Tier | Scope | Rule | Oracle |
|---|---|---|---|
| 1 | ASCII input, documented output format | byte-identical to Perl | `cases.sh` byte diff, `tfiles/good_*.html` goldens |
| 2 | non-ASCII / Unicode, resource limits, error handling | may differ, **must be better** — Perl is the defect | goldens, plus the property suite (P12) |
| 3 | streaming, parallelism, diagnostics, GUI | no Perl analogue; must stand on its own | property suite |

Tier 1 is unchanged and stays strict. Tier 2 is new: a divergence there is not a
bug to be closed by making the port match Perl, and no Tier 2 item may be verified
by byte-comparing against the reference.

### How little of the input space any oracle actually reaches

| Measurement | Result |
|---|---|
| `cases.sh` coverage of upstream `tfiles` | **2 of 65** files |
| upstream non-ASCII files under test | **0 of 4** — `utf8.txt`, `good_utf8.html`, `umlauttest.txt`, `list-styles.txt` |
| non-ASCII characters in 16 000 fuzz cases | **0** — `fuzz.py:310` `sanitise()` rewrites every character `>= 0x80` to `?` |
| non-ASCII characters in our 6 corpus inputs | **0** |
| corpus inputs containing emoji / combining marks | **0 / 0** |
| crate dependencies | `fancy-regex`, `pyo3` — no Unicode capability at all |

`fuzz.py`'s generator alphabet lists `ä ü Ä € 中`, but `sanitise()` replaces every
one of them with `?` before the case runs, so the dynamic oracle is ASCII by
construction. The upstream author wrote tests for exactly the hard classes —
`utf8.txt`, `umlauttest.txt`, `list-styles.txt` — and we reference none of them.

The last row is the deeper problem. The layout heuristics that ought to measure
display width — setext and underline headings, table alignment, heading
tolerances — count characters or bytes. For CJK and emoji that is wrong in the
port *and* in Perl, so byte-parity renders a shared defect invisible. A single
oracle structurally cannot find this class, which is the actual answer to
"is the differential approach myopic": the approach is not, the oracle set is.

### P12. A property suite that does not reference Perl — **done**

The missing capability. Invariants that must hold whatever Perl does, so the tool
can be judged on its own terms:

- **No data loss.** Unescaping the output must recover every input character.
  Catches silent truncation and mangling without needing a reference.
- **Well-formedness.** `--xhtml` output must parse as XML. Perl's CJK output does
  not; this alone would have caught the finding above.
- **Determinism and idempotence.** The same input converts to the same bytes;
  re-converting already-converted output does not compound.
- **Resource bounds.** Cumulative allocation and wall time under fixed budgets.

This is also where A2's regression test belongs, for the reason A2 already
records: a byte-comparison passes while the process leaks, because the output is
correct. Use a counting `#[global_allocator]` in its own test binary rather than
peak RSS — it is deterministic and machine-independent, and it catches both the
leak and the recompile thrash. Keep peak RSS and `valgrind --tool=massif` as
diagnostics, not gates.

**Implemented.** All four properties, in `txt2html-rs/tests/`:

- `proptest.py` — the four Perl-independent properties, plus no-panic on hostile
  input. No new dependencies: Python 3 plus the standard library's `ElementTree`,
  driving the built binary. Run with `make proptest`.
- `alloctest.rs` — a counting global allocator, so retention and churn are
  measured rather than inferred. Run with `make alloctest`.

Two things the implementation settled that the sketch above did not:

- **Budgets must be asserted as marginal cost, not totals.** A total-allocation
  budget is dominated by ordinary churn — 66 MB of allocate-and-free for a
  100 KB document, most of it the long-paragraph path — and would measure that
  instead of the leak. Measuring the *difference* between N and N+16 distinct
  delimiters cancels the converter's fixed overhead and isolates retention.
- **The cache is `thread_local`, so the harness must measure on a fresh thread.**
  Two measurements on one thread share the cache, the second run reuses the
  first's compiled patterns, and the marginal cost reads as zero no matter how
  badly the cache leaks. `retained_for` spawns a thread for this reason; the
  comment in it says so.

Known-open budgets are printed with the owning plan item rather than silenced, in
the same style as `proptest.py`'s A8 handling, and `KNOWN_OPEN` in the test is
the audit list. A *new* exceeded budget has no such record and fails the run. A
gate that is permanently red gets ignored, which is not a gate.

### P13. Decide what "stands on its own" means for packaging — **decided**

_Answered 2026-10-01. The single self-contained artifact wins: the deliverable is
one binary, so the GUI moves off Python + PySide6 onto Rust + Qt. Phase 5 below
is the plan; P5–P11 are sequenced ahead of it._

_Corrected 2026-10-02. The conclusion holds — one artifact, GUI off Python — but
two mechanisms in this answer did not survive contact with the toolkit sizes, and
one of them is a load-bearing claim. "One binary" is not achievable with either
Qt or GTK: the CLI here is 2.4 MB, GTK4 alone is 20 MB of shared library plus
glib, gdk, pango, cairo, gsk and harfbuzz transitively, and Qt Widgets is
comparable. **Flatpak** delivers the single artifact instead, since its runtime
carries the toolkit and every shared library. And **Qt was an assumption, not a
reason**: no Qt is installed on the machine that ran the whole corpus suite, so
the Qt preference rested on a plausible argument about PySide6 semantics that has
never been measured against the GUI suite. Both are recorded rather than
rewritten, because the reasoning trail is the point of this file. See "Licensing
and distribution"._

The question was whether "stands on its own" means one artifact or two. The CLI
is a self-contained Rust binary; the GUI was not — `txt2html-gui` needs a Python
runtime plus PySide6 at run time, and ships as a wheel. Meeting a
single-artifact requirement with the Python wheel means the requirement is not
met, so the shell is rewritten. **The engine is not rewritten.** That distinction
is the whole shape of this item and is recorded here because the plan's original
framing — "Rust + Qt is a different architecture, not a refactor" — invites
reading the entire project as up for replacement. It is not.

| component | lines | fate |
|---|---|---|
| **engine** (`convert.rs`, `links.rs`, `options.rs`, `chars.rs`, `cli.rs`) | **5,644** | **kept** — this is the hard part, and it is already byte-verified against Perl |
| `python.rs` (pyo3 bindings) | 212 | **deleted** — exists only to cross a language boundary |
| `main.rs` (CLI entry point) | 111 | **kept** — the CLI is a second binary on the same crate |
| `lib.rs` (crate root) | 65 | kept, minus the pyo3 module declaration |
| `txt2html-gui` shell (`mainwindow.py`, `optionspanel.py`, `worker.py`, `app.py`, `files.py`) | 1,365 | **rewritten** in Rust (`egui`/`eframe`; Qt was the original assumption — see Phase 6) |
| `txt2html-gui/tests/test_gui.py` | 887 | **the spec**, not the code — roughly half survives as Rust tests |

So the rewrite is 1,365 lines out against 5,644 kept, and the 212-line binding
layer goes with it. The crate is 6,032 lines in total.

**Why the rewrite is cheap, and it is not the reason people expect.** The engine
is where the difficulty was, and it is already paid for. What the rewrite removes
is *duplication that only exists because two languages are involved*:

1. **The encoding rule is currently implemented twice, in two languages.**
   `convert.rs:30` (`read_any_file`) decodes UTF-8 when the bytes are valid UTF-8
   and falls back to Latin-1 otherwise. `files.py:46` (`decode_bytes`) implements
   the same rule in Python, with a docstring that says so outright. A Rust GUI
   calls `read_any_file` and the second implementation is deleted. This is why
   **P7 must land before the rewrite, not after** — see Phase 5.
2. **The pyo3 layer disappears.** Five `#[pyfunction]`s (`convert`,
   `convert_file`, `process_chunk`, `option_specs`, `version`) exist to cross
   into Python. In Rust these are direct calls, and the ~212-line binding file
   goes with them.
3. **`option_specs` becomes a direct read of `cli::SPECS`.** It already reports
   the engine's own `NUMERIC_RANGES`, so a front end cannot offer a value the
   engine will reject. A Qt options panel built from that table is *more*
   correct than the Python one, which reconstructs it across a boundary.

Two consequences worth stating plainly, because both cut against the decision and
both are accepted deliberately:

- **P7 gets fixed by construction rather than patched.** The CP1252 defect — 0x80-0x9F
  bytes becoming C1 control characters in the output — is currently a policy
  split across two languages that *disagree*. Merging them into one removes a
  whole class of bug rather than fixing an instance of it.
- **The GUI's 887-line test suite is the expensive part, and it is being
  rewritten, not inherited.** It is also the most valuable document in the GUI,
  because it is the only place the shell's behaviour is written down. Treat it as
  the port's acceptance criteria (§5.3) rather than as code to translate.

**Resolved 2026-10-02. CLI stays; coexistence during the port; Flatpak packaging.**
The decision above settles the *architecture*; these three settle delivery. All
three were open and are now closed, leaving only the toolkit open:

1. **The CLI stays**, as a second binary linking the same crate, and is
   distributable **independently** of the GUI. This is now settled rather than
   assumed, and it is what makes P11 worth doing — `.txt2htmlrc` support has no
   meaning without a CLI, so P11 would be dead work if the CLI were dropped.
2. **Replace `txt2html-gui`, or sit beside it during the port?** Both, in
   sequence: the Python GUI stays runnable until the Rust one passes the ported
   suite, then it is replaced. Coexistence is not a hedge, it is what makes the
   differential testing in §6.3 step 5 possible at all.
3. **Packaging is Flatpak**, not a bundled binary. See "Licensing and
   distribution" — this supersedes P13's "one self-contained binary" wording.
4. **Qt6 or GTK4** remains open, but is now a much smaller decision: Flatpak
   carries the toolkit and every shared library in its runtime, so neither
   toolkit costs us anything at the artifact level. It is a choice of Flathub
   runtime (both are published) and therefore of which widget model maps more
   cleanly onto PySide6 across the 58-test GUI suite. GTK4 changes the widget
   mapping and nothing in the engine.


### Corrections to the items above

- **A2's original RSS table was wrong, and P12 has now measured it.** The
  30 000-delimiter row (142 440 KB, 13.6 s) cannot come from real input: a
  document can only induce one pattern per distinct delimiter *character*, and
  `convert.rs:2636` reduces that to ~28 reachable delimiters, so no input can
  generate 30 000 patterns. It was a direct-call micro-benchmark. Measured
  retention is 3 919 B per distinct pattern directly and 3 451 B per additional
  delimiter from real input — the leak is real and linear, but total exposure is
  bounded near ~0.3 MB per process rather than growing with file size. Severity
  corrected High → Low; A2 is no longer the most damaging item. Relatedly,
  `fuzz.py`'s `manydelims` fixture generates at most 60 distinct delimiters
  against a cap of 128, so it cannot reach the thrash path either.
- **P7's step 1 cannot be executed as written.** It asks for a UTF-8 fixture with
  wide characters and "assert Rust == Perl"; by the above, that assertion cannot
  hold. Under this policy it becomes a golden assertion instead. P7's remaining
  steps (explicit encoding option, `meta_charset`, GUI write-back) stand.
- **A2's own caveat stands** and is worth repeating: it needs `--make_tables`,
  which is off by default. Do not verify the fix by running the corpus without
  the flag.
- **P23's warning count was 69, not the 74 first written.** The commit message
  for `09d13d9` and the first draft of the P23 item both said 74; measuring
  `cargo clippy --release --all-targets` at the parent commit `81a23f0` gives
  69 (52 in the lib, 17 in tests). The conclusion is unchanged, but the number
  was inflated by counting the two occurrences of a few lints twice, and it is
  corrected here and in the P23 item rather than quietly amended into history.

## Phase 0 — Make the harness trustworthy

Do this before anything else. Two of the three bugs in later phases were
invisible *because* the harness reports false passes, so nothing else can be
validated until this is fixed.

### P1. The corpus can report PASS on a run that crashed — **done**

_Landed 2026-09-30. See §0.1. The `38/40` in §0 is the number this item
produced, kept as the record of the defect; the corpus was `43/43` when this
landed and is `48/48` now._

`tests/corpus/run.sh:7` does `mkdir -p` but never clears `$RUNDIR`, and
`run_case` ignores the Rust binary's exit status (`run.sh:85`). A Rust-side
error leaves the previous run's output in place, and `cmp.py` compares that.

Live proof: `sample` and `xhtml_sample` were reported PASS against output files
timestamped 13:41 from an earlier run, while the binary actually exits 1 with
`Unknown option 'system_link_dict'`. With a clean `RUNDIR`: `PASS=38 FAIL=2`.

Root cause: `cases.sh:82-83,85-86` pass `--system_link_dict` on the CLI side.
Upstream 3.0 dropped that flag from `scripts/txt2html` (it is not in
`init_our_data`); `t/20tfiles.t` still passes it to the *module*, whose `args()`
accepts arbitrary keys. So it belongs in `EXTRA[]` only, never in `CLI[]`.

Fix:

- `run.sh`: `rm -rf "$RUNDIR/ref" "$RUNDIR/mine"` at start-up, and
  `rm -f` the two case outputs inside `run_case`.
- `run.sh`: capture the exit status of the Rust invocation and mark the case
  failed (with the captured stderr) when it is non-zero, even if a stale file
  happens to be present.
- `cases.sh`: drop `--system_link_dict` from `CLI[sample]` and
  `CLI[xhtml_sample]`; keep it in the matching `EXTRA[]` to mirror
  `t/20tfiles.t`.

Note this is the same failure class as the stale-binary guard at
`run.sh:13-19`, whose own comment records that it already bit us once.

### P2. A typo in a case definition silently disabled coverage — **done**

_Landed 2026-09-30. See §0.1._

`cases.sh:6-7` uses `^\d+\.\d+ +\.\w+` and `^\d+\.\d+\.\d+ +\.\w+` where
`ref/txt2html-3.0/t/20tfiles.t:87` uses `^\d+\.\d+\. +\w+` and
`^\d+\.\d+\.\d+\. +\w+`. Wrong patterns, so `1.1.  SCOPE` does not match
heading level 2 and falls through to `caps_tag`, emitting `<strong>` instead of
`<h2>`. The case still passes because the same typo is applied to both sides.

Confirmed: with the typo corrected, the case both matches Perl *and* reproduces
`tfiles/good_custom-headers.html` byte for byte. The engine was always correct.

Fix: correct the four regexps in `cases.sh:6-7`, then add an assertion to the
harness that each case's Rust output equals the matching `tfiles/good_<stem>.html`
where one exists. Comparing against a golden the harness did not itself generate
is what catches an option set that is merely self-consistent.

### P3. Promote the differential fuzzer to a permanent, seeded test — **done**

_Landed 2026-09-30, and it paid for itself immediately by finding E1–E3.
See §0.1._

The 700-case fuzz found no engine bugs, which is itself the most valuable result
in this review — but it only exists in `/tmp`. Move it into
`tests/corpus/fuzz.py` with a fixed seed list, a fixed case count, and a
documented seed corpus (mutations of `tfiles/*.txt` rather than synthetic text,
so failures are reproducible from the repo). Run it in CI with a modest count and
allow a larger count locally.

## Phase 0b — the harness still could not fail (P14–P20)

_Added 2026-10-01. Phase 0 was closed on 2026-09-30 on the belief that the harness
was trustworthy. It was not. Six gates in `make verify` were later found to be
**structurally incapable of reporting failure**, which means every green run
recorded before this section is weaker evidence than it looked._

### The rule this section exists to establish

> **A gate that has never been observed failing is not a gate. It is a script
> that prints reassuring text.**

So the bar for every check in this repository is no longer "does it pass" but
**"show me it failing, on purpose."** A check is not finished until someone has
fed it a deliberately broken input and watched it exit non-zero. That is a cheap
test — it took about twenty minutes to run against the whole gate — and it is the
only thing that distinguishes a check from a decoration.

The general form of the bug is always the same: a status code is discarded on the
way out. A pipe reports its *last* command. A script ends on a successful `echo`.
A counter is printed for humans and never compared to anything. None of these look
wrong when reading the output, which is precisely why reading the output is not
enough.

### What was found

All four defects were found by deliberately breaking each gate in turn and checking
the exit status — not by reading the code looking for bugs, and not by reading any
output.

Provenance matters here, because it says how much of this project's recorded
history to trust:

- **P15 and P17 date from the initial import `3bfc2c1`.** They were present for
  every green figure this repository has ever recorded.
- **P14 arrived with `1035268`** — the commit titled "one verify command". The gate
  shipped structurally incapable of failing in the same commit that made it *the*
  gate.

| # | Item | State |
|---|---|---|
| P14 | `make fuzz` cannot fail — the recipe pipes into `tail -1` | **done** |
| P15 | `run.sh` cannot fail — no exit statement at all | **done** |
| P16 | One hanging converter silently ended a whole fuzz run | **done** |
| P17 | `run.sh <stem>` dies on an unbound `GOLDEN_N` after printing PASS | **done** |
| P18 | Re-establish the fuzz figure on an instrument that can report failure | **done** |
| P19 | Run the eight fuzz seeds concurrently | **done** |
| P20 | Guard `CLI[]`/`EXTRA[]` alignment so a typo cannot silently disable a case | **done** |

### P15. `run.sh` cannot fail — **done**

_The worst of the set, because this is the primary gate._

`run.sh` counted `pass`/`fail` and printed `PASS=46 FAIL=0`, and compared 29
goldens — but it never exited on either. Its only two `exit` statements, present
since the initial import, were the `exit 2` guards on the reference smoke check,
which is exactly why the script *looked* guarded: a reader sees two deliberate
exits and reasonably concludes the failure paths are handled. The case and golden
failure paths simply fell off the end, so the script's status was whatever the
last `echo` returned — 0. `make corpus` runs it, and `make verify` depends on
`make corpus`.

Demonstrated with a stub converter that exits 0 and writes wrong output:

```
PASS=0 FAIL=46
GOLDEN: 0/29 compared, 29 differing
$ echo $?          ->  0
```

**Every case failed and `make corpus` reported success.**

This is the Tier 1 invariant — "59/59 and 33/33", restated in this plan after
almost every item since Phase 0 — and it was not being enforced by anything. It
was being *reported*. Those are different things, and the difference is the whole
of P15.

It is also P1 one level up. P1 made a single crashed case count as a pass; P15
made every crashed case count as a pass **without anyone counting**. The
single-stem entry point (`run.sh <stem>`) had the same defect and is fixed the
same way.

Fix: compute a status and `exit` with it, on both paths. Verified in both
directions — real corpus exits 0 at 48/48 with 33/33 goldens; the stub exits
non-zero at `FAIL=47` (re-measured at the current 48 cases, 2026-10-01);
`run.sh list` alone exits 0 on the real port and non-zero on the stub.

### P14. `make fuzz` cannot fail — **done**

Same shape, one level up again. The recipe ended each seed in `| tail -1` to print
the summary line, and a pipeline reports the status of its *last* command:

```
$ (python3 failing.py; exit 3) | tail -1 ; echo $?   ->  0
```

`fuzz` was the only recipe in `verify` that piped; the others were audited and do
not discard a status. The recipe now captures each seed's output and status
separately, prints the summary line as before, prints the whole log for any seed
that failed, and fails the target if any seed did.

### P16. One hanging converter silently ended a whole fuzz run — **done**

Both converters are invoked with `timeout=120`, but nothing caught
`subprocess.TimeoutExpired`, so it propagated out of the loop and terminated that
seed after however many cases it had reached. This is not hypothetical: **P5, the
inherited hang, was open when this was written** (it is guarded now), so a
timeout was a plausible outcome — and combined with P14 it was reported as
success.

Now caught per case, so one bad input costs one case instead of the remaining
~1 900. Deliberately asymmetric: the **port** hanging is a defect in the port and
fails the run on its own, while the **reference** hanging says nothing about the
port and is skipped like any other reference refusal — but counted, printed, and
included in the exit status rather than folded into a clean-looking
"0 mismatches". Timeouts appear in the summary line too, so an aborted run cannot
be mistaken for a clean one by reading the output either.

### P17. `run.sh <stem>` dies on an unbound `GOLDEN_N` — **done**

`GOLDEN_N` was only initialised in the full-run branch, and `golden_check`
increments it, so a single-stem invocation over any stem that has a golden died on
`set -u` with `GOLDEN_N: unbound variable` — **after** printing `PASS` and
`GOLDEN pass`. A misleading way to fail: the output looks like a pass right up to
the error.

Single-stem mode was therefore unusable for the 28 of 46 cases that have a golden,
and nothing caught it, because no gate invokes it. This is the P2 shape again — a
code path that exists, is correct-looking, and is never executed.

### P18. Re-establish the fuzz figure — **done**

**The plan's old "16 000 cases across 8 seeds, 0 mismatches" was void.** It was
measured on the P14 harness, where an aborted run and a clean run were
indistinguishable from the exit status. It may have been a clean run; it may have
been a seed that died on case 300. There was no way to tell from the record.

Re-measured on the fixed target, 2026-10-01, immediately after P19:

| | |
|---|---|
| seeds × cases | 8 × 2 000 = **16 000** |
| compared | **16 000** — every case genuinely compared, none skipped |
| mismatches / known / skipped / timeouts | **0 / 0 / 0 / 0** |
| wall / CPU | 5m30s / 39m18s |
| end-to-end `make verify` | **OK**, 7m49s |

This is the first figure in the project worth quoting, because the harness's
failure modes are demonstrated rather than assumed: five separate ways to make it
report a false green were injected and each one is caught (see P19, P21).

### P19. Run the eight fuzz seeds concurrently — **done**

The full fuzz was **~99 minutes and single-threaded on a 20-core machine**. That
is why `make verify` was impractical to run, and why P18 was hard to establish —
the gate nobody runs gets no audit. Commit `bd488f7`.

**Measured: 5m30s wall, 39m18s CPU, peak overlap 8 of 8.** A 7.1x ratio of CPU to
wall time is the parallelism being confirmed rather than assumed; the stub trace
agrees. 8 seeds × 2s: 16.35s at `FUZZ_JOBS=1`, 2.25s at `FUZZ_JOBS=8`.

Both hazards this introduced are closed:

- **Aggregate status is still reported, and the obvious way to do it was
  rejected.** Collecting per-seed status through `wait $pid` means a seed that
  dies before it can be waited on *vanishes* from the results, and the remaining
  seven look like a clean sweep. Instead each seed writes a status file, statuses
  are read back in seed order so the report is stable regardless of completion
  order, and **a missing status file is a failure**. So is a log with no `fuzz:`
  summary line — a run that printed nothing has proved nothing.
- **`RUNDIR/fuzz-fail` can no longer collide.** `--keep` wrote
  `fuzz-fail/{name}-{case}` with no seed in it, so two seeds reaching the same
  source at the same case index wrote the same path. Demonstrated: seed 7
  overwrote seed 99's saved input, leaving 3 files where there should have been 6.
  Names are now `{name}-{seed}-{case}`, and `make fuzz` additionally gives each
  seed its own `--fail-dir`.

Verified by breaking it, per the Phase 0b rule. Each of these must fail, and does:
1 bad seed of 8; 3 bad seeds of 8; exit 0 with no summary line; 1 silent seed of 8;
a seed that `SIGKILL`s itself. All-clean still passes.

### P21. The fuzzer counted mismatches but never counted comparisons — **done**

Found while testing P19, and it is the P1 bug again — still live in the P14-fixed
harness. `fuzz.py` counted `mismatches` but had no `compared` counter, so a run in
which the reference refused or hung on **every** case printed `0 mismatches` and
returned 0.

Demonstrated against the previous commit: reference refuses all 5 cases, port is
fine, **return code 0**. A green gate that compared nothing. The module's own
comment at the top records that a wiped `/tmp` already caused exactly this once,
and the fix that time was to keep the reference in the repo — which addressed that
instance, not the shape.

`compared` is now incremented at the point a case is genuinely read and compared,
a run that compared nothing returns 1 with an explicit message, and the summary
line discloses the count. This is also what stops "skip the cases the reference
refuses" from becoming a way to pass: the P1 fix, applied to the fuzzer.

Related, same commit: a **reference** timeout was being counted in the same
`timeouts` total as a port timeout and failed the run via
`return 1 if (mismatches or timeouts)`, despite the code comment saying a port
timeout is the defect. The oracle hanging would have failed the gate and blamed
the port — and would have wasted a 5-minute sweep doing it. Port and reference
timeouts are now counted and printed separately, and only a port timeout fails.

### P22. An invalid user regexp panicked the port — **done**

Found in the `make verify` output while confirming P18. Originally written up as
an undecided judgement call, which was wrong: the tier table settles it (see "How
to complete a task", rule 1).

`convert.rs:209` compiles user-supplied patterns with
`Regex::new(..).unwrap_or_else(|e| panic!(..))` and there is no `catch_unwind`
anywhere in the path. A pattern that does not compile therefore aborts the
process rather than producing a diagnostic:

| | `--custom_heading_regexp 'a('` | exit | output file |
|---|---|---|---|
| **port** | `panicked at convert.rs:209: bad regex "a("` | **101** | **not written** |
| **reference** | `Unmatched ( in regex; marked by <-- HERE in m/a( <-- HERE /` | 0 | written |

So the port is *less* forgiving than the thing it ports: the reference reports
the bad pattern, carries on, and still writes a document. This is
user-reachable through `--custom_heading_regexp` / `-H`, `--pre_regex` /
`--post_regex`, and anything else that reaches `re()`.

It surfaced as a `panicked at src/convert.rs:209` line in the middle of a
**passing** GUI run, which looked alarming and is not a second defect:
`test_gui.py` provokes exactly this on purpose, to exercise the GUI's panic
handler, and asserts that the preview shows "stopped on invalid input" and that
the window stays usable. That handler is the A4 work and it works — pyo3 surfaces
the Rust panic as `txt2html.PanicException`, re-exported through `python.rs`
precisely so a Python caller can catch it.

So the divergence is narrower than it first appears, and the narrowing is the
point: the panic is *handled* at the Python boundary and *unhandled* at the CLI
boundary. A GUI user sees an error message. A shell user gets exit 101 and no
document. An embedder who does not know to catch `PanicException` gets a
`BaseException`.

The fuzzer cannot find this. It samples option *values* from a generator that
produces valid ones, so a malformed pattern never comes up — which is a gap in the
fuzzer's strategy, not a coincidence.

**The decision, per the tier table.** "Error handling" is named explicitly in
Tier 2 — *may differ, **must be better**, Perl is the defect.* So the port does not
have to match the reference here, and the three candidate behaviours rank
cleanly:

| behaviour | verdict |
|---|---|
| panic, exit 101, no output (current) | **worse than Perl** — violates the tier |
| warn and carry on (the reference) | not *worse*, but silently ignores the user's pattern and still writes a document that does not do what was asked |
| validate patterns up front, then fail cleanly with a diagnostic and non-zero exit | **better** — satisfies the tier |

Implemented as specified: **validate every user-supplied pattern before
conversion starts, and report the offending pattern and the parser's complaint,
exiting non-zero without writing a partial file.** The same shape as A3's
`tab_width=0` fix, which is the precedent.

```
$ txt2html --custom_heading_regexp 'a(' --infile in.txt
txt2html: custom_heading_regexp: invalid regular expression "a(":
          Parsing error at position 6: Opening parenthesis without closing parenthesis
exit 1, no output file
```

Three parts to the change, and the third is the one that matters for staying
fixed:

1. **`links::try_compile_pattern`** is now the only place a pattern becomes a
   `Regex`, and both `Options::validate` and `Convert::re` call it. Two orderings
   of "add the flags" and "translate" had coexisted in the crate — `Convert::re`
   prefixed before translating, `links::compile_pattern` translated before
   prefixing — and they happened to agree. Now they cannot stop agreeing.
2. **`Options::user_patterns`** returns the three user-facing options, and
   `validate` compiles each. Both entry points already called `validate`, so the
   CLI (`main.rs:52`) and the bindings (`python.rs:99`, a `ValueError`) are both
   covered by construction.
3. **A test that keeps the list honest.** The list is hand-written, and the
   failure mode of a hand-written list is not being wrong today, it being right
   today and wrong after the next person adds a regexp option. So
   `Options::REGEXP_OPTIONS` is compared against the CLI option table in
   `every_regexp_option_is_validated`, and that test also checks each declared
   option really does reject a bad pattern. Demonstrated: adding a fourth name
   to the table without adding it to `user_patterns` fails two tests.

**One flake found and fixed, in the new tests themselves.** `cliexit.rs` pipes
a document into the child, and a pattern rejected up front means the child exits
before reading it — so the parent's `write_all` got EPIPE and failed the test.
It was intermittent, 1 run in 12, and the first full `make verify` after these
changes was the run that caught it. Worth recording because the interesting part
is *why* it was only 1 in 12: it is a race between the parent's write and the
child's exit, and the child wins more often on an idle machine and less often
under the parallel load `make verify` puts on the box. So a test added under
`make test` alone would have been red roughly one time in twelve, which is
exactly the rate at which a team learns to re-run rather than read. Now 20/20
consecutive runs green, and `cargo fmt` is in the loop because `fmt-check` is
the first thing `verify` does and caught a formatting slip in the fix.

**Also closes P4 part 2.** While testing, a second live panic turned up in a
different code path: a link-dictionary entry in `/pattern/` form
(`/a(/ --> http://example.com/`) aborted at `links.rs:151` with the same exit
101, because it reaches the engine through `LinkParser::add_regexp` rather than
through `Options::validate`. That route is now reported and skipped rather than
being fatal — `Convert::convert_text` returns `String`, so turning it into a
`Result` is a much larger change than the defect, and warn-and-skip is what the
reference does anyway. Narrower than it looks: `add_literal` and `add_glob` both
escape their input before compiling, so `/pattern/` was the only dictionary form
that could fail. `LinkParser::rejected_patterns` records the skips for a front
end to report.

The earlier draft of this item argued that warn-and-skip "keeps the Tier 1 corpus
byte-identical". That was vacuous and is retracted: all five corpus cases that pass
a `custom_heading_regexp` use **valid** patterns, so no Tier 1 case constrains
this behaviour either way. The only thing that would have broken was nothing.

The reference's own version of this is worth recording as a reference defect, not
as a compatibility target — see the tier table note at the top of this document.

> **Note on the 2026-10-01 machine reboot, so nobody re-investigates it.** The
> host rebooted partway through a `make fuzz` run and the run was abandoned. It
> was *not* memory exhaustion in the fuzzer: all eight seeds at 150 cases peaked
> at a flat ~32 MB, and a timeout is CPU-bound rather than memory-bound. The
> cause was external — a person rebooting the machine, or power / screen / lock
> handling. No product defect is implied and none was found. Recorded because the
> obvious hypothesis was wrong, and the expensive move would be to keep hunting a
> memory bug that does not exist.

### P23. The lint gate reported and never stopped anyone — **done**

_Landed 2026-10-01, immediately before P8._

`make clippy` ran `cargo clippy --release --all-targets` with a comment
explaining the choice: *"Warns rather than fails: the crate is not clippy-clean
yet and a lint gate that is always red gets ignored, which is worse than no
gate. Flip to `-- -D warnings` once the cleanup lands."*

The reasoning was sound about the 69 warnings it sat next to and wrong about
what to do with them. An always-red gate gets ignored; a gate that is red
*because of new code* does not, and the only way to tell those apart is to have
cleared the backlog first. Instead the backlog stood at 74 for the life of the
target and every warning since has been a line someone scrolls past.

69 warnings, all fixed:

| lint | count | resolution |
|---|---|---|
| `ptr_arg` (`&mut Vec` → `&mut [_]`) | 15 | slice, where the callee only indexes. **Kept as `&mut Vec`** in 3 places: `mailheader` and `make_aligned_table` clone in and write a whole new `Vec` back, and `liststuff` forwards `indents` to `startlist`, which pushes to it |
| `needless_borrow` | 13 | `&x` → `x` where the callee takes by value |
| `field_reassign_with_default` | 7 | folded into struct literals |
| `collapsible_if` | 5 | merged |
| `needless_borrow`-adjacent (`&*x`, `&format!` arg, `len() > 0`) | 4 + 2 | deref/redundant-ref/length-comparison fixes |
| `if_same_then_else` | 3 | `get_tag` closed the same tag from two conditions; conditions merged with `\|\|`, bodies were already identical |
| `needless_range_loop` | 3 | `mailheader` via `split_last_mut`, table alignment via `find` |
| `manual_strip` / `manual_clamp` / loop counters | 2 each | `strip_prefix`/`strip_suffix`, `clamp`, `enumerate` |
| `unneeded_late_initialization`, `char_comparison`, `match`→`if`, `while_let_loop`, `eq_op`, `redundant_format` | 1–2 each | mechanical |
| **allowed, with the reason inline** | 4 | the three state-machine dispatchers take the reference's own argument list, and boxing `fancy_regex::Error` would change a signature to save 136 bytes on a path that has already failed |

Counts are as clippy 1.98 reported them at `81a23f0`; `cargo clippy --release
--all-targets` on that commit gives 69 (52 in the lib alone, 17 in tests).

**Correction.** The commit message for `09d13d9` and the first draft of this
item both said 74. That was wrong, and it is recorded rather than quietly
corrected: 69 is the measured number. The direction of the claim is unaffected,
but a count that is inflated in the one document whose whole subject is not
believing numbers would be a poor place to be casual.

Those conversions are load-bearing rather than cosmetic — `get_tag` decides which
tags close, `mailheader` decides where the `<br/>`s go — so the differential
corpus is what settles them: 48/48 and 33/33 byte-identical, 61 cargo tests,
16 000 fuzz cases. `not_preceded_by` in particular moved from `chars().next_back()`
to `ends_with`, and it is byte-sensitive because of the non-ASCII delimiter fix
on the branch that carried this work, so it was checked against the reference in
both directions: `_bold_` is marked up, `x_bold_x` is not, `é_bold_` is, and
`ééboldéé` is not. Perl agrees on all four.

The `clippy` target now passes `-- -D warnings`, and a new warning was added
deliberately to confirm the gate is real: the build errors out.

### P20. Guard `CLI[]`/`EXTRA[]` alignment — **done**

`run.sh` iterates `"${!EXTRA[@]}"` and reads `CLI[$stem]`. A stem present in
`CLI[]` but absent from `EXTRA[]` would therefore never run — which is **P2**, the
typo that silently disabled coverage, and nothing prevented a recurrence. The two
arrays are aligned today (46 and 46), but that was a fact about the file, not an
invariant the harness checked.

The guard is `alignment_check` in `run.sh`, called from both the full run and the
single-stem path. Three decisions in it, each of which the "three lines" framing
skipped past:

- **By name, in both directions, not by count.** The two directions are not
  symmetric. `CLI[]`-only is skipped in silence, because a case that does not run
  cannot fail. `EXTRA[]`-only trips `set -u` on `"${CLI[$stem]}"` — loud, but only
  by accident, and the message names a shell variable rather than a case. A
  `comm` diff of the two key sets reports each asymmetry by name, and the count
  comparison the obvious version would use cannot tell those two apart.
- **It aborts the run rather than appending to its output.** The first version
  printed `ALIGN:` lines and then carried on, which produced `PASS=46 FAIL=0` on
  an intentionally broken table — a report that reads as a pass. Since `PASS` is
  the number of cases that *ran*, no count means anything once the tables
  disagree, so the run stops and says so. Continuing also turned one fault into
  two errors, the `set -u` death arriving a few lines below the diagnosis.
- **It also checks `NOGOLDEN[]`.** A `NOGOLDEN` entry is a suppression, and one
  that suppresses nothing is a lie in the file whose entire job is being believed
  — the same failure mode as the fuzzer's `KNOWN_DIVERGENCES`, which this project
  has now removed for exactly that reason. Each entry must name a case that
  exists *and* that has a golden to skip.

Demonstrated, by injecting each fault into `cases.sh`: a `CLI[]`-only case, an
`EXTRA[]`-only case, a `NOGOLDEN` entry naming a nonexistent case, and a
`NOGOLDEN` entry on `ci_dict`, which has no golden. All four fail the gate, the
first two naming the case.

**Cleanup, same commit.** The fuzzer's `KNOWN_DIVERGENCES` suppression table and
its ~90 lines of matching machinery are gone, along with `PERL_DRIVER` — 25 lines
of embedded Perl unreferenced since the initial import. The "reference refused
this option set" skip went with them, and that one was a live false green rather
than dead weight: measured with an injected reference failing on one input size in
four, the old code printed `12 compared, 0 mismatches, 8 skipped` and exited 0,
having discarded 40% of the sweep. `fuzz.py` is 719 lines and now 659.

**One finding from writing the guard, now fixed under A9 — and it was worse
than "no case for `good_empty.html`."** This said the reference tree has 32
goldens, the corpus covers 31, and `good_empty.html` is the missing one.

The corpus *did* have four cases for it: `empty1`–`empty4`, one per
`extract`/`xhtml` combination, which is exactly what `t/20tfiles.t:554,579,604,629`
does. They were reading the wrong file the whole time. Upstream's stems there are
its *output* names — `empty1.html`, `empty2.html` — and `run_case` defaults
`INPUT[stem]` to `"$stem.txt"`, so the cases asked for `tfiles/empty1.txt` through
`tfiles/empty4.txt`, none of which exist. Both converters therefore failed to
read their input, each wrote 0 bytes, and `cmp.py` compared two empty files and
reported PASS. All four had been green since the corpus was imported, testing
nothing, and `good_empty.html` read as uncovered because the case that should
have covered it never read anything.

The reasoning that "the port already agrees on an empty file, so a case would
pass today" was the reason it stayed invisible: it was true, and it was tested
against the *wrong input*.

A9 is what surfaced it. An unreadable input now exits non-zero, so four cases
that had been quietly vacuous started failing loudly with
`port exited 1: Could not open tfiles/empty1.txt`. `INPUT[empty*]` now points at
`tfiles/empty.txt`, and a new `GOLDEN[stem]` override points all four at the
golden they were always meant to be scored against — 29/29 becomes 33/33, and the
32-golden coverage is now complete. `golden_check` fails rather than skipping
when an explicitly named golden does not exist, since a wrong `GOLDEN[]` entry
would otherwise report itself as a case that has no golden.

Two reporting defects found while confirming it, both the same shape as the bug
they were hiding: a case whose converter errored printed `PASS` from the byte
comparison *and* the `ERROR` above it, and `good_empty.html` being 0 bytes meant
`GOLDEN pass` too. A failed run matching an empty golden is a green that means
nothing. A case that errored now says so and is not scored either way.

### What this section does not claim

The engine is unaffected by all of this. None of P14–P17 is a product defect, and
none of them changed a byte of output. They are defects in the instrument, which
is the harder class to see, because the instrument is what you trust when the
product looks fine.

## Phase 1 — Turn crashes into errors

### P4. Invalid user-supplied regexps panic — **done, as part of P22**

Originally six panic sites: `links.rs:144`, `links.rs:168`, `links.rs:184`
(`.expect("valid pattern")`), `convert.rs:164` and `convert.rs:174`
(`panic!("bad regex {pat:?}")`), and `convert.rs:2323` (`table_re`).

One character of typo in a link-dictionary pattern or in
`custom_heading_regexp` — which the GUI exposes as a free-text list at
`optionspanel.py:135` — aborted the CLI with `exit 101` and a Rust backtrace
note. Perl prints `Unmatched ( in regex; marked by <-- HERE in m/a( <-- HERE /
at .../TextToHTML.pm line 4298` and carries on.

In the GUI it did not crash (`worker.py:51` catches it) but the user saw a
`PanicException` in the preview, and the raw `thread '<unnamed>' panicked` line
still reached stderr.

This was written as three parts, and the third turned out to be unnecessary
once the first was done properly. The parts:

1. ~~Add `links::validate_pattern`~~ — done as `links::try_compile_pattern`,
   which returns the engine's own `fancy_regex::Error` rather than a `String`,
   so the parser's wording and position are preserved instead of paraphrased.
   It is now the only function in the crate that turns a pattern into a `Regex`.
2. **Validate up front, once.** Done for the three option-level patterns in
   `Options::validate`, which both entry points already call, so the CLI and the
   bindings are covered by construction. Done for dictionary patterns in
   `LinkParser::add_regexp`, which is now the only dictionary form that reaches
   the engine verbatim: `add_literal` and `add_glob` both escape first. A bad
   `/pattern/` entry is reported and skipped rather than being fatal, because
   `Convert::convert_text` returns `String`; see P22 for the reasoning.
3. **Replace the remaining `expect`/`panic!` with skip-and-warn.** *Not done, on
   purpose.* What remains is `links.rs:151` (`compile_pattern`, a thin wrapper
   over `try_compile_pattern` for callers that have already validated),
   `links.rs:169` and `links.rs:205` (`ascii_re`, `ascii_re_cached`), and
   `convert.rs:214` and `convert.rs:224` (`Convert::re`, `re_i`). Every pattern
   reaching any of them is a literal in the source or an internal invariant now
   guarded by the list in `Options::REGEXP_OPTIONS`, and a panic is the right
   answer to "a literal in this crate does not compile" — that is a bug in the
   port, and turning it into a warning would let the port quietly do less than
   its own source says it should. Converting them to warnings would also have
   meant deciding what the converter should do when its table-detection pattern
   fails, and the answer is "nothing sensible".

Checked rather than assumed, since the argument above is the kind that ages
badly: the one remaining site that builds a pattern out of *document* text is
`convert.rs:2650`, where a table delimiter from the input is interpolated into a
character class. The delimiter is captured one character at a time
(`table_re(r"^\s*([^A-Za-z0-9])")`) and stripped of `^ [ ] \` before use, so no
input can close the class. A sweep of 36 delimiter characters through the table
path produced no panic, and no fuzz case has ever hit one.

Regression tests, one per reachable route: `no_regexp_option_can_crash_the_process`
and `a_bad_pattern_is_a_clean_error_naming_the_option_and_the_pattern` (CLI),
`a_rejected_pattern_writes_no_output`, `every_regexp_option_is_validated` (the
guard on the guard), and `a_bad_link_dictionary_pattern_is_reported_and_skipped`.
The GUI test was rewritten rather than deleted, and the rewrite is the more
interesting part: it used to provoke a real panic, and its own docstring had
predicted that it would have to be given an injected fault once the last
reachable panic closed. It now raises `PanicException` directly to test the
worker handler, with a second test asserting the new clean-error wording —
because the thing A4 verifies is the handler, and reaching for a new crashing
input each time a fix lands meant the test was really verifying whichever defect
happened to be open that week.

### P5. Guard the inherited hang — **done**

A `/|.../`-delimited dictionary pattern (empty leading alternation, e.g.
`/|Perl\b/ -> http://x/`) loops forever. **Verified: this hangs upstream Perl
identically** — it is an upstream pathology, not a port defect, so the corpus
will never catch it.

It is still worth guarding, because a hang is the worst failure mode for a GUI
and for a CLI in a pipeline. Reject a pattern whose compiled form can match
empty at every position, and report it as a bad dictionary entry (reusing the
P4 diagnostics). Correct usage is the `|...|` form, which is already handled
and verified identical to Perl for glob, literal, `-o`, `-i`, `-h` and `$1`
templates.

**Implemented** in `links.rs` as `can_match_empty`, called from `add_regexp`
alongside the P4 compile check, so a rejected entry reuses the existing
`rejected_patterns` channel and the rest of the dictionary still loads.

Two decisions worth recording, both measured rather than assumed:

- **The criterion is `re.is_match("")`, not inspection of the compiled form.**
  `translate_pattern` rewrites `\b` into a zero-width lookaround *alternation*
  (`(?:(?<=[A-Za-z0-9_])(?![A-Za-z0-9_])|(?<![A-Za-z0-9_])(?=[A-Za-z0-9_]))`),
  so a `*` glob (`\b.*\b`) looks empty-capable in the source text and is not.
  Reasoning from the pattern text would have rejected legitimate globs and
  dropped links the reference produces. All 65 loaded system-dictionary rules
  load and none match empty; a test asserts that against the real load path,
  not a hand-picked sample.
- **`-o` and `-s` are deliberately *not* guarded.** They substitute at most once
  per paragraph or per section, so an empty-matching pattern terminates there,
  and Perl accepts it. Guarding them would have been a Tier 1 byte-parity
  divergence for no benefit: `/|x/ -o-> url` emits the same empty anchor in both.
  A test pins that, since it is the kind of narrowing that later looks like an
  oversight.

6 tests in `tests/linktest.rs` (95 → 101). Verified end to end that the plain
and `-i` cases now exit 0, that a dictionary mixing good and bad entries still
links the good ones, and that the 16 000-case fuzzer and 59-case corpus are
unaffected — the oracle hangs on this input, so nothing but a direct assertion
could have covered it.

## Phase 2 — Performance

### P6. The port is ~2x slower than the Perl it replaces — **done: the prefilter was silently off for `\b` rules**

2 MB link-dense document, release build: Rust 2.87 s, Perl 1.47 s. The original
figure could not be reproduced from the tree — the 2 MB fixture was not kept, and
`big_para.txt` contains no links at all, so it cannot exercise the link phase. A
deterministic generator was written to rebuild an equivalent document (4 945
paragraphs, 60 432 link-ish tokens); its output is byte-identical to Perl, so no
measurement below can be explained by doing less work.

**Measured phase breakdown** (`--no_make_links` isolates the link path):

| Phase | Rust | Perl |
|---|---|---|
| link processing | 2.70 s | 1.41 s |
| everything else | 0.17 s | 0.06 s |

The link phase is **~94%** of Rust's runtime on this fixture, which is link-dense
by design. **This figure does not generalise, and an earlier draft of this plan
presented it as if it did.** Direct phase timing on the 2 MB dense fixture measures
the link pass at 0.649 s of roughly 4.3 s total, and on the 742 KB sparse fixture
at 0.172 s of about 1.07 s -- i.e. **15% and 16% of wall-clock, not 94%**. The
difference is the fixture: this 500 KB document has 60 432 link-ish tokens in
4 945 paragraphs, whereas the other two are sparse. The 94% number is a property of
this input, not of the engine, and the honest general statement is that the link
phase is 15-94% of runtime depending on link density.

**The two prescribed fixes were implemented, and they are worth ~0% of wall-clock.**

- `links.rs` — dropped `let cur = para_ref.clone()` from the innermost match loop.
  Confirmed it was never needed for borrow-checking: `split_front` returns owned
  `String`s, so the borrow ends before the reassignment.
- `links.rs` — guarded all three `*para_ref = format!("{line_with_links}{para_ref}")`
  sites with `if !line_with_links.is_empty()`.

Output stays byte-identical to Perl (corpus 59/59, goldens 33/33, 16 000 fuzz
cases). They do cut the link pass from **11 602 to 5 510 bytes/paragraph, a 52%
reduction in allocation**, so they are a real improvement and are kept. But
measured wall-clock, pinned to one core, 15 reps of a 500 KB document, min-of-N:

```
baseline 0.720s   fixed 0.720s   delta +0.0%
```

**The plan's diagnosis was wrong.** Neither the clone nor the no-op `format!` was
on the critical path. Instrumenting the pass shows where the time actually goes:
**~321 000 regex invocations** for 65 loaded rules × 4 945 paragraphs, of
which the large majority match nothing, and raw `is_match` over the same pairs alone costs 1.33 s of the
2.70 s link phase. The bottleneck is `fancy_regex` being a backtracking engine
where Perl uses PCRE, which prefilters internally.

**Two things made the original numbers hard to act on, and both are now fixed.**
The benchmark host has background load — a desktop greeter measured at 32% CPU,
load average reaching 7.6 — which made every timing arm bimodal and produced
successive "improvements" of -3% and +11% for the *same* binary. All timings are
now A/B-interleaved with min-of-N, and the primary regression guard is the
allocation budget rather than a wall-clock threshold, because allocation counts are
deterministic and machine-independent.

**What actually closes the gap** is a required-literal prefilter: skip the
rule/paragraph pairs that cannot match before invoking the regex engine.

**Now implemented** (this section's follow-up, landed as a separate commit with
its own soundness argument). `src/prefilter.rs` extracts a literal that every
match must contain, from the HIR that `regex-syntax` parses out of the *translated*
pattern -- the same string handed to `fancy_regex`, so the analysis describes the
regex that actually runs. `regex-automata`'s `from_hir_prefix` was not usable:
it misses exactly the highest-value rules, the ~19 newsgroup patterns whose
literal sits inside after a leading character class.

Coverage on the shipped dictionary, measured through the loader
(`cargo run --release --example count_rules -- doc/txt2html.dict`): of **65 loaded
rules, 56 get a literal and 9 do not**, with zero patterns rejected at load. The
nine are cases where the only literal is separated from its anchor by a
construct the analysis cannot cross, so nothing is provably required:

```
\bSeth\ Golub\b                      \btxt2html\b
\b([[:alpha:]][\w])*ftp[\w]*(\.[\w+\-]+){2,}
\b([[:alpha:]][\w])*www[\w]*(\.[\w+\-]+){2,}
\bKathryn\ Andersen\b                \bHTML\:\:TextToHTML\b
\bhypertoc\b                         \bHTML\:\:GenToc\b
RFC ?(\d+)
```

The two `[[:alpha:]]`-prefixed host rules are the costly misses, not the trivia
like `\bhypertoc\b`. It is a byte scan rather than a regex call, and the extractor
returns `None` -- always run the regex -- whenever it cannot *prove* a literal, so
the failure direction is wasted time, never a lost link.

**A note on numbers here, because two earlier drafts of this section disagreed.**
An intermediate measurement reported "38 of 41 rules" against a 41-rule subset.
That was a real measurement of a real subset, not of the shipped dictionary: the
loader produces 65 rules (54 rule lines, plus rules added by continuation-line
joining and by the built-in URL group), and an earlier figure of "52 rules" was a
line count of the dict file rather than a count of loaded rules. The 65/56 figures
above are what `load_links` actually returns. The runtime table is unaffected --
it was measured against whatever the loader produced at the time, and the loader
has not changed since.

Measured effect, and it is small:

| benchmark | prefilter off | prefilter on | change |
|---|---|---|---|
| link pass, sparse (50 links / 4 945 paras) | 0.202s | 0.172s | −15% |
| link pass, dense (4 945 links) | 0.680s | 0.649s | −5% |

92.5% of the 202 745 rule/paragraph pairs on the sparse fixture are now rejected
without touching the regex engine, yet the link pass only gets 15% faster. The
reason is that a rejection still costs a `memmem` search over the paragraph, and
`fancy_regex` already delegates non-lookaround patterns to the **linear `regex`
crate**, which does its own SIMD literal prefiltering. The two filters are doing
each other's work. So the honest conclusion stands: **P6's goal is still not met**,
and the remaining gap is not the absence of a prefilter but `fancy_regex`'s
backtracking engine for the patterns that cannot be delegated.

One implementation note that cost a measurement: the first version used a naive
`windows().any()` scan and was *slower* than no prefilter at all. The literals are
searched with `memchr::memmem::Finder`s built once at dictionary-load time;
that turned a regression into the table above.

**Follow-up, 2026-10-03: the prefilter was not running at all for the `\b`
family, and that was the real gap.** The conclusion above blamed `fancy_regex`'s
backtracking engine for the nine rules with no literal. That blamed the wrong
thing: the reason those nine had no literal was mechanical. `translate_pattern`
rewrites `\b`/`\B` into look-around, and `\b`-wrapped patterns are exactly what
`add_literal` and `glob2regexp` emit, so `regex-syntax::parse` failed on the
translated pattern and `required_literal` returned `None` for the *whole* family
-- all six `\b<literal>\b` entries plus the two `[[:alpha:]]` host rules. The
`(?i)` rules failed for a second reason: under a case-insensitive flag
`regex-syntax` folds a literal like `RFC` into a class, which proves nothing.
Either way the extractor gave up, and every paragraph paid for a backtracking
`captures()` on rules whose literal is almost never present. That is why the
earlier prefilter only bought 5–15%: it was filtering the rules that were
*already* delegating to the linear `regex` engine, and doing nothing for the
rules that were not.

`add_rule` now falls back to the **original** pattern when the translated one
will not parse. That is sound for the same one-directional reason the whole
module is: translation only rewrites zero-width anchors and escape classes,
never a literal run, so a literal proven from the original is still required by
the translated regex, and both sides are lowercased, so the fallback can only
accept more often. `prefilter_rejection_implies_no_match_for_shipped_rules`
runs the exact production `may_match` path against a battery of haystacks for
every shipped rule; `the_shipped_dictionary_is_fully_prefiltered` pins coverage
at 52/52 (it was 43/52).

Measured on a reproducible 2 MB link-dense document -- the 200-byte unit
`See http://example.com/path?q=1 and mail foo@bar.example and news
comp.lang.rust plus www.example.org/foo and ftp.host.example/pub for more. Also
visit <http://site.example/x> or alt.test.example today.` repeated with a blank
line after each repetition (9 901 paragraphs) -- release build, min of runs:

| build | link-dense 2 MB, 9 901 paras | Perl |
|---|---|---|
| prefilter fallback off | 3.75 s | 1.96 s |
| prefilter fallback on | **1.27 s** | 1.96 s |

That is **2.95×** from this change alone, byte-identical to Perl, and it moves
Rust from ~1.9× slower than Perl to ~1.5× *faster*. On a single huge paragraph
(Rust 11.1 s, Perl 23.4 s) both implementations are dominated by the O(n²)
paragraph rewrite and the fallback is worth ~1%, so no measured input class is
still slower than the reference. **P6's goal is met.** The residual cost is
still `fancy_regex`'s backtracking VM whenever a `\b` rule's literal *is*
present, but that is now the rare case the prefilter cannot avoid, not every
paragraph.

Practical impact: the ~3× link-pass improvement pushes the point at which the
GUI's 300 ms debounce (`AUTO_CONVERT_DELAY`) is exceeded well past the ~180 KB
first measured (not re-measured here). The single giant-paragraph case is still
O(n²) in the paragraph rewrite, so live preview on one huge paragraph remains
the thing to fix next if it matters; on ordinary multi-paragraph documents the
fallback removes the stall.

## Phase 3 — Encoding policy

### P7. The documented deviation is narrower than the real one, and there is no charset — **done**

**The root cause was worse than this section assumed, and it was in the engine
rather than in the documentation.** `read_any_file`'s fallback decoded as
**Latin-1**, while `demoronize_char`'s table (`chars.rs:53-70`, not `22-38` —
the P4 escaping work had shifted it) is keyed on the **CP1252** code points.
Those two facts cancel out: a Latin-1 decode of `0x93` produces U+0093, which is
not in the table, so **every substitution `demoronize` performs silently did
nothing on exactly the files it exists to serve**, and the C1 control character
was re-emitted as UTF-8 `c2 93` where the file said `93`. The two encodings
differ only on `0x80`-`0x9F` — the whole of the rest of Latin-1 is identical to
CP1252 — and no fixture had a byte in that range, so nothing was red.

Measured, on the same `0x93 0x94 0x96 0x97` file the original analysis used:

| | before | after |
|---|---|---|
| `0x93` in the editor and the output | U+0093, invisible | `"` |
| `demoronize` | never fired | fires |
| `resolved_encoding()` | did not exist | `cp1252` |

Nothing about the divergence is now a byte-difference-only question, so the
corpus cases below are declared Tier 2 divergences with unit tests as the
oracle. Details of what landed, in the order the section asked for it:

1. **`read_any_file` decodes CP1252, not Latin-1** (`convert.rs`). CP1252's five
   undefined bytes (`81`, `8D`, `8F`, `90`, `9D`) stay Latin-1 control
   characters so the decode is total, which is what browsers do and what
   Python's own `cp1252` codec will not do.
2. **Two corpus fixtures**, `cp1252_smart` and `cjk_table`, both recorded as
   `differential must fail:` with the reason inline. `cjk_table` needed its rows
   padded to a uniform **byte** length, because `byte_slice` cuts cells by byte
   offset — the plan's claim that "the byte/char column logic is sound" is right,
   but only for rows of equal byte length, which nothing had ever tested.
3. **`tests/encodingtest.rs`**, 17 tests built from raw byte literals. A literal
   `\u{201c}` in a test would pass under either decoder and prove nothing; each
   one was checked by reverting the fix and watching it go red.
4. **`lib.rs` wording corrected** to state the real scope, including that the
   divergence is visible for *CP1252 input* and not only UTF-8 input.
5. **`--encoding auto|utf-8|cp1252`** (`Encoding` in `options.rs`). `auto` is
   the default and the probe; the forced values exist because a short CP1252
   document can be valid UTF-8 by accident, and no amount of probing settles
   that. Reported by `Converter::resolved_encoding()` and to Python as
   `file_encoding()`.
6. **`--meta_charset`**, default **off** so no golden moves, on by default in the
   GUI where a browser is the consumer. It follows `lower_case_tags`, and the
   newline belongs to the element rather than the option so the generator meta
   keeps exactly the single trailing newline it always had.
7. **GUI write-back preserves the encoding** — and this turned out to be the
   part that mattered most, because the GUI had its *own* Latin-1 decode. The
   editor was showing U+0093 while the preview showed `"`: the user was editing
   one document and looking at another. `files.py` now mirrors the engine, with
   its own CP1252 table for the reason in (1), and refuses to write a character
   CP1252 cannot hold rather than substituting U+FFFD and losing the text.

One A5 test asserted the Latin-1 reading and its comment defended the wrong
answer at length — "the file says a control character and the file must keep
saying so". True of Latin-1, and beside the point, since the converter decodes
CP1252. It was rewritten to assert the agreement instead. Recording that
because the comment was the kind that survives review: it was confident,
specific, and wrong, and only measuring the converter settled it.

### P7.4. The detection *order* was wrong, not just the fallback — **done**

**P7.1-3 fixed the fallback and called the encoding question settled. It was
settled in the wrong place.** Checking UTF-8 validity first means the fallback is
only reached by files that fail it — so the fallback is reached by *files that
could not be anything else*, and every file that could be either is decided
without ever looking at the evidence. Two consequences, both found by measuring
rather than by reasoning:

**A BOM was being discarded as a decode error.** `FF FE` is a specification-level
guarantee of UTF-16LE. It fails the UTF-8 probe, falls to the CP1252 fallback,
and those two bytes have no CP1252 meaning at all, so a BOM'd UTF-16 file opened
as `&yuml;&thorn;Hello`. Treating a *declaration* as an error is the opposite of
what a declaration is for.

**BOM-less UTF-16 was passing as UTF-8.** This is the one that mattered. ASCII
prose in UTF-16LE is `H\0e\0l\0l\0o\0` — every byte below `0x80`, so the file is
*valid UTF-8* and the probe accepts it. The output carried a NUL between every
character, in the engine and in the editor alike. There is no fix for this at
this layer: a UTF-8 validity check cannot distinguish "UTF-8" from "UTF-16LE of
ASCII", because they are the same byte sequence. The only discriminator is the
NUL *pattern*, so it has to be consulted before validity, not after.

Measured across the fixture matrix before any code changed:

| input | Perl | port (P7.3) | verdict |
|---|---|---|---|
| Cyrillic/Greek UTF-8 | `&ETH;&Ntilde;&ETH;…` | `Привет, мир!` | port better; Perl mangles each byte |
| CP1251 / KOI8-R / CP1253 | mojibake | mojibake, **byte-identical to Perl** | equally wrong; not detectable |
| UTF-16LE ASCII, no BOM | — | `H<NUL>e<NUL>l<NUL>…` | visibly broken |
| UTF-16LE with BOM | — | `&yuml;&thorn;H<NUL>e<NUL>…` | visibly broken |

The second row is the one worth internalising: **P7.3 did not make Cyrillic
better, and no amount of work on the fallback could have.** CP1251, KOI8-R and
CP1253 are mutually indistinguishable in the bytes — a CP1251 file is a valid
CP1252 file with different meanings for ~60 of its 128 high bytes. Any "fix"
that claims otherwise is guessing, and CP1251 vs KOI8-R in particular disagree
about nearly *every* high byte, so a wrong guess between them is not a small
error.

**What landed.** `Encoding::Auto` is now ordered by *kind of evidence*, and the
order is the whole design:

1. **BOM** — a declaration. UTF-32LE is checked before UTF-16LE because its mark
   starts with UTF-16LE's, and reading it the short way round yields pairs of
   Latin-1 characters: a wrong answer that still looks like text.
2. **UTF-16 NUL alignment** — a structural fact. Threshold **1/8**, measured
   rather than guessed: positives run 0.15 (dense Cyrillic) to 1.00 (ASCII
   prose), while everything that must stay UTF-8 tops out at 0.04. An earlier 2/3
   threshold passed the English rows and failed Russian and Greek — it would have
   left the interesting input broken while the easy cases looked fine.
3. **UTF-8 validity.**
4. **CP1252** — the only *guess* in the list, and the only thing the earlier
   version was really about.

`--encoding` grew the encodings detection cannot supply: `iso-8859-1`, `cp1251`,
`cp1253`, `koi8-r`, `utf-16le/be`, `utf-32le/be`. Two consequences worth naming.
**`latin-1` no longer means CP1252** — before P7.4 it was an accepted alias, which
was not a harmless spelling: a Latin-1 file's `0x93` *is* a C1 control, and a
user asking for Latin-1 was asking for the C1 control. And **`Encoding` is the
user's choice, deliberately**, for the cases the probe cannot reach.

Output remains UTF-8 in every case; `--meta_charset` only declares that, and it
still does not touch input decoding.

**`files.py` no longer hand-writes its tables.** P7.1 wrote out a CP1252 table
so the two implementations could be diffed; with five encodings × 128 high bytes
that stops being reviewable, and a transcribed table is a mojibake bug showing up
for one character in one encoding — the hardest kind to notice. The GUI derives
them from Python's own codecs at import, the Rust tables are generated from the
same codecs by `tests/gen_encoding_tables.py`, and both are checked against that
reference, so the two implementations agree *by construction* rather than by
proofreading. The one thing the shared reference cannot express — an *undefined*
byte — each side falls back to the Latin-1 C1 reading for, which is what keeps a
CP1251 file's `0x98` and CP1253's 17 holes lossless and round-trippable.

**Deliberate limits, pinned by tests rather than left to be rediscovered:**

- BOM-less UTF-16 with very little ASCII in it is **not** detectable. Pure
  Cyrillic in UTF-16LE is `04 xx` per unit: no NULs, no evidence. `--encoding
  utf-16le` recovers it. What decides detection is ASCII *density*, not script —
  `"Привет, мир!"` is detected, `"Привет"` is not.
- BOM-less **UTF-32** is not inferred at all, though its three NULs per unit are
  distinctive: a document with no NULs is ambiguous in a way that does not
  resolve. UTF-32 needs its BOM, or the flag.
- A UTF-8 BOM is **kept** in the text (the reference keeps it, so stripping it
  would make the preview differ from the saved file); wide-encoding BOMs are
  consumed, because they are metadata.
- A wide encoding is written back **with** a BOM even if it was read without one.
  Two bytes of declaration against carrying "this file had no BOM" as state that
  the GUI's `self.encoding` string has nowhere to put.

Six corpus cases and one unit-test oracle: `utf16le_ascii`, `utf16be_ascii` and
`utf16le_bom` are declared divergences; `cp1251_cyrillic`, `koi8r_cyrillic` and
`cp1253_greek` are registered **twice** — once under the default, where port and
reference mangle identically and the case is a differential **PASS**, and once
with `--encoding`, which is the declared divergence. Declaring the default half
as `NOGOLDEN` was the first attempt and the runner rejected it ("matches the
reference; the declared divergence is gone"). The runner was right; a declared
divergence that does not diverge is a lie that outlives its fix.

34 encoding tests (17 new), each verified by sabotage: reverting the NUL sniff to
P7.3's order fails 7, swapping the UTF-32LE BOM check fails 3, removing BOM
handling fails 3.

#### Deferred: single-byte charset *detection* (Part B)

Option A above is what shipped. Option B — scoring candidate decodes for
CP1251/KOI8-R/CP1253/Turkish and picking statistically — is **not** in this
change, and the reason is the second row of the table above rather than a
scheduling preference.

Charset detection is a research-grade problem with a long tail of false
positives, and the failure mode is asymmetric in the wrong direction: a *wrong
confident guess* is worse than the current mojibake, because the mojibake is
obviously broken and therefore trivially correctable with `--encoding`, while a
detector that silently renders CP1251 bytes as KOI8-R gives the user a
well-formed document full of plausible wrong letters, which is a document people
do not check. The bar for shipping a guess is therefore not "better on average" —
it is "never confidently wrong", which single-byte detection does not meet for
short files.

Cost, for whoever picks this up: it is a separate project, not a cleanup. It needs
a per-language byte-frequency model, a confidence threshold with an explicit
"unknown" outcome that must be reachable, an evaluation corpus with known
encodings, and a decision about whether a guess may ever override the user's flag.
None of that belongs inside a fix for "UTF-16 files emit NULs", and none of it
should be added by someone who is trying to close that bug.

Phase 6 note: this is one of the few places where a GUI is *better* than a CLI —
a dialog can ask, and can show the candidate encodings with a preview of what each
would produce. That is an argument for doing it after the rewrite, not for
skipping it.

The original analysis is preserved below because its *measurements* were right;
only its attribution of the cause was wrong.

`lib.rs:23-31` states the difference is visible "only for UTF-8 input containing
characters whose encoding has a byte in the `0x80`-`0x9F` range". That is
inaccurate. It is visible for **any** input containing bytes `0x80`-`0x9F`,
which includes every CP1252/Latin-1 file written on Windows — the reason
`demoronize` exists in the first place.

Measured on a CP1252 file containing `0x93 0x94 0x97 0x96`:

- Perl emits the raw bytes; a browser defaulting to Latin-1/CP1252 renders
  `" " – —`.
- The port decodes Latin-1 to U+0093 etc., leaves them alone (`chars.rs:22-38`
  maps the *CP1252 code points* U+201C etc., not U+0093), and re-encodes as
  UTF-8 `C2 93` — C1 control characters in the output.

And the document carries no charset declaration: `do_file_start`
(`convert.rs:2076-2139`) emits only `<title>` and
`<meta name="generator">`, so the browser guesses. Same story for
`eight_bit_clean`, where the reference passes Latin-1 bytes through and the
port emits `CafÃ©`.

Confirmed improvements in the same area, currently untested and undocumented: a
CJK table (`convert.rs:845-866`, `byte_slice` at `convert.rs:2460`) slices
columns by byte offset and the port emits correct `日本` where Perl mangles it
into `&aelig;…`. The byte/char column logic is sound — it is panic-safe because
`starts[col] <= min <= row.len()` by construction. Worth locking in with tests
before anything touches it.

Corpus gap: no case has non-UTF-8 bytes in `0x80`-`0x9F`. `list-styles.txt` and
`umlauttest.txt` are Latin-1 but contain only `>= 0xA0`; `utf8.txt` is valid
UTF-8. So the exact range where the port diverges is untested.

Fix, in order:

1. Add two corpus fixtures: a CP1252 file with `0x80`-`0x9F` bytes, and a UTF-8
   file with wide characters used in a table. Assert Rust == Perl, and record
   the expected difference in the README rather than hiding it.
2. Correct the `lib.rs:23-31` wording to state the real scope.
3. Add an explicit encoding option (`auto` / `utf-8` / `latin-1`, default
   `auto` = current behaviour) and record the resolved encoding on the
   converter, so the CLI and the GUI can report it.
4. Add an opt-in `meta_charset` option (default **off**, so the goldens still
   pass) that emits `<meta charset="utf-8">` next to the generator meta at
   `convert.rs:2126-2139`. Off by default because byte-identical output is a
   stated design goal; on by default in the GUI, where a browser is the consumer.
5. GUI: `files.py:38` writes UTF-8 unconditionally, so **Save text…** on a
   Latin-1 file silently transcodes it. Remember the encoding the file was read
   with and write it back in the same one.

Note on the deliberate deviations already documented in `lib.rs:16-31`
(`instring`, no `inhandle`, `read_any_file`): those are sound decisions. This
phase is about making the third one *honest and complete*, not reversing it.

## Phase 4 — Housekeeping and CI

### P8. No CI anywhere — **done**

_Landed 2026-10-01 (`.github/workflows/ci.yml`)._

Given that the harness produced two false passes, this is the highest-value
remaining item. A single workflow runs, on every push:

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --release` — the `clippy` target warned rather than failed until
  the 69 warnings behind it were cleared (P23); it now stops the build.
- the corpus with a **clean** `RUNDIR` (Phase 0 makes this meaningful), plus
  the golden assertions from P2
- the fuzzer from P3 with a fixed seed
- `QT_QPA_PLATFORM=offscreen python -m unittest discover -s tests` with
  `T2H_TFILES` pointed at the reference corpus
- upstream's own `t/*.t` as a canary, so the reference does not drift

It is three jobs rather than one, so a lint failure reports in under a minute
instead of after 16 000 fuzz cases. The reference is built by `make ref` from
the tracked tarball, so a run needs no network and no CPAN and every runner
compares against the same bytes.

Two details were wrong until they were run locally rather than reasoned about.
The canary calls `prove t/` directly, not through a make target, so nothing
exports `PERL5LIB`; without it the canary fails with "Bad plan. You planned 16
tests but ran 0" — a differential gate quietly comparing against a reference
that cannot load. And `T2H_TFILES` is load-bearing: with it set the
reference-golden GUI test runs and passes, without it the test skips itself and
the run is still green.

The gate was then made to fail, each class observed rather than assumed: a new
clippy warning, a one-character output divergence (`&amp;` → `&AMP;`, which
turns 3 corpus cases and 3 goldens red and is caught by the fuzzer with 9
mismatches in 60 cases), and a stale `MINE` (47 of 48 fail). A gate that has
never been observed failing is not a gate.

### P9. Claims that do not hold — **done**

_Landed 2026-10-01; see the item commits. The counts below are kept as the
record of what was claimed, not as the current state._

- `tests/corpus/README.md:36` says "36/36 cases byte-identical"; there are 40
  cases.
- The same line claims the run "reproduces the 30 upstream goldens in
  `tfiles/good_*.html` byte for byte". 29 cases have a comparable golden; 3
  (`custom-headers`, `custom-headers2`, `pre2`) did not match — 2 of those
  because of the P2 typo. After P2 the number should be re-measured and the
  claim restated precisely. Comparing against freshly generated Perl is the
  right primary methodology; the golden comparison is a useful *second* check,
  not a substitute.
- `txt2html-gui/README.md:97-104` says the golden test is "skipped unless the
  reference files are available", which is accurate, but it should also say
  that it passes with them set.

The re-measured figures are **48/48** and **33/33**, and `tests/corpus/README.md`
now states both. The GUI README's golden claim was verified rather than
rewritten: with `T2H_TFILES` set the test executes and passes, which is now also
what CI does.

### P10. Small items

- `cli.rs:39-92` is missing the script-level options present in
  `ref/txt2html-3.0/scripts/txt2html:848-905`: `--manpage`/`--man_help`,
  `--debug`, `--verbose`, `--dict_debug`/`--db`. All are plumbing or debug
  aids; `--help`, `--version` and `--utf8` are already handled. Low value,
  cheap.
- `links.rs:439` — the comment says "escape all slashes" but the code omits
  Perl's `$key =~ s|/|\\/|g` (`TextToHTML.pm:4777`). Behaviourally harmless:
  verified identical output for `|a/b/c|`, because an unescaped `/` is already
  literal in a Rust regex. Either add the line or fix the comment.
- ~~`cargo clippy` reports 42 warnings in the lib~~ — **done (P23)**. 69
  warnings total across all targets, now 0, and the gate fails on a new one.
  Four are reviewed `#[allow]`s with the reason recorded next to them.
- ~~`cargo fmt --check` is not clean — the hand-aligned comment table at
  `chars.rs:22-38` is the only diff~~ — **done**. `cargo fmt --check` is clean;
  the table was realigned rather than skipped.
- The port accepts dictionary patterns Perl rejects (e.g. `/a/b/c/`, where Perl
  dies with a clear message). The port silently proceeds. Covered by P4.
- Option abbreviation is intentionally not supported: upstream accepts any unique
  prefix (`--titl` for `--title`) via `Getopt::Long`, the port rejects it. This
  is the right call, since unique-prefix matching silently changes meaning
  whenever an option is added or renamed. Record it in the README so it does not
  read as an omission. The `no-` / `no_` negation (`cli.rs:138,249`) *is*
  implemented and needs no work.
- See also P11: P10 above was produced by reading `cli.rs` by eye and missed an
  entire feature. Diff `ref/txt2html-3.0/scripts/txt2html` against `cli.rs`
  option by option instead.

### P11. Config/rc file support was lost — **done**

Found by the survey in `TOOL-SURVEY.md`, not by the original review.

The shipped upstream script reads option files. `scripts/txt2html:838-845` calls
`Getopt::ArgvFile::argvFile(startupFilename=>".txt2htmlrc", home=>1,
current=>1)`, and the POD at `scripts/txt2html:509,759-771` documents
`~/.txt2htmlrc`, `./.txt2htmlrc`, and `@filename` grouping as active behaviour.

The port had none of it. `textrill @opts.txt` treated `@opts.txt` as an input
filename and failed with `Could not open @opts.txt`, which is a confusing failure
rather than an honest "unknown option". **Fixed**: `src/rcfile.rs` plus
`cli::parse_args_with_rc`.

Precedence is `@file` < `~/.txt2htmlrc` < `./.txt2htmlrc` < command line, matching
upstream, where `argvFile` prepends its expansion to `@ARGV` and the command line
is therefore parsed last. Each layer was tested against the one below it rather
than the whole stack being assumed correct.

**A deliberate Tier 1 divergence, and the only one in this item.** Upstream reads
the rc files only `if (eval("require Getopt::ArgvFile"))`, and that module **is not
installed on this machine**, so the oracle used by the corpus reads no rc files at
all. This port reads them unconditionally. The behaviour therefore cannot be
checked against the reference here — only the option *values* can be, and those go
through the same `set_value` path the command line uses. The alternative, matching
the oracle by ignoring rc files unless a Perl module happens to be present, would
make correct behaviour depend on an unrelated CPAN install; that is the reference's
bug, not its specification. The corpus is unaffected because neither the corpus
directory nor `$HOME` contains a `.txt2htmlrc`, and that is checked rather than
assumed.

Two smaller judgement calls:

- A missing `@file` **is** an error; a missing rc file is not. The rc files are
  documented as optional, whereas `@nope.txt` was a name the user typed.
- `~/.txt2htmlrc` and `./.txt2htmlrc` are compared by path, so `HOME=$PWD` — common
  in containers — reads the file once. Upstream would read it twice and duplicate
  every array option.

Beyond the reference, an error in an option file is reported as `file:line:
message`, naming both. That is the ergonomic win over upstream, whose own error
names neither.

## Phase 5 — Feature work

Deferred deliberately. Nothing here is a defect; these are gaps worth
considering once the above is solid, informed by the survey in
`/home/vicpu/build/TOOL-SURVEY.md`.

1. ~~**HTML5 output mode.**~~ — **done** (P5.1): an opt-in `--html5` emits the
   short `<!DOCTYPE html>`, an `<html>` element with no namespace, and a forced
   `<meta charset="utf-8">`, leaving the body markup identical. Off by default,
   so the reference goldens do not move; the default prolog is still HTML 4.01 /
   XHTML 1.0 Strict. `TOOL-SURVEY.md` §4 found txt2tags, pandoc and Asciidoctor
   all emit HTML5 and docutils moves its default in Docutils 2.0; the named
   future version at which this port's default changes is still to be chosen.
2. ~~**Explicit encoding parameter** on the API and CLI~~ — **done**, twice: P7.3 added `--encoding auto|utf-8|cp1252` and P7.4 widened it to `iso-8859-1`, `cp1251`, `cp1253`, `koi8-r`, `utf-16le/be` and `utf-32le/be`. Single-byte charset *detection* remains open and is deferred to a separate project — see **Deferred: single-byte charset detection (Part B)** under P7.4, including why a wrong confident guess is worse than the mojibake it would replace.
3. ~~**Table of contents.**~~ — **done** (P5.2): `--toc` (and the sectioning
   `--section` / multi-file `--chunk` around it) generate the TOC in conversion
   from `src/section.rs`. The survey (§4.1) rated this the one clearly
   high-value gap; the reference *explicitly disclaims* it and points users at
   the `htmltoc`/`hypertoc` post-processing passes. The original instruction
   here was to reuse the `make_anchors` `section_x_y` ids exactly, including
   duplicate suffixes; that was **superseded** (see the design note below) in
   favour of sequential `chunk-N` ids assigned by the sectioner, which cannot
   collide or leave a dead link. Opt-in and default off, so the goldens do not
   move.
4. ~~**Heading numbering.**~~ — **done** (P5.3): an opt-in `--number_headings`
   prefixes each heading with its hierarchical position (`1`, `1.1`, `1.1.1`,
   …), inserted before any sectioning so `--toc` labels and `--chunk` pages
   carry the numbers too. Independent of `--section`/`--toc`, default off, so
   the goldens do not move. txt2tags `-n`, Asciidoctor `sectnums`, docutils
   `sectnum` were the surveyed analogues (§4.2).
5. ~~**Streaming/large-file mode.**~~ — **done** (P5.4): `--stream` feeds one
    paragraph at a time through `Converter::convert_stream`, using a
    `ParagraphReader` whose record boundary is the reference's `$/ = ""`
    paragraph mode. All cross-paragraph state already lives in the converter, so
    the output is byte-identical to the buffered path; that equality is pinned by
    `stream_tests` in `convert.rs` and by `tests/streamtest.rs` end to end.
    Input must be UTF-8: a wide BOM/NUL structure is refused before anything is
    written and an invalid byte is an error rather than a replacement `char`,
    because the buffered `Auto` path would have decoded such a file as CP1252.
    Refused with `--instring` and with the whole-body passes `--number_headings`,
    `--section`, `--toc`, `--chunk`. Opt-in and default off, so the goldens do
    not move. The GUI panel picks the option up from `cli::SPECS` for free; it is
    meaningful when the generated command is run on a file, and the live preview
    (which converts in-memory text) is unaffected.
6. **Footnotes.** Requested feature in the docutils/pandoc/asciidoctor class of
   tool, but rejected here: footnotes need unambiguous inline markers, and
   guessing `[^1]` in ordinary prose would silently turn text into links. Not
   compatible with the tool's contract.
7. **Templates and slots.** Not in the survey's gap list, but the natural
   companion to the TOC: let a user supply the page skeleton instead of the
   engine hard-wiring it, so a TOC can sit in a sidebar, a print page can carry
   its own footer, and a whole document can take on a house style. Design
   settled below; **not yet implemented**. Distinct from the citation/glossary
   work, which stays deferred (see the notes policy in FINDINGS.md §2).

### Sectioning and TOC — design (2026-10-04)

Settled while mining a deleted prior attempt at this product — see
`template-research1/FINDINGS.md`.

_Implemented 2026-04: `src/section.rs` plus P5.2 `--section`/`--toc`/`--chunk`
and P5.3 `--number_headings`; `convert.rs::convert_sources` was split so the
passes run over the body only, and `try_convert_chunked` writes the multi-file
model. Numbering runs before sectioning so TOC labels carry the numbers. Gates:
corpus 59/59, goldens 33/33, `sectiontest` 9/9, GUI acceptance 26/26, clippy and
fmt clean. Chunk links are sibling file names, not write paths._

- **Both output models, behind flags.** Single-page sectioning wraps each
  heading-delimited section in `<article class="section" id="chunk-N">` and emits
  one generated TOC; multi-file chunking splits at headings into separate pages
  with a prev/next pager.
- **Sequential `chunk-N` ids, assigned by the sectioner, not the
  `make_anchors` `section_x_y` names.** Item 3 above warned that a TOC reusing
  `make_anchors` ids must reproduce duplicate-heading suffixes exactly or it
  contains dead links. Own sequential ids make that failure impossible: a
  duplicate heading gets a distinct `chunk-N` because the counter is positional,
  not derived from the heading text or level. This supersedes item 3's "reuse
  `make_anchors` ids" instruction.
- **Heading-based, not paragraph-count-based.** The mined tool's TOC used
  first-paragraph snippets because its converter was not heading-aware; ours is,
  so sections are heading boundaries.
- **New options stay default-off** so every golden is unmoved, and note markers
  use collision-proof sigils only. Evidence for the sigil rule is in FINDINGS.md
  §3: the inherited default `#` bold delimiter already eats `C#`/`F#` in
  ordinary prose, so ordinary-character markers cannot be trusted.

Explicitly rejected by the survey, so they are not reconsidered later: multi-target
output (a different product), syntax highlighting inside `<pre>`, a built-in
stylesheet that is not opt-in, and the htmltoc-style post-processing TOC.

### Templates and slots — design (2026-10-04)

Settled while talking through how a user other than the author could slot in
their own layout. **Not yet implemented.**

The problem: the engine hard-wires the document skeleton (doctype, `<head>`,
`<body>`), so the only customisation today is a handful of string options
(`--append_head`, `--body_deco`, `--style_url`, `--prepend_file`,
`--append_file`). That is enough to tweak, not enough to put a TOC in a sidebar
or to take over the page.

The model is **engine makes blocks, template arranges them**. The engine already
knows the things a user must not hand-write — anchor ids, TOC targets that must
not go stale, pager links — so it pre-renders named blocks and the template only
decides where they go.

- **Two ownership levels, the wrapper as the default.**
  - `--template FILE` is a **fragment** placed inside `<body>`; the engine still
    emits the doctype, `<head>` and the `<body>` tags. This is the common case
    (sidebar TOC, footer, house wrapper) and does not require the user to know
    the prolog.
  - `--document_template FILE` is a **whole page**; the engine emits none of the
    skeleton. For a different doctype or head.
  - With neither, output is **byte-identical to today**, so the goldens do not
    move and the default path is untouched.
- **Namespaced slots, `textrill` prefix.** Slots are `{{textrill:name}}`:
  `{{textrill:content}}` (the only **required** slot — the converted body with
  its anchors), `{{textrill:toc}}` (the generated `<nav class="toc">`, empty when
  `--toc` is off), `{{textrill:title}}` (`--title`/`--titlefirst`, escaped),
  `{{textrill:head}}` (the engine's `<meta>`/`<link>`/generator block), and
  `{{textrill:pager}}` (per-page prev/next under `--chunk`).
- **Unknown `{{textrill:*}}` is a hard error; every other `{{...}}` passes
  through untouched.** This is why the slot is namespaced rather than bare: in
  HTML templates bare `{{ }}` is the most contested token space there is
  (Mustache, Handlebars, Jinja2, Nunjucks, Twig, Liquid, Vue, Angular). A
  namespace lets a template that also carries another engine's tokens keep
  working, and leaves the general `{{ }}` space free for later slot families —
  the same reason XML namespaces, `data-*`, `--css-vars`, `x-` headers and
  `APP_` env prefixes exist. Familiarity buys learnability; uniqueness buys
  stability, and the template is authored once and read many times.
- **Not a template language.** No loops, conditionals, expressions, or
  includes; no JavaScript. `{{textrill:toc}}` is a single pre-rendered block, so
  there is nothing to iterate. If someone needs control flow, that is a static
  site generator's job, not a text converter's.
- **Composition with the existing options.** A template takes over
  *arrangement*, not *content generation*: `--title`, `--style_url`,
  `--meta_charset`, `--append_head` still feed `{{textrill:title}}` and
  `{{textrill:head}}`. If a template omits a slot an active option would have
  filled, that is reported rather than silently dropped (fail-loud, consistent
  with `--stream`'s encoding refusal).
- **Per project, not per invocation.** Because `./.txt2htmlrc` and `@file`
  already work (P11), a project commits its template and points at it once in
  the rc file.

Open sub-decisions, to settle at implementation time: the exact slot set
(and whether `{{textrill:head}}` should be auto-injected when absent), how
`--chunk` applies the template per page, and whether `--document_template`
supersedes conflicting prolog flags or errors on them.

## Phase 6 — The GUI rewrite (P13)

Added 2026-10-01, after P13 was answered: the deliverable is a single
self-contained artifact, so `txt2html-gui` is rewritten from Python + PySide6
onto Rust + Qt. **The engine is kept.** It is 5,644 lines, already byte-verified
against Perl by a differential corpus, and is the expensive part of this project
— which is already paid for. What is rewritten is a 1,365-line shell, and the
212-line pyo3 layer is deleted outright.

_Updated 2026-10-02: "single self-contained artifact" is delivered as **Flatpak**
rather than a bundled binary — see "Licensing and distribution". Qt is an
assumption of this plan, not a decision; GTK4 is a live option._

_Resolved 2026-10-03: the toolkit is **`egui`/`eframe`** (pure Rust). Qt was
rejected on the spike, not on preference — see §6.6. GTK4 was not needed._

This is Phase 6 and not Phase 5 because Phase 5 is opt-in *feature* work that
touches the engine, and doing it before the rewrite means doing it against a
shell that is about to be deleted. Nothing in Phase 5 is lost by waiting; §6.5
says where it goes.

### 6.1 The sequence, and why P5–P11 come first

**Do P5–P11 before starting the rewrite. All of it assists, and one item is
close to a prerequisite.**

The reason is that the rewrite *deletes* the Python GUI, and the Python GUI
currently holds a second copy of an engine rule. `files.py:46` re-implements
`convert.rs:30`'s encoding rule in Python — UTF-8 if valid, else Latin-1
byte-as-codepoint — and the two implementations do not fully agree. Landing P7
first means the Rust GUI inherits one correct policy instead of porting a
disagreement forward. Doing P7 after the rewrite means finding the same defect a
second time, in a language where nothing points at the other copy.

| item | assists the rewrite? | why |
|---|---|---|
| **P7** (encoding) | **prerequisite** | the split encoding policy is the defect. Fix it in the engine first and the rewrite inherits one implementation. P7.2's `meta_charset` and the GUI write-back path are the same rule that `files.py` duplicates |
| **P6** (2× slower) | strongly | links are 60% of runtime and 3.3× slower than Perl. In a GUI this is latency on every keystroke, not a batch number. Fix once, both artifacts benefit |
| **P5** (inherited hang) | strongly | a hang is the worst failure mode for a GUI. The guard becomes a dialog in Rust/Qt; the error already exists if built now |
| **P10, P11** | neutral-to-positive | engine-side, unrelated to the shell. Would otherwise surface as unexplained behaviour in the new GUI |
| **Phase 5** (features) | **defer** | touches the engine, so it is independent of the rewrite — but see §6.5 |

None of P5–P11 is GUI-coupled. All of it is engine or reference-comparison work,
so none of it is wasted on a rewrite, and all of it is cheaper to land while the
tree has one front end instead of two.

### 6.2 What the rewrite deletes, and why that is the win

Three things exist only because two languages are in the path:

1. **A duplicated encoding rule.** `convert.rs:30` and `files.py:46`. One
   implementation after the rewrite; `files.py`'s 75 lines go away.
2. **The entire pyo3 binding layer.** Five `#[pyfunction]`s exist to marshal
   across a boundary. In Rust they are direct calls, and `python.rs` (212 lines)
   is deleted. `maturin`, the venv, and `pip install PySide6` leave the
   dependency graph with it — which is what "stands on its own" was asking about.
3. **The options-panel reconstruction.** `optionspanel.py` (491 lines) rebuilds
   widgets from `option_specs()` output that crossed a boundary as a Python list
   of tuples. A Rust GUI reads `cli::SPECS` and `options::NUMERIC_RANGES`
   directly, so a widget cannot offer a value the engine rejects — a guarantee
   the Python version has to be trusted to preserve.

### 6.3 Order of work

1. ~~**Freeze the surface as a written spec, not as maintained code.**~~ **Done**
   — `textrill-gui/SURFACE.md`. The six `pyfunction`s, `option_specs`' six fields,
   the widget mapping, the A6 concurrency contract and the test disposition are
   written down and checked against the build rather than from memory. Three
   things it settled that were previously assumed: `option_specs()` returns
   **54** options (20 bool / 18 str / 11 int / 4 str_array / 1 table_type);
   `process_chunk` has **no caller anywhere in the GUI**, so it should not be
   ported unless something starts needing it; and `file_encoding` is a
   process-global that should become a return value. Do not keep the pyo3 layer
   in sync during the port — it will be deleted, so maintaining it is work with
   no consumer.
2. **Port the tests before the GUI.** `test_gui.py` (1123 lines) is the spec, and
   the split matters:
   - `ConverterTests` (6) — pure engine, already covered by cargo tests.
     **Redundant; delete.**
   - `FileTests` (20) — engine rules currently expressed in Python.
     **Move to the engine's Rust tests**, where the rule lives. **Done** — ten
     tests now cover the save path in `tests/encodingtest.rs`, and doing that
     required adding the thing they were guarding, which did not exist in the
     engine at all: `src/encode.rs`. The engine could decode but not encode, so
     `files.py` had grown a private Python encoder, and Phase 6 would have
     deleted `files.py` and the guarantee with it. Two defects surfaced while
     porting, both now fixed:
     - **The UTF-16 decoder replaced every astral character with two U+FFFD.**
       `char::from_u32` is `None` for a surrogate, and each UTF-16 code unit was
       being converted on its own, so a file containing one emoji decoded as a
       pair of replacement glyphs. Anything outside the BMP was affected.
       `units_to_string` now joins surrogate pairs, and a genuinely lone
       surrogate still becomes U+FFFD.
     - **`Encoding::Auto` was being offered as an output encoding.** There is
       nothing to detect on output, and defaulting it to UTF-8 is precisely the
       corruption that mangles a CP1252 file, so it is refused by name.
   - `BacklogTests` (3) and `GuiTests` (29) — 32 of the 58, and **the port's
     acceptance criteria.** Debounce, generation-counter cancellation,
     dirty-tracking, drag-and-drop, settings persistence, save semantics.
   The class sizes above are counted from the actual `def test_` definitions in
   `test_gui.py`, and sum to the 58 the suite runs; this section previously
   claimed "27 / 46", which never matched the suite.
3. **Port the shell**, in dependency order: `worker.py`'s concurrency contract →
   `optionspanel.py` from `cli::SPECS` → `mainwindow.py` → `app.py`. The first
   two are **done** in `textrill-gui-rs`: the worker keeps the generation
   counter, queue-drop, bounded in-flight work and panic-to-error path, and the
   panel is generated from `cli::SPECS` with bounds from `numeric_range`, now
   with the Python panel's filter and reset. The document model, file state
   (source and output paths, source encoding, dirty and output-stale flags,
   last-saved HTML), the `QSettings`-compatible store (the `auto` flag and the
   whole option blob, in the JSON and INI escaping `QSettings` used), the 300 ms
   debounce, runtime UI zoom, the menu/toolbar/filter/drag-and-drop chrome, and
   the unsaved-changes prompt (Save/Discard/Cancel, including the window's own
   close button) are ported, and the window geometry is persisted as its normal
   size, maximized flag and zoom — never a position (see `RUST-GUI-FINDINGS.md`
   §6 item 15), with a `View → Reset window size and zoom` escape hatch and a
   wrapping toolbar so controls stay reachable at high zoom. Native file choosers are now wired through `rfd`
   (XDG desktop portal over D-Bus, `can_create_directories` off, no temp or
   scratch writes) behind a `Chooser` seam so the explicit-path API stays
   testable; `Open`, `Save As` and a `Save` with no output path open a real
   dialog, and a cancelled chooser drops the waiting command without losing
   edits. `Open` also asks about unsaved text before it opens the chooser, and
   the window's command line (`app.py`: an optional file,
   `--xhtml`/`--no-xhtml`, `--tables`, `--version`, `--help`) is    ported. Fonts are bundled
   rather than taken from the desktop (`egui` never consults fontconfig): the
   stock set plus Noto Sans Regular (Greek/Cyrillic) and Noto Sans CJK TC (the
   full pan-CJK repertoire, Traditional-default shapes) as fallbacks
   (`assets/fonts/`, OFL-1.1). The CJK font dominates the app's size — ~28.6 MB
   stripped, ~15.7 MB compressed (see `RUST-GUI-FINDINGS.md` §6 item 14). The
   ported
   worker was then hardened past the Python original: mutex locks recover from
   poisoning instead of panicking, the worker thread is spawned with
   `thread::Builder` and a spawn failure becomes a reported error (rather than a
   stuck "converting…"), the repaint waker is invoked outside the lock, and the
   GUI crate carries `#![forbid(unsafe_code)]`.
4. **Keep the A6 concurrency contract verbatim in intent.** `worker.py:75` solved
   a real problem: a keystroke burst queues faster than the pool drains, so a
   backlog is also a memory backlog. Generation-counter plus queue-drop is the
   design. Do not rediscover it — a 300 ms debounce alone does not bound memory
   if the queue drains slower than the user types.
5. ~~**Run both GUIs side by side until the ported suite passes.**~~ **Done
   2026-10-03.** The Python suite passed 58 tests (1 skipped) and the native
   suite 60/60; both windows ran under Xvfb on the same fixtures; and across ten
   encodings the two file layers decoded to identical text, detected the same
   encoding, and wrote identical round-trip bytes — the duplicated encoding rule
   agrees. The Python GUI and the pyo3 bindings it needed are retired to
   `legacy-archive/` (nothing there is built or tested), and `make gui` and the
   CI `gui` job are gone.

### 6.4 Verification

The existing gate mostly survives, because the engine is untouched:
`make fmt-check`, `make clippy` (`-D warnings`), `cargo test`, `make corpus`
(59/59, 33/33 goldens), `make fuzz`, and upstream's `t/*.t` as a canary all
apply unchanged. The GUI suite was the part rewritten; it is now the native
`textrill-gui-rs` suite.

The native crate was put in the gate before the swap, so it could not rot while
the Python GUI still shipped: `make fmt-check` and `make clippy` cover both crates,
`make test-gui-rs` runs its headless `egui_kittest`/`kittest` suite (no display
needed), and CI's `rust` job builds, lints and tests it. `verify` includes
`test-gui-rs`. The crate's licence and privacy posture — all dependencies
permissive or GPL-3.0-compatible, no network/telemetry crates, `gethostname`
confined to local X11 auth — is recorded in `RUST-GUI-FINDINGS.md` §5.5.

The corpus is the reason this rewrite is verifiable at all: a Rust GUI
conversion can be compared byte-for-byte against Perl through the same harness
the engine already uses. A GUI rewrite with no byte-level oracle would be
guesswork; this one is not.

`.github/workflows/ci.yml`'s `gui` job is **gone** (2026-10-03): no venv, no
maturin, no `PySide6`, no `T2H_TFILES` indirection. The Rust GUI links the engine
directly, and the `rust` job runs `make test-gui-rs` as ordinary cargo tests. The
`T2H_TFILES` mechanism and the whole pyo3 install path were deleted with it.

### 6.5 Where Phase 5 goes

Phase 5 (TOC, heading numbering, streaming; HTML5 mode and footnotes are now
settled) is **independent of the rewrite** — it is all engine-side. It was deferred until
the rewrite was done, for one reason: each new engine option has to be added to
`cli::SPECS`, and while the Python GUI existed that meant updating two front
ends. The rewrite is done (2026-10-03), so there is now one front end and the
options panel picks up a new option for free — Phase 5 is unblocked. **Phase 5 is
now complete** (P5.1 `--html5`, P5.2 `--section`/`--toc`/`--chunk`, P5.3
`--number_headings`, P5.4 `--stream`, all opt-in and default off).

Two Phase 5 items had a real interaction with Phase 6 and were sequenced
deliberately:

- **Explicit encoding parameter** is P7.3, and P7 is the rewrite's
  prerequisite. Done there, not here.
- **Streaming mode** was held back until the rewrite was done because it looked
  like it would change `process_chunk`'s role, which the then-Python GUI's live
  preview used. In the event it did not touch that signature at all: P5.4 adds
  `Converter::convert_stream`, which *calls* `process_chunk` once per record, so
  the buffered and streaming paths share the same per-paragraph engine.

### 6.6 Toolkit decision (closed 2026-10-02)

All three questions below are now decided. The full evidence is in
`RUST-GUI-FINDINGS.md`.

1. **Which toolkit.** Decided: **`egui`/`eframe`** (pure Rust, immediate mode,
   0.36). Qt was tried and rejected on the spike, not on preference: `cxx-qt`
   0.10 fails to compile any `extern "RustQt"` QObject on this toolchain
   (it emits `include!(<QtCore/QObject>)`, which `syn` cannot parse), and
   Qt's official `qtbridge` requires Qt 6.10 while this host ships 6.4.2.
   Slint was the runner-up. `egui_kittest` provides AccessKit-based headless
   widget queries, which is what the 32 acceptance tests need. Consequence:
   `SURFACE.md`'s QtWidgets widget mapping is revised to egui; the behavioural
   contract does not change.
2. **Does the CLI stay?** Decided: yes, independently distributable. Nearly
   free once the engine is a library.
3. **Replace or coexist during the port?** Decided: coexist until the ported
   suite passes, then replace. Coexisting is what makes §6.3 step 5 possible.

A fifth sequencing constraint follows from the toolkit: the GUI must be its own
crate, because `eframe`/`winit` dlopen GL and cannot be part of the CLI's static
musl build.

## Licensing and distribution

_Decided 2026-10-02._

**The engine and CLI stay GPL-3.0-or-later. The GUI is GPLv3.** This is the status
quo, so it needs no change, and it is the branch of upstream's dual grant that
every distro accepts without discussion.

**Why a BSD CLI was considered and dropped.** The idea was to license the CLI
permissively so it could be adopted anywhere, and keep the GUI copylefted. That
split is legal in principle — a BSD library linked by a GPL application is
compatible — but it is **not available here without upstream's permission.**
txt2html 3.0 licenses itself "under the same terms as Perl itself", i.e.
**Artistic-1.0 or GPL**, and `txt2html-rs/LICENSE` records that this port is "a
derivative work". A derivative of GPL/Artistic code cannot be relicensed to BSD,
because BSD removes the downstream copyleft that the upstream grant imposes. Only
the upstream copyright holders can waive that:

    Copyright 1994-2000 Seth Golub
    Copyright 2002-2013 Kathryn Andersen
    Copyright 2018-2019 Joao Eriberto Mota Filho

**Artistic-1.0 is the permissive option that needs nobody's permission**, since
upstream's dual grant already offers it and `txt2html-rs/LICENSE` already notes
it is available. It was still declined, for two reasons: "Artistic" is not on most
corporate allow-lists the way BSD/MIT/Apache are, so it does not actually buy the
adoption it was meant to buy; and picking the unusual branch of a dual grant
invites a packager question in every distro that touches the package. GPL-3.0-
or-later is boring and nobody asks.

**What the licence was never the blocker for.** The real cross-distro obstacle is
naming, not licensing: `txt2html` is already packaged by Debian and by other
distros as the Perl program, so a Rust port claiming the same name forces a
replace-or-reparallel decision in each. That is a packaging-policy conversation,
which is why going through the current upstream maintainer is the right route and
not a detour around it. GPL-3.0-or-later is what gets a distro to own the package.

**Cross-distro status.** No code here assumes systemd, D-Bus or XDG paths, so
non-systemd systems are unaffected; Flatpak itself does not require systemd at
run time. The CLI is a plain Rust binary with no init-system dependency, and as
of 2026-10-03 CI builds it statically for `x86_64-unknown-linux-musl` and runs
the differential corpus against that binary, so the Alpine claim is exercised
rather than asserted. The CI *runner* is still `ubuntu-latest` only; only the
compiled binary targets musl.

- P1 and P2 gate everything. Nothing else can be trusted until they land.
- P4 is done (as part of P22), so this ordering note is spent: it was
  independent of P1/P2 and was the highest
  user-visible robustness win.
- P6 should be measured before and after each of the two changes, separately,
  so it is clear what actually helped.
- P7.1 and P7.2 (tests and docs) are cheap and should land with Phase 0; P7.3-5
  are the real design work.
- **musl build** — done 2026-10-03. CI's `musl` job builds
  `x86_64-unknown-linux-musl` (`make musl`), asserts the result is static, and
  runs the differential corpus against it (`make corpus-musl`): 59/59 and 33/33
  byte-identical, so the static binary is verified, not merely built. Alpine is
  the one platform with real technical risk — glibc-linked binaries do not run
  there and the engine needs nothing from glibc — and this is the cheapest way
  to keep the byte-identical claim honest. The target is self-contained, so no
  `musl-gcc` is needed for this crate's no-C-dependency tree.
- P11 is independent of all of the above and can land any time; it is the only
  item in the plan that restores lost compatibility rather than fixing a defect,
  so it is the safest thing to hand to a new contributor.
- Phase 5 items 3 and 4 (TOC, heading numbering) should be implemented together
  or not at all — a numbered TOC is the only reason to have heading numbering,
  and both depend on the same heading pass. **Done:** TOC as P5.2, numbering as
  P5.3, both from `src/section.rs`.
- Every phase must keep the corpus at **59/59** and the goldens at **33/33**
  byte-identical, except where a change is explicitly declared a deviation.
  (47/33 as of 2026-10-01, after A8 and A9; 46/29 as of 2026-09-30; 40 when
  this was written.) One of the 47 is a declared divergence, so 46 of them are
  byte-identical to the reference and one must not be.
- That invariant is the **Tier 1** rule and it holds for every item below, all of
  which are Tier 1 or have no non-ASCII surface. A Tier 2 item — anything that
  changes behaviour for non-ASCII input, resource limits, or error handling — is
  verified by goldens and the P12 property suite instead, and must be recorded as
  a deliberate divergence rather than closed by matching Perl.
- **P12 before A2.** A2 has no test that can see it: the output is correct while
  the process leaks, so a byte-comparison passes. P12's counting allocator is that
  test, and A2's published RSS figures are unverified until it exists.
- **P13 is a decision, not work.** Answered 2026-10-01 — one self-contained
  artifact. It changed what Tier 3 means and blocked neither Tier 1 nor Tier 2,
  which is why it could be left open that long. The work it implies is
  **Phase 6**, and its prerequisite is P7: the rewrite deletes `files.py`, which
  is a second implementation of an engine rule, and P7 is the fix to that rule.
  Fixing the rule before deleting the copy is cheaper than discovering the
  disagreement again in Rust.

---

# Addendum — adversarial remediation

Added 2026-09-29, after the attack pass written up in
`ADVERSARIAL-FINDINGS.md`. That document is the evidence; this is the work
plan.

**Read this before Phase 0.** Three of the new items are reachable with default
options from ordinary input, which outranks the harness work in urgency — but
the harness work still has to come first *in the dependency sense*, because the
new items need fixtures to prove they are fixed, and P1/P2 are what make a
fixture result mean anything. So: land P1 and P2, then this addendum's Phase A,
then resume the main phases. A1 in particular is worthless to attempt before
A4's diagnostics exist, because right now the failure is invisible in the GUI.

Items are numbered `A1`–`A10` so they do not collide with `P1`–`P11`, which are
referenced from `TOOL-SURVEY.md` and must not be renumbered.

| Addendum | Finding | Severity | Touches |
|---|---|---|---|
| A1 | paragraph over ~500 KB panics the engine | **done** (panic + hang; see A1b) | `chop_trailing_cr`/`chop_leading_cr`, `delim_replace` |
| A2 | one compiled regex is leaked per distinct pattern, ~3.9 KB each | **done** (was High, corrected to Low by measurement) | `links.rs:154`, needs `--make_tables` |
| A3 | numeric options accept 0 and unbounded values | **done** | `options.rs:174` `validate`, enforced by `tests/cliexit.rs` and the GUI's spin boxes |
| A4 | GUI cannot catch a Rust panic | **done** | `PanicException` re-export; `worker.py` re-raises completion |
| A5 | GUI corrupts non-UTF-8 files on save | **done** (line endings still normalised; see A5) | `files.py:33` returns the encoding; the window writes it back |
| A6 | GUI never cancels superseded conversions | **done** (was Low–Med) | `worker.py:92` `Converter.convert` clears the queue |
| A7 | save silently creates directories | **done** | `files.py:64` no longer creates parents |
| A8 | `--title` / `--style_url` unescaped | Low | declared deviation |
| A9 | unreadable input exits 0 | Low | declared deviation |
| A10 | unbounded `re_cache` | Low | `convert.rs:160` |

## Addendum Phase A — the engine must not crash or leak

### A1. Replace the one panicking call, do not just raise the limit — **done**

**This is narrower than the findings document implies, and the narrowness is the
whole fix.** The traceback is `process_para → Regex::replace → replacen →
unwrap`, and only one `.replace()` in `process_para` can produce it:
`convert.rs:1764`.

The cause is not the length of the paragraph by itself. It is that
`expand_ascii_escapes` (`links.rs:60-69`) rewrites every `$` outside a character
class into the **lookahead** `(?=\n?$)`:

```rust
if c == '$' {
    if in_class { out.push('$'); } else { out.push_str(r"(?=\n?$)"); }
    continue;
}
```

`fancy-regex` can only run its fast automaton when no lookaround is present, so
this one translation moves `[ \t]*\x0D$` off the fast path and onto the
backtracking engine, where a 1 MB haystack exhausts the default 1 000 000-step
budget. `convert.rs:1766` is unaffected because `^` is not rewritten.

Verified directly, same 25 000-line paragraph, same two patterns:

| pattern | result |
|---|---|
| `(?s)[ \t]*\x0D$` (raw) | ok |
| `(?s)[ \t]*\x0D(?=\n?$)` (translated) | **panic** |
| `(?s)^[ \t]*\x0D` | ok |

So the fix is to stop using a regex for these two patterns at all. They are
anchored at the ends of the paragraph, so they are trivial string surgery, and
string surgery has no backtracking budget to exhaust.

The subtlety is that `$` means "end of text, or before a single trailing
newline" — a plain `ends_with('\r')` is **wrong**, and so is the obvious
first attempt. This implementation is verified equivalent to the current regex
behaviour on 660 000 randomised inputs over the alphabet
`{a, space, tab, CR, LF}`:

```rust
// self.re(r"[ \t]*\x0D$").replace(&para, "")   -- $ is (?=\n?$) after translation
fn chop_trailing_cr(s: &str) -> String {
    let (body, had_nl) = match s.strip_suffix('\n') { Some(b) => (b, true), None => (s, false) };
    if !body.ends_with('\r') { return s.to_string(); }
    let t = body[..body.len() - 1].trim_end_matches([' ', '\t']);
    let mut out = String::with_capacity(t.len() + 1);
    out.push_str(t);
    if had_nl { out.push('\n'); }
    out
}

// self.re(r"^[ \t]*\x0D").replace(&para, "")
fn chop_leading_cr(s: &str) -> String {
    match s.trim_start_matches([' ', '\t']).strip_prefix('\r') {
        Some(rest) => rest.to_string(),
        None => s.to_string(),   // no leading CR means no match, so leave alone
    }
}
```

Two traps, both of which the 660 000-case check caught during this review and
which a reimplementation will otherwise hit:

- The `[ \t]*` sits **before** the CR, so the CR must be removed first and the
  spaces/tabs trimmed after. Trimming first leaves a stray CR.
- The leading pattern must only strip when a CR actually follows the run of
  whitespace. Unconditionally trimming leading whitespace corrupts every
  indented paragraph.

Test: a corpus case of a single ~1 MB paragraph with no blank lines, asserted
equal to freshly generated Perl. Perl handles it in about a second
(`ADVERSARIAL-FINDINGS.md` §2), so this stays a differential test rather than a
golden. Add the same case with CRs and CRLFs mixed in, since that is the only
path that exercises the two helpers.

**Landed as:** `cr_chop_tests` (3 tests, including the 660 000-case
equivalence check above) plus corpus cases `huge_paragraph` (~1.1 MB, one
paragraph) and `huge_paragraph_crlf` (~830 KB, CRLF). Both are differential
against fresh Perl, not goldens. **This was not the end of A1** — the same
paragraph then hung rather than panicked, which is A1b below.

Do **not** fix this by raising `backtrack_limit` (available as
`RegexBuilder::backtrack_limit`, fancy-regex `lib.rs:575`). It converts a crash
into a multi-second stall on exactly the inputs that are already large, and it
leaves every future `replace_all` on a long string one mistake away from the
same panic.

### A1b. The hang behind the panic — **done**

Fixing the two CR patterns above did not finish A1. With them out of the way the
same ~500 KB paragraph stopped panicking and started **hanging**, which cost
most of the time this item took. The cause is the same class of mistake, in
`do_delim` (`convert.rs`) rather than `process_para`.

`expand_ascii_escapes` rewrites `\B` as a lookaround alternation:

```rust
// links.rs
r"\B" => r"(?:(?<=[A-Za-z0-9_])(?=[A-Za-z0-9_])|(?<![A-Za-z0-9_])(?![A-Za-z0-9_]))"
```

so every `\B` and every `(?<!delim)` in Perl's delimiter patterns moves
fancy-regex onto its backtracking VM, and `do_delim` then iterates matches with
`captures_iter`. Measured on one 512 KB paragraph, with a match present:

| pattern | Perl | Rust, before |
|---|---|---|
| `\B[*]([A-Za-z])[*]\B` | ok | hang |
| `\B#([A-Za-z])#\B` | ok | hang |
| `\B_([A-Za-z])_\B` | ok | hang |
| `\B\^([A-Za-z])\^\B` | ok | hang |
| `(?<![*])[*]([^*]+?[...])[*]` | ok | hang |

**The hang is not the bold branch.** `--italic_delimiter=` (empty) makes the
64 000-line case finish immediately, and Perl renders the same input in ~925 ms.
Disabling bold only moved the hang from one delimiter to the next.

**The fix is the same idea as the CR fix, one level up:** the lookarounds are
not the point of these patterns, they are decoration, and they are checkable in
code. Each was rewritten to a lookaround-free regex plus an `accept` predicate
applied by a new `delim_replace`, which walks candidates left to right and
retries one character along after a rejection, exactly as the engine does:

| pattern | replaced by | predicate |
|---|---|---|
| `\B d ([A-Za-z]) d \B` | `d([A-Za-z])d` | `is_word(neighbour) == is_word(d)` on both sides |
| `(?<!d)d([^d]+?[...])d` | `d([^d]+?[...])d` | previous byte is not `d` |
| `(?<!d)d((\w\|["'])...[^\s])d` | same, no lookbehind | the `len(d)` bytes before are not `d` |
| `#([^\d#](?![^#]*(?:<li>\|<LI>\|<P>\|<p>))[^#]*[^# \t\n])#` | `#([^\d#][^#]*[^# \t\n])#` | none of those four tags in the interior |

The `\B` predicate is the one that looks wrong at first and is not: a `\B`
position is *not* a word boundary, i.e. the two sides agree on word-ness, and
one side is the delimiter. So the neighbour must be a word character exactly
when the delimiter is. `_` is a word character and `#`, `*`, `^` are not, so
`_a_` is **not** marked up while ` #a# ` is. The differential tests caught this;
the first attempt hardcoded the non-word case and was wrong for `_`.

The bold assertion looks unbounded but is not: every character of the group is
`[^#]`, so the `[^#]*` inside the assertion can never reach past the closing
`#` — it starts one character in, after the group's leading `[^d#]`, and stops
at that `#`. The predicate therefore only ever inspects the match's own
interior.

**Two bugs in the rewrite itself, both caught by the tests, both recorded here
because both were silent:**

- The predicate sliced `t[s + 2..e - 1]` on a `str`. A regex match offset can
  land inside a multi-byte character, so this panicked with "start byte index N
  is not a char boundary" on the reference's own `umlauttest` fixture. The
  search is now byte-wise over ASCII needles, which cannot be spanned by a
  UTF-8 continuation byte and so is exact *and* panic-free.
- A rejected candidate was consumed whole instead of retried one character
  along. Found by `fuzz.py` seed 99: a `#` pair spanning a `</p><p>` boundary is
  rejected, and the pair nested inside it was then skipped, so the wrong `#` in
  the paragraph got bolded. Pinned as corpus case `delim_retry`.

**Also fixed, since it was on the way past:** Perl interpolates `${delim}` into
the multi-character patterns raw, so a delimiter containing a metacharacter
(`**` being the obvious one) makes *Perl itself* raise `Quantifier follows
nothing in regex` and drop the substitution. The port panicked on the same
input. It now escapes the delimiter, which is identical output for every
delimiter Perl can compile and working behaviour for the ones it cannot.

Do **not** fix any of this by raising `backtrack_limit`; see the note above.

### A2. Stop leaking compiled regexes — **done**

`links::ascii_re_cached` (`links.rs:153-172`) returns `&'static Regex` by
`Box::leak`-ing each compiled pattern. The 128-entry map cap at `links.rs:159`
bounds the map; it does not bound the leak, because clearing a map of `&'static`
drops no memory. Originally reported peak RSS against distinct table delimiters
in the input, with `--make_tables` — **this table is unverified and the last two
rows are not reachable; see "Measured" below**:

| distinct delimiters | input | peak RSS |
|---|---|---|
| 100 | 2 KB | 6 916 KB |
| 800 | 21 KB | 10 436 KB |
| 3 000 | 92 KB | 20 180 KB |
| 30 000 | 1 065 KB | **142 440 KB** |

**Measured (P12, 2026-09-30).** `tests/alloctest.rs` instruments every
allocation with a counting global allocator, and `make alloctest` reports:

| measurement | result |
|---|---|
| direct call, 1 000 distinct patterns | 3 927 180 B retained |
| direct call, 4 000 distinct patterns | 15 676 924 B retained (**3 919 B/pattern**) |
| real input, marginal cost per *additional* distinct delimiter | **3 451 B/delimiter** |

So the leak is real, it is `Box::leak` and not the map cap, and it is **linear in
the number of distinct patterns** — the original "not saturating at 128" claim
holds. Two independent routes agree on ~3.5–3.9 KB per pattern: the direct call,
and ordinary document content through the table path.

**The severity claim above was wrong by about three orders of magnitude, and the
correction matters.** "A 10 MB file approaches 1.3 GB" assumes the number of
distinct patterns grows with the file size. It does not. A document can only
induce a pattern per *distinct delimiter character*, and the delimiter is a
single non-alphanumeric printable character that `convert.rs:2636` strips down
(`^`, `[`, `]`, `\` are removed, and an empty result is rejected). That leaves
roughly 28 reachable delimiters, plus ~39 fixed literal patterns at the
`do_delim` call sites — on the order of 70 patterns, or **~0.3 MB retained for
the life of the process**, once, not per megabyte of input.

The 30 000-distinct-delimiter row of the old table is therefore **unreachable
from a document**: only 94 printable ASCII characters exist, 62 of them
alphanumeric. It can only have come from calling the cache API directly, which is
what the direct-call row above reproduces. The old 142 MB / 13.6 s figure was a
micro-benchmark of the API, not a cost any input can impose.

Revised severity: **low.** A bounded one-time ~0.3 MB for a long-lived process
that converts many documents, with correct output throughout. It is worth fixing —
it is a real leak, it is trivially avoidable, and it is the only defect here that
P12 exists to see — but it is hygiene, not a denial-of-service, and it should not
outrank A3. The allocation *churn* is the larger cost and belongs to P6, not here:
~3.4 KB is allocated per pattern, and 66 MB of allocate-and-free is observed for a
100 KB document, most of it from the long-paragraph path.

**Reachability caveat, and it is easy to get wrong:** this needs
`--make_tables`, which is **off by default** (`options.rs:112`). Without the
flag no table is ever detected, `is_delim_table` is never reached, and the
delimiter is never compiled. Do not "verify" this fix by running the corpus
without the flag and concluding it is fine.

The patterns are attacker-shaped: `convert.rs:2629` reads the delimiter out of the
document and `convert.rs:2645`/`2652`/`2653` compile `[{delim}]` from it.

**Fixed (2026-09-30), by option 1, and slightly differently than option 1 is
written.** `ascii_re_cached` now takes `&'static str` instead of `&str`. The
type is the fix: the function returns `&'static Regex` and so can never free what
it hands out, but if the *input* is `&'static` then the set of patterns is fixed
by the source and there is nothing unbounded to leak. A caller holding a
document-derived pattern now gets a compile error instead of silently growing
the cache, and uses `ascii_re`, which returns an owned `Regex` that is dropped
with it.

That is stronger than the plan's version of option 1, which would have fixed the
one bad call site and left the invariant as a comment for the next caller to
violate. It is not stronger than option 2, which would end the leak completely:
**a bounded leak remains for the 31 fixed literals, measured at 341 KB**,
`valgrind` will still flag it, and this plan does not pretend otherwise. Ending
it means changing the return type to a lifetime-bound borrow at 39 call sites,
which is a bigger change than a 341 KB one-time cost justifies. The
`MAX_CACHED = 128` bound is now a guard against a future mistake rather than the
thing that makes this safe, and it never bounded the leak in any case: clearing a
map of `&'static` drops no memory.

Result on a table-heavy document (960 delimiter-table rows, `--make_tables`):
output byte-identical, 0.23 s before and after, peak RSS 6.5 MB → 6.4 MB. The
recompilation the fix introduces is not measurable, because a single-character
class like `[;]` is cheap to compile and the alternative was retaining every one
of them forever. `make alloctest` reports marginal retention per additional
distinct delimiter of **4 B**, down from 3 451 B.

The original two options, for the record:

1. **Do not cache dynamic patterns.** The delimiter is a single character and the
   set in practice is tiny; compiling one small pattern per candidate table costs
   almost nothing and leaks nothing. This is the smallest correct change.
2. If caching is wanted, store owned `Regex` values in the map and return a
   borrow tied to the map's lifetime, or keep an append-only `Vec<Regex>` in the
   `thread_local` and hand out `&'static` references into it. Either is leak-free
   because the storage is bounded by the patterns actually used.

The `&'static` return type is what forces the leak, so this item cannot be
closed by a one-line change inside the function — the signature has to change
or the cache has to go.

Test: `make alloctest`, which exists and fails on the current code in exactly the
way a regression would. It is the only kind of test that can see this item; a
byte-comparison against Perl passes while the process leaks, because the output
is correct. Two tests, both currently reporting `KNOWN-OPEN [A2]`:

- `a2_retained_bytes_do_not_scale_with_delimiter_count` — the one that
  exercises a real path: it converts actual DELIM-table documents containing 4
  versus 20 distinct delimiters, on separate threads, and asserts the marginal
  retention per additional delimiter stays under 2 KB. **Measured 3 451 B before
  the fix, 4 B after.**
- `a2_literal_cache_retention_is_bounded` — the residual, recorded rather than
  hidden: the 31 fixed literal patterns cost 341 KB once and are never released.
  Budget 512 KB.

The first version of the direct-call test (`a2_retains_one_regex_per_distinct_pattern`,
4 000 generated patterns, ~3.9 KB each) was what established the shape of the
defect, and it earned its keep, but it could not survive the fix: it called
`ascii_re_cached` with a runtime-built pattern, which the new `&'static str`
signature correctly rejects. That compile error is the guarantee working, and it
is how the test knows the invariant now holds rather than merely being intended.

Note the original plan for this test — "~3 000 delimiter tables with 3 000
distinct delimiters, asserting peak RSS" — was doubly unworkable: 3 000 distinct
delimiters cannot be expressed in a document, and peak RSS is neither
deterministic nor machine-independent. Getting a test that measures the right
thing took three attempts; the two failures are written up under P12 because
they are the more likely mistakes for the next person.

### A3. Clamp numeric options at parse time — **done**

`set_int` (`cli.rs:375-397`) floors with `v.max(0) as usize` at `cli.rs:386`. That does two
harmful things:

- **`0` is accepted**, and `convert.rs:1794` then evaluates `tab % tw` — modulo
  by zero, panic, exit 101. Reachable from the GUI, because
  `optionspanel.py:112` returns 0 as the spin-box minimum for every option
  except the two `preformat_*_lines` ones.
- **There is no upper bound**, so `i64::MAX` reaches `" ".repeat(tw)` at
  `convert.rs:1793` and aborts the process with a failed allocation (exit 134,
  `SIGABRT` — uncatchable, unlike a panic). Perl handles `tab_width => 1000000`
  in under a second, so this is a regression, not parity.

While there: the tab expansion at `convert.rs:1791-1797` re-runs
`line.find('\t')` over the whole line after every replacement, so a line with
*k* tabs costs O(k²·tw). At `tab_width=10000`, 10 tabbed lines take 0.11 s and
100 take over 30 s. Rewrite it as one left-to-right pass at the same time.

Fix: validate in `Options::deal_with_options` (`options.rs:147`), which P4
already touches for `custom_heading_regexp`, and reject out-of-range values with
a message naming the option, the value, and the range. `tab_width` needs a floor
of 1; `hrule_min`, `min_caps_length`, `short_line_length`, `indent_width` and
`par_indent` need a ceiling (999 is what the GUI already assumes, so align the
CLI to it rather than inventing a second number). This one change also gives
the GUI correct spin-box bounds for free — `OptionSpec.maximum` should then read
its range from the extension module instead of hardcoding 999 at
`optionspanel.py:115-120`, so the two cannot drift apart again.

Test: one case per numeric option at 0, at the maximum, and one past it,
asserting a clean error and a non-zero exit rather than a panic or an abort.

**Fixed (2026-09-30), and the measurement did not match the list above.**
`Options::validate` (`options.rs:174`) enforces a table, `NUMERIC_RANGES`, and
both entry points call it before any work: `main.rs` prints the message and
returns exit 1, following the convention already used for argument errors, and
`python.rs::options_from_dict` raises `ValueError`, which the GUI shows as a
message while A4's worker still reports completion. Output is not written when a
value is rejected.

The bounded set is **not** the one above. Reading every use of all nine numeric
options found two distinct hazards, and the second one is not in this plan:

| option | hazard | range |
|---|---|---|
| `tab_width` | `tab % tw`, and `" ".repeat` | 1–999 |
| `indent_width` | `" ".repeat(listnum * w)` | 0–999 |
| `preformat_whitespace_min` | `\s{{n},}\S+` | 0–999 |
| `hrule_min` | `([\-_~=*]\s*){{n,}}` | 0–999 |
| `min_caps_length` | `[A-Z]{{n,}}` | 0–999 |
| `par_indent`, `short_line_length`, `underline_length_tolerance`, `underline_offset_tolerance` | compared only | unbounded |

Three corrections to the analysis above, all of them found by writing the test
before the fix:

- **`preformat_whitespace_min` can crash the process, and was missing.**
  `--preformat_whitespace_min=2147483647` exits 101: the value is interpolated
  into a regex quantifier, and `convert.rs:209` turns the compile failure into a
  panic. It is the same shape of defect as `tab_width=0`, reachable the same
  way, and it is not in the list above.
- **`short_line_length` and `par_indent` are harmless and stay unbounded.** They
  appear only in comparisons. Bounding them would make the port stricter than the
  reference for no safety gain, which the compatibility policy forbids, and
  `tests/cliexit.rs` now asserts they accept `i64::MAX`.
- **The engine's quantifier limit is pattern-dependent, so 999 is a margin
  rather than a threshold.** `\s{200000,}` is rejected; `[A-Z]{200000,}` and
  `([\-_~=*]\s*){200000,}` are accepted. A single "regex limit" does not exist,
  so the ceiling is a comfortable bound checked in the tests, not a number
  derived from the engine.

The GUI half is done too, and it is the half that actually caused the original
report: `option_specs` now reports the accepted range per option from the same
`NUMERIC_RANGES` table, and `OptionSpec.minimum`/`maximum` read it instead of
restating 0 and 999 in Python. A spin box can no longer offer a value the engine
rejects, and the two cannot drift apart again.

`tests/cliexit.rs` covers it: every numeric option against 0, negative values,
the ceiling and one past it, asserting no exit is 101, 134 or 139, plus the
specific range and message for each bounded option, plus that rejection happens
before any bytes reach stdout. A GUI test asserts the spin-box bounds.

**Not done, deliberately: the O(k²·tw) tab expansion.** It is a performance
problem rather than a crash, it needs a timing test to be trustworthy, and
bundling it here would have meant committing a perf claim without a
corresponding measurement. It moves to P6, where the other allocation costs are
now recorded.

### A4. The GUI must survive anything the engine throws at it — **done**

`worker.py:48-53` catches `Exception`. pyo3's `PanicException` inherits
`BaseException`, so the handler never runs, the `finished` signal at
`worker.py:55` is never emitted, and the main window waits forever. Verified
through the real `QThreadPool`: of two queued conversions, the healthy one
delivered and the panicking one delivered nothing, with no error shown anywhere
in the UI.

This makes A1 and A3 far worse than a CLI crash. A1 alone is "open a ~600 KB
paragraph in the GUI and the preview dies silently, with default options."

Fix, in two parts:

1. Catch `BaseException` — or `pyo3_runtime.PanicException` explicitly, which is
   more honest about intent — and emit the result signal from a `finally` so a
   failure can never leave the window waiting. Note that `_on_converted`
   (`mainwindow.py:288-291`) already renders `error` into the preview, so once
   the signal arrives the existing UI does the right thing with no new widget.
2. Fix A1–A3 at the source. The catch is defence in depth, not the fix; a
   handler that swallows panics is how A1 stayed invisible for this long.

**This corrects P4.** The main plan states "In the GUI it does not crash
(`worker.py:51` catches it)". It does not catch it, so P4's fix must widen the
catch rather than assume it is already there, and P4's GUI regression test
should assert that a *message appears*, not merely that the process survives.

Test: drive a conversion through the real `Converter` and event loop with
`tab_width: 0`, asserting that `finished` fires, carries a non-empty `error`, and
that the window's generation counter advances. Repeat with a document over the
A1 threshold.

## Addendum Phase B — GUI correctness

### A5. Remember the encoding a file was read with — **done**

`read_text_file` (`files.py:33`) decodes UTF-8-or-Latin-1 and throws the
encoding away; `write_text_file` (`files.py:38`) defaults to UTF-8. So
"Save text…" (`mainwindow.py:376`) and "Save HTML" (`mainwindow.py:421`)
transcode. Demonstrated on a CP1252 file: `Café — naïve "quotes"` comes back as
`Caf\xc3\xa9 \xc2\x97 na\xc3\xafve \xc2\x93quotes\xc2\x94`, with the
`0x80`–`0x9F` bytes turned into C1 control characters. This is P7.5 with actual
bytes attached.

Fix: have the read path return `(text, encoding)` and thread the encoding
through to the write. P7.3 asks for an explicit encoding option on the API and
CLI; do this first and in the same shape, so the GUI gains a per-file encoding
for free and the two do not need reconciling later.

Test: open a CP1252 fixture, save, assert the bytes are unchanged. Add the same
assertion for UTF-8 and for a file with a BOM.

**Fixed (2026-09-30).** `read_text_file` returns `(text, encoding)`, the window
keeps the encoding of the file it has open, and "Save text" writes it back. The
encoding is decided by `detect_encoding`, which is the converter's own rule --
try UTF-8, fall back to Latin-1 -- rather than a declared charset or a locale, so
the GUI cannot disagree with the converter about what a file contains. Loading a
new file replaces the remembered encoding rather than keeping the old one, since
the previous file says nothing about the new one.

"Save HTML" writes UTF-8 deliberately and says so in a comment: the HTML is text
this program produced, not a transcription of the input, so writing it in the
source file's CP1252 would fail on any character CP1252 cannot represent. The
plan above lists `mainwindow.py:421` as a transcode site, and it is not one --
there is no original byte sequence for generated HTML to diverge from. What the
generated HTML *does* lack is a `charset` declaration, so the browser guesses;
that is P7.2, and it is the real problem at that call site.

Two things measured while writing the tests, both worth recording:

- **A byte-order mark is not stripped, deliberately.** The converter does not
  strip it -- `txt2html` on a BOM file emits the U+FEFF into the output -- so
  stripping it in the editor would make the preview disagree with both the saved
  file and the reference. Parity beats tidiness, and the test says so.
- **Line endings are *not* preserved by the window, and the tests now say that
  too.** `files.py` round-trips CRLF byte for byte, but `QPlainTextEdit`
  normalises CRLF to LF on the way in and back out again, so the text that
  reaches `write_text_file` no longer has them. Opening and saving a CRLF file
  therefore still changes it. This is Qt, not this code, and it is a real
  byte-level loss of the kind A5 is about -- but whether an editor should
  preserve the line endings of the file it opened is a decision, not an obvious
  bug, so it is recorded here rather than fixed. The current behaviour is pinned
  by a test so that it is a choice and not a surprise.

### A6. Cancel superseded conversions — **done**

`Converter.cancel_pending` (`worker.py:84`) exists and is dead code —
`grep -rn cancel_pending` finds only the definition. `convert_now`
(`mainwindow.py:277`) queues unconditionally onto an unbounded `QThreadPool`
queue, and every queued `_Job` holds its own copy of the document
(`worker.py:37`).

The arithmetic: one conversion of a 793 KB document takes 1.25 s, so the 2-thread
pool manages 1.60 jobs/s while the 300 ms debounce (`mainwindow.py:41`) offers
3.33 jobs/s. Past roughly 250 KB the backlog grows for as long as the user types.
Measured with 10 jobs queued over 3 s of typing: 6.4 s of wall clock for 1.25 s
of useful work, and RSS 40 → 61 MB.

Fix: call `cancel_pending()` in `convert_now` before queueing the new job, and
set a pool expiry timeout so abandoned jobs are eventually reaped. The
generation check at `mainwindow.py:285` already discards stale *results*; this
is about not doing the stale *work*.

Test: queue N conversions of a document slower than the debounce and assert the
pool never holds more than a small constant number of active jobs, and that
`activeThreadCount` returns to idle promptly after the last keystroke.

**Fixed (2026-09-30).** `Converter.convert` now calls `pool.clear()` before
queueing, which drops every runnable that has not started. It is in that method
rather than in `convert_now` so that no future caller can forget it, and it
replaces the dead `cancel_pending` that the plan above notes was defined and
never called. A job already on a thread cannot be recalled, so at most
`max_threads` conversions are ever in flight and the rest are dropped at each
keystroke instead of accumulating.

The pool's expiry timeout is left at its 30 s default. It reaps *threads*, and
`clear()` already removes the queued work, so changing it would have been motion
without effect; a test asserts the default rather than a new number.

Measured, 20 conversions of a 793 KB document offered over 3 s of typing at the
window's 300 ms debounce, one thread and two threads both tried, the second
figure being the same scenario with `clear()` removed:

| | with the fix | without |
|---|---|---|
| conversions actually performed | **3 of 20** | 20 of 20 |
| CPU time | **1.36 s** | 7.69 s |
| peak RSS | **97 MB** | 118 MB |
| wall clock to settle | 3.51 s | 4.34 s |

**The wall-clock claim above was overstated.** The plan's arithmetic predicted
"6.4 s of wall clock for 1.25 s of useful work", and the real saving is 0.8 s,
because the time to settle after the last keystroke is bounded by *one*
conversion either way: the window discards a stale result, so a user waiting
for the preview waits for the newest conversion and no amount of cancelling
makes that one faster. What the fix actually removes is 85% of the CPU and 21 MB
of the memory, which is what keeps the machine usable while someone types, and
what stops the backlog growing without bound the longer they type. That is a
worthwhile fix, and it is a different fix from the one described above.

`tests/test_gui.py::BacklogTests` covers it. Writing that test needed one thing
worth recording: `QThreadPool.waitForDone` blocks the main thread, and the
results arrive as queued signals on that same thread, so a test that waits with
it observes nothing and asserts nothing. The first version of this test passed
with the fix reverted. It has to spin the event loop, and stop after the pool has
been idle for a moment, which also covers the gap between `start` and the thread
spinning up. Reverted, it fails with "8 of 8 queued conversions ran".

### A7. Stop creating directories on save — **done**

`write_text_file` did `target.parent.mkdir(parents=True, exist_ok=True)`. A typo
in the save dialog invents a directory tree — measured, saving to
`newtree/a/b/c/out.html` created **five** directories, not the three first
recorded. A save that cannot happen should be an error, not a new filesystem
layout. Drop the `mkdir` and let the `OSError` surface, or keep it only when the
parent already exists.

**Fixed (2026-09-30).** Dropped. A save into a directory that does not exist now
raises `FileNotFoundError`, which the window already catches and reports as
"Cannot save", so the user is told something they can act on. Verified: the same
path created five directories before and now creates nothing and fails cleanly.
The second option in the paragraph above is the same code, so there was nothing to
choose between them.

**The removed behaviour was load-bearing, and was found by a hang.**
`test_saving_twice_reuses_the_chosen_name` wrote to `tmp/out/book.html` and relied
on the `mkdir` to create `out`. With it dropped the save failed, that test did not
stub the message box, and the suite wedged on a modal dialog for thirty minutes
instead of failing. The test now writes into the directory that exists, because
what it is about is the second save reusing the name it was given, not directory
creation. Worth keeping: a test that passes for a reason other than the one it
names will not fail when that other reason is removed, and if it also leaves a
dialog open it hangs rather than reporting. Both A7 tests were confirmed to fail
with the `mkdir` restored before the fix was kept.

## Addendum Phase C — declared deviations from Perl

These three reproduce byte-identically in `HTML::TextToHTML` 3.0, so each is a
decision rather than a patch. They are last because they are the only items here
that break the byte-identity rule the rest of the plan protects.

### A8. Escape `--title` and `--style_url`

`--title '</title><script>alert(3)</script>'` and
`--style_url 'x.css" onload="alert(4)'` both emit live XSS; Perl emits the same
bytes. Low severity, because these are option values the invoking user chose
rather than document content — and the document path is already safe, since
`--titlefirst` is escaped. Worth doing anyway: the output of this tool is
normally served as a page, and once P11 lands, `~/.txt2htmlrc` becomes a
plausible injection vector in a dotfile-managed or shared environment.

Escape `<`, `>`, `&`, `"` in both. Declare it as a deviation and record it in
the deviation list at `lib.rs:16-31`.

**Done.** `chars::escape_attr` escapes all four; `convert.rs` applies it at the
two points the values are interpolated. Two consequences that were not obvious
before implementing it:

* **The title has two routes and they need different treatment.** An explicit
  `--title` is an option value, so it is always escaped. A `--titlefirst` title
  is lifted out of the document's own first line, so it is document text
  already, and the reference's rule for document text — `escape_html_chars` —
  is both correct and sufficient there, since `<`, `>` and `&` are all that can
  break out of element text. Escaping both routes with `escape_attr` was the
  first attempt and it introduced a *Tier 1* divergence:
  `--titlefirst --no-escape_HTML_chars` on `a & b` emitted
  `<title>a &amp; b</title>` where the reference emits `<title>a & b</title>`,
  which `fuzz.py` caught at seed 20260929 case 600. The derived route now keeps
  the flag.
* **Escaping twice is worse than escaping once.** The pre-existing
  `titlefirst` path already escaped when `escape_html_chars` was on, so adding an
  escape at the emission point double escaped: `&lt;` became `&amp;lt;`, which
  renders as the literal text "&lt;" instead of "<". There is now exactly one
  escape per route.

Oracles. The differential comparison *cannot* be the oracle for this, because
the reference is the defect and the bytes must differ — so `opt_injection` in
`cases.sh` is declared `differential must fail:` and `run.sh` inverted its
verdict for such cases (a match is then the failure, since it would mean the
escaping had stopped). The real oracle is XML well-formedness in `proptest.py`,
which is what the 30 known-open checks measured; all 30 now pass and
`KNOWN_OPEN_XML_ARGS` is empty. `fuzz.py` no longer generates `--title` or
`--style_url` at all, via `OPTION_DIVERGENT`, a list of *options* that cannot
silence a mismatch — there is no mechanism in it for that.

### A9. Exit non-zero when the input cannot be read

`--infile /nonexistent` prints `Could not open …` to stderr, writes a 0-byte
output file, and **exits 0**. Perl does the same. In a Makefile, a CI job, or a
shell pipeline that reads as success and silently produces an empty document.
`--outfile` to an unwritable path is already handled correctly (exit 1 with a
message), so the tool is inconsistent with itself.

Fix: return an error from the read path and exit non-zero. This is a deviation
from Perl's exit code but not from its output, so it should not disturb any
goldens. A9 is the one item here that makes the tool *safer* in automation
without changing a single byte of what it produces.

**Done.** `--infile /nonexistent` wrote a 0-byte output file and exited 0, on
both sides; it now exits 1.

The design point is that **only the exit code changes**. The output is
untouched, deliberately: an unreadable file contributed nothing to it either
way, so changing the bytes would move goldens for no gain and would make this a
much larger deviation than it needs to be. That is checked rather than assumed —
`a_mix_of_readable_and_unreadable_still_converts_and_still_exits_non_zero`
converts one readable and one unreadable input, and the document it writes is
byte-identical to what the reference writes for the same invocation (verified
by `cmp` against the reference, not just asserted about the port).

Two smaller decisions, both visible in `cliexit.rs`:

* **Which failures?** A path that does not exist, and a path that opens but
  cannot be read (a directory is the portable way to ask for it). Both exit 1.
* **Not an empty file.** `an_empty_input_still_exits_zero` pins the boundary: an
  empty file *is* readable, it produces the same near-empty document, and it
  exits 0. Without that test "input failed to be read" and "input had no
  content" are indistinguishable from the outside, which is the confusion that
  made the original exit 0 worth fixing.
* **stdin is not a file.** `--infile -` is caught before the read check, so
  `stdin_is_not_treated_as_an_unreadable_input` guards the obvious regression of
  "just check every entry in `infile`".

`try_txt2html` returns `Result<String, UnreadableInput>`; `txt2html` is kept as
the forgiving wrapper, because the Python bindings and the in-process tests want
a `String` and a library should not force its callers to care. Only `main.rs`
asks for the `Result`. Recorded as the fifth deviation in `lib.rs:16-31`.

### A10. Bound `re_cache`

`convert.rs:160-175` caches into a per-converter `HashMap` with no size cap,
unlike `ascii_re_cached`'s 128. I could not construct unbounded growth, because
the patterns that reach it from input-derived data (`convert.rs:1646,1652,1658`,
`:451-459`) come from the user's finite `bullets` options. Add a cap for
symmetry with A2 and as insurance, but record it as unproven rather than
shipping it as a fix for something.

**Done, and the original reasoning was right about the cause and wrong about the
consequence.** The growth is reachable, but not through document content.

Measured, by instrumenting `re_cache` and throwing a document at it that reaches
every construct which compiles a pattern — ordered and bulleted lists,
definition lists, all four inline delimiters, tables, hrules, preformatted
blocks, caps, short lines: **19 distinct patterns**, with default options and
also with *every* pattern-varying option set to a distinct value at once
(`bullets`, `bullets_ordered`, all three delimiters, `hrule_min`, both preformat
markers, custom heading patterns). It is 19 either way, and repeating the
document 30 times does not move it, because the cache memoises per *pattern* and
a given converter's patterns are a function of its options, not of its input.

The one unbounded source is `custom_heading_regexp`, which is a user-supplied
**list**: 500 patterns give 512 entries, since each is compiled and cached as it
is tried. So the cap is 128 against a user option, and the worst realistic
working set is 19 — about 6× headroom. The cap number is measured rather than
picked, and `the_cap_is_above_the_measured_working_set` fails if a future change
pushes the working set to it, which is what stops this becoming a performance
regression wearing a fix's clothes.

**What it costs, measured rather than asserted.** The cap clears the whole cache
when full rather than evicting one entry: a compiled `Regex` is expensive to
build and cheap to keep, the working set of any real conversion is small, and an
LRU would put a lookup on the hot path that the cache exists to avoid. Clearing
cannot change the output — the cache is a pure memo, so a miss recompiles
exactly what a hit returned — and `clearing_the_cache_does_not_change_the_output`
measures that by converting one document with a cache that clears repeatedly
part-way through and comparing against the same document converted with the
cache intact.

The worst case is 200 heading patterns cycled over 12 000 paragraphs, which
makes the uncapped cache a 100% hit rate: **1.25 s capped against 1.17 s
uncapped, and 7 536 KB against 7 480 KB.** So it costs about 7% of wall clock
in a case no real invocation produces, and saves memory that is not measurable at
realistic pattern sizes. That is the honest trade, and it is a weak one: the
value here is symmetry with `ascii_re_cached` and having a bound at all, not a
demonstrated saving. Shipping it as a fix for anything would be overstating it.

## Harness work this addendum requires

The existing harness story in Phase 0 is unchanged and still comes first, but
these fixtures are new, and each one needs a *different kind* of assertion.
Getting the assertion type right matters more than usual here, because two of
these defects produce perfectly correct output.

| Fixture | Assert | Why not a golden |
|---|---|---|
| ~1 MB single paragraph, no blank lines | Rust == fresh Perl | Perl succeeds, so this is a differential case; the golden would be 1 MB of noise |
| same, CRs and CRLFs mixed | Rust == fresh Perl | exercises A1's two helpers; must not be a hand-written golden |
| ~3 000 tables, 3 000 distinct delimiters, run with `--make_tables` | peak RSS under a budget | **output is correct while it leaks** — a byte comparison passes |
| `tab_width` 0 / max / max+1 | clean error, non-zero exit | no output to compare |
| one bad regexp via the GUI list editor | a message appears | the failure was silence, not a crash |

The RSS assertion needs a mechanism the harness does not have. `/usr/bin/time -v`
prints `Maximum resident set size`, so the runner can scrape it and fail above a
threshold; a fixed budget is crude but far better than nothing, and a regression
here is a 100× jump rather than a 5% drift.

A1 and A2 are both cases where the differential fuzzer from P3 should be pointed
as well, since both are input-driven rather than option-driven. Neither will be
found by the seeded option sweeps, because they need input *structure*
(a long paragraph, many delimiters) rather than a particular option set — so the
P3 seed corpus should be extended with the new fixtures as seeds.

## Addendum sequencing

- **P1, P2 first.** Without them, none of A1–A4 can be shown to be fixed, for
  exactly the reason the plan gives for doing them first.
- **A4 before A1–A3 in the sense of diagnostics, not of order.** Land the
  `BaseException` catch and the `finally` emit first even though it is 5 lines:
  until it exists, every A1–A3 regression is invisible in the GUI, and the GUI is
  where users meet this tool.
- **A1 is done; A2 and A3 next, in that order** — but A2 waits on P12, because
  nothing else in the harness can observe it. A3 is the smallest item here and
  also unlocks correct GUI spin-box ranges.
- **A5 before A6.** A5 is data loss; A6 is wasted CPU.
- **A7 any time.** A8, A9, A10 are decisions, not blockers, and should not hold
  up anything above them. All three are done: A8 escapes two option values, A9
  exits non-zero on an unreadable input, A10 bounds the pattern cache. What
  remains is P5–P11, none of which is a decision.
- **Every item in Phases A and B must leave the corpus at 59/59 and the goldens
  at 33/33 byte-identical.** None of them should change output for any input that
  does not currently fail. A8 and A9 are the exceptions and must be recorded as
  declared deviations in `lib.rs:16-31` and in the README — both now are.
  (47/33 as of 2026-10-01; A8 added `pre_explicit_blank` and `opt_injection`, and
  A9 turned the four vacuous `empty1`–`empty4` cases into real ones, which is
  where the fourth golden came from.)
- **A2 is the exception to "the harness will show you".** It is the one item
  whose defect is invisible to byte-comparison, so it is verified by P12's
  allocation budget instead, and its current figures are unverified.
