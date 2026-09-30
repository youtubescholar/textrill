# txt2html — remediation plan

Status: **in progress**, 2026-09-30. Covers `txt2html-rs` (Rust engine + CLI +
Python bindings) and `txt2html-gui` (PySide6 front end).

**Progress is recorded in §0.1 below. P1–P3, A1, A1b and A4 are done; E3 and
P4–P13 are not started.** Phase 0 work turned up three defects in the
transformation logic itself (E1–E3), which corrects the original survey's
central claim — see §0.1.

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

## 0. Where things stand

| Check | Result |
|---|---|
| `cargo test --release` | 33/33 pass (12 unit, 5 linktest, 7 optionstest, 9 paratest) |
| GUI `unittest` (offscreen) | 30/30 pass, 1 skipped |
| corpus, clean `RUNDIR` | **46/46** byte-identical, 29/29 goldens (was 38/40, 2 false passes — see P1) |
| differential fuzz | 16 000 cases across 8 seeds, 0 mismatches (was 700 x 12) |
| `good_sample.html`, `good_xhtml_sample.html` | byte identical |
| upstream Perl `t/*.t` (7 functional files) | 102/102 assertions pass — the canary for P1 |
| speed, 2 MB document | Rust 2.94 s vs Perl 1.51 s (**1.9–2.0x slower**) |

The conversion engine is in good shape. Every defect found below is in the
harness, in error handling, in performance, or in encoding policy — not in the
transformation logic.

## 0.1 Progress, 2026-09-30

Phase 0 is complete, and the corpus is at **46/46** with **29/29** goldens
byte-identical and the GUI at **30/30**. Fuzzing is now 2 000 cases across eight
seeds (16 000 cases), against 700 cases before.

| Item | State | Note |
|---|---|---|
| P1 | done | `RUNDIR` cleared, non-zero exit fails the case, `--system_link_dict` removed from `CLI[]` and kept in `EXTRA[]` |
| P2 | done | regexps corrected; the four custom-header cases now reproduce `tfiles/good_custom-headers.html` |
| P3 | done | `fuzz.py` is seeded, permanent, runnable in CI; `--cases` defaults to 300 |
| harness false-greens | done | durable `PERL5LIB`, reference smoke gate, stale-binary warning |
| E1 | done | `table_type` merged instead of replacing |
| E2 | done | explicit-quote `<pre>` dropped text after a blank line |
| E3 | **open** | CR-only lines leave two stray blank lines in the body |
| A1 | **done** | `chop_trailing_cr`/`chop_leading_cr`; the ~500 KB panic and the hang behind it (A1b) |
| A4 | **done** | `PanicException` re-export; GUI worker reports Rust panics and always completes |
| non-ASCII delimiter predicate | **done** | `(?<!é)` was vacuous; predicates extracted so the tests exercise production code |
| A2 | **open** | re-measured by P12: real, linear, ~3.9 KB per distinct pattern, but bounded to ~0.3 MB per process; low severity; fix next |
| P12 | **done** | `proptest.py` (5 properties, no Perl oracle) + `alloctest.rs` (counting allocator); wired into `make verify` |
| P13 | **open** | packaging decision, blocks the scope of Tier 3 |
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
3. **A2 — the fix**, verified by `make alloctest` rather than by output, since
   the output was always correct. Now the smallest remaining item rather than the
   most urgent, which is the point of measuring first.
4. **A3**, then A5–A7. E3 whenever it is convenient.
5. **P13 — the packaging decision.** It scopes Tier 3 only and blocks nothing
   above, but answer it before starting any Tier 3 work.

Two lessons worth carrying to the next item, because both cost time here: a
micro-benchmark of a helper API is not evidence about the tool's exposure to
hostile input, and a measurement harness needs its own validation before its
output is believed. The first produced a three-orders-of-magnitude error in
A2's severity; the second produced a first allocator test that passed because it
generated a document no table was ever detected in, and a second that measured
nothing because the `thread_local` cache was already warm from the first
measurement.

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

### E1. `--table_type` merged into the defaults instead of replacing them

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

### E2. Explicit-quote `<pre>` dropped everything after its first blank line

`split_end_explicit_preformat` buffers the continuation text, but the join back
lived inside a branch that only runs when text remains, so the continuation was
discarded. Silent content loss, reachable from ordinary input with default
options.

Fixed in `src/convert.rs`. The continuation must also reach `apply_links`:
`--use_preformat_marker` with `<pre>\n\n*d*` gives `<em>d</em>`, not `*d*`.

### E3. CR-only lines leave two stray blank lines (open)

Narrow, and not general CR handling: `\r\n`, `\r`, `\n`, `\r\n\r\n`, `\n\n` and
`a\r\n\r\n\r` all agree.

```sh
printf '\r\n\r\n\n' | txt2html --xhtml --make_anchors \
  --preserve_indent --no-use_mosaic_header --no-titlefirst
# reference  <body>\n\n\n</body>       3 newlines
# port       <body>\n\n\n\n\n</body>    5
```

A paragraph-boundary accounting bug in the CR path. Recorded in
`KNOWN_DIVERGENCES` in `tests/corpus/fuzz.py` with a repro rather than fixed.

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

### P12. A property suite that does not reference Perl

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

### P13. Decide what "stands on its own" means for packaging

Undecided, and it should be settled before more engine work rather than after.
The CLI is a self-contained Rust binary. The GUI is not: `txt2html-gui` needs a
Python runtime plus PySide6 at run time, and is built as a wheel. If the
deliverable is a single artifact, Rust + Qt (C++) is a different architecture, not
a refactor. Tier 3 of the policy above is scoped by this answer.

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

## Phase 0 — Make the harness trustworthy

Do this before anything else. Two of the three bugs in later phases were
invisible *because* the harness reports false passes, so nothing else can be
validated until this is fixed.

### P1. The corpus can report PASS on a run that crashed — **done**

_Landed 2026-09-30. See §0.1. The `38/40` in §0 is the number this item
produced, kept as the record of the defect; the corpus was `43/43` when this
landed and is `46/46` now._

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

## Phase 1 — Turn crashes into errors

### P4. Invalid user-supplied regexps panic

`links.rs:144`, `links.rs:168`, `links.rs:184` (`.expect("valid pattern")`) and
`convert.rs:164`, `convert.rs:174` (`panic!("bad regex {pat:?}")`),
`convert.rs:2323` (`table_re`).

One character of typo in a link-dictionary pattern or in
`custom_heading_regexp` — which the GUI exposes as a free-text list at
`optionspanel.py:135` — aborts the CLI with `exit 101` and a Rust backtrace
note. Perl prints `Unmatched ( in regex; marked by <-- HERE in m/a( <-- HERE /
at .../TextToHTML.pm line 4298` and carries on.

In the GUI it does not crash (`worker.py:51` catches it) but the user sees a
`PanicException` in the preview, and the raw `thread '<unnamed>' panicked` line
still reaches stderr.

Fix, in three parts:

1. Add `links::validate_pattern(pat: &str) -> Result<(), String>` that runs
   `translate_pattern` + `Regex::new` and returns a readable diagnostic naming
   the offending pattern and position.
2. Validate up front, once, at configuration time — `custom_heading_regexp` in
   `Options::deal_with_options` (`options.rs:147`) and every dictionary pattern
   in `LinkParser::parse_dict` (`links.rs:398`). Bad input is then reported
   before any output is produced.
3. Replace the remaining `expect`/`panic!` with a skip-and-warn so an
   unforeseen pattern can never take the process down.

Add regression tests: one per panic site, plus a GUI test that a bad regexp
typed into the list editor shows a message rather than a backtrace.

### P5. Guard the inherited hang

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

## Phase 2 — Performance

### P6. The port is ~2x slower than the Perl it replaces

2 MB document, release build: Rust 2.94 s, Perl 1.51 s. Scaling is linear
(measured 125 K -> 2 M), so this is a constant factor, not an algorithmic
regression. Breakdown:

| Phase | Rust | Perl |
|---|---|---|
| link processing | 1.73 s | 0.52 s |
| everything else | 1.13 s | 0.93 s |
| total | 2.86 s | 1.45 s |

Links are 60% of the time and ~3.3x slower than Perl. The causes are in
`check_dictionary_links` (`links.rs:488-564`) and are straightforward:

- `links.rs:544` — `let cur = para_ref.clone();` inside the innermost match
  loop. The whole remaining paragraph is cloned for every match of every rule.
  The clone is not needed for borrow-checking: `repl` takes `&self`
  (`links.rs:566`), so the immutable borrows of `para_ref` and `caps` are dead
  by the time it is called. Drop the clone and match on `para_ref` directly.
- `links.rs:518`, `links.rs:541`, `links.rs:561` —
  `*para_ref = format!("{line_with_links}{para_ref}");` runs for all ~40
  system-dictionary rules on every paragraph even when the rule matched nothing,
  where `line_with_links` is empty and the `format!` is a full copy of the
  paragraph for a no-op. Guard with `if !line_with_links.is_empty()`.

Those two changes should be worth a large fraction of the link-phase gap.
Re-measure with the same 2 MB file, and add a timing guard to
`tests/linktest.rs` next to the existing quadratic-regression test so this does
not silently come back.

A separate, larger question: Perl uses PCRE via `qr//` with a precompiled
substitution hash; the port uses `fancy_regex`, a backtracking engine, and
recompiles nothing but still runs every rule against every paragraph. If the
gap remains after the two fixes above, the next step is a literal-prefix
prefilter per rule so most rules can be skipped without invoking the engine at
all. That is a real design change, so only do it with measurements in hand.

Practical impact meanwhile: the GUI's 300 ms debounce
(`mainwindow.py:41`) is exceeded past roughly 180 KB, so live preview stalls on
moderate documents. Consider raising `AUTO_CONVERT_DELAY` for large inputs, or
converting only the visible region, until Phase 2 lands.

## Phase 3 — Encoding policy

### P7. The documented deviation is narrower than the real one, and there is no charset

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

### P8. No CI anywhere

Given that the harness produced two false passes, this is the highest-value
remaining item. A single workflow should run, on every push:

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --release`
- the corpus with a **clean** `RUNDIR` (Phase 0 makes this meaningful), plus
  the golden assertions from P2
- the fuzzer from P3 with a fixed seed
- `QT_QPA_PLATFORM=offscreen python -m unittest discover -s tests` with
  `T2H_TFILES` pointed at the reference corpus
- upstream's own `t/*.t` as a canary, so the reference does not drift

### P9. Claims that do not hold

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
- `cargo clippy` reports 42 warnings in the lib (15x `&mut Vec` should be
  `&mut [_]`, 5x collapsible `if`, 3x identical `if` blocks, 3x too many
  arguments, 2x manual prefix stripping, 2x clamp-able patterns).
- `cargo fmt --check` is not clean — the hand-aligned comment table at
  `chars.rs:22-38` is the only diff. `#[rustfmt::skip]` with a reason, or
  realign.
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

### P11. Config/rc file support was lost

Found by the survey in `TOOL-SURVEY.md`, not by the original review.

The shipped upstream script reads option files. `scripts/txt2html:838-845` calls
`Getopt::ArgvFile::argvFile(startupFilename=>".txt2htmlrc", home=>1,
current=>1)`, and the POD at `scripts/txt2html:509,759-771` documents
`~/.txt2htmlrc`, `./.txt2htmlrc`, and `@filename` grouping as active behaviour.

The port has none of it. `txt2html @opts.txt` treats `@opts.txt` as an input
filename and fails with `Could not open @opts.txt`, which is a confusing failure
rather than an honest "unknown option".

Fix: accept `@file` as an option group, and read `~/.txt2htmlrc` and
`./.txt2htmlrc`, with precedence `@file` < `~/.txt2htmlrc` < `./.txt2htmlrc` <
command line to match upstream. Report a bad option inside an rc file as
`file:line: unknown option`, which is the main ergonomic win over upstream. Note
the GUI needs no equivalent — it has `optionspanel.py` and should not gain one.

## Phase 5 — Feature work

Deferred deliberately. Nothing here is a defect; these are gaps worth
considering once the above is solid, informed by the survey in
`/home/vicpu/build/TOOL-SURVEY.md`.

1. **HTML5 output mode.** Confirmed by the survey (`TOOL-SURVEY.md` §4): of the
   converters surveyed, txt2tags, pandoc and Asciidoctor all emit HTML5, and
   docutils has committed to moving its default from `html4css1` to `html5` in
   Docutils 2.0. This port emits HTML 4.01 / XHTML 1.0 Strict to match Perl. An
   opt-in `html5` option with a `<meta charset>` and `<!DOCTYPE html>` would cost
   little. Ship it the way docutils is doing it: an opt-in mode now, the default
   changed at a named future version, and a changelog entry saying so.
2. **Explicit encoding parameter** on the API and CLI (see P7.3).
3. **Table of contents.** Strongly recommended by the survey
   (`TOOL-SURVEY.md` §4.1) — this is the one clearly high-value gap. Every
   comparable tool has it except the reference, which *explicitly disclaims* it:
   the upstream README says txt2html "is not a program for automatically
   generating a table of contents" and tells users to run `htmltoc` or
   `hypertoc` over the generated file. That is a post-processing pass which must
   re-parse the output it just produced. Generating the TOC in-conversion is
   strictly better, and everything needed is already there: `make_anchors` emits
   heading anchors, and `HTML::GenToc` is a sibling module in the same
   distribution that the system dictionary already links (`SYSTEM_DICT`).
   Must be opt-in, default off, or every golden changes; the TOC must reuse
   `make_anchors` ids exactly, including duplicate-heading suffixes, or it will
   contain dead links.
4. **Heading numbering.** Recommended (§4.2). txt2tags `-n`, Asciidoctor
   `sectnums`, docutils `sectnum`. Composes with the TOC above. Same opt-in
   constraint.
5. **Streaming/large-file mode.** The engine holds the whole document plus the
   accumulated output in memory. `process_chunk` already exists for the
   incremental case; a documented streaming path would let very large inputs be
   piped.
6. **Footnotes.** Requested feature in the docutils/pandoc/asciidoctor class of
   tool, but rejected here: footnotes need unambiguous inline markers, and
   guessing `[^1]` in ordinary prose would silently turn text into links. Not
   compatible with the tool's contract.

Explicitly rejected by the survey, so they are not reconsidered later: multi-target
output (a different product), syntax highlighting inside `<pre>`, a built-in
stylesheet that is not opt-in, and the htmltoc-style post-processing TOC.

## Sequencing notes

- P1 and P2 gate everything. Nothing else can be trusted until they land.
- P4 is independent of P1/P2 and can run in parallel; it is the highest
  user-visible robustness win.
- P6 should be measured before and after each of the two changes, separately,
  so it is clear what actually helped.
- P7.1 and P7.2 (tests and docs) are cheap and should land with Phase 0; P7.3-5
  are the real design work.
- P11 is independent of all of the above and can land any time; it is the only
  item in the plan that restores lost compatibility rather than fixing a defect,
  so it is the safest thing to hand to a new contributor.
- Phase 5 items 3 and 4 (TOC, heading numbering) should be implemented together
  or not at all — a numbered TOC is the only reason to have heading numbering,
  and both depend on the same heading pass.
- Every phase must keep the corpus at **46/46** and the goldens at **29/29**
  byte-identical, except where a change is explicitly declared a deviation.
  (46 as of 2026-09-30; it was 40 when this was written.)
- That invariant is the **Tier 1** rule and it holds for every item below, all of
  which are Tier 1 or have no non-ASCII surface. A Tier 2 item — anything that
  changes behaviour for non-ASCII input, resource limits, or error handling — is
  verified by goldens and the P12 property suite instead, and must be recorded as
  a deliberate divergence rather than closed by matching Perl.
- **P12 before A2.** A2 has no test that can see it: the output is correct while
  the process leaks, so a byte-comparison passes. P12's counting allocator is that
  test, and A2's published RSS figures are unverified until it exists.
- **P13 is a decision, not work.** It changes what Tier 3 means, so answer it
  before starting anything in it, but it does not block Tier 1 or Tier 2.

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
| A2 | one compiled regex is leaked per distinct pattern, ~3.9 KB each | Low (was High; bounded to ~0.3 MB/process once measured) | `links.rs:167`, needs `--make_tables` |
| A3 | numeric options accept 0 and unbounded values | High | `cli.rs:386`, `convert.rs:1794` |
| A4 | GUI cannot catch a Rust panic | **done** | `PanicException` re-export; `worker.py` re-raises completion |
| A5 | GUI corrupts non-UTF-8 files on save | Medium | `files.py:38` |
| A6 | GUI never cancels superseded conversions | Low–Med | `mainwindow.py:277` |
| A7 | save silently creates directories | Low | `files.py:41` |
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

### A2. Stop leaking compiled regexes

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

Fix, in order of preference:

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

- `a2_retains_one_regex_per_distinct_pattern` — calls the cache directly with
  4 000 distinct patterns and asserts retained bytes stay under 64 KB. It also
  asserts the per-pattern cost is still the ~3.9 KB *linear* leak, so that a
  change of shape is noticed and the figures in this document revisited.
- `a2_retained_bytes_do_not_scale_with_delimiter_count` — the honest one, and
  the only one that exercises a real path: it converts actual DELIM-table
  documents containing 4 versus 20 distinct delimiters, on separate threads, and
  asserts the marginal retention per additional delimiter stays under 2 KB. It
  currently measures 3 451 B.

Note the original plan for this test — "~3 000 delimiter tables with 3 000
distinct delimiters, asserting peak RSS" — was doubly unworkable: 3 000 distinct
delimiters cannot be expressed in a document, and peak RSS is neither
deterministic nor machine-independent. Getting a test that measures the right
thing took three attempts; the two failures are written up under P12 because
they are the more likely mistakes.

### A3. Clamp numeric options at parse time

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

### A5. Remember the encoding a file was read with

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

### A6. Cancel superseded conversions

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

### A7. Stop creating directories on save

`files.py:41-42` does `target.parent.mkdir(parents=True, exist_ok=True)`. A typo
in the save dialog invents a directory tree — verified, saving to
`newtree/a/b/c/out.html` created three directories. A save that cannot happen
should be an error, not a new filesystem layout. Drop the `mkdir` and let the
`OSError` surface, or keep it only when the parent already exists.

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

### A10. Bound `re_cache`

`convert.rs:160-175` caches into a per-converter `HashMap` with no size cap,
unlike `ascii_re_cached`'s 128. I could not construct unbounded growth, because
the patterns that reach it from input-derived data (`convert.rs:1646,1652,1658`,
`:451-459`) come from the user's finite `bullets` options. Add a cap for
symmetry with A2 and as insurance, but record it as unproven rather than
shipping it as a fix for something.

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
  up anything above them.
- **Every item in Phases A and B must leave the corpus at 46/46 and the goldens
  at 29/29 byte-identical.** None of them should change output for any input that
  does not currently fail. A8 and A9 are the exceptions and must be recorded as
  declared deviations in `lib.rs:16-31` and in the README.
- **A2 is the exception to "the harness will show you".** It is the one item
  whose defect is invisible to byte-comparison, so it is verified by P12's
  allocation budget instead, and its current figures are unverified.
