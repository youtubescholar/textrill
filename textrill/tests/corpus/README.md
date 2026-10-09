# Corpus tests

This directory is the project's behavioural harness. Two runners share the
recipes in `cases.sh` and the local fixtures tracked at `tfiles/` and
`workflows/`:

- **`accept.sh`** — the gate, since Phase 8 S12/S13. It converts every case
  with textrill alone and compares the output byte for byte against a
  hand-reviewed **self-golden** (`tests/golden/corpus/<stem>.html`), then
  checks the upstream author goldens (the `good_*.html` files in `tfiles/`)
  on top, honouring `NOGOLDEN[]`. `make accept` runs it; nothing about it
  needs Perl.
- **`run.sh`** — the differential harness. It converts every `tfiles/*.txt`
  with both the Perl reference (`HTML::TextToHTML` 3.0) and textrill, and
  compares the two outputs byte for byte. It is now *advisory*: `make diff`
  runs it (plus `fuzz.py`, below) as a non-gating cross-check. A fresh Perl
  run that disagrees with a frozen output is a finding to review, never an
  edit to make green.

The reference remains an oracle for the differential tier, not the
specification — see `docs/PLAN.md` § "What this is". Its job is historical:
it found the three silent semantic divergences a reference-free oracle cannot
judge, and it stays available to check the frozen outputs against a fresh
original. That is all it does now.

## Reference-free acceptance (`accept.sh`)

`accept.sh` has three modes:

```sh
tests/corpus/accept.sh                 # full run: SELF + AUTHOR checks
tests/corpus/accept.sh --write         # (re)capture every self-golden
tests/corpus/accept.sh definitions     # a single case
```

A full run converts all 61 cases with the port, normalises the one generator
line (see below), and compares:

- **SELF** — against the frozen self-goldens in `tests/golden/corpus/`. This
  is the gate most cases actually hang on; it is the replacement for the 61
  byte-identical differential comparisons.
- **AUTHOR** — against the tracked author goldens in `tests/corpus/tfiles/`
  (`good_*.html`), skipping the `NOGOLDEN[]` entries. Each AUTHOR line prints
  the golden pass/fail outcome; the summary reports them as compared counts.

`--write` captures and freezes the port's current output. That must be a
reviewed act, not the ironing of a regression: a newly captured self-golden is
believed only after it has been read by a person and cross-checked — by the
differential when the case is a parity case, by the case's own unit tests when
it is one of the declared divergences (the encoding fixtures are asserted in
`tests/encodingtest.rs`). The `PROVENANCE.md` next to the `tfiles/` copies
records where the fixtures came from and how to refresh them.

`accept.sh` and `run.sh` share the shared-guard library `lib.sh`
(`duplicate_key_check`, `alignment_check`), exercise the same cases, and print
the same `NORMALISED` counter. Either runner returns non-zero on the first
problem; both were broken on purpose before the other was believed.

## The differential side (`run.sh`, `fuzz.py` — `make diff`)

The reference is not in version control. `make ref` builds it from the tracked
`txt2html-3.0.tar.gz` plus a tracked stub, offline:

```sh
make ref                      # extract the reference; run this first
make diff                     # corpus + goldens + all 8 fuzz seeds
cd textrill
cargo build
tests/corpus/run.sh          # all cases
tests/corpus/run.sh sample   # a single case
```

Environment overrides. Every one of these defaults to a path derived from the
script's own location, so a fresh checkout works; none of them is load-bearing:

| variable    | default                                                |
|-------------|--------------------------------------------------------|
| `REFDIR`    | `<checkout>/ref/txt2html-3.0`                          |
| `STUBS`     | `<checkout>/ref/stubs`                                 |
| `MINE`      | `<crate>/target/debug/textrill`                        |
| `RUNDIR`    | `${TMPDIR:-/tmp}/textrill-corpus`                      |
| `PERL5LIB`  | `$STUBS:$REFDIR/lib`                                   |

`<checkout>` is the directory holding this repository and `ref/`; `<crate>` is
`textrill`. These used to be the absolute path of the machine that developed
the port, which meant a fresh clone had no working gate at all — the defaults
pointed into a home directory that does not exist elsewhere, and `ref/` is
gitignored, so there was nothing to point *at*. `run.sh` and `fuzz.py` now
refuse to start with a clear error if the reference is missing, rather than
letting both halves of every case fail identically and report a clean pass.

Both halves of the default `PERL5LIB` live under `ref/` in the repo on purpose.
`ref/stubs/YAML/Syck.pm` is a stub for a module `TextToHTML.pm` `use`s at load
time but never calls; the real `YAML::Syck` is long superseded and not
installable on a current perl. The canonical copy is tracked at
`stubs/YAML/Syck.pm` and `make ref` copies it into place, so the
reasoning for the stub is reviewable rather than living on one machine. Keeping
these in `/tmp` was tried and a reboot wiped them mid-run — see "Two ways this
reports success wrongly" below.

`cases.sh` holds one recipe per case:

* `CTOR[stem]` — options passed to `HTML::TextToHTML->new()`. Only used where
  upstream's own tests do the same (`sample` needs `xhtml=>0` at construction
  time, because `lower_case_tags` is derived from it).
* `EXTRA[stem]` — options passed to `txt2html()`.
* `CLI[stem]` — the equivalent command line for the Rust binary. The string is
  `eval`ed, so quoting behaves like a shell command line.
* `INPUT[stem]` — input file, defaults to `<stem>.txt`. A comma-separated list
  converts several files with a single converter to test state that carries
  over between files (heading anchor counters, for instance).
* `NOGOLDEN[stem]` — cases that have a `tfiles/good_*.html` which this port
  deliberately does not reproduce, with the reason. There are two:

  * `custom-headers2` — upstream's `t/20tfiles.t` runs several custom heading
    regexps through one converter and compares the *last* result, so the golden
    file is only valid for the last of them. `run.sh` starts a fresh converter
    per case, and the port agrees with the fresh-converter reference.
  * `pre2` — the golden file has a trailing newline the reference output does
    not. Upstream's own comparison strips CR and LF before diffing.

### The doctype is pinned on both sides

textrill's default doctype is HTML5 (PLAN Phase 3); the reference's is XHTML.
A case whose doctype came from a *default* rather than from its own entry
would therefore change meaning on one side only — 34 of the cases above
relied on defaults before the flip — so neither harness is allowed to have
one:

* the reference driver is constructed with `HTML::TextToHTML->new('xhtml' => 1,
  @ctor)`, the module's own default spelled out and placed *first*, so
  `CTOR[]` (`sample`) and `EXTRA[]` (`empty1`, `empty3`) still override it;
* the port is invoked with `--xhtml` before the case's own flags, so later
  flags still win — `CLI[sample]='--no-xhtml …'` reaches HTML 4 as it did
  before the flip;
* `fuzz.py` puts exactly one of `--xhtml` / `--no-xhtml` at the front of every
  case's argv, and the pair is excluded from the random option pool: a fuzzer
  samples options rather than declaring them, and a default it does not know
  about is a comparison it cannot reproduce.

With the pins in place the flip moved no case: PASS stays 61/61 and all 33
goldens stay byte-identical, because no case reads the default doctype.

Current status of the differential side: **61/61 cases byte-identical**, and
all 33 upstream golden checks reproduce byte for byte across 29 distinct files
(the `empty1`–`empty4` cases all compare against the one `good_empty.html`,
which is why the count of checks exceeds the count of files; the other skipped
cases are the `NOGOLDEN` ones above). Since S13 that is the state of `make
diff`, not of the gate: the gate is the frozen outputs, which `make accept`
reports as `SELF: 61/61` and `AUTHOR: 33/33` and `make examples` as 8/8.
`tests/paratest.rs` additionally mirrors `t/10para.t`, `t/25handles.t`,
`t/30sample.t`, `t/50xsample.t` and `t/70bugs.t` from the reference
distribution for the string-level API.

## Six ways this reported success wrongly

All six were live bugs in this harness, and all six produce a green run while
comparing nothing, nothing at all, or less than it appears to. They are worth
stating rather than just fixing, because in every case the *output* was correct
and only the exit status was wrong — which is exactly why reading the output
never found them.

1. **Both sides fail identically.** If `PERL5LIB` is wrong, every perl
   invocation exits non-zero and writes nothing, so comparing two absent files
   would be vacuous. The runner clears its output directories first, fails any
   case where either converter exited non-zero, and additionally runs a smoke
   conversion before the loop: a broken environment now stops the run with the
   perl error on stderr instead of 40 misleading "port regression" failures.
2. **A stale port binary.** The runner warns when `$MINE` is older than the
   sources, because a stale binary passes cases the current code would fail.
   The default is `target/debug/textrill`, so **`cargo build --release` on its
   own leaves the corpus testing a stale debug binary** — which will happily
   report a clean pass for code that does not even compile into it. Either `cargo
   build` as well, or point the run at the release binary explicitly:

   ```sh
   MINE=$PWD/target/release/textrill tests/corpus/run.sh
   ```
3. **The runner could not fail at all** (`P15`, found 2026-10-01). It counted
   `pass`/`fail` and printed `PASS=46 FAIL=0` — but had no `exit` statement, so
   it ended on a successful `echo` and returned 0 whatever the counters said. A
   stub converter that exits 0 and writes wrong output produced `PASS=0 FAIL=46`
   with all 29 goldens differing, and `make corpus` reported success. The
   Tier 1 invariant was being *reported*, not *enforced*. Both entry points now
   compute a status and exit with it.
4. **`run.sh <stem>` died after printing PASS** (`P17`, same date).
   `GOLDEN_N` was only initialised in the full-run branch, so a single-stem
   invocation over any stem with a golden hit `set -u` and aborted with
   `GOLDEN_N: unbound variable` — after printing `PASS` and `GOLDEN pass`. That
   is the P2 shape: a correct-looking path that no gate ever executes.

5. **A case declared on one side of the tables only** (`P20`, same date).
   The full run iterates `"${!EXTRA[@]}"` and reads `CLI[$stem]`, so a case
   written into `CLI[]` but not `EXTRA[]` never runs — and a case that does not
   run cannot fail, which is the P2 shape again. Nothing checked the two arrays
   agreed. `alignment_check` now compares the key sets and names any asymmetry,
   and it runs *before* the loop: an early version appended `ALIGN:` lines to the
   report and carried on, which printed `PASS=46 FAIL=0` on a deliberately
   broken table. Since `PASS` counts the cases that ran, no count means anything
   once the tables disagree, so the run aborts instead. It also checks
   `NOGOLDEN[]`, because an entry suppressing a case that has no golden is a
   lie in a file whose job is being believed.

   ```sh
   printf "\nCLI[typo]='--xhtml'\n" >> tests/corpus/cases.sh   # must fail
   make corpus | grep ALIGN
   ```

6. **A case key assigned twice** (`P21`, 2026-10-01). `alignment_check` compares
   key *sets*, so it structurally cannot see a stem that was assigned twice: by
   the time it runs, bash's associative array has already kept the second
   assignment and the first is gone. A8 added a new `pre_explicit_blank` case
   without noticing the stem was taken, and the older case it displaced stopped
   running. Nothing went red — both variants happen to agree with the reference,
   so the corpus reported its usual clean pass with one case silently absent from
   it. That is a case that cannot fail, which is the P2 shape one level down.
   `duplicate_key_check` now greps the *source text*, where both assignments are
   still visible, across every array a case is defined in.

   ```sh
   printf "\nCLI[pre_explicit]='--xhtml'\n" >> tests/corpus/cases.sh  # must fail
   ```

   The displaced case is restored as `pre_explicit`, named after its input.

> **The rule these five produced: a gate that has never been observed failing is
> not a gate.** Before trusting any check here, break it on purpose and confirm it
> exits non-zero. `MINE=/path/to/stub-that-writes-garbage tests/corpus/run.sh`
> is the test, and it should print `PASS=0 FAIL=60` *and* exit non-zero. The
> equivalent for `make fuzz` is a fuzzer stub that exits 3. See
> `legacy-archive/REMEDIATION-PLAN.md` Phase 0b.

## The one thing this harness deliberately ignores

`normalize.py` canonicalises a single line — a `meta` start tag whose `name`
attribute is `generator` — to a fixed sentinel, on both sides, before any
comparison. Every corpus run prints the count:

```
NORMALISED: 67 generator meta line(s) (P1.1 declared divergence; value asserted by tests/provenance.rs)
```

The reason is P1.1. The port used to emit
`<meta name="generator" content="HTML::TextToHTML v3.0"/>` because that made it
byte-identical to the reference. It is its own tool, so it now emits
`content="textrill v0.1.0"`, and a fresh Perl run still names itself — which
made 18 differential cases and 13 goldens fail.

Suppressing those 13 with `NOGOLDEN[]` was the cheaper edit and the wrong one.
`NOGOLDEN` means *skipped*, so those stems would stop being compared at all, and
five of them are list cases — the family holding two known inherited defects.
Losing golden coverage there to avoid touching one string trades a visible
divergence for an invisible hole.

So the line is excluded and the value is asserted elsewhere:
`tests/provenance.rs` requires the exact expected string and fails if
`TextToHTML` reappears in any spelling. Together: this harness proves content
parity with the reference, that test proves correct provenance.

This is the one place the harness normalises anything, and it is deliberately
not a general normaliser — it matches one line shape and will not touch a body
paragraph that happens to contain the word "generator". Verified by breaking
each half on purpose:

- `PROG` reverted to `HTML::TextToHTML` → 5/5 provenance tests fail, corpus
  stays 61/61 and 33/33. The corpus genuinely does not judge this value.
- Every heading level shifted `+1` → corpus exits 1 with 11 differential and 11
  golden failures, `NORMALISED` unchanged at 67. A real content regression
  2 000 lines from the generator is still caught.

The count is printed unconditionally, including when it is zero, because a
normalisation that quietly stopped applying would leave the corpus green.

The fuzzer applies the same rule by importing `normalise_bytes` from
`normalize.py` rather than growing a second copy of it, and prints its own
`NORMALISED: <n>` in the summary for the same reason. It needed one: at `c6fc271`
the corpus normalised the generator line and the fuzzer compared it raw, so
`make verify` was red at the fuzz stage with 1 864 mismatches in 2 000 cases on
seed 1 while `make corpus` beside it was green. Every mismatch was the one line.
With the shared rule: 60/60 compared, 0 mismatches, `NORMALISED: 106`. That the
rule still catches what it is supposed to catch was checked with `caps_tag`
defaulted to `B` instead of `STRONG` — 6 mismatches, exit 1, `NORMALISED` still
106, i.e. the canonicalisation did not swallow a real difference in tag
rendering.

## Fuzzer

`fuzz.py` is a seeded **differential** fuzzer over the same pair of converters.
Like `run.sh`, it is now advisory: `make diff` runs all eight fixed seeds, and
the honest gap in the reference-free cut-over (recorded in `docs/PLAN.md` S12)
is that its random-option coverage is only partly replaced by the property
suite. It builds each case by taking a seed from `tfiles/`, mutating it, and
choosing a random subset of the real options, then compares the two outputs
strictly — after the single-line generator canonicalisation described above,
and nothing else.

```sh
tests/corpus/fuzz.py                          # 300 cases, default seed
tests/corpus/fuzz.py --cases 2000 --seed 7    # longer sweep
tests/corpus/fuzz.py --cases 2000 --keep      # save failing inputs
tests/corpus/fuzz.py --seed 7 --dump 41 >case.txt   # case 41's input, to hand
                                                    # to minimise.py
```

The default is small on purpose: each case is two converter invocations, and a
gate nobody runs is not a gate. Longer sweeps are opt-in.

Exit status and hangs, both found 2026-10-01 (`P14`, `P16`):

- `fuzz.py` returns non-zero if any case mismatches **or any case times out on
  the port**. A port timeout is a defect in the port. A *reference* timeout is
  not the port's fault, so it is skipped — but it is counted and printed under
  its own heading, never folded into a clean-looking `0 mismatches`.
- A **reference exit** (non-zero without a timeout) also fails the run. It used
  to be counted as "the reference refused this option set" and stepped over,
  which is the P1 false-green shape in miniature: a broken `PERL5LIB` makes the
  reference exit non-zero on every case, and a run that skips those cases
  reports `0 mismatches` having checked nothing. `build_case` only ever draws
  from hand-checked choice lists, so a refusal is unreachable by construction —
  it has never fired, across 16 000 cases and eight seeds. An unreachable branch
  whose only effect would be to reduce the number of things checked is not a
  safety net. `smoke_check` catches a total failure before the loop and by
  name; `compared == 0` catches it independently; this catches the partial
  case, which neither of the others can see.
- Both converters run under `timeout=120`, and that exception is now caught per
  case. Uncaught, it terminated the whole seed silently after however many cases
  it had reached.
- `make fuzz` used to pipe through `tail -1`, and a pipeline reports the status of
  its *last* command — so `make fuzz` was structurally incapable of failing and
  reported a crashed or mismatching run to `make verify` as a pass. This is why
  the "16 000 cases, 0 mismatches" figure in the remediation plan is marked void.
- The full 8-seed sweep runs **concurrently** — 5m30s wall for 39m18s of CPU, so
  it is short enough to sit in `make diff`. `FUZZ_JOBS` sets the width
  (default 8) and the run echoes it.
- Each seed reports through its own status file, read back in seed order so the
  output is stable however the seeds interleave. A **missing status file is a
  failure**, as is a log with no `fuzz:` summary line — collecting status via
  `wait $pid` would let a seed that died simply vanish from the results and leave
  the other seven looking clean.
- `fuzz.py` fails the run if it compared **zero** cases. A reference that refuses
  or hangs on everything used to produce `0 mismatches` and exit 0 — a green gate
  that checked nothing.
- The summary line reports `compared` separately from `mismatches`, and splits
  `timed out (port N, reference M)`. Only a *port* timeout fails the run; the
  reference hanging is the oracle's problem, not the port's. It also prints
  `NORMALISED: <n> generator meta line(s)` — unconditional, same rule as the
  corpus, so a canonicalisation that stopped applying shows as a zero rather
  than as silence.
- `--keep` saves to `--fail-dir` (default `RUNDIR/fuzz-fail`), and saved names
  include the seed. Give each concurrent run its own directory; `make fuzz`
  already does.

The oracle is `scripts/txt2html`, the reference's own command line tool, driven
with the *same* argv as the port. An earlier version drove the module through
an embedded `perl -e` program instead, which is a different program: for the
same option values it produced a different document (`case 369`: the script
emits `<strong>a</strong>` where the embedded driver left `%a%` alone).
`run.sh` uses a module driver too, but pins the construction-time options it
needs in `CTOR[]`; a fuzzer sampling 32 options at random has no way to know
which those are, so it needs the oracle that has no such table.

### The domain the fuzzer claims

`sanitise()` restricts generated input to ASCII with no trailing whitespace
before a line ending, and strips dangerous scheme tokens, for three documented
reasons.

**Non-ASCII.** The reference is byte-oriented end to end — it opens the file
with no `:encoding` layer and never decodes — while the port decodes UTF-8 when
it can and falls back to Latin-1. So the two disagree on *every* non-ASCII
input, in one of three ways:

* `demoronize` sees bytes in the reference and code points in the port, so
  `\xc3\xa4` becomes `&Atilde;&curren;` versus `&auml;`;
* a byte that is escaped identically still shifts the byte offsets that tab
  expansion and table column slicing count, so a Latin-1 character followed by a
  tab lands one stop apart;
* a high byte that reaches the output unescaped (under `--eight_bit_clean` or
  `--titlefirst`) is re-encoded as UTF-8 rather than passed through.

The corpus's non-ASCII fixtures do not cover any of this: `utf8.txt`,
`umlauttest.txt` and `list-styles.txt` are all *invalid* UTF-8, so they take
the single-byte fallback. `umlauttest.txt` and `list-styles.txt` only contain
bytes `>= 0xA0`, and `0xA0`-`0xFF` is the entire range where Latin-1 and CP1252
agree, so they stay byte-identical either way. `utf8.txt` is valid UTF-8.

That is precisely the gap P7.1 closed. The `0x80`-`0x9F` range — where the two
encodings differ, and where every CP1252 file written on Windows puts its
punctuation — had no fixture at all, which is why the fallback decoded as
Latin-1 and demoronize did nothing for so long without anything going red. The
`cp1252_smart` and `cjk_table` cases below now cover it.

**Trailing whitespace** is the trigger for two open divergences where the port
drops a space the reference keeps: `'   e\n '` with `--indent_par_break`, and
the three-space-indent ALIGN table `'   . \n3  .'`.

**Scheme tokens** (`javascript:`, `data:`, `vbscript:`, `file:`) leave the
input, added 2026-10-06. The first sweep after the fuzzer started normalising
the generator line reported 4 mismatches in 16 000 cases, all one shape:
upstream's `tfiles/pre.txt` contains `file:Here`, the reference links it, and
textrill refuses it. That refusal is policy — `src/urlscheme.rs` names those
four as `DANGEROUS_SCHEMES` and unwraps a refused anchor with the text kept —
and it has its own oracles in `tests/urlschemetest.rs` and the corpus case
`opt_injection`. The reference has no scheme policy, so no byte sequence could
match it: Tier 2, and a byte comparison is the wrong oracle. It is removed from
the claimed domain rather than recorded as a known divergence, because that list
is the machinery this file removed on purpose and it would outlive its reason.

Tabs are *kept*: with ASCII-only input, tab expansion agrees with the
reference, and the structural fixtures need tabs and CRs.

### Known divergences

There are none, and there is no list. The fuzzer used to carry a
`KNOWN_DIVERGENCES` table plus ~50 lines that matched a mismatch against recorded
option sets and output shapes and stepped over anything that matched. It is
gone, for a reason worth keeping: the table was empty, and stayed empty,
because every entry was deleted in the same change that fixed the defect it
described.

The one entry it held for any length was the CR path's extra blank lines, and
it was recorded as a *signature* — "the port's line list is the reference's with
two blank lines spliced in" — rather than as a defect. A signature outlives its
fix by construction. Left in place after E3, it would have suppressed the same
regression the moment it returned, and the fuzzer would have had nothing to say
about it. Suppressing on a symptom is a machine for hiding a bug that got fixed.

So the rule matches the corpus's own: a divergence is a failing gate, and it
becomes a passing gate by being fixed, in the same change. A defect that needs
tracking goes in `cases.sh`, as a case that currently fails.

Ten cases are *expected* to fail the differential comparison, by design rather
than by neglect, and all are recorded as `differential must fail:` in `cases.sh`
so the alignment guard holds them to it. Eight share a shape: the reference's
output is wrong, or the reference has no way to be right, and the port's is
right — so there is no byte sequence the port could emit to match it. The other
two are deliberate Tier 2 security divergences, `opt_injection` (A8) and
`url_scheme` (A11).

* `opt_injection` (A8) — `--title` / `--style_url` containing `</title>`,
  `<script>` and a quote-breaking attribute. The reference interpolates them
  into the document unescaped, which is a live XSS; the port escapes them, so a
  byte comparison cannot match by design. Its oracle is the XML well-formedness
  check in `proptest.py`, not `encodingtest.rs`.
* `url_scheme` (A11) — a `<URL:…>` naming `javascript:` and `data:`. The
  reference emits it as a live `href`; the port refuses every scheme outside
  `--allowed_url_schemes`, unwraps the anchor and keeps the text, so a byte
  comparison cannot match by design. Its oracle is `tests/urlschemetest.rs`.
* `cp1252_smart` (P7.1) — a CP1252 file with smart quotes and dashes. The
  reference emits the raw bytes and depends on the browser guessing CP1252; the
  port decodes CP1252 and demoronize rewrites the punctuation to ASCII. Same
  rendered text, different bytes.
* `cjk_table` (P7.1) — a 3-byte CJK character in an aligned table. The
  reference demoronizes each byte of the sequence independently and emits
  `&aelig;`+`&yen;`+`&not;`; the port decodes UTF-8 first and the character
  survives.
* `cp1251_named`, `koi8r_named`, `cp1253_named` (P7.4) — Cyrillic and Greek in
  a legacy single-byte encoding, read with `--encoding`. The reference has no
  encoding option and emits the bytes for a browser to guess, so it cannot
  produce this output at all.
* `utf16le_ascii`, `utf16be_ascii`, `utf16le_bom` (P7.4) — UTF-16, with and
  without a byte-order mark. BOM-less UTF-16 of ASCII prose is *also valid
  UTF-8*, so both implementations used to accept it and emit a NUL between every
  letter. The reference has no UTF-16 concept; the port reads the BOM as a
  declaration and infers the rest from the NUL alignment.

The oracle for the eight encoding cases is `tests/encodingtest.rs`, which asserts
the decoded code points, that no C1 control character reaches the output, and —
for the UTF-16 cases — that no NUL survives. A unit test cannot rot into a false
pass
the way a suppressed corpus line can, and it fails the moment the decode
regresses rather than waiting for a byte comparison nobody reads.

### What detection does *not* do, and why the fixtures come in pairs

The legacy single-byte cases are deliberately registered **twice**, once under
the default and once with `--encoding`, and only the flagged half is declared
`NOGOLDEN`:

* `cp1251_cyrillic` / `koi8r_cyrillic` / `cp1253_greek` — **differential
  PASSes.** Under the default the port and the reference are *equally wrong*:
  neither can detect these encodings, both guess CP1252, and both emit the same
  mojibake. This is pinned deliberately. P7.4 did **not** change the default
  behaviour for non-Western text, and if a later change starts guessing, these
  cases stop matching the reference and say so.

  This is also the case that most looks like a defect and is not. CP1251 and
  KOI8-R disagree about nearly every byte above `0x80` and both are valid
  CP1252, so a wrong guess between them is not a small error — which is the
  argument for the user naming the encoding rather than for the tool guessing
  harder. See `legacy-archive/REMEDIATION-PLAN.md` for why the guessing is a separate project.

  Declaring these `NOGOLDEN` was the first attempt and the corpus runner
  rejected it: it asserts that a declared divergence actually diverges, and
  reported `matches the reference; the declared divergence is gone`. The runner
  was right and the declaration was the lie.

### Fixed, and pinned

Three divergences the fuzzer found were fixed and are now fixed corpus cases,
so a refactor cannot quietly reintroduce them. Their inputs live in
`tests/corpus/inputs/`, referenced by absolute path (`run.sh` passes an
absolute `INPUT` entry through unchanged).

* `table_type_replace` — a `--table_type` set that merged into the four
  defaults instead of replacing them. Getopt::Long's `n%` builds a fresh hash
  from the options actually present, so `--table_type DELIM=0` leaves only
  `{DELIM => 0}` and ALIGN, PGSQL and BORDER are not there at all; the port
  kept them at 1 and built a BORDER table out of a `+-+-+` drawing.
* `table_type_named` — the positive control for the above, so the previous
  case cannot pass by having switched table detection off wholesale.
* `pre_explicit_blank` — an explicit-quote `<pre>` block dropped everything
  after its first blank line. `split_end_explicit_preformat` buffers the
  continuation text and the join back was inside the branch that only runs
  when text remains. Related and fixed at the same time: the continuation also
  has to reach `apply_links`, or `*d*` stays literal where the reference emits
  `<em>d</em>`.
* `huge_paragraph` — one ~1.1 MB paragraph with no blank lines. Used to abort
  with fancy-regex's `BacktrackLimitExceeded` on the two CR-chopping patterns,
  and then to hang in `do_delim` once those were fixed. Differential against
  fresh Perl, not a golden: Perl renders it in about a second, so the case is
  about staying linear, not about a fixed byte sequence.
* `huge_paragraph_crlf` — the same at ~830 KB with CRLF endings, which is the
  only path that exercises the CR helpers.
* `delim_retry` — from `fuzz.py` seed 99. A `#` pair spanning a `</p><p>`
  boundary is rejected by the bold pattern's assertion, and the pair nested
  inside it still has to be picked up when the scan retries one character
  along; consuming the rejected candidate instead bolded the wrong `#`. The
  buffer also contains a link, which is what selects the in-link-context
  substitution path.

## `minimise.py`

A debugging aid, not a test. It shrinks a failing case to a smaller input that
still diverges, character-wise and then line-wise, and prints a `python3 -c`
one-liner that reproduces it. It bypasses `sanitise()`, so it can find inputs
outside the fuzzer's claimed domain; that is intended, and the reason several of
the divergences above were pinned down.

## What this harness can and cannot judge

`accept.sh` and `run.sh` are both **Tier 1** oracles in the old sense: strict
byte-identity, which is the correct and strict rule for ASCII input and the
documented output format. `run.sh` asserts it against the Perl module, which
since S13 makes it the cross-check rather than the gate; `accept.sh` asserts
the same identity against the frozen record of the same runs, which is the
gate. Neither is a universal correctness oracle, and two of the differential's
limits are worth knowing before trusting a green run:

- **The reference is broken on genuine UTF-8 input.** It decodes bytes as
  Latin-1, so `tfiles/utf8.txt` produces mojibake and a spurious `<sup>TM</sup>`
  where `tfiles/good_utf8.html` — the author's own expected output — shows the
  correct text. The port matches the golden. A byte diff there is reporting a
  Perl bug, not a port bug; such a case belongs in the golden list, not in a
  byte-identity case.
- **It generates no non-ASCII.** `fuzz.py`'s `sanitise()` rewrites every
  character `>= 0x80` to `?`, so all fuzz cases are ASCII by construction.

The tiers and the reasoning are in `legacy-archive/REMEDIATION-PLAN.md`, "Compatibility policy".
The non-Tier-1 oracles — the author's goldens, the property suite
(`tests/proptest.py`) and the allocation-budget suite (`tests/alloctest.rs`) that
do not reference Perl at all — are P12 and are implemented (`make proptest`,
`make alloctest`). The frozen self-goldens and the frozen example outputs are
the same kind of oracle, applied to the whole corpus (`make accept`,
`make examples`).
