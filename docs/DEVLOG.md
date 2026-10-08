# DEVLOG — the development log

What this file is:

The history ran as git commit messages, some of them long. On 2026-10-08
those long messages were trimmed to a pointer to this file, so nothing is
lost; this is the verbatim record, oldest first. The living rationale lives
in PLAN.md, CAPABILITIES.md, OFFERING.md, LANDSCAPE.md and DOCS.md.

## 01. chore: import txt2html-rs work under version control

Recorded 2026-09-30.

This tree was never version-controlled; every change so far survived only as
ad-hoc copies under /tmp. This is the baseline import, taken at the last
fully verified state:

  cargo test --release   33 tests, 0 failures
  tests/corpus/run.sh    46/46 byte-identical, 29/29 goldens
  fuzz.py                16 000 cases over 8 seeds, 0 mismatches
  GUI suite              30/30
  1 MB paragraph         654 ms, byte-identical to Perl

Work already in this commit, none of it attributable to a diff:

  P1-P3  corpus harness cannot report a false green; cases are linted;
         fuzz.py is seeded and permanent
  E1, E2  --table_type merged instead of replacing; explicit-quote <pre>
         dropped text after a blank line
  A4     PanicException re-exported so the GUI worker can catch a Rust
         panic and always signal completion
  A1     the two CR-chopping patterns replaced with chop_trailing_cr /
         chop_leading_cr, ending the ~500 KB BacktrackLimitExceeded panic
  A1b    the hang behind it: every \B and (?<!delim) in do_delim moved out
         of the regex and into a code predicate (delim_replace), plus the
         multi-character delimiter escaping that stops the port panicking
         where Perl itself dies

REMEDIATION-PLAN.md and ADVERSARIAL-FINDINGS.md carry the reasoning,
including two misdiagnoses worth not repeating. Future changes go on a
branch, one logical change per commit, verified before committing.

## 02. fix: non-ASCII delimiters tested as bytes, so (?<!é) rejected nothing

Recorded 2026-09-30.

A user delimiter that is not ASCII went through `delim as u8` twice: once
for the \B assertions and once for the general pattern's `(?<![delim])`.
Truncating the code point makes the test vacuous rather than wrong-looking.
`é` truncates to 0xE9, `ü` to 0xFC; neither byte appears in the UTF-8
encoding of the character it came from (0xC3 0xA9, 0xC3 0xBC), so nothing
was ever equal to it and the assertion always passed.

Visible as `ééwordé` being wrapped in <em> where Perl leaves it alone, since
Perl compares the preceding character. The neighbours are still compared a
byte at a time, which is right: Perl's \b works on the bytes it was handed,
and no byte of a multi-byte character is a word character.

The predicates move out of do_delim into named functions. They were inline
closures, so the differential tests could only pass in their own copies --
meaning a wrong predicate in do_delim was invisible to all nine of them. A
reverted fix still passed; I confirmed that before making the tests use the
production functions, after which the regression fails on `ééwordé`.

Verified: cargo test --release 33/33; corpus 46/46 and goldens 29/29;
fuzz 8 000 cases over 4 seeds, 0 mismatches (seed 424242's 1 known is the
E3 blank line); 1 MB paragraph byte-identical to Perl in 0.38 s; GUI 30/30.

## 03. chore: one verify command, and forbid unsafe code

Recorded 2026-09-30.

Two problems with how the checks were being run, both of which have
already cost time:

The corpus runner defaults MINE to target/debug/txt2html, and more than
once a stale debug binary -- built before a fix, not after -- made the
corpus report a green run against old code. make verify builds first and
exports MINE as the release binary it just produced, so no invocation can
pick up anything else. It also reinstalls the extension via maturin before
the GUI suite, which imports the compiled module rather than the crate and
was likewise testing whatever maturin last built.

The checks were also spread across whatever shell history had in it, with
the fuzz seeds and case count varying by hand. make verify fixes the seed
list, including 99 (which found the delimiter retry bug) and 424242 (which
carries the one known E3 divergence).

#![forbid(unsafe_code)] in the binary and the library. The crate has no
unsafe and does not need any; forbid rather than deny so it cannot be
locally overridden later by accident.

clippy warns instead of failing: the crate is not clippy-clean yet, and a
lint gate that is always red gets ignored.

## 04. style: cargo fmt

Recorded 2026-09-30.

1086 lines of drift, none of it deliberate. Kept as its own commit so the
mechanical churn cannot hide inside a behavioural diff -- the A1 review was
hard enough to read with real changes mixed in.

Before committing I checked that this is only formatting: every touched file
is byte-identical to its previous version once whitespace and trailing
commas are normalised away. The two exceptions are rustfmt wrapping a
match arm that overflowed 100 columns in a block, and unwrapping a
redundant block from a single-expression closure body -- both inert.

make verify: 33 Rust tests, corpus 46/46, goldens 29/29, 16 000 fuzz cases
across 8 seeds with 0 mismatches, GUI 30/30.

## 05. docs: compatibility policy -- Perl is the oracle, not the specification

Recorded 2026-09-30.

Reviewing the A1 diff surfaced a non-ASCII delimiter predicate that tested
bytes where the pattern compares characters, and fixing it forced the
question this commit answers: byte-parity with Perl has been the goal of
every test in the tree, and on non-ASCII input Perl is the bug.

tfiles/utf8.txt contains 門牌號碼規劃. The port emits it correctly and
matches the author's own good_utf8.html. Perl 5.38 as shipped emits
&eacute; then raw 96 80, a spurious <sup>TM</sup> from a CP1252 smart-quote
byte, and a truncated &cent. A second probe gets &aelig;&yen;&not; mojibake
plus an injected <em>. Byte-parity there would mean reproducing corruption
and inventing markup, and would have flagged correct behaviour as a bug.

So the plan now states three tiers. Tier 1, ASCII and the documented output
format, stays byte-identical and strict -- that is where the harness earns
its keep, and it found A1, A1b, E1, E2 and the retry bug. Tier 2, non-ASCII
and resource limits, may differ and must be better, verified by the author's
goldens and a property suite rather than by byte-comparison. Tier 3 has no
Perl analogue.

Tier 2 does not mean the port is always right: P7's CP1252 and
eight_bit_clean cases run the other way, and the plan says so.

New items:

  P12  a property suite that does not reference Perl at all -- no data
       loss, xhtml well-formedness, determinism, resource bounds. This is
       the oracle the tool lacks, and it is where A2's regression test
       belongs, because A2 leaks while producing correct output.
  P13  decide what "stands on its own" means for packaging. The CLI is a
       static binary; the GUI needs Python and PySide6. Rust + Qt is a
       different architecture, not a refactor, and it scopes Tier 3.

Also recorded, because both were going to mislead:

  - A2's 30 000-delimiter figures (142 440 KB, 13.6 s) are unverified. Only
    ~90 printable delimiter characters exist, so no real document generates
    30 000 distinct patterns in one process; it looks like a direct-call
    micro-benchmark. Not to be used as a target.
  - fuzz.py's manydelims fixture generates at most 60 distinct delimiters
    against a cap of 128, so it cannot reach the thrash path.
  - P7's step 1 asks to assert Rust == Perl for a UTF-8 fixture. That
    assertion cannot hold; it becomes a golden assertion.
  - The headline table still said 21/21 tests and 38/40 corpus, and both
    sequencing sections still said 43/43.

Coverage is thinner than the green runs suggest: cases.sh uses 2 of 65
upstream files, none of the four non-ASCII ones; sanitise() rewrites every
character >= 0x80 to ?, so all 16 000 fuzz cases are ASCII; emoji and
combining marks appear nowhere; and the crate depends only on fancy-regex
and pyo3, so display-width heuristics count bytes and are wrong in both
implementations. A single oracle cannot see that class. The corpus README
now says plainly which questions it can and cannot answer.

Documentation only; no code touched.

## 06. P12: Perl-independent property suite and allocation bounds

Recorded 2026-09-30.

The tool had no oracle for anything Perl does not or cannot demonstrate, and
so no way to see a defect that leaves the output correct. Both halves of P12
address that and both are now part of `make verify`.

proptest.py (no new dependencies, Python 3 + ElementTree):

- No data loss: unescaping the output recovers every input character. This is
  what catches silent truncation, and it needs no reference.
- --xhtml output parses as XML. Perl's own CJK output does not, so the
  reference could never have asserted this.
- Determinism, and reconversion of --extract text.
- No panic on hostile input.

The reference is not consulted anywhere in it, which is the point: it is the
oracle for Tier 1 only, and it mangles non-ASCII input. 30 known-open checks
are counted and attributed to A8 rather than ignored.

alloctest.rs (counting global allocator):

- a1_long_paragraph_allocation_is_linear
- a2_retains_one_regex_per_distinct_pattern
- a2_retained_bytes_do_not_scale_with_delimiter_count
- the_instrument_works, which validates the allocator before trusting it

A2 is measured rather than assumed, and the plan's figure was wrong. Retention
is 3919 B per distinct pattern by direct call and 3451 B per additional
delimiter from real document content, so the leak is real and linear -- but a
document can only induce one pattern per distinct delimiter character, and
convert.rs:2636 reduces that to ~28 reachable delimiters, giving a bounded
~0.3 MB per process. The plan's "a 10 MB file approaches 1.3 GB" came from a
direct-call micro-benchmark, not from any input. Severity High -> Low; A2 is no
longer the most urgent item. The unreachable 30000-delimiter row is marked as
such.

Budgets are asserted as marginal cost on a fresh thread, not as totals: the
cache is thread_local, so a second measurement in the same thread reuses the
first's compiled patterns and reads as zero. Both lessons are written up under
P12, since they are the likely mistakes for the next person.

Exceeding a known budget prints KNOWN-OPEN [A2] and attributes it to the plan
item; an unattributed excess still fails. A permanently red gate gets ignored.

## 07. A2: bound the regex leak to the fixed patterns, by type

Recorded 2026-09-30.

`ascii_re_cached` returns `&'static Regex` and therefore cannot free what it
hands out; the 128-entry cap does not help, because clearing a map of
`&'static` drops no memory. The only thing that made the leak bounded in
practice was that its callers pass fixed literals -- a claim made in a comment,
and false for the one production caller that builds its pattern from the
document.

The fix is the signature: `pat: &'static str`. If the input is `&'static` then
the pattern set is fixed by the source, so there is nothing unbounded to leak,
and a caller holding a document-derived pattern gets a compile error rather
than silently growing the cache. The seven dynamic callers now use `ascii_re`,
which returns an owned `Regex`.

This is stronger than fixing the one bad call site and leaving the invariant as
a comment, and weaker than changing the return type at all 39 call sites: a
bounded leak remains for the 31 fixed literals, measured at 341 KB, and
valgrind will still flag it. The plan says so rather than claiming the item is
now leak-free.

Measured, `make alloctest`:

- marginal retention per additional distinct delimiter: 3 451 B -> 4 B
- fixed-literal cache: 31 patterns, 341 461 B, budget 512 KB

On a 960-row table-heavy document with `--make_tables`: output byte-identical,
0.23 s before and after, peak RSS 6.5 MB -> 6.4 MB. The recompilation the fix
introduces is not measurable, because `[;]` is cheap to compile and the
alternative was retaining every one of them for the life of the process.

The allocator test that established the defect called the cache with
runtime-built patterns, so the new signature rejected it. That is the guarantee
working, and the test is now the residual-literal check that the fix makes
possible.

## 08. A3: bound the numeric options, from one table the GUI also reads

Recorded 2026-09-30.

`tab_width=0` divided by zero and exited 101; a large value asked the
allocator for an impossible size and exited 134 with SIGABRT, which is
uncatchable even by A4's GUI handler. The GUI could produce 0 directly, because
every numeric option had a spin-box minimum of 0.

`Options::validate` now enforces a table, `NUMERIC_RANGES`, and both entry
points call it before doing any work: main.rs prints the message and returns
exit 1, following the convention already used for argument errors, and
python.rs raises ValueError, which the GUI shows as a message while A4's worker
still reports completion. Nothing reaches stdout when a value is rejected.

The bounded set is not the one the plan predicted, and the difference is worth
recording. Reading every use of all nine numeric options found two hazards
rather than one. Writing the test first found the second:

- `preformat_whitespace_min` is interpolated into a regex quantifier, so a
  large value made the engine fail to compile the pattern and panic. Same shape
  as tab_width=0, reachable the same way, and the plan did not list it.
- `short_line_length` and `par_indent` are compared only, so bounding them
  would make the port stricter than the reference for no safety gain. They stay
  unbounded, and a test now asserts they accept i64::MAX.

The engine's quantifier limit is pattern-dependent -- `\s{200000,}` is rejected
while `[A-Z]{200000,}` is accepted -- so 999 is a margin checked by tests, not a
threshold derived from the engine.

The GUI half matters most, since that is where the original report came from.
`option_specs` reports the accepted range from the same table the engine
validates against, and OptionSpec reads it instead of restating 0 and 999 in
Python, so a spin box cannot offer a value the engine rejects and the two
cannot drift apart again.

tests/cliexit.rs runs every numeric option against 0, negative values, the
ceiling and one past it, asserting no exit is 101, 134 or 139, and checks each
bounded option's range and message. The A4 GUI test provoked its panic with
`tab_width=0`, which no longer panics, so it now uses an invalid
`custom_heading_regexp` -- the panic P4 has not closed yet -- and says so.

The O(k*tw^2) tab expansion is left undone on purpose: it is a performance
problem, it needs a timing test to be trustworthy, and it moves to P6 where the
other allocation costs now are.

## 09. A5: remember the encoding a file was read with, and write it back

Recorded 2026-09-30.

Opening a CP1252 file and saving it transcoded the file. The read path decoded
Latin-1 and threw the fact away; the write path defaulted to UTF-8. For CP1252
that is lossy rather than untidy, because 0x80-0x9F are Windows punctuation and
an em dash there, and Latin-1 turns them into C1 control characters: `Café —
naïve "quotes"` came back as `Caf\xc3\xa9 \xc2\x97 na\xc3\xafve \xc2\x93quotes\xc2\x94`.

read_text_file now returns (text, encoding) and the window writes the text back
in it. detect_encoding decides by the converter's own rule -- try UTF-8, fall
back to Latin-1 -- rather than trusting a declared charset or a locale, so the
GUI cannot disagree with the converter about what a file contains. Loading a
second file replaces the remembered encoding instead of keeping the first
file's, which would be a corruption bug in the other direction.

"Save HTML" writes UTF-8 deliberately: the HTML is text this program produced,
not a transcription of the input, so writing it in the source file's CP1252
would fail outright on any character CP1252 cannot represent. The real problem
at that call site is that the generated HTML declares no charset, which is
P7.2, and this plan listed it as a transcode site when it is not one.

Two behaviours pinned by test rather than changed:

- A byte-order mark is kept, because the converter keeps it: it emits the U+FEFF
  into the output. Stripping it in the editor would make the preview disagree
  with both the saved file and the reference.
- Line endings are not preserved, because QPlainTextEdit normalises CRLF to LF in
  both directions before files.py sees the text. The module round-trips CRLF
  byte for byte; the window does not. That is a real byte-level loss of the kind
  this item is about, and whether an editor should preserve the line endings of
  the file it opened is a decision rather than an obvious bug, so it is recorded
  in the plan and pinned by a test instead of quietly fixed here.

## 10. A6: drop the conversions that a keystroke has already made pointless

Recorded 2026-09-30.

The window discarded a stale result, but it went on doing the stale work, and
each queued job holds its own copy of the document. Converter.convert now calls
pool.clear() before queueing, which drops every runnable that has not started.
It is in that method rather than in convert_now so no future caller can forget
it, and it supersedes the cancel_pending the plan noted was defined and never
called.

Measured, 20 conversions of a 793 KB document offered over 3 s of typing at the
window's 300 ms debounce, with clear() removed for the comparison:

                        with fix   without
  conversions performed     3 of 20   20 of 20
  CPU time                     1.36 s    7.69 s
  peak RSS                       97 MB   118 MB
  wall clock to settle         3.51 s     4.34 s

The plan predicted 6.4 s of wall clock for 1.25 s of useful work; the real
saving is 0.8 s, because settling after the last keystroke is bounded by one
conversion either way -- the window discards stale results, so a user waiting
for the preview waits for the newest one and cancelling does not speed that up.
What the fix removes is 85% of the CPU and 21 MB, which is what keeps the
machine usable while someone types and what stops the backlog growing the
longer they type. The plan's arithmetic is corrected rather than repeated.

The pool expiry timeout stays at its 30 s default: it reaps threads, and
clear() already removes the queued work, so changing it would be motion without
effect. A test asserts the default rather than a new number.

BacklogTests covers it, and writing it needed one thing worth recording:
QThreadPool.waitForDone blocks the main thread while results arrive as queued
signals on that same thread, so a test that waits with it observes nothing and
asserts nothing. The first version of this test passed with the fix reverted.
It spins the event loop instead and stops after the pool has been idle for a
moment, which also covers the gap between start and the thread spinning up.
Reverted, it fails with "8 of 8 queued conversions ran".

## 11. A7: a save that cannot happen is an error, not a new filesystem layout

Recorded 2026-10-01.

write_text_file did target.parent.mkdir(parents=True, exist_ok=True), on the
theory that a save should always succeed. It should not: it is called from a
save dialog, which is exactly where a mistyped path happens. Measured, saving to
newtree/a/b/c/out.html created five directories -- the plan had recorded three.
It now raises FileNotFoundError, which the window already catches and reports as
"Cannot save", so the user is told something they can act on. Same path, now:
nothing created, and a clean failure.

The removed behaviour turned out to be load-bearing, and was found by a hang.
test_saving_twice_reuses_the_chosen_name wrote to tmp/out/book.html and relied
on the mkdir to create `out`; with it dropped, the save failed, that test did not
stub the message box, and the suite wedged on a modal for thirty minutes rather
than failing. It now writes into the directory that exists, because what it is
about is the second save reusing the name it was given, not directory creation.
A test that passes for a reason other than the one it names will not fail when
that other reason is removed, and if it also leaves a dialog open it hangs
instead of reporting. Both new tests were confirmed to fail with the mkdir
restored before this fix was kept.

GUI suite: 45 tests, 0 failed, 1 skipped.

## 12. E3: fix the trailing-blank-line separator accounting

Recorded 2026-10-01.

The paragraph slurper in the Rust port split on /\r?\n\r?\n/ but treated a
string that was nothing but separators as producing a single empty paragraph
instead of producing nothing, because it only trimmed the final trailing empty
field when there was text after it -- a trailing empty field after consuming
the entire input slipped through. Perl drops *every* trailing empty field from
split(); our split_blank_lines now does the same, and is exhaustively checked
against Perl's behaviour over length<=5 strings of {a, space, \r, \n} (1365
inputs), with explicit repros for the E3 case.

The corpus fuzzer's KNOWN_DIVERGENCES listed this exact defect (two extra
blank lines when the input ended in a CR-terminated blank sequence). That entry
was removed as part of fixing it: it was a suppression keyed to the symptom, and
after the fix that suppression would have hidden a regression of the same bug.
KNOWN_DIVERGENCES is now empty; any future divergence must be diagnosed and
either fixed or re-added with a precise signature.

Also fixed the test that was relying on the old behaviour to be correct in its
assertions (it was written by reason and reason lost), replacing its check with
Perl-derived ground truth.

Lib tests: 14/14 pass (including the new exhaustive split test). GUI: 45/45
pass. Integration: passes with no changes to corpus expectations. verify: OK.

## 13. P12: serialise the allocation tests, which raced on a global counter

Recorded 2026-10-01.

`the_instrument_works` failed intermittently under `cargo test --release`,
reporting that a 1 MiB vector was not released: "live bytes moved by
100614". The true residual is zero, so the instrument was not broken and
neither was the engine. The counters LIVE and CUMULATIVE are
process-global, and libtest runs test functions in parallel threads by
default, so a sibling test converting a 1 MB paragraph (a1:
68356658 -> 168002813 bytes) or a delimiter-heavy document (a2:
69588897 -> 105181057) allocates tens of megabytes inside another
test's measurement window. The 64 KB residual tolerance cannot survive
that.

This is why the symptom looked arbitrary: it passed 3/3 when the
alloctest binary was run on its own and failed only inside full-suite
runs. That is thread scheduling, not configuration, so no invocation of
`make test` was ever going to be reliably green. `make alloctest`
passed --test-threads=1 and so appeared healthy, while `make test` did
not and `verify` runs `test` before `alloctest` -- which is why the gate
was red.

The fix is MEASURE_LOCK, taken for the whole test body and not inside
`measure()`. Tests also allocate outside their measured closures --
building the 1 MB fixtures, delim_document, printable_delimiters -- and
a lock scoped to measure() would leave exactly those allocations able to
land in a neighbour's window, which is the same bug in a narrower
window. Spawned measurement threads deliberately do not take the lock;
their parent holds it for them and taking it twice would deadlock.
Poisoning is ignored so that a tripped budget stays one failing
assertion instead of cascading into every later test.

The budgets now mean the same thing at any --test-threads, so
`cargo test` and `cargo test --test-threads=1` agree and correctness no
longer depends on the Makefile remembering a flag. Measurements are
unchanged: A1 68356658 -> 168002813 (2.5x), A2 31 fixed patterns
+341461 B, marginal 4 B per additional delimiter.

Verified: alloctest 25/25 under default parallel; full `cargo test
--release` 25/25; fmt clean; clippy unchanged at 3 pre-existing
warnings in this file; make test, proptest, alloctest, corpus, gui all
green. `fuzz` not run here -- 8 seeds x 2000 cases is ~90 min of
sustained load and this machine rebooted during it earlier; 1200 cases
across all 8 seeds passed clean at ~32 MB peak.

## 14. docs: reconcile the plan, the survey and the Makefile with what is fixed

Recorded 2026-10-01.

The status text had drifted behind the work. The header still said "E3 and
P4-P13 are not started" while E3, A2, A3 and A5-A7 were all landed, and
§0.1's table marked E3 open on the same day it was fixed in ce8f949. A
reader starting from the documents would have re-derived a picture of
the project that was wrong in both directions -- understating what was
done, and, worse, listing E3 as open when the fix had just landed and
removed the suppression that had been hiding it.

Re-baselined against measurements taken today rather than by reading old
numbers. Re-verified: cargo test --release 45/45 (14 lib, 5 linktest, 7
optionstest, 9 paratest, 4 alloctest, 6 cliexit), GUI 45/45 with 1
skipped, corpus 46/46 with 29/29 goldens, proptest OK, alloctest 4/4,
fmt clean, clippy 0 errors / 51 lib warnings, upstream Perl t/*.t
102/102 across the 7 functional files.

Corrections that are more than bookkeeping:

* §0's speed row cited a 2 MB document that is no longer in the tree, so
  the figure could be neither reproduced nor refuted. Replaced with the
  two fixtures that do exist: big_para 1.1 MB is Rust 0.37 s vs Perl
  0.18 s (~2.1x slower), and big_para_crlf 0.8 MB is Rust 0.37 s vs Perl
  0.41 s, i.e. the port is *faster* on CRLF input. The single old figure
  hid that, and P6's "~2x slower" is confirmed in direction while being
  wrong to present as uniform.

* §0 ended by claiming every defect lay outside the transformation
  logic. §0.1 has said for a week that this is wrong, and the two
  sections contradicted each other in the same file. §0 now states the
  correction instead of the claim.

* lib.rs told users the output is byte-identical "for every file of the
  upstream tfiles corpus". It is 29 of 31; the other two are cases where
  upstream's own comparison is looser than byte-comparison, so the port is
  right and the goldens are not. A user-facing doc should not overstate
  the thing it is reassuring you about.

* The Makefile credited `make alloctest`'s --test-threads=1 with
  preventing false greens. That flag was hiding a race, and the same
  harness was red through `make test`. Corrected, with the serialisation
  named as what actually makes it trustworthy.

* The Makefile described seed 424242 as carrying "the one known
  divergence (E3)". E3 is fixed and KNOWN_DIVERGENCES is empty; 424242
  is now a regression seed for that input shape, and saying otherwise
  would invite someone to drop it.

* Removed txt2html-rs/xhtml_sample.html, a tracked 0-byte file from the
  initial import. Nothing references it -- the corpus case of that name
  writes into RUNDIR -- and an empty file sitting in the crate root
  looks like an expected-output golden, which is the exact shape of
  false green this project has already been bitten by twice.

ADVERSARIAL-FINDINGS.md keeps its 2026-09-29 severities, since it is the
point-in-time record of that pass, but now carries an S-to-A status
table so §7's recommended order cannot be misread as a to-do list, and
its regression line is labelled as of the pass rather than as current.

Not re-run here: `make fuzz` (see 4f48dbd). Every other verify step is
green, so `make verify` is expected to pass end to end, but that has not
been observed on this machine in one run.

## 15. fix: make fuzz able to fail, and survive a hanging converter

Recorded 2026-10-01.

Asks whether anything besides the machine was stopping `make verify`.
Two things were, and both were in the fuzz step itself.

**`make fuzz` could not fail.** The recipe ended each invocation in
`| tail -1`, to print just the summary line. A pipeline reports the
status of its *last* command, so `tail` exited 0 whatever the fuzzer
did. A run that crashed, found a mismatch, or stopped early was
reported to `make verify` as a pass. Demonstrated: a script exiting 3
through that pipeline yields status 0.

This is the P1 false-green shape in a third place, after run.sh's
crashed-run PASS and the alloctest race. It is also why the plan's
"16 000 cases across 8 seeds, 0 mismatches" was not evidence of
anything: on this harness an aborted run and a clean one were
indistinguishable, so that figure should be treated as unestablished
until it is reproduced with the fixed target.

The recipe now captures each seed's output and status separately,
prints the summary line as before, prints the whole log for any seed
that failed, and fails the target if any seed did. Checked both ways
against a stub fuzzer: failing stub -> "fuzz: FAILED", make error 1;
passing stub -> "fuzz: OK", exit 0. `fuzz` was the only recipe in
`verify` that piped; nothing else swallows a status.

**One hanging converter ended the whole run, silently.** Both
converters are invoked with `timeout=120`, but nothing caught
`subprocess.TimeoutExpired`, so it propagated out of the loop and
terminated that seed after however many cases it had reached. That is
not hypothetical: P5, the inherited hang, is still open, so a timeout
is a plausible result -- and combined with the pipe above it was
reported as success.

A timeout is now caught per case, so one bad input costs one case
instead of the remaining ~1 900. The two sides are not equal: the port
hanging is a defect in the port and fails the run on its own, while
the reference hanging says nothing about the port and is skipped like
any other reference refusal -- but counted, printed, and included in
the exit status rather than folded into a clean-looking "0 mismatches".
Timeouts are now in the summary line too, so an aborted-looking run
cannot be mistaken for a clean one by reading the output either.

Not addressed here, for the record: the run is ~99 minutes, single
threaded, on a 20-core box (measured 0.37 s/case, 2 000 cases/seed,
8 seeds). Each seed mkdtemps its own scratch directory, so the seeds
could run concurrently and bring that to ~13 min, but that changes the
character of the gate and is a decision rather than a fix.

## 16. P15: run.sh could not fail -- the primary gate reported FAIL=46 and exited 0

Recorded 2026-10-01.

Asked whether anything besides the machine was stopping `make verify`.
This was it, and it is the worst of the set, because run.sh is the
primary differential gate: `make corpus` runs it and `make verify`
depends on `make corpus`.

run.sh counted `pass`/`fail` and printed PASS=46 FAIL=0, and compared 29
goldens -- but it never exited on either. Its only two `exit`
statements, present since the initial import 3bfc2c1, were the `exit 2`
guards on the reference smoke check. That is exactly why the script
looked guarded: a reader sees two deliberate exits and concludes the
failure paths are handled. The case and golden failure paths fell off
the end, so the status was whatever the last `echo` returned.

Demonstrated with a stub converter that exits 0 and writes wrong output:

  PASS=0 FAIL=46
  GOLDEN: 0/29 compared, 29 differing
  exit 0

Every case failed and `make corpus` reported success. The Tier 1
invariant -- "46/46 and 29/29", restated in the plan after almost every
item since Phase 0 -- was being *reported*, not *enforced*.

This is P1 one level up. P1 made a single crashed case count as a pass;
P15 made every crashed case count as a pass without anyone counting.

Both entry points now compute a status and exit with it. Verified in
both directions: real corpus exits 0 at PASS=46 FAIL=0 with 29/29
goldens; the stub exits non-zero at FAIL=46 with 29 differing.

P17 in the same commit: `run.sh <stem>` hit `GOLDEN_N: unbound
variable` under `set -u`, because GOLDEN_N was only initialised in the
full-run branch and golden_check increments it. It died *after* printing
PASS and "GOLDEN pass", so it failed misleadingly, and single-stem mode
was unusable for the 28 of 46 cases that have a golden. No gate invokes
that path, which is the P2 shape -- a correct-looking path nobody runs.

## 17. docs: Phase 0b -- the gate-integrity findings, and the rule they produce

Recorded 2026-10-01.

Documents P14-P20 and records the audit that produced them. The plan
grew a phase because the phase it already had was closed on a false
premise: Phase 0 was finished on 2026-09-30 believing the harness was
trustworthy, and it was not.

The rule the section exists to establish:

  A gate that has never been observed failing is not a gate. It is a
  script that prints reassuring text.

So the bar for every check here is no longer "does it pass" but "show me
it failing, on purpose". Deliberately breaking each gate and watching
the exit status took about twenty minutes and found four defects, two of
them older than any recorded green run. The bug is always the same shape
-- a status discarded on the way out: a pipe reporting its last command,
a script ending on a successful echo, a counter printed for humans and
never compared to anything. None of them look wrong when you read the
output, which is why reading the output was never going to find them.

Recorded, with provenance, because it says how much history to trust:

* P15 and P17 date from the initial import 3bfc2c1, so they were present
  for every green figure this repository has ever recorded.
* P14 arrived with 1035268, "chore: one verify command". The gate shipped
  structurally incapable of failing in the same commit that made it the
  gate.

Consequences propagated through the document rather than papered over:

* The "16 000 cases, 8 seeds, 0 mismatches" figure is marked **VOID** in
  the summary table and in P18. It was measured on the P14 harness, where
  an aborted run and a clean run were indistinguishable from the exit
  status. The honest observed figure is the 1 200 cases actually run.
* Every green row in the summary table that predates this is labelled for
  what it is.
* The machine reboot is recorded as external, with the evidence that
  rules out the obvious hypothesis (flat ~32 MB across all eight seeds; a
  timeout is CPU-bound, not memory-bound). Cheaper than letting the next
  person hunt a memory bug that does not exist.

New open work, ordered in the plan's next-steps list:

* P19 -- run the eight fuzz seeds concurrently. 99 min -> ~13 min on a
  20-core box, and it is what makes P18 possible at all. The two ways it
  can reintroduce a false green are written down in advance: an unchecked
  `wait`, and the shared RUNDIR/fuzz-fail directory colliding between
  seeds.
* P18 -- re-measure the fuzz figure once the harness can report failure.
* P20 -- guard CLI[]/EXTRA[] alignment. They are aligned today (46/46),
  but that is a fact about the current file, not an invariant anything
  checks, and violating it is P2 exactly.

The corpus README's "two ways this reports success wrongly" is now four,
with the stub-converter test written out so the next person can re-run it.

Gate re-verified after all of it: fmt clean, clippy 0 errors, test 45/45,
proptest OK, alloctest 4/4, corpus 46/46 with 29/29 goldens, gui 45/45.
fuzz is unchanged and still the one step not run to completion.

## 18. P19: run the eight fuzz seeds concurrently, and close a false green found on the way

Recorded 2026-10-01.

8 x 2000 cases took ~99 minutes one seed at a time, which is why the fuzz
gate had almost never been run to completion. Eight at a time it is 5m30s
wall / 39m18s CPU, so it can now live in `make verify` and actually be run.

Parallelising a gate is exactly when it is most likely to acquire a new way
to lie, so both hazards the plan named are closed rather than deferred.

Status collection does not go through `wait $pid`. Each seed writes its
status to a file, and the statuses are read back in seed order afterwards.
That keeps the report stable regardless of which seed finishes first, and
it means a seed that dies before writing its status is *reported* rather
than absent -- with `wait $pid` a killed seed vanishes from the results and
the remaining seven look like a clean sweep. A missing status file is a
failure, and so is a seed whose log has no "fuzz:" summary line, on the
grounds that a run which printed nothing has proved nothing.

P21, found while testing the above and fixed here. The fuzzer counted
mismatches but never counted *comparisons*, so a run in which the
reference refused or hung on every case reported "0 mismatches" and
exited 0 -- the P1 bug, still live in the P14-fixed harness, and the same
one a wiped /tmp already caused once. Demonstrated against the previous
commit: reference refuses all 5 cases, port is fine, return code 0. Now
returns 1 and says why. The summary line also discloses the compared count
and splits port from reference timeouts, because a reference hang was
counting as a port defect: `return 1 if (mismatches or timeouts)` failed
the run on the oracle's misbehaviour and would have wasted a 5-minute P18
sweep blaming the port for it.

P19's other hazard, the shared failure directory. `_save` named cases
`<source>-<n>`, with no seed, so two seeds reaching the same source file at
the same case index wrote the same path. Demonstrated: seed 7 overwrote
seed 99's saved input and left 3 files where there should have been 6. Names
are now `<source>-<seed>-<n>`, `make fuzz` gives each seed its own
`--fail-dir`, and the input is kept even when the reference has not written
its output yet, so a port timeout is still worth saving.

Gate tested by breaking it, per the Phase 0b rule. All of these must fail
and do: 1 bad seed of 8; 3 bad seeds of 8; exit 0 with no summary line;
1 silent seed of 8; a seed that SIGKILLs itself. All-clean still passes.
Concurrency is measured, not assumed -- 8 seeds x 2s: 16.35s at
FUZZ_JOBS=1, 2.25s at FUZZ_JOBS=8, peak overlap 8 of 8 from a trace.

P18 falls out of this. The full sweep now runs: 8 seeds x 2000 cases,
16 000 compared, 0 mismatches, 0 skipped, 0 timeouts. First end-to-end
`make verify: OK` in the project's history, 7m49s, on a harness whose
failure modes are demonstrated above.

## 19. docs: P18 done on a trustworthy harness, and P22 -- a new panic divergence

Recorded 2026-10-01.

P19 made the full sweep affordable, so P18 could be done properly rather
than approximately. The void figure is replaced with a measured one:

  8 seeds x 2000 cases = 16 000, 16 000 compared, 0 mismatches,
  0 skipped, 0 timed out. 5m30s wall, 39m18s CPU.
  make verify end-to-end: OK, 7m49s.

That is the first figure in this project worth quoting, because the
harness's failure modes are demonstrated rather than assumed -- five
separate ways to make it report a false green were injected, and each is
caught. The old 16 000 was the same number from a harness that could not
tell a clean run from a dead one, which is why the number alone was never
the point.

P22 is new, and is the reason this is not only a status commit. Found in
the `make verify` output while confirming P18: a `panicked at
convert.rs:209` line, from a user-supplied pattern that does not
compile.

  --custom_heading_regexp 'a('
    port:       panicked, exit 101, NO output file
    reference:  "Unmatched ( in regex; marked by <-- HERE", exit 0, written

The port is less forgiving than the thing it ports. Recorded, not fixed,
because the fix is a judgement call: warn-and-skip matches the reference
and keeps the corpus byte-identical, while failing cleanly is better
engineering and diverges. That decision is flagged as unmade rather than
quietly taken.

Worth noting what this is *not*: the panic did not make `make gui` fail,
and `make gui` is right. test_gui.py provokes this exact panic on purpose
to exercise the GUI's handler and asserts the preview shows "stopped on
invalid input" and the window stays usable -- that is the A4 work, and
pyo3's PanicException re-export exists so a Python caller can catch it. I
had written the opposite into the plan, checked the test before
committing, and corrected it. The real divergence is the boundary: handled
in Python, unhandled at the CLI, where a shell user gets exit 101 and no
document.

The fuzzer structurally cannot find this -- it samples option values from
a generator that only produces valid ones. Recorded as a gap in the
fuzzer's strategy, since "the fuzzer missed it" is otherwise read as "the
fuzzer says there is nothing there".

Backlog reordered: P22 and P20 first (newly found correctness issue, then
three lines of guard), then P13, then A8, then the Phase 2-4 items.

## 20. docs: rules for completing tasks, and P22 reclassified -- the tier table already decided it

Recorded 2026-10-01.

The process had started costing more than the work: a 1 800-line plan
where 15% is gate-audit narrative, a new section per finding, and the
next action progressively harder to see. Five-minute tasks were turning
into an afternoon of bookkeeping. "How to complete a task" is the fix,
and its first rule exists because of a specific failure of mine.

P22 -- an invalid user regexp panics the port -- was written up as an
undecided judgement call, with two options presented: warn-and-skip to
match the reference, or fail cleanly and diverge. It was never a decision
at all. The compatibility policy in this document has a three-tier table
and "error handling" is named explicitly in Tier 2: may differ, must be
better, Perl is the defect. I had the answer in the repository and
declared the question open anyway, then asked the user to make a call
that was already made.

So the first rule is: classify against the tier table before calling
anything a divergence, and when a tier settles it the item is not open,
it is unimplemented.

The second reason that draft was wrong is worth keeping. It argued that
warn-and-skip "keeps the Tier 1 corpus byte-identical" -- true, and
meaningless, because all five corpus cases passing a custom_heading_regexp
use valid patterns. No Tier 1 case constrains the behaviour either way. An
argument that cannot lose is not an argument, and it was doing the work of
a real trade-off in the writeup. Retracted in place.

On the reference itself, since that was the question. It is fallible in a way
that matters here. TextToHTML.pm:4298 and :4323 interpolate a user-supplied
pattern straight into a match operator, unguarded, once per line. The
"Unmatched ( in regex" it prints is not the reference's message -- it is
Perl's own runtime warning, and the string appears nowhere in the
reference's source. So the reference has no error handling there at all; it
degrades gracefully by accident, because Perl warns where Rust aborts. It
also recompiles the pattern per line. Nothing here is a compatibility
target worth copying, and the rule now says so generally: "the reference
does it" is not a reason, and reference behaviour that looks like error
handling gets a second look before it is copied.

Also recorded, from the conversation that prompted this: the fuzzer is
convoluted, and the evidence is that its exception machinery never fires.
KNOWN_DIVERGENCES is an empty list, and 0 of 16 000 cases took the
skipped path. The distinction worth preserving when simplifying it: that
machinery is dead, but smoke_check, the compared counter, the port/reference
timeout split and the status aggregation are not -- each one closes a
false green that was actually observed (P1, P14, P21). The cut is the
exception machinery, never the guards. The reason it accumulated is that
the fuzzer encodes policy -- what counts as a known divergence -- in the
same place as mechanism, and policy buried in a tool cannot be reviewed as
policy.

## 21. P22: reject an invalid user regexp with a diagnostic, not a panic

Recorded 2026-10-01.

`--custom_heading_regexp 'a('` aborted the CLI with exit 101 and a Rust
backtrace note. The pattern reached `Convert::re` and was compiled lazily,
mid-conversion, through `unwrap_or_else(panic!)`. The reference instead
interpolates it into a match, warns, and carries on with the pattern silently
inactive -- accidental behaviour, but it means the reference is not the oracle
for a panic, and the tier table settles this: validate up front and fail
cleanly, diverging deliberately.

Options::validate now compiles each caller-supplied pattern before any
conversion starts. Both entry points already called it, so the CLI and the
Python bindings (a ValueError) are covered by construction. The diagnostic
names the option, quotes the pattern, and passes through the engine's own
wording and position:

  txt2html: custom_heading_regexp: invalid regular expression "a(":
            Parsing error at position 6: Opening parenthesis without closing

Also closes P4 part 2. A `/pattern/` link-dictionary entry took the same route
through LinkParser::add_regexp and aborted identically; it is now reported and
skipped, which is what the reference does and the better message. That was the
last user-reachable panic: add_literal and add_glob both escape their input
first, so /pattern/ was the only dictionary form that could fail.

Three things beyond the direct fix, each because the obvious version was not
enough:

- try_compile_pattern is now the only function in the crate that turns a
  pattern into a Regex. Convert::re and links::compile_pattern had coexisted
  with the flag prefixing and the pattern translation in opposite orders. They
  agreed, and nothing was checking that they agreed.

- The list of regexp options is hand-written, so a test compares it against the
  CLI option table and checks each declared option really does reject a bad
  pattern. Demonstrated: adding a fourth name to the table without adding it to
  user_patterns fails two tests. Without this, the next person to add a regexp
  option reinstates the panic with nothing failing.

- The A4 GUI test used to provoke a real engine panic, and its own docstring
  predicted it would need an injected fault once the last reachable panic
  closed. It now raises PanicException directly to test the worker handler, with
  a second test asserting the new clean-error wording. Reaching for a new
  crashing input after each fix meant the test was verifying whichever defect
  happened to be open that week.

A flake in the new tests is worth recording: cliexit.rs pipes a document to the
child, and a pattern rejected up front means the child exits before reading it,
so the parent's write got EPIPE. 1 run in 12, caught by the first full
`make verify`. The rate matters more than the bug -- it is a race the child
wins less often under the parallel load verify puts on the box, so it would
have been red about one time in twelve, which is the rate at which a team
learns to re-run rather than read. Now 20/20 green.

Tests: cargo test --release 52/52, corpus 46/46 and 29/29 goldens, fuzz 16000
compared / 0 mismatches, GUI 46/46 (1 skipped), make verify OK in 9m46s. Tier 1
unchanged; the divergence is confined to the error path, as the tier table
requires.

## 22. Simplify the fuzzer, and fix a false green found while doing it

Recorded 2026-10-01.

Two things, one of which was not simplification.

Removed: KNOWN_DIVERGENCES and the ~90 lines that matched a mismatch against
recorded option sets and output shapes, plus PERL_DRIVER, 25 lines of embedded
perl that has been unreferenced since the initial import commit. 719 lines to
659.

The suppression table was empty, and stayed empty, because every entry was
deleted in the same change that fixed the defect it described. Its one long-lived
occupant -- the CR path's extra blank lines, fixed as E3 -- was recorded as a
*signature* ("the port's line list is the reference's with two blank lines
spliced in") rather than as a defect. A signature outlives its fix by
construction: left in place, it suppresses the regression that comes back. The
rule is now the corpus's own, and it is stated in the code: a divergence is a
failing gate and becomes a passing one by being fixed. Anything that needs
tracking goes in cases.sh, as a case that currently fails.

The "reference refused this option set" skip is gone, and that one was a live
false green. A reference exit was counted as "skipped" and stepped over, so a
reference failing on some cases shrank the sweep silently. Measured with an
injected reference that fails on inputs of one size modulo four: the old code
printed "12 compared, 0 mismatches, 8 skipped" and exited 0, having thrown away
40% of the run. The new code prints 8 refusals and exits 1.

build_case only draws from hand-checked choice lists, so a refusal was
unreachable by construction -- 0 in 16 000 cases across eight seeds. An
unreachable branch whose only effect would be to reduce the number of things
checked is not a safety net. smoke_check still catches a total failure before the
loop and by name, compared == 0 catches it independently, and neither can see a
partial one.

P20: a case wired into CLI[] but not EXTRA[] never ran, because the loop
iterates EXTRA. That is the P2 shape -- a case that does not run cannot fail --
and nothing checked the tables agreed. alignment_check now diffs the key sets
and names any asymmetry in both directions, and checks NOGOLDEN[] too, since an
entry suppressing a case with no golden is a lie in a file whose job is being
believed.

It runs before the loop and aborts on failure. The first version appended ALIGN
lines to the report and carried on, which printed PASS=46 FAIL=0 on a
deliberately broken table; since PASS counts the cases that ran, no count means
anything once the tables disagree.

All four fault modes demonstrated by injecting them into cases.sh: CLI[]-only,
EXTRA[]-only, a NOGOLDEN naming a nonexistent case, and a NOGOLDEN on ci_dict
which has no golden. All four fail the gate, the first two naming the case.

Tests: make verify OK in 8m43s -- 52/52 Rust, corpus 46/46 and 29/29 goldens,
fuzz 16000 compared / 0 mismatches, GUI 46/46 (1 skipped). No output byte
changes; this is entirely the instrument, which is the class of defect that
matters most precisely because the product looks fine.

## 23. Reconcile the plan's Phase 0b table: P18-P20 are all done

Recorded 2026-10-01.

The per-item table still showed P18, P19 and P20 as open after their sections
were marked done, and the header still listed P20 in the Open line. Three
sources of truth disagreeing about the same fact is how a reader ends up
re-deriving what is already settled.

Also folds the fuzzer cleanup into P20's writeup, since the two landed together
and the cleanup's most important result -- the reference-refusal skip being a
live false green rather than dead code -- belongs next to the other Phase 0b
finding of the same kind.

## 24. A8: escape --title and --style_url, and fix a pre_explicit bug found on the way

Recorded 2026-10-01.

--title '</title><script>alert(3)</script>' and --style_url 'x.css" onload="alert(4)'
both emitted live XSS. chars::escape_attr now escapes < > & " in both, at the two
points the values are interpolated.

The title reaches the document by two routes and they need different treatment.
An explicit --title is an option value, so it is always escaped. A --titlefirst
title is lifted from the document's own first line, so it is document text
already, and escape_html_chars governs it exactly as the reference does.
Escaping both routes the same way was the first attempt and the fuzzer caught it:
--titlefirst --no-escape_HTML_chars on 'a & b' emitted '<title>a &amp; b</title>'
where the reference emits '<title>a & b</title>'. That would have been a Tier 1
divergence introduced by an over-broad fix.

Neither value is reachable from document text, so the fuzzer cannot generate
them: fuzz.py now drops both options via OPTION_DIVERGENT, a list of *options*
with no mechanism in it for silencing a mismatch. The corpus case opt_injection
carries them instead, declared 'differential must fail:' -- the reference is the
defect, so a byte comparison must differ, and run.sh inverts the verdict for such
cases: a match is now the failure, since it would mean the escaping stopped.

The real oracle is XML well-formedness in proptest.py, which is what the 30
known-open checks measured. All 30 pass and KNOWN_OPEN_XML_ARGS is empty.

The second fix is unrelated to A8 and was not being looked for. Removing two
VALUE_OPTS entries shifted the fuzz RNG stream, and seed 90210 case 30 then
reached an input the baseline never generated: an explicit <pre> whose end marker
is in a different paragraph, because of a blank line. That routes the input
through split_end_explicit_preformat, whose reference counterpart tests the end
marker with

    if (${para_ref} =~ /$pe_mark/io)

-- a symbolic reference, missing its '$', to a global nothing ever assigns. The
test never matches, so the whole paragraph is emitted as preformatted text with
the marker escaped into it, and PRE_EXPLICIT is not cleared, so trailing text
stays inside the block. The port matched the marker and ended the block, dropping
an escaped '&lt;/pre&gt;' line and moving text out of the block. Confirmed by
adding the missing deref to a copy of the reference and watching the output
change. Corpus 46/46 -> 47/47, goldens still 29/29.

verify: OK. 52/52 Rust, corpus 47/47, 16 000 fuzz cases with 0 mismatches,
proptest 0 known-open, GUI 46/46. fmt clean, no new clippy or shellcheck
warnings.

## 25. A9: an unreadable input exits non-zero, and four corpus cases stop passing vacuously

Recorded 2026-10-01.

--infile /nonexistent printed 'Could not open ...' and exited 0 having written a
0-byte output file. Perl does the same, so this is a declared deviation, but the
reference's behaviour is the one that breaks a build: make and CI read exit 0 as
success and hand the next step an empty document. --outfile to an unwritable path
was already exit 1, so the tool was inconsistent with itself.

Only the exit code changes. The output is untouched on purpose -- an unreadable
file contributed nothing to it either way, so changing the bytes would move
goldens for no gain. With several inputs the readable ones are still converted and
the partial document is still written; checked by cmp against the reference, not
just asserted about the port. try_txt2html returns Result<String, UnreadableInput>
and txt2html stays as the forgiving wrapper, so the Python bindings and the
in-process tests still get a String and the library does not force its callers to
care.

That is the whole fix. The finding is what it broke.

'PORT EXITED 1: Could not open tfiles/empty1.txt' -- four corpus cases that had
been green since the corpus was imported were reading a file that does not exist.
empty1..empty4 are the four extract/xhtml combinations of the empty-file test at
t/20tfiles.t:554,579,604,629. Upstream's stems there are its *output* names
(empty1.html..empty4.html), and run_case defaults INPUT[stem] to "$stem.txt", so
the cases asked for tfiles/empty1.txt through tfiles/empty4.txt. Both converters
failed to read their input, wrote 0 bytes, and cmp.py compared two empty files and
said PASS. Four vacuous tests, and good_empty.html -- which the plan recorded as
the one golden the corpus did not cover -- read as uncovered precisely because
the case that should have covered it never read anything.

INPUT[empty*] now points at tfiles/empty.txt, and a new GOLDEN[stem] override
points all four at the golden they were always meant to be scored against. 29/29
becomes 33/33 and the 32-golden coverage is complete. golden_check now fails
rather than skipping when an explicitly named golden is missing, since a wrong
GOLDEN[] entry would otherwise report itself as a case that has no golden.

Two reporting defects found while confirming it, both the same shape as the bug
they were hiding: a case whose converter errored printed PASS from the byte
comparison *and* the ERROR above it, and because good_empty.html is 0 bytes it
also printed GOLDEN pass. A failed run matching an empty golden is a green that
means nothing. An errored case now says so and is not scored either way.

verify: OK. 58/58 Rust, corpus 47/47 with 33/33 goldens, 16 000 fuzz cases with 0
mismatches, proptest 0 known-open, GUI 46/46. fmt clean, no new clippy warnings.

## 26. A10: bound the pattern cache at a measured 6x the worst realistic working set

Recorded 2026-10-01.

The plan called for a cap on Converter::re_cache for symmetry with
links::ascii_re_cached's 128, and said to record it as unproven rather than ship
it as a fix for something. Measuring it says the plan was right about the cause
and wrong about the consequence.

re_cache is memoised per *pattern*, and a converter's patterns are a function of
its options, not its input. A document reaching every construct that compiles one
-- ordered and bulleted lists, definition lists, all four inline delimiters,
tables, hrules, preformatted blocks, caps, short lines -- compiles 19 distinct
patterns, both with default options and with every pattern-varying option set to
a distinct value at once. Repeating the document 30 times does not move it. So no
document can grow the cache, and the cap is not a fix for untrusted input.

It does bound something real: custom_heading_regexp is a user-supplied *list*,
and 500 patterns give 512 entries, each compiled and cached as it is tried. So
128 is ~6x the worst realistic working set, and the number is measured rather
than picked. the_cap_is_above_the_measured_working_set fails if a future change
pushes the working set to the cap, which is what stops this becoming a
performance regression wearing a fix's clothes.

The cap clears the whole cache when full rather than evicting one entry: a
compiled Regex is expensive to build and cheap to keep, real working sets are
small, and an LRU would put a lookup on the hot path the cache exists to avoid.
Clearing cannot change the output, since the cache is a pure memo and a miss
recompiles exactly what a hit returned --
clearing_the_cache_does_not_change_the_output measures that by converting one
document with a cache that clears repeatedly part-way through and comparing it
against the same document with the cache intact.

What it costs, measured rather than asserted: 200 heading patterns cycled over
12 000 paragraphs, which makes the uncapped cache a 100% hit rate, run 1.25 s
capped against 1.17 s uncapped and 7 536 KB against 7 480 KB. About 7% of wall
clock in a case no real invocation produces, and no memory saving measurable at
realistic pattern sizes. The value here is symmetry and having a bound at all,
not a demonstrated saving, and the plan now says so.

verify: OK. 61/61 Rust, corpus 47/47 with 33/33 goldens, 16 000 fuzz cases with 0
mismatches, proptest 0 known-open, GUI 46/46. fmt clean, no new clippy warnings.

## 27. P21, P22: a corpus case that never ran, and a gate a fresh clone could not run

Recorded 2026-10-01.

Two defects in the instrument, both the same shape as P1: something that
reports success while checking less than it appears to.

P21. A case key assigned twice silently stops a case running.

A8 added a second, different `pre_explicit_blank` case without noticing the
stem was already taken. Bash's associative arrays keep the last assignment, so
the older case -- continuation text joining back inside an explicit-quote
<pre> block -- stopped running at the A8 commit. Nothing went red: both variants
agree with the reference, so the corpus reported its usual clean pass with one
case silently absent. `alignment_check` could not catch it and cannot, by
construction, since it compares key *sets* and the duplicate has already been
resolved by the time it runs.

duplicate_key_check greps the source text, where both assignments are still
visible, across every array a case is defined in, and aborts before the loop --
because PASS counts the cases that ran, so a count next to a dead case means
nothing. Demonstrated by injecting a duplicate into CLI[], EXTRA[] and the
single-stem path: all three fail with a non-zero exit. The displaced case is
restored as `pre_explicit`, named after its input.

P22. The differential gate could not run in a fresh clone at all.

ref/ is gitignored, nothing extracted the tracked tarball, the YAML::Syck stub
existed only on one machine, and every default in run.sh, fuzz.py, minimise.py
and linktest.rs was that machine's absolute path. `git archive HEAD` had no
ref/ and no working corpus, so this was not a latent risk -- it was the state
of every checkout that was not this one, and it is why CI could not exist yet.

`make ref` now materialises the reference from txt2html-3.0.tar.gz and a
tracked stub (canonical copy at tests/refstub/YAML/Syck.pm, so the reasoning
for it is reviewable). It is phony -- a directory target make considers up to
date is a trap here, since deleting ref/stubs alone would make `make ref` a
no-op and silently reinstate the unreproducible gate. `make ref-large` does the
same for the 26 MB scale fixture, which one test wanted and which had become a
silent always-green skip; that test now says it skipped.

All 14 hardcoded paths are derived from the script's own location and stay
overridable. run.sh and fuzz.py refuse to start without a reference rather than
letting both halves of every case fail identically and compare two empty files.

Verified: git archive of this tree, then make ref && make corpus, gives 48/48
with 33/33 goldens, and make fuzz green, with no environment set. Corpus is
48 cases now, not 47 -- the restored case.

## 28. Item 3: embed full GPLv3 text and clarify Qt tri-license choice

Recorded 2026-10-01.

txt2html-rs and txt2html-gui had only a six-line notice pointing to the GPL.
Both LICENSE files now include the upstream copyright notices for the
original Perl work, a short explanation of why GPLv3 is chosen (including that
Qt is dynamically linked in the GUI and the LGPLv3 option is available, but
consistency with the GPLv3 Rust core is preferred), and the complete,
verbatim GNU GPLv3 text (byte-identical to gnu.org / Debian's GPL-3).

Packaging metadata updated accordingly:
- Cargo.toml: license remains GPL-3.0-or-later (SPDX)
- txt2html-rs/pyproject.toml: license switched from a free-text table to the
  SPDX expression GPL-3.0-or-later
- txt2html-gui/pyproject.toml: bump setuptools floor to >=77 to support PEP
  639's license-files, change license to the SPDX expression and add
  license-files=["LICENSE"]

Built wheels for both packages: GUI and the maturin-built extension now ship
txt2html_*.dist-info/licenses/LICENSE with the full GPLv3 text and declare
License-Expression: GPL-3.0-or-later in METADATA. This closes the distribution
blocker of not shipping the licence text.

The GUI's test suite still passes (46/46, 1 skipped). The corpus still runs
48/48 with 33/33 goldens.

## 29. Clean the 74 Clippy warnings so the P8 gate can be strict

Recorded 2026-10-01.

The Makefile clippy target warns rather than fails, with a note to flip it
to -- -D warnings once the cleanup lands. This is that cleanup: 74 warnings
to zero, so the CI workflow added next can enforce the gate instead of
reporting on it.

Most of it is mechanical -- &mut Vec to &mut [_] where the callee only
indexes, Default::default() reassignment folded into struct literals, loop
counters to enumerate, manual strip/prefix to strip_prefix and strip_suffix,
clamp-like bounds to clamp, a match to a while let. A few needed judgement:

- get_tag closed the same tag from two conditions, which Clippy reads as a
  duplication. The conditions are distinct (a p is closed by a nested p or by
  a block element; a li by a nested li or by the end of its list) and the
  bodies are identical, so the conditions are merged with || -- same result,
  one branch. The reference states them separately; that is a repetition here,
  not a distinction.

- mailheader terminated every row but the last with a <br/>, expressed as an
  index loop that needed rlen to express the exception. split_last_mut says
  it directly.

- Two routines genuinely rewrite their Vec argument (mailheader clones in and
  writes back, make_aligned_table assigns a whole new Vec), so those two keep
  &mut Vec. liststuff forwards its indents to startlist, which pushes to it,
  so indents stays a Vec while its lines and actions become slices.

Four warnings are allowed rather than fixed, each with the reason next to it:
the three state-machine dispatchers take the reference's own argument list,
and threading that through a struct would hide which state each routine reads
and writes -- the mapping the corpus exists to test. The boxed alternative for
fancy_regex::Error changes a signature to save 136 bytes on a path that has
already failed.

The conversions are load-bearing, not cosmetic, so the gate is what settles
them: 48/48 differential cases and 33/33 goldens byte-identical to Perl, 61
cargo tests, proptest, alloctest, and 16000 fuzz cases across 8 fixed seeds.
not_preceded_by moved from chars().next_back() to ends_with, which is
byte-sensitive and the subject of the non-ASCII delimiter fix on this branch,
so it was checked against the reference in both directions: _bold_ is marked
up, x_bold_x is not, é_bold_ is, and ééboldéé is not -- Perl agrees on all
four.

Also ignores txt2html-gui/build/ and dist/, setuptools droppings from the
wheel build that were showing up as untracked.

make verify: OK

## 30. P8: CI on every push, and a clippy gate that now fails

Recorded 2026-10-01.

The project shipped two false greens -- A1, where the corpus compared against
a stale debug binary, and P1, where `make fuzz` piped to `tail -1` and so could
not report failure. Both shipped because nothing ran the gate and nothing
watched it fail. So the first half of this is the Makefile clippy target, which
warned rather than failed on the reasoning that an always-red gate gets
ignored. That was right about the 74 warnings it sat next to and wrong about
them: 09d13d9 cleared them, and the target now passes -- -D warnings, so a new
warning stops the build. The four remaining allows are reviewed exceptions with
their reasons next to them, not a backlog.

The workflow is three jobs, split so a lint failure reports in under a minute
rather than after sixteen thousand fuzz cases:

  rust         fmt, clippy -D warnings, unit/property/allocation tests
  differential make ref, the corpus and goldens against a clean RUNDIR, the
               fuzzer on the same fixed seeds the local gate uses, and
               upstream's own t/*.t as a canary
  gui          offscreen, with T2H_TFILES set so the reference-golden test runs
               instead of skipping

Two things in there were wrong until they were run locally. The canary called
`prove t/` directly rather than through a make target, so nothing exported
PERL5LIB and it failed with "Bad plan. You planned 16 tests but ran 0" -- the
exact shape of a differential gate silently comparing against a reference that
cannot load. And the GUI job ran the suite twice, once to build and once to
capture the log; it now runs once and uploads the log either way.

T2H_TFILES is load-bearing and was verified as such: with it set the
reference-golden test executes and passes, without it the test skips itself and
the run is still green. A skip in CI is indistinguishable from a pass.

The reference is built from the tracked tarball by `make ref`, not cloned, so
the job needs no network and no CPAN and every runner compares against the same
bytes. The venv is rebuilt each run rather than cached, because the extension
is installed into it and a cached venv holding a stale .so would be A1's
mistake one layer up.

The gate was then made to fail, each class observed rather than assumed:
a new clippy warning (error 101), a real output divergence -- &amp; to &AMP;,
which turns 3 corpus cases and 3 goldens red and exits 2, the same divergence
caught by the fuzzer with 9 mismatches in 60 cases, and a stale MINE, which
fails 47 of 48. A gate that has never been observed failing is not a gate.

## 31. Re-measure the plan's figures after P8, and correct a count I inflated

Recorded 2026-10-01.

The plan's headline numbers were a week stale: 47/47 where the corpus is 48/48,
"51 lib warnings — deliberately warn-only" where the lint gate is now strict and
the tree is clean, and a P8 still written as future work with no CI. Three
claims were also recorded as the *reason* a figure was what it was, which meant
they had to be re-measured rather than edited:

  - P15's "the stub exits non-zero at FAIL=46" was measured against 47 cases.
    Re-run at 48 with a stub binary: FAIL=47, exit 1. Corrected to the
    re-measured figure, not a guessed one.
  - P10's clippy and fmt bullets are now done rather than pending, and say so.
    fmt --check is clean because the chars.rs comment table was realigned
    rather than skipped.
  - P9's golden claim is now stated as "all 33 upstream golden checks", and the
    GUI README's "skipped unless the reference files are available" now records
    that it passes with them set, which was verified by running it that way.

P23 and the "Corrections" entry for it are new. The item exists because I wrote
"74 Clippy warnings" in a commit message and then measured it: clippy 1.98 at
the parent commit reports 69, 52 in the lib and 17 in tests. The conclusion is
unchanged, but the number was inflated by counting a few lints twice, and this
document's entire subject is not believing numbers, so it is corrected in the
open in both places rather than amended away in history.

Also fixes "33 of the 32 upstream goldens" in the corpus README, which was
never a claim anyone could check.

## 32. The GUI suite's libshiboken error, which was never failing anything

Recorded 2026-10-01.

Every run of the GUI suite printed this at interpreter shutdown:

  RuntimeError: libshiboken: Internal C++ object
  (PySide6.QtWidgets.QPlainTextEdit) already deleted.

BacklogTests.setUpClass did `cls.app, _window, _ = build_application([])`. The
local dies the moment setUpClass returns, so the MainWindow is collected while
`cls.app` is still held on the class; the C++ object goes with it, and the
converter's QThreadPool -- which outlives the class -- is left pointing at a
deleted widget. GuiTests already did this correctly, keeping `cls.window` and
closing it in tearDownClass, which is why only this one class warned.

Holding the window on the class is the whole fix. Verified over three
consecutive runs: 46/46 pass and the message is gone.

It is worth being clear about how long-lived this was, because the reason is
the reason it is fixed now rather than tolerated. A stderr line at interpreter
shutdown cannot fail a test, so nothing reported it, so nothing looked at it --
it was carried around in the tail of `make verify` output as background noise
across several items. The P8 workflow was about to encode it: the first draft
grepped for "already deleted" and emitted a ::notice:: tolerating it, which
would have made the noise permanent and self-documenting on every future run.

That is the P1 shape one level down. A known defect that a gate explains away
is indistinguishable, in the log, from a defect the gate cannot see. So the
workflow step is now a plain run plus an artifact upload, with no branch on the
warning, and the reason is recorded in the test where the bug actually was.

## 33. P13 decided: one self-contained artifact, so the GUI is rewritten in Rust + Qt

Recorded 2026-10-01.

P13 was the last open item on the agreed sequence and the only question in the
plan that was not really an engineering question -- it was waiting on a
requirement nobody had written down. Answered: the deliverable is a single
self-contained artifact.

The plan's original framing invited the wrong reading. "Rust + Qt is a
different architecture, not a refactor" sits next to a scope note that says
Tier 3 is blocked, and read together they suggest the whole project is up for
replacement. It is not. The engine is kept. Of the crate's 6,032 lines, 5,644
are the engine and it is already byte-verified against Perl by a differential
corpus -- that is the expensive part of this project and it is already paid
for. What gets rewritten is a 1,365-line shell, and the 212-line pyo3 layer is
deleted outright because it exists only to cross into Python.

The rewrite is cheap for a reason worth writing down, because it is not the
reason people expect: two implementations of an engine rule exist only because
two languages are in the path. files.py:46 re-implements convert.rs:30's
encoding rule in Python, and the two do not fully agree. A Rust GUI calls
read_any_file directly and the second copy is deleted. The pyo3 functions
become direct calls. optionspanel.py rebuilds 491 lines of widgets from a
`cli::SPECS` table that crossed a boundary as a list of tuples, where a Rust GUI
reads the same table directly and so cannot offer a value the engine rejects.

**This inverts the order of work, which is the point of recording it.** P7 --
the encoding defect, where CP1252 punctuation becomes C1 control characters --
was going to be a Phase 3 item. It is now a prerequisite of Phase 6. Fixing the
rule in the engine first means the rewrite inherits one correct implementation;
fixing it after means finding the same disagreement again in a language where
nothing points at the other copy. P5, P6, P10 and P11 all assist for the same
structural reason: none is GUI-coupled, so all of it is engine work that the
rewrite inherits rather than discards. None of it is worth doing *after* the
rewrite instead.

Phase 5 (HTML5, TOC, streaming) is deferred past Phase 6 rather than before it,
and the reason is narrow: each new engine option currently has to be added to
two front ends, and after the rewrite there is one.

Phase 6 records the port's acceptance criteria as the test suite, split three
ways. ConverterTests (6) is pure engine and already covered by cargo -- delete.
FileTests (10) is engine rules written in Python and belongs in the engine's
Rust tests. BacklogTests (3) and GuiTests (27) are the 30 shell tests that
define what the new GUI has to do. The A6 concurrency contract is called out to
be ported rather than rediscovered: a 300 ms debounce does not bound memory if
the queue drains slower than the user types, and that was the actual bug.

Also corrects an earlier miscount in this plan. I first wrote the engine as
5,921 lines by lumping main.rs and lib.rs in with the pyo3 bindings; main.rs is
the CLI entry point and stays. Every line reference in the new sections was
checked against the tree rather than carried over.

Three questions are recorded as open, none of which blocks P5-P11: Qt6 vs GTK4
(plan assumes Qt6, being the closest match to PySide6 and so the least
behavioural drift), whether the CLI stays as a second binary, and whether the
Python package is replaced outright or kept runnable alongside during the port.

The GUI README now says up front that it is scheduled to be rewritten, so
nobody builds on it without knowing. Gate re-run after the doc changes:
48/48, 33/33, fmt and clippy clean.

## 34. P7: the fallback decoded Latin-1, so demoronize never fired on anything

Recorded 2026-10-02.

The plan called this "the documented deviation is narrower than the real one".
The documentation was wrong, but the defect underneath it was not a wording
problem. read_any_file's fallback decoded as Latin-1 while demoronize_char's
table is keyed on the CP1252 code points -- U+201C, U+2019, U+2013 -- and those
two facts cancel out. A Latin-1 decode of 0x93 produces U+0093, which is not in
the table. Every substitution demoronize performs silently did nothing on
exactly the files it exists to serve, and the C1 control character was re-emitted
as UTF-8 `c2 93`, which renders as nothing at all.

Measured on the 0x93 0x94 0x96 0x97 file the original analysis used:

  before: <p>He said U+0093helloU+0094 and U+0096dashU+0097, caf&eacute;.</p>
  after:  <p>He said "hello" and -dash--, caf&eacute;.</p>

Neither form matches the reference byte-for-byte and neither can. Perl emits the
raw CP1252 bytes and relies on the browser guessing the encoding; the port
decodes and demoronizes to ASCII. Both render the same text, the bytes differ,
and that is the correct outcome rather than a regression to be chased.

Why nothing caught it: CP1252 and Latin-1 are identical everywhere except
0x80-0x9F, and 0xA0-0xFF is the entire range Latin-1 defines. umlauttest.txt and
list-styles.txt contain only >= 0xA0 and took the fallback without noticing; the
differs range had no fixture at all. A test suite covering the encoding rule
without touching the range where the encoding rule was wrong is the specific
shape of gate that looks like coverage.

CP1252 also leaves 0x81, 0x8D, 0x8F, 0x90 and 0x9D undefined. Those stay Latin-1
control characters, as browsers treat them, because inventing a glyph is a
visible guess. Python's own cp1252 codec *rejects* them, so files.py carries a
written-out table rather than delegating -- noted in both implementations so the
two can be diffed against each other.

Also worth recording, because the plan's version was wrong in a way that looked
right: it claimed byte_slice's column logic was "sound" and merely untested.
It is sound only for rows of equal *byte* length, since it cuts cells at byte
offsets derived from an OR-ed column map. The first CJK fixture I wrote padded
each row independently and the table was not detected at all -- not a table
regression, just a malformed fixture. Padded to a uniform 21 bytes it converts
correctly, and the port beats the reference: Perl demoronizes each byte of a
multi-byte sequence independently and emits &aelig;&yen;&not;, while the port
decodes UTF-8 first and the character survives.

The GUI turned out to matter more than the engine. files.py had its own Latin-1
decode, so the editor displayed U+0093 where the preview displayed a quote --
the user was editing one document and looking at another. A5's write-back was
faithful and still wrong, because faithfully reproducing a misread is still
misreading. The two implementations now agree, which is also the thing Phase 6
wanted before the rewrite: one correct rule rather than two divergent ones.

Two new options, both off the default path:

  --encoding auto|utf-8|cp1252   forced values exist because a short CP1252
                                 document can be valid UTF-8 by accident, and
                                 no amount of probing settles that
  --meta_charset                 default off so no golden moves; on in the GUI,
                                 where a browser is about to guess

One A5 test asserted the Latin-1 reading and defended it in a comment: "the file
says a control character and the file must keep saying so". True of Latin-1,
beside the point, and the kind of comment that survives review because it is
confident and specific. Rewritten to assert the agreement instead. The plan
records this because the failure mode is not carelessness -- it is a plausible
local argument that never gets checked against the thing it is about.

17 new tests in tests/encodingtest.rs, built from raw byte literals rather than
Rust strings: a literal U+201C in a test passes under either decoder and proves
nothing. Each was checked by reverting the fix and watching it go red. Two
corpus cases, cp1252_smart and cjk_table, declared "differential must fail:"
with the reason inline, and their oracle is the unit tests rather than a byte
comparison that cannot succeed.

Gate: 61 cargo tests, 17 new, 50/50 corpus, 33/33 goldens, 16 000 fuzz cases
across 8 seeds, 49 GUI tests, proptest and alloctest clean, upstream Perl 102
assertions, fmt and clippy -D warnings clean.

## 35. P7.4: fix the detection order, not just the fallback

Recorded 2026-10-02.

P7.1-3 made the encoding rule CP1252-instead-of-Latin-1 and called it settled.
The measurement says the fallback was the smallest part of the problem, because
UTF-8 validity was checked FIRST -- which means the fallback is only ever reached
by files that could not have been anything else, and every file that could be
either is decided without looking at the evidence.

Two defects follow, both found by measuring a fixture matrix before changing code:

  * A byte-order mark was being discarded as a decode error. FF FE is a
    specification-level guarantee of UTF-16LE; it failed the UTF-8 probe, fell to
    CP1252, and those two bytes have no CP1252 meaning, so a BOM'd UTF-16 file
    opened as "&yuml;&thorn;Hello". A declaration was being treated as an error.

  * BOM-less UTF-16 was passing as UTF-8, which is the one that mattered. ASCII
    prose in UTF-16LE is "H\0e\0l\0l\0o\0" -- every byte below 0x80, so the file is
    *valid UTF-8* and the probe accepted it, emitting a NUL between every letter
    in both the engine and the editor. No fix exists at the fallback layer: a
    validity check cannot distinguish UTF-8 from UTF-16LE-of-ASCII because they
    are the same byte sequence. Only the NUL *pattern* can, so it is consulted
    first.

Detection is now ordered by kind of evidence -- BOM, then UTF-16 NUL alignment,
then UTF-8 validity, then CP1252 -- and that order is the design. UTF-32LE is
checked before UTF-16LE because its mark starts with UTF-16LE's, and reading it
the short way round yields pairs of Latin-1 characters: a wrong answer that still
looks like text.

The 1/8 NUL threshold is measured, not guessed. An earlier 2/3 passed the English
prose rows and failed Russian and Greek, which would have left the interesting
input broken while the easy cases looked fine. Positives now run 0.15-1.00;
everything that must stay UTF-8 tops out at 0.04.

--encoding grows to iso-8859-1, cp1251, cp1253, koi8-r, utf-16le/be and
utf-32le/be: the encodings detection cannot reach, which is a deliberate choice
rather than an omission. CP1251, KOI8-R and CP1253 are mutually indistinguishable
in the bytes -- a CP1251 file is a valid CP1252 file with different meanings for
~60 of its 128 high bytes, and CP1251 vs KOI8-R disagree about nearly all of them.
Measured: under the default the port and Perl mangle such files *identically*, so
this is not an improvement over the reference and is not claimed to be. "latin-1"
no longer means CP1252; a Latin-1 file's 0x93 IS a C1 control, and asking for
Latin-1 was asking for it.

files.py derives its tables from Python's codecs instead of transcribing them.
P7.1 hand-wrote a CP1252 table so the two implementations could be diffed; at
five encodings x 128 bytes that stops being reviewable. The Rust tables are
generated from the same codecs by tests/gen_encoding_tables.py and both sides are
checked against that reference, so they agree by construction. Undefined bytes --
the one thing the reference cannot express -- each side falls back to the Latin-1
C1 reading, which is what keeps a CP1251 0x98 and CP1253's 17 holes lossless.

Also fixes a plan/code disagreement: the plan has claimed since P7.3 that the GUI
enables --meta_charset, and it never did. It does now, since the panel's consumer
is a browser reading a file this program wrote, and the engine default is off only
to keep goldens still.

Nine corpus cases, six fixtures. The three legacy files are registered TWICE --
once under the default, where port and reference mangle identically and the case
is a differential PASS, and once with --encoding, which is the declared
divergence. Declaring the default half NOGOLDEN was the first attempt and the
runner rejected it ("matches the reference; the declared divergence is gone");
the runner was right.

Limits, pinned by tests: BOM-less UTF-16 with too little ASCII is undetectable
("Привет, мир!" is detected, "Привет" is not -- density, not script), BOM-less
UTF-32 is not inferred, a UTF-8 BOM stays in the text (the reference keeps it)
while wide BOMs are consumed, and a wide encoding is written back with a BOM even
if read without one.

Single-byte charset *detection* is deferred, with the reasoning recorded in the
plan: a wrong confident guess is worse than the mojibake it replaces, because
mojibake is obviously broken and --encoding corrects it, whereas plausible wrong
letters are not.

Verified: 34 encoding tests (17 new, each sabotage-checked -- reverting the NUL
sniff to P7.3's order fails 7, swapping the UTF-32LE BOM check fails 3, removing
BOM handling fails 3); 95 cargo tests; corpus 59/59; goldens 33/33; 16k fuzz
cases across 8 seeds; GUI 58 including the skipped golden; upstream Perl 102/102;
proptest, alloctest, fmt and clippy -D warnings clean. No golden moves: output is
UTF-8 as before and --meta_charset stays off in the CLI.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>

## 36. Settle the delivery questions, and record why BSD is not available

Recorded 2026-10-02.

CLI stays and is independently distributable; coexist with the Python
GUI until the ported suite passes; ship as Flatpak. The first makes P11
worth doing, since .txt2htmlrc support means nothing without a CLI.

A BSD CLI was considered for adoption reach and is not available: the
engine is a derivative work of a program licensed "under the same terms
as Perl itself" (Artistic-1.0 or GPL), and only the upstream copyright
holders can waive the copyleft BSD would remove. Artistic-1.0 needs
nobody's permission but is not on most corporate allow-lists. GPL-3.0-
or-later is the branch everyone accepts, and it is the status quo.

Flatpak also supersedes P13's "one self-contained binary", which no
toolkit delivers: the CLI is 2.4 MB and GTK4 alone is 20 MB of shared
library. The Qt preference was an assumption resting on an unmeasured
argument about PySide6 semantics, so it stays open rather than decided.

Records the decision trail rather than rewriting P13 in place; the
reasoning is the point of that file.

## 37. Rename to textrill, and start it at 0.1.0

Recorded 2026-10-02.

The Flatpak ID becomes permanent once listed, so the name is fixed now
rather than after a release. textrill was clear on crates.io, GitHub and
Flathub; textgully was rejected as signalling erosion of the input when
the tool adds structure to it, and textcast as colliding with podcast.

Crate, lib, binary and Python module are all textrill; Converter::
txt2html becomes convert() and try_txt2html becomes try_convert(). The
version drops from 3.0.0 to 0.1.0, since claiming 3.0.0 implies
continuity with the Perl release line this forks from.

Every reference to upstream stays: ref/txt2html-3.0, scripts/txt2html,
the copyright block, and the corpus harness paths all name a program
that still exists under that name.

Adds a README, which the tree did not have. Its claims are checked rather
than assumed -- two were wrong when first written (--extract means body
only, and Converter::new takes Options by value), so the examples are
compiled and run against the crate. It also documents that upstream's
delimiter option names do not match their output: --bold_delimiter
emits <strong> and --italic_delimiter emits <em>, confirmed
byte-identical against the Perl original.

95 cargo tests, corpus 59/59, goldens 33/33, 58 GUI tests and 16,000
fuzz cases all still pass.

## 38. P5: an empty-matching dictionary pattern hangs forever, so reject it

Recorded 2026-10-02.

/|Perl\b/ is an alternation whose first and last branches are both empty,
so it matches at every position without consuming anything. The
substitution loop splits the paragraph into (pre, "", post) where post is
the whole paragraph again, reassigns para_ref to an unchanged value and
matches empty again. Verified the Perl original hangs identically, so this
is an upstream pathology rather than a port defect -- which is also why
the differential corpus could never catch it: the oracle hangs too.

Rejected at load in add_regexp, reusing the P4 rejected_patterns channel,
so one bad entry no longer poisons the rest of the dictionary.

The criterion is re.is_match("") rather than inspecting the compiled form,
because translate_pattern rewrites \b into a zero-width lookaround
*alternation*: \b.*\b looks empty-capable in the source text and is not,
and reasoning from the text would have rejected legitimate globs and
dropped links the reference produces. All 52 built-in system-dictionary
rules load and none match empty, asserted against the real load path
rather than a hand-picked sample.

-o and -s are deliberately left unguarded. They substitute at most once
per paragraph or per section, so the same pattern terminates there and
Perl accepts it; guarding them would be a Tier 1 byte-parity divergence
for no benefit. Confirmed /|x/ -o-> url emits the same empty anchor in
both implementations. That narrowing looks like an oversight, so a test
pins it.

6 tests, 95 -> 101. Corpus 59/59, goldens 33/33, 16,000 fuzz cases and the
58-test GUI suite all unaffected.

## 39. P6: land the two prescribed fixes, and record that they are worth ~0%

Recorded 2026-10-02.

Both fixes are in and output is byte-identical to Perl (corpus 59/59,
goldens 33/33, 16,000 fuzz cases):

- dropped `let cur = para_ref.clone()` from the innermost match loop.
  Confirmed never needed for borrow-checking: split_front returns owned
  Strings, so the borrow ends before the reassignment.
- guarded all three no-op `format!("{line_with_links}{para_ref}")` sites.

They cut the link pass from 11,602 to 5,510 bytes/paragraph, a 52%
reduction in allocation, so they are a real improvement. But measured
wall-clock they are worth +0.0%: 0.720s baseline vs 0.720s fixed, pinned
to one core, 15 reps, min-of-N.

The plan's diagnosis was wrong. Instrumenting the pass shows where the
time actually goes: 257,140 regex invocations for 52 rules x 4,945
paragraphs, 81% of which match nothing, and raw is_match over those pairs
alone is 1.33s of the 2.70s link phase. Neither copy was on the critical
path. The bottleneck is fancy_regex being a backtracking engine where
Perl uses PCRE, which prefilters internally. So P6's goal is NOT met and
the section now says so.

Two measurement problems had to be fixed first, or the numbers lie:

- The 2 MB fixture was not kept and big_para.txt has no links at all, so
  the original breakdown could not be reproduced. Rebuilt it with a
  deterministic generator whose output is byte-identical to Perl, so no
  figure here can be explained by doing less work. That fixture shows the
  link phase is 94% of runtime, not 60%.
- This host has background load (a desktop greeter at 32% CPU, load
  average 7.6) which made every arm bimodal and produced successive
  "results" of -3% and +11% for the same binary. All timings are now
  A/B-interleaved with min-of-N.

Two guards, both verified to actually fail when the regression is
reintroduced rather than merely printing a worse number: an allocation
budget in alloctest.rs (the primary guard, since allocation counts are
deterministic and machine-independent, unlike a wall-clock threshold on a
loaded box) and a linear-scaling ratio in linktest.rs. The alloctest
parser is built outside the measured window on purpose: building it
compiles all 52 patterns and costs ~86 MB regardless of document size,
which would have drowned the per-paragraph signal in a constant.

The prefilter that would actually close the gap is not implemented. It
needs a sound required-substring extractor, since regex-automata only
exposes from_hir_prefix and prefix extraction misses exactly the ~19
newsgroup rules whose literal sits inside after a character class. That is
a design change with a silent-link-loss failure mode, so it is recorded as
its own item rather than folded in here.

## 40. P6: a sound required-literal prefilter, and a regex security audit

Recorded 2026-10-02.

The follow-up P6 asked for, plus the question of whether the regex engine is
attackable at all.

Security audit: no exploitable ReDoS, and that is a three-layer result rather
than an assumption. fancy_regex's analyze() marks a pattern "hard" only when it
needs lookaround or backreferences; everything else is delegated to the linear
`regex` crate, so the classic `(a+)+$` bombs are compiled away -- verified, they
return Ok rather than Err. Hard patterns get backtrack_limit = 1_000_000, so even
the worst case is bounded: a catastrophic dictionary pattern costs 0.35s over
5,000 paragraphs, linear in paragraph count. Document-derived patterns cannot
reach a backtracking engine at all, because the table delimiter is a single
character with ^[]\ stripped, used inside [...] where metacharacters are
literal. So a malicious input file cannot cause a hang; only a malicious
dictionary can, and it is bounded. Left as-is: no CVE search (offline), and
`?` on the twelve `.ok().flatten()` sites still swallows BacktrackLimitExceeded
into a silent no-match. That is observability, not a vulnerability.

Prefilter: `src/prefilter.rs` extracts a literal every match must contain, from
the HIR that regex-syntax parses out of the translated pattern -- the same string
handed to fancy_regex, so the analysis describes the regex that actually runs.
from_hir_prefix was not usable: it misses exactly the ~19 newsgroup rules whose
literal sits inside after a leading character class. Coverage is 38 of 41 shipped
rules; the three without are RFC ?(\d+), \bSeth\ Golub\b and \btxt2html\b, where
nothing is provably required. The extractor returns None whenever it cannot prove
a literal, so the only failure direction is wasted time, never a lost link.

92.5% of the 202,745 rule/paragraph pairs on a sparse 2 MB fixture are now
rejected without touching the regex engine. The link pass improves 15% on that
fixture and 5% on a link-dense one. Modest, and the plan says so: a rejection
still costs a memmem scan, and fancy_regex already delegates these patterns to
`regex`, which prefilters internally. The two filters duplicate each other.

Two bugs found and fixed on the way, both mine:

- The fold was recomputed inside the substitution loop from `para_ref`, but by
  then the matched prefix had moved into `line_with_links`, so the fold described
  only the unprocessed tail. Literals in the emitted prefix looked absent and
  links were dropped: news 30/33, sample and xhtml_sample likewise. Caught by the
  corpus, which is exactly what it is for.
- Four unit tests I wrote for it do not reproduce the failure; only the corpus
  does. They are kept because they check the soundness invariant directly, but
  the regression guard is the corpus and the commit message says so.

The first version of the filter used a naive `windows().any()` scan and was
slower than no filter at all. memchr::memmem::Finder, built once at load time,
is what turned it into a win.

Verified: fmt and clippy clean, 121 tests, corpus 59/59, goldens 33/33, fuzz
16,000 cases with zero mismatches, alloctest, proptest, GUI 58 with zero skips
given T2H_TFILES. Output byte-identical to the pre-prefilter binary on both
benchmarks. P6's goal remains unmet and the plan now says so in those words.

## 41. P11: restore @file, ~/.txt2htmlrc and ./.txt2htmlrc support

Recorded 2026-10-02.

Upstream reads option files via Getopt::ArgvFile::argvFile, and the POD
documents @filename grouping and both rc files as active behaviour. The
port had none of it: `textrill @opts.txt` treated the group as an input
filename and failed with "Could not open @opts.txt".

Precedence is @file < ~/.txt2htmlrc < ./.txt2htmlrc < command line, which
matches upstream: argvFile prepends its expansion to @ARGV, so the command
line is parsed last. Each layer is tested against the one below it.

A deliberate Tier 1 divergence, and the only one in this item. Upstream
reads the rc files only if Getopt::ArgvFile is installed, and it is not
installed here, so the corpus oracle reads no rc files at all. This port
reads them unconditionally. The alternative -- ignoring rc files unless an
unrelated CPAN module happens to be present -- would make correct behaviour
depend on that install; the reference's guard is its bug, not its
specification. The corpus is unaffected, and that is checked rather than
assumed: neither the corpus directory nor $HOME has a .txt2htmlrc.

Two judgement calls. A missing @file is an error, since the name was typed,
while a missing rc file is not, since both rc files are documented as
optional. And the two rc paths are compared, so HOME=$PWD -- common in
containers -- reads the file once instead of duplicating every array option.

Beyond the reference, an error in an option file reports file:line: message,
naming both. Upstream's error names neither.

Also corrects three stale figures in the REMEDIATION plan. Prefilter
coverage was recorded as 38 of 41 rules against a 41-rule subset; the loader
actually returns 65 rules, 56 of them prefilterable, now asserted by a test
so the number cannot drift silently. The 52-rule invocation count was a
line count of the dict file, not a count of loaded rules. And "the link
phase is 94% of runtime" was a property of one link-dense fixture, not of
the engine; direct timing puts the link pass at 15-16% of wall-clock on the
sparse and 2 MB dense fixtures, so the honest range is 15-94% depending on
link density.

fmt and clippy clean. 141 tests, corpus 59/59, goldens 33/33, 4000 fuzz
cases, GUI 58.

## 42. Phase 6 step 1: freeze the GUI/engine surface in SURFACE.md

Recorded 2026-10-02.

The plan asked for the boundary to be written down before the port, so the
port is a transcription rather than a re-derivation from memory.

Checked against the build rather than from memory, which changed four things:

- option_specs() returns 54 options (20 bool, 18 str, 11 int, 4 str_array,
  1 table_type), not the 51 the count implied.
- process_chunk has no caller anywhere in the GUI, so it should not be
  ported unless something starts needing it.
- file_encoding is a process-global; the rewrite should return the
  encoding from the read instead. The existing comment's reasoning is
  about where it is read, not about globals being sound.
- The test suite is 58 tests (FileTests 20, GuiTests 29), not the 46 the
  plan claimed, so 32 of them are the acceptance bar rather than 30.

Also records the toolkit constraint: only Qt 5.15 is installed here, Qt6
and GTK4 are absent, and Flathub ships Qt 6, so building on Qt5 now risks
a port.

## 43. Phase 6 step 2: add the engine's encoder, and fix a UTF-16 decoder defect

Recorded 2026-10-02.

Porting FileTests needed the thing they were guarding, which the engine did
not have: it could decode but not encode, so files.py had grown a private
Python encoder. Phase 6 deletes files.py, so the rewrite would have lost the
guarantee that opening and saving a file leaves its bytes alone.

src/encode.rs encodes to UTF-8, the five single-byte encodings and the four
wide ones, building the reverse of the decoder's table from the same
ENCODING_TABLES rather than keeping a second copy -- a second table would be
free to drift, and a drifted encoder corrupts files on save. write_with
refuses a missing parent directory rather than creating it, and an
unrepresentable character is reported with its byte offset instead of being
substituted, so a front end can underline it.

Two defects surfaced while porting:

- The UTF-16 decoder replaced every astral character with two U+FFFD.
  char::from_u32 is None for a surrogate, and each code unit was converted
  on its own, so one emoji decoded as a pair of replacement glyphs. This
  affected every character outside the BMP. units_to_string now joins
  surrogate pairs; a genuinely lone surrogate still becomes U+FFFD, which is
  the right reading of a malformed file.
- Encoding::Auto was reachable as an output encoding. There is nothing to
  detect on output, and quietly defaulting it to UTF-8 is exactly the
  corruption this work exists to end, so it is refused by name.

The decoder fix changes conversion output, so it is checked against the
oracle rather than assumed: corpus 59/59, goldens 33/33, 153 tests, GUI 58,
fmt and clippy clean. The reference has no --encoding option, so the UTF-16
path has no oracle and is pinned by unit tests instead.

## 44. Phase 6: decide Qt 6 and record the install list

Recorded 2026-10-02.

Qt 6 rather than Qt 5, because the deliverable is a Flatpak and Flathub
ships the Qt 6 runtime, so building on the locally installed Qt 5.15 would
mean porting the whole shell afterwards. GTK 4 was the other live option.

The install list is verified rather than assembled from memory: every
package was checked with apt-get --dry-run -s, all five resolve in this
machine's Ubuntu 24.04 repos, 48 packages total, no daemons restarted.
Each package is justified individually, including why xvfb is probably
unnecessary (the suite runs under QT_QPA_PLATFORM=offscreen) and why
cargo-fuzz and heaptrack were left out (make fuzz is the Python differential
harness, and the allocation work is already guarded by a deterministic
budget that heap tracking would not be).

Bindings come from cargo, not apt: cxx-qt 0.10.0 resolves on crates.io.

## 45. Phase 6: choose egui/eframe, and record why Qt was rejected

Recorded 2026-10-02.

The Qt spike failed on the binding, not on Qt: cxx-qt 0.10 does not compile
any extern "RustQt" QObject on this toolchain (it emits
include!(<QtCore/QObject>), which syn cannot parse), and Qt's official
qtbridge requires Qt 6.10 while the host has 6.4.2.

Choose egui/eframe instead: pure Rust, and egui_kittest provides the
AccessKit-based headless widget queries the 32 acceptance tests need.
Verified on this host: a headless widget test passes, and a windowed run
survives under Xvfb + software GL.

RUST-GUI-FINDINGS.md records the evidence, the dependency and packaging
analysis (298 lock packages with the recommended features, no build-time
system libraries, only a GL stack at runtime), and the sequencing lessons.
SURFACE.md's widget mapping moves from QtWidgets to egui; the behavioural
contract is unchanged.

## 46. Phase 6 step 3: add the native GUI crate

Recorded 2026-10-02.

Start textrill-gui-rs, an egui/eframe front end built against the engine
crate directly. The options panel is generated from cli::SPECS, so the 54
widgets cannot drift from --help, and the numeric bounds come from
options::numeric_range -- the same table the engine validates against. A
Rust panel therefore cannot offer a value the engine rejects, which is the
reason optionspanel.py is deleted rather than ported.

Drawing is a plain egui::Ui closure, which is the seam the headless
acceptance tests drive; the eframe entry point is a shim over it.

Make pyo3 optional, enabled only by the extension-module feature that
maturin already passes. It was already confined to src/python.rs, so the
Python build is unchanged while the GUI no longer links libpython.

Five acceptance tests pass with no display, and the window survives a 20s
run under Xvfb + software GL. The generation-tagged worker contract in
SURFACE.md section 3 is the next step; conversion is synchronous for now.

## 47. Phase 6 step 3: port the concurrency contract off the GUI thread

Recorded 2026-10-02.

Add worker.rs, the Rust form of SURFACE.md section 3. Each request gets a
generation; only the newest job waits, and replacing that slot drops the
previous job and its copy of the document; at most max_threads conversions
run; and a panic is caught and turned into an error result, so no path can
leave the front end waiting. Results cross back on a channel, so a late
result during shutdown fails a send instead of signalling a dead window.

Wire the app to it: edits and option changes queue a conversion, the preview
drains finished ones and drops anything older than the newest request, and a
waker asks egui to repaint when a result lands. The options panel now reports
whether a widget changed, so an option edit re-converts.

Tests: four worker tests (a burst does not run everything, the newest
generation survives, the worker returns to idle, and a panic is reported),
and the preview test now waits for the real worker. Nine tests pass with no
display, and the window still runs under Xvfb + software GL.

## 48. Phase 6 step 3: harden the worker and put the native crate in the gate

Recorded 2026-10-02.

The worker kept worker.py's contract but took Rust's defaults where they
are worse than the Python original:

- Locks now recover from poisoning instead of panicking. A panic on the
  conversion thread used to poison the mutex and then panic again in
  poll/convert, turning one caught engine panic into a dead worker.
- The thread is spawned with thread::Builder; an OS failure to spawn
  previously dropped the job silently and left the UI on "converting...".
  It now reports an error and releases the in-flight slot.
- The repaint waker is cloned out and called outside the lock.
- The crate carries #![forbid(unsafe_code)].

Wire the crate into the gates so it cannot rot while the Python GUI
still ships: make fmt-check/clippy cover both crates, make test-gui-rs
runs the headless egui_kittest suite, verify includes it, and CI's rust
job builds, lints and tests it with the lockfile in the cache key.

Record the licence and privacy audit in RUST-GUI-FINDINGS.md 5.5: every
dependency is permissive or GPL-3.0-compatible; no network/telemetry
crates; gethostname is confined to local X11 auth; only libc/libm/libgcc
are linked at build time.

## 49. Phase 6 step 3: port the document, file state and settings

Recorded 2026-10-03.

The window now owns a Document (text, source path, output path, source
encoding, dirty/output-stale flags, last-saved HTML) and a QSettings-
compatible store at ~/.config/textrill-gui/textrill.conf.  Saving the
HTML refuses to overwrite the file being converted, and saving the text
round-trips the source encoding rather than transcoding it.

Auto-convert is debounced with the Python's 300 ms window and the same
rule: the timer always runs before the first conversion, and an option
change with a preview already up converts at once.  Conversions force
meta_charset on, matching the option panel's values().

21 document/settings tests join the 5 acceptance and 4 worker tests.

## 50. Phase 6 step 3: make the UI scale readable on any screen

Recorded 2026-10-03.

Whole-UI zoom is an egui built-in (Ctrl/Mod + = / - / 0), so it costs
nothing; the Display row just makes it discoverable.  The OS scale factor
(Xft.dpi on X11, output scale on Wayland) is applied underneath, so a
HiDPI panel and a 96 DPI projector are already reconciled and the user
multiplies on top.  The window now asks for 900x720 instead of winit's
800x600, with a 360x300 minimum.

Sending the viewport Title every frame requests a repaint every frame, so
the UI could never idle and Harness::run failed with 'exceeded max_steps'.
The title is now sent only when it changes.

Runtime-only: persistence is left off, so zoom is per-session.

## 51. Phase 6 step 3: port the window chrome onto the native UI

Recorded 2026-10-03.

## 52. Phase 6 step 3: ask before discarding unsaved text

Recorded 2026-10-03.

## 53. Phase 6 step 3: persist the whole option set next to the auto flag

Recorded 2026-10-03.

## 54. Phase 6 step 3: wire the native file choosers

Recorded 2026-10-03.

Open, Save As and a Save with no output path now open the desktop's own
chooser through rfd's XDG desktop portal backend. Only rfd and pollster are
new in the graph, can_create_directories is off, and nothing besides the
chosen path crosses the boundary. A Chooser trait keeps the command logic
testable headlessly; a cancelled chooser drops the waiting unsaved-changes
command without losing edits.

## 55. Phase 6 step 3: port app.py's command line and guard Open

Recorded 2026-10-03.

The window now takes the same arguments as app.py -- an optional file,
--xhtml/--no-xhtml, --tables, --version and --help -- parsed by a small
hand-written module (src/args.rs) rather than pulling in clap, so the
options stay in step with the engine. --version and --help print and exit
before any window is created; an unknown flag or a second file exits 2.

Opening a file also goes through the unsaved-changes prompt now: PendingAction
gained Open, open_document()/do_open() run save-or-discard first, and a
cancelled chooser drops the waiting command without losing edits.

## 56. Phase 6 step 3: bundle Noto Sans and Noto Sans CJK TC

Recorded 2026-10-03.

egui draws only the fonts it is handed and never reads the system font
configuration, so the app now ships its own fallbacks in
textrill-gui-rs/assets/fonts (both OFL-1.1):

  - Noto Sans Regular, widening Greek and Cyrillic.
  - Noto Sans CJK TC Regular, the full pan-CJK repertoire (CJK Unified
    Ideographs + Ext-A, kana, Hangul, Bopomofo). egui applies no OpenType
    language features, so one default shape set is chosen; TC gives
    Traditional-default shapes, and SC/TC coverage is identical.

The CJK face dominates the app's size: stripped binary 12.9 MB -> 28.6 MB,
about 15.7 MB compressed. This is in line with comparable self-contained
Rust GUIs and far below Electron editors. The engine's output is unaffected
either way -- this is display-only.

Docs: RUST-GUI-FINDINGS.md sections 5.3/5.5/6 item 14 and REMEDIATION-PLAN.md
section 6.3 record the choice, the coverage evidence and the size.

## 57. Phase 6 step 3: retire the Python GUI and pyo3 bindings

Recorded 2026-10-03.

The native egui GUI passed the ported acceptance suite and a side-by-side
differential run (Python suite 58 tests, native 60/60; ten encodings
decoded, detected and round-tripped byte-identically), so the
Python/PySide6 front end and the pyo3 layer it needed move to
legacy-archive/. Nothing there is built or tested.

- Move textrill-gui/, python.rs + its pyproject.toml, and the Python
  wrapper package into legacy-archive/.
- Drop the pyo3 dependency, the extension-module feature and mod python
  from textrill; crate-type is now rlib.
- Remove the make gui target and the CI gui job with its
  maturin/PySide6/T2H_TFILES install path.
- Update the licence rationale (the GUI is pure Rust GPLv3, no Qt or
  Python), README, plan and findings; add legacy-archive/README.md.

## 58. Phase 6 step 3: persist the window geometry

Recorded 2026-10-03.

## 59. P6: derive the prefilter literal from the original pattern

Recorded 2026-10-03.

translate_pattern rewrites \b into look-around, which regex-syntax refuses
to parse, and \b-wrapped patterns are what add_literal and glob2regexp emit.
So required_literal(regex.as_str()) returned None for the whole \b family:
all six \b<literal>\b rules plus the two [[:alpha:]] host rules ran
fancy_regex's backtracking VM on every paragraph. add_rule now falls back to
the original pattern when the translated one will not parse; translation only
rewrites zero-width anchors and escape classes, never a literal run, so the
literal is still required. Coverage 43/52 -> 52/52. Link-dense 2 MB x 9901
paras: 3.75s -> 1.27s (2.95x), byte-identical to Perl; Perl 1.96s.

Pin it with the_shipped_dictionary_is_fully_prefiltered (now 52/52) and
prefilter_rejection_implies_no_match_for_shipped_rules (the soundness
property, over the production may_match path). Also make
rejection_happens_before_any_output tolerate a BrokenPipe, a pre-existing
race between the child exiting and the test's stdin write.

## 60. Refresh stale status figures, and drop unreferenced artifacts

Recorded 2026-10-03.

The plan and both READMEs carried numbers from earlier phases: corpus
48/48 (now 59/59), 95/61 Rust tests (now 164), 60/46-test GUIs (now 74),
and a 'not done yet' list that still called the GUI unfinished and the port
2x slow after P6. The sequencing list in the plan still presented P20, P22,
P13, A8, P5/P6/P11 and Phase 6 as remaining; all are done. The toolkit line
in P13 still called Qt an open assumption though Phase 6 chose egui.

Also correct the corpus README: it said seven declared divergences where
cases.sh has nine (opt_injection was missing entirely), and claimed the P12
property/allocation suites were not implemented when both exist.

Remove five tracked files nothing references: the tarzan conversion
samples, the txt2html-2.51 tarball, and the sander nupkg. The 3.0 tarball
and the two survey zips stay (the Makefile and TOOL-SURVEY.md use them).
Delete the ignored 2.2 GB .venv left from the retired Python bindings.

## 61. Build a static musl CLI and run the corpus against it in CI

Recorded 2026-10-03.

The CLI's distribution claim was a single static binary that runs on
glibc-less distros, but nothing built or tested one. Add `make musl`, which
builds x86_64-unknown-linux-musl and asserts the result is static, and
`make corpus-musl`, which runs the differential corpus against that binary;
a build-only check would pass on a musl target whose output differed.

The dependency tree is pure Rust (no C, no FFI, no libc crate), so musl
introduces no ABI risk. Verified on the static binary: ELF PIE, NX stack,
full RELRO (BIND_NOW); bundled musl 1.2.5; corpus 59/59 and goldens 33/33
byte-identical; proptest OK; fuzz 800 cases, 0 mismatches, 0 timeouts.

The new CI job runs this on every push. Not part of `verify` -- most dev
machines have no musl target, and a gate that fails for a missing toolchain
rather than a defect is the kind people skip.

## 62. P5.1: Add an opt-in HTML5 output mode

Recorded 2026-10-04.

--html5 emits the short <!DOCTYPE html>, an <html> element with no
namespace, and a forced <meta charset="utf-8">, leaving the body
markup identical. It is off by default so output stays byte-identical
to the reference; the default prolog is still HTML 4.01 / XHTML 1.0
Strict. Tag case still follows --xhtml / --lower_case_tags.

## 63. Add template research sandbox

Recorded 2026-10-04.

Track the chunked-reader template under template-research1/ as the
starting point for experimenting with textrill output features.

## 64. Add template research findings

Recorded 2026-10-04.

Synthesise the deleted prior attempt into decisions, reusable designs and
engine hazards: both sectioning output models behind flags, sequential
chunk-N ids, the demoronize/#-delimiter warps, and what not to copy.

## 65. P5.2: Add opt-in sectioning, TOC and multi-file output

Recorded 2026-10-04.

Three default-off flags:

  --section  wrap each heading run in <article class="section" id="chunk-N">
  --toc      prepend a generated <nav class="toc"> (implies --section)
  --chunk    write one file per shallowest-level heading, with a
             cross-file TOC and prev/next pager

Ids are assigned sequentially by the sectioner, so they cannot collide or
produce a stale link, unlike the make_anchors section_x_y names. Sectioning
is a pure post-pass over the body; convert_sources was split into
convert_to_parts (start/body/tail) so the reference path is byte-identical
when the flags are off.

Gates: corpus 59/59, goldens 33/33, sectiontest 7/7, GUI acceptance 26/26,
fmt and clippy clean.

## 66. P5.3: Number headings hierarchically

Recorded 2026-10-04.

Opt-in --number_headings prefixes each heading with its position (1, 1.1,
1.1.1, ...). It runs before the P5.2 section pass, so the numbers appear in
--toc labels and on every --chunk page. A level stack handles a document
that starts at h2 (numbered 1, not 0.1). Off by default, so the reference
goldens do not move.

Gates: corpus 59/59, goldens 33/33, sectiontest 9/9, optionstest 19/19,
cliexit 19/19, GUI acceptance 26/26, fmt and clippy clean.

## 67. P5.4: Add streaming conversion

Recorded 2026-10-04.

## 68. Record template slot design and refresh plan status

Recorded 2026-10-04.

## 69. P5.5: templates (--template/--document_template) with {{textrill:*}} slots

Recorded 2026-10-04.

- Add template module (apply/validate), namespace-aware slots, hard errors on unknown textrill slots
- Split sectionize into sectionize_parts (separate TOC/body)
- Extract build_head from do_file_start; store head/title/template state in Converter
- Wire --template/--document_template in CLI/options; mutual exclusions + mode refusals
- Update counts (62/116), README with Templates section, GUI acceptance
- 12 template tests, corpus/goldens unchanged (byte-identical default output)

## 70. Design: detailed citations/glossary plan (deferred)

Recorded 2026-10-04.

## 71. Design: security meta boundary notes (deferred)

Recorded 2026-10-04.

## 72. Design: Flatpak packaging notes (preparation)

Recorded 2026-10-04.

## 73. A11: Refuse script-bearing URL schemes in generated hrefs

Recorded 2026-10-06.

A document could write a live `javascript:` or `data:` href into its own output
through a `<URL:...>` tag, and the reference does exactly that.
`src/urlscheme.rs` is now the single decision point, and it scans *finished
markup* rather than checking each construction site, so a producer nobody
thought about is covered too. Refused anchors are unwrapped rather than
deleted, so a hostile document cannot deny service by making the converter
fail; a refused dictionary URL is a load-time diagnostic instead, because that
one the operator can fix.

The default is a four-scheme denylist (`javascript`, `data`, `vbscript`,
`file`) rather than an allowlist; `--allowed_url_schemes https` sets a strict
allowlist instead. Upstream's own CI fixture is the proof that an allowlist
cannot be the default: `.github/workflows/xyz.dict` links `xyz://example.com`,
a scheme nobody could have predicted, and an unknown scheme is not an
*executing* one. The cost is stated in the module docs and that flag mitigates
it.

`rel="noopener noreferrer"`, which this plan's security-meta note proposed for
external links, is deliberately not added. txt2html never emits `target`, so
there is no new browsing context for `noopener` to defend, and `noreferrer`
would only strip referrers the author may want. Adding it would also have moved
the bytes of every external link in all 33 goldens.

The `url_scheme` corpus case is declared `differential must fail:` for the same
reason `opt_injection` is under A8 -- the reference is the defect, so the port
must diverge and byte comparison cannot be the oracle. The oracle is
`tests/urlschemetest.rs`, which asserts no refused scheme survives any producer,
that the words are kept, and that the relative href the label spelling produces
is left alone.

Gates: corpus 60/60, goldens 33/33, 258 engine tests, urlschemetest 20/20,
optionstest 20/20, GUI 74/74, fmt and clippy clean.

## 74. P5.6: Make every engine-generated internal link resolve

Recorded 2026-10-06.

Asked whether the links textrill *generates* actually work, given that A11 had
just established the links it *refuses* are handled. The answer for the links
themselves was yes, but the corpus covered none of them: all 60 cases are
reference-differential and none passes `--toc`, `--section` or `--chunk`, so a
link checker over the whole corpus output finds zero engine-generated links. The
code that invents hrefs and the ids they point at was resting on six unit tests
in `section.rs`.

`tests/linkintegrity.rs` makes that permanent: adversarial documents crossed
with the option sets that change link structure, single-file and chunked,
asserting that every generated internal reference resolves, that ids are unique
per document, and that each TOC entry names the heading it points at. Duplicate
ids do not break a link -- the browser jumps to the first match -- so uniqueness
needs its own assertion. Two mutations confirm the guard can fail, which was
worth checking: a one-character typo in the single-file TOC href and a one-page
off-by-one in the cross-file TOC are both caught.

It also found a real defect. `--section` was silently ignored under `--chunk`,
because a page *is* one top-level section, so there was nothing left to wrap. The
cross-file TOC could therefore only link to the top of a page, never to its
heading. Each page now carries `<article class="section" id="chunk-N">` and the
TOC links `page.html#chunk-N`. The wrapper is emitted for `--toc` as well as
`--section`, which is the important half: single-file `--toc` already implies
its targets, since `sectionize_parts` wraps whenever either flag is set, and a
TOC that can emit a dangling link is worse than a redundant `<article>`. The
first cut got this wrong and `linkintegrity.rs` caught it immediately; chunk
mode has no upstream equivalent and no golden, so nothing else would have.

Scope note, because it is the part that is easy to get wrong. A document can
write its own URLs through `<URL:...>` or a link dictionary, and those are the
author's claims about the world: the converter emits `docs/readme` faithfully
and cannot know the file exists. Holding those to a file-existence rule tests
the input, not the output. The guard is scoped to the `<nav>` blocks the engine
builds. External URLs are out of scope, and so are resource references -- a
`<link href>` to a stylesheet that 404s is a missing asset, not a dead link.

Also fixed: three error messages in `main.rs` had a stray comma in the format
string, so a write failure read `unable to open out.html,: ...`.

Gates: corpus 60/60, goldens 33/33, 263 engine tests, linkintegrity 5/5,
optionstest 20/20, GUI 74/74, fmt and clippy clean.

## 75. P6.1: Add opt-in citations and glossary

Recorded 2026-10-06.

Implements the design recorded above, which had been deferred. `--citations` and
`--glossary` are both opt-in and default-off, triggered by namespaced markers
rather than anything a reader could type by accident. Definitions and
references are collected in their own pass over the finished body, after
numbering and sectioning, and it touches nothing else. With both modes off the
output is byte-identical to the behaviour before this commit, which the corpus
and the 33 goldens confirm rather than assume.

It fails closed. A dangling reference, an orphan definition, a duplicate
definition, an empty definition, an unbalanced or mismatched block, an invalid
key, or any well-formed `textrill:` token the engine cannot read is an error,
and `main.rs` reports it and exits non-zero before the output file is opened --
a refused document produces no output at all.

There is no CSS to reveal either, because the note body is emitted once as a
real list rather than once per reference. That is the reason the original
CSS-only checkbox sketch was dropped, and it is a shape problem rather than a
taste one: repeating the list per reference would leave every mention but the
first unaddressable, and would leave the "back" link with several possible
targets. Only the first reference to a key carries an `id`.

`tests/notest.rs` asserts the collision-proof half directly, building a document
from `[^1]`, `^2`, `(3)`, `[4]`, `@five`, `{6}` and `~x~` and asserting it
converts identically with the modes on and off. The generated links are
same-document fragments, so no scheme is ever chosen from document text and
A11's policy has nothing to do here; the keys land in `id` attributes, so the
key charset is restricted to ASCII letters, digits, `-`, `_` and `.`.

Two options are added rather than one because citations and glossaries have
genuinely different failure modes -- a citation with no definition is a broken
promise, a glossary term with no use is just noise -- and a caller should be
able to want one without the other.

Gates: corpus 60/60, goldens 33/33, 304 engine tests, notest 20/20,
optionstest 20/20, GUI 74/74, fmt and clippy clean.

## 76. A12: A document cannot inject an attribute into a generated anchor

Recorded 2026-10-06.

Found while auditing the decision to omit `rel="noopener noreferrer"`, when the
question asked was the broader one: can a document subvert a generated link at
all? It can, and not through the scheme policy. Severity High, and higher than
A11's, because unlike A11 it needs no `javascript:` URL and no cooperation from
any option: the default `--make_links` is enough.

The gap is not in the escaping. The engine escapes `&`, `<` and `>` in document
text. It does not escape `"`, which is correct for prose -- a double quote is a
printable character and never needs escaping in running text. But the autolinker
writes what it captures into `HREF="$1"`, and four built-in rules captured
`\S+`, which admits `"`. The four rules now capture `[^\s"]+` instead, so a
quote cannot reach the attribute at all. No other producer is affected: the TOC,
pager and section ids are engine-generated and cannot contain a quote, and A11's
scrubber already scans finished markup, so it would have caught a rule this
change missed.

`tests/urlschemetest.rs` asserts the guard rather than the absence of the bug.
Ten payloads try to smuggle a second attribute past the engine -- `"onmouseover=`
with and without a leading space, an `<http:...>` spelling, the bare `www` and
`ftp` dictionary rules, `target="_blank"`, and an `onfocus`/`autofocus` pair --
and the test parses the attributes of every anchor in the output and fails on
any event handler or on `target`.

Note the ordering, because it is the point. A document that cannot name a
dangerous scheme can still end the `href="…"` attribute and supply its own
handler, so attribute injection is the sharper boundary. A11's scheme policy and
`rel="noopener noreferrer"` both do nothing about it. Any future URL-emitting
feature needs the same check.

Gates: corpus 60/60, goldens 33/33, 306 engine tests, urlschemetest 22/22,
optionstest 20/20, GUI 74/74, fmt and clippy clean.

## 77. Reconcile stale status and cross-references after A11/A12

Recorded 2026-10-06.

Splitting one finished document into four commits left the plan describing a
world that no commit ever existed in. None of this is a code change; every item
below is a statement that was true at some point during the work and stopped
being true, usually two commits before the one that made it false.

- The progress line and the numbering note both stopped at `A11` while the table
  beside them had gained an `A12` row. They now agree with it.
- The security-meta section still opened with "design-only. No code change yet",
  directly above bullets saying A11 is "now enforced" and the citations are
  "implemented". It now says what is true: the autolinking boundary is enforced
  as A11 and the rest is recorded as it is settled.
- That same bullet said "the note below originally said ...", pointing at a note
  that the edit had rewritten in place, so there was nothing below.
- The `{{textrill:head}}` design note still described the citation work as
  deferred, one paragraph above the paragraph that records it as implemented.
  It is separate work; that is what the sentence needed to say.
- Three numbers in the new prose did not survive contact with the finished
  tests. The A11 section quoted `tests/urlschemetest.rs` at 20 cases and the link
  integrity section at 12 documents; they are 22 and 10. It also said the section
  code rested on six unit tests in `section.rs`, a figure that was never right:
  the module had seven when the audit was done and has ten now. A guard sentence
  described one test as covering ten injection payloads *and* eight option
  combinations; those are two tests, and it now says so.
- `ADVERSARIAL-FINDINGS.md` pointed at "the A1-A10 addendum". It is A1-A12 now,
  and the findings table correctly has no S-row for either new item, because both
  were found by auditing this plan's own security note rather than by the attack
  pass.
- `### A12.` sat above `### A11.`, because A12 was appended to a region that had
  already taken A11's place. Reordered, and the A11 and A12 headings now match
  the wording of their own table rows.

Each correction is applied at the level where it becomes true, so every commit
in this series is self-consistent: `A1`-`A11` and 20 cases in A11's commit,
`A1`-`A12` and 22 cases in A12's.

Gates: corpus 60/60, goldens 33/33, 306 engine tests, GUI 74/74, fmt clean.

## 78. Record P5.7: budget-driven page boundaries, as a proposal

Recorded 2026-10-06.

Asked whether textrill can count a text file's characters including whitespace
and then choose how much of it to pour into each page of a multi-file
conversion. The answer is yes, and the counting is free -- `try_convert_chunked`
already holds every `--infile` as a `String` before conversion starts -- but the
question is not really about counting.

Written down rather than implemented, because four things have to be settled
first and none of them is a detail:

- Page boundaries are heading boundaries, so a single section larger than the
  budget overflows at *every* level. The budget cannot be guaranteed, only
  requested, and an implementation that overflowed quietly would be worse than
  one that has no budget, because the user asked for a limit.
- Cutting at exactly level L is not monotone. A document with no heading at L
  yields one page holding the whole body, so a search for "coarsest cut that
  fits" can step past a level that fitted onto one that does not. Cutting at
  every heading of level or shallower restores monotonicity and costs an orphan
  page per heading preamble.
- Chars, bytes and grapheme clusters are three different numbers, and source
  characters, body HTML and output bytes are three more. Each page pays the
  template again, so "how much text went into each template" has to be its own
  figure.
- Nothing here measures the section-size distribution of any real input, so
  whether a budget can do anything at all is unknown. Hence the recommendation
  to ship a report-only flag first: not avoidance, but the step that makes the
  cut decision informed.

Recorded at level 4 only, since it is recorded in this commit and the feature
does not exist yet. Gates unchanged: corpus 60/60, goldens 33/33, fmt clean; no
Rust touched.

## 79. Audit oracle coverage: 2 options unverified, not 34

Recorded 2026-10-06.

Claimed last commit that 35 flags had no external oracle. Checked it, and the
number was wrong in both directions -- so the claim is replaced with a derived
one.

The method matters more than the result, because the first attempt used a grep
and a grep cannot answer this question. `cli.rs` SPECS gives the port side (65
options, 121 names with aliases); the reference's GetOptions block gives what it
implements (58 entries, 113 names); the 60 cases are read by *sourcing*
cases.sh, not pattern-matching it. The option surface is 65, not the 28 Flag
lines a naive count reports.

| class | n |
| --- | --- |
| explicit differential oracle (corpus case or fuzz seed, compared byte for byte) | 45 |
| no reference equivalent, so nothing can be diffed | 13 |
| default path byte-verified, non-default branch not | 3 |
| nothing verified | 2 |
| plumbing | 1 |
| no-op | 1 |

Where the first pass went wrong, since the correction is the useful part:

- It ignored `fuzz.py`, which is a genuine differential oracle and varies 30
  options -- 45 options have an oracle, not 19.
- It treated "never passed on the command line" as "unverified". An option whose
  default is inert and which nothing varies is the only category that means
  nothing was checked; an option whose default is active is byte-verified in
  every one of the 60 cases whether or not it is named. That distinction is the
  whole question and the first pass missed it.

The surviving gap is two options, `append_head` and `prepend_file`. Both were
then checked by hand against the reference and are byte-identical, so this is a
coverage gap and not a correctness one: correct, but unpinned. The fix is two
corpus cases.

The 13 with no reference equivalent are recorded as a standing limit rather than
an oversight. They rest on goldens and hand-written assertions, and a golden
pins output against our own past output: it catches unintended change and cannot
catch a wrong decision implemented consistently and then frozen. Worth stating
plainly, because "306 tests" otherwise implies an external check that 13 of these
options do not have.

Recorded at level 4 only. Gates unchanged: corpus 60/60, goldens 33/33, fmt
clean; no Rust touched.

## 80. Draft the Flatpak manifest with a deliberately unusable app-id

Recorded 2026-10-06.

The name is textrill, which settles the leaf of the app-id but not the rest of
it: Flatpak wants a reverse-DNS id, and the only thing that fixes the domain is
the GitHub account sitting in the `repository = "https://github.com/<you>/textrill"`
TODO that both Cargo.toml files still carry. Since that account does not exist
yet, the manifest is written against `io.github.example.Textrill` -- with
`example` chosen precisely because it is not a real owner, so the id cannot be
published by accident. Substituting it later means changing three places at once:
both Cargo.toml files and the manifest filename.

The manifest is therefore complete but not buildable, and the plan says so rather
than implying otherwise:

- `cargo-sources.json` is consumed by both modules and does not exist. It is
  generated rather than committed: it is vendored crate metadata that only has to
  agree with one lockfile, and a stale copy is a build failure nobody can read.
  `make cargo-sources` produces it, and fails with the pip install line rather
  than pretending -- `flatpak-cargo-generator` is absent on this host, while
  `flatpak` and `flatpak-builder` are both present.
- Whether to generate that file or `cargo vendor` instead is left undecided. The
  Flatpak design note prefers offline/vendored dependencies; vendoring removes the
  generator dependency at the cost of a large tree. That is a real trade and is
  not silently decided here.

Getting the module graph right needed one correction while drafting: the GUI
module cannot source only `textrill-gui-rs`, because it depends on `../textrill`
by path and cargo will not configure without the sibling present. Both modules
now source the repo root, the engine builds first, and the bundled Noto fonts are
installed from `textrill-gui-rs/assets/fonts` to match where the binary looks
for them.

Recorded at level 4 only. No Rust touched; `make -n build`, fmt and the shell
syntax check all pass.

## 81. Add a document register and archive the stale feature-gap survey

Recorded 2026-10-06.

Seven instruction documents had accumulated with no stated status, and every
session had begun by re-deriving which were still true. That had been done
three times in three days and disagreed each time -- one pass treated
TOOL-SURVEY.md as live, the next treated ADVERSARIAL-FINDINGS.md as a live
checklist. A document with no status is the failure mode DOCS.md exists to end.

TOOL-SURVEY.md is archived rather than deleted. Its feature matrix had gone
false, not just stale: it still calls the port `txt2html-rs`, and reports
table of contents, HTML5 output, rc/config files, `charset` and a built-in
stylesheet as absent. All five exist. Every recommendation in its section 4 is
implemented. Keeping it matters for two things -- the upstream TOC disclaimer,
which is the evidence that generating the TOC in-conversion beats the
`htmltoc` post-processor upstream recommends, and the rationale for not
supporting option abbreviation, which is a divergence we still want. Both are
cited from live documents, so both pointers are repointed rather than dropped.

ADVERSARIAL-FINDINGS.md stays at the root and is marked Closed, not archived.
It is an attack pass whose findings A1-A12 are all implemented, so it is
evidence rather than instruction, and its scope note already flags the very
assumption being retired -- it judged the port "against the Perl module as the
specification".

The register also records the thing that prompted it. Running the tool on
Homer.txt -- 37 KB of real prose with an obvious human structure, PREFACE TO
FIRST EDITION, BOOK I. through BOOK XXIV. -- produces a single 38 KB page from
--chunk, because the file has no txt2tags markup at all for a sectioner to cut
on. Under the parity framing that reads as "does Perl do the same", which has a
known and irrelevant answer. The useful reading is that the tool cannot see
structure a human sees instantly, and that is this project's actual problem.
The plan's governing rule for such cases is the three-tier compatibility policy
at line 442, which says match Perl where Perl is right and must be better where
it is wrong. DOCS.md says so, and points at that section.

Two notes on the mechanics, both of which bit during this change:

- ADVERSARIAL-FINDINGS.md and textrill/README.md are generated by the level
  script, so editing them in the tree is pointless -- the next paint overwrites
  it. The status edits go in the script as a level-4 edit instead.
- The path rewrite runs once per level edit, so at level 4 it sees the same
  text four times. A plain string replace of `TOOL-SURVEY.md` was not
  idempotent: the second pass rewrote the filename inside the path it had just
  inserted, producing `legacy-archive/legacy-archive/TOOL-SURVEY.md`. It now
  uses a negative lookbehind, and the anchor check accepts either the old or
  the new form while still failing loudly if neither is present.

No Rust touched. fmt clean, corpus 60/60, goldens 33/33, all four levels paint
and the level-4 snapshot reproduces.

## 82. Retire the remediation plan and add a real-document smoke run

Recorded 2026-10-06.

The remediation plan is retired to legacy-archive/. It was 3 653 lines built by
a level-gating script that reconstructed it at four points in the project's
life, and it answered the wrong question: "what does Perl do that we do not?".
Its inventory of 65 options exists to produce a list of gaps, and producing
that list is what kept the work oriented toward mimicry. The plan is kept,
along with the script that built it, because it is the record of why decisions
went the way they did -- including the ones that turned out wrong.

ADVERSARIAL-FINDINGS.md goes with it, for the same reason and one more: it
judged the port "against the Perl module as the specification", which is the
assumption being retired. Its findings A1-A12 are all implemented and verified,
so it is evidence about a finished job, not instruction for the next one.

Retaining the Perl reference is deliberate and is not a half-measure. ref/ stays
where it is, on three grounds: it is the oracle for the parity tier of the test
harness, which is the only thing that found A1-A12; the user asked for it to
remain available for historical artifact investigation; and the corpus and CI
both read it. It is an oracle now, not a specification.

quicknote1.txt is agent residue from the abandoned prior attempt -- a transcript
in which the writer is reasoning about its own todo list. It is the concrete
evidence for the note in template-research1/FINDINGS.md that the prior project's
process apparatus became its failure mode, so it is archived rather than
deleted.

The interesting part of this change is examples/. Every gate so far runs on
short synthetic inputs, because a differential harness needs inputs whose
expected output is already known. That leaves the opposite question open: does
the tool hold up on a document somebody actually wrote? examples/homer.txt is
37 KB of the Project Gutenberg Odyssey -- running prose whose section titles are
set in capitals with no underline and no markup whatsoever.

`make examples` reports what the engine recovered. On that file:

    homer.txt   38888 B   h=0   p=64   strong=39   br=34

h=0 is the finding. The document has PREFACE TO FIRST EDITION and BOOK I.
through BOOK XXIV. plainly set in the text, and the engine found all 39 of
those capitalised runs as <strong> -- but nothing in the file marks them as
headings, so there is no structure to cut on and --chunk emits a single 38 KB
page. That is not a bug to fix by matching Perl, which has the same limitation.
It is the actual problem, and it is what the next plan is about.

No Rust touched. fmt clean, 306 engine tests pass, corpus 60/60, goldens 33/33.

## 83. Research the landscape and plan from the tool's own results

Recorded 2026-10-06.

The remediation plan is gone, so the gap it filled needed filling properly:
nothing said what this project should now do. These four documents are built
from measurements instead of from the Perl interface, and each item in the plan
traces to one.

docs/CAPABILITIES.md -- what textrill does, established by running it. Layout
inference is measured feature by feature, and the five things the tool does
badly are reproduced rather than described. Two of them are worth reading twice
because the old framing never surfaced them: an ordered list that does not start
at 1 is not recognised as a list at all, and a blank-line-separated ordered list
after a bullet list nests inside it. Both are byte-for-byte what the Perl
original does. That is the whole argument for retiring parity as an organising
principle -- a harness that reports success whenever the reference is reproduced
cannot tell you that the reference is wrong.

The encoding result is the cleanest evidence for the other side. textrill reads
examples/homer.txt as UTF-8 and renders "The Authoress of the Odyssey" intact.
The Perl original reads the same bytes as CP1252 and emits &acirc; on every
multi-byte character. The port is right and the reference is wrong, on the
author's own expected output.

docs/LANDSCAPE.md -- the category, from competition-files/pandoc-3.12.tar.gz
and the installed pandoc 3.1.3. The finding is structural rather than numeric:
pandoc has no plain-text reader. `-f plain` is rejected outright, and forcing
markdown on the Odyssey yields zero headings, zero strong, zero breaks, and a
3 665-character paragraph where the title block, contents and first chapter
headings should be -- because markdown's soft-wrap joining rule assumes an
author who never meant a line break. For a document that was never markdown, it
destroys information.

It also records where we are behind, which is where the user pointed: a user's
first impression of a converter is its templates, and pandoc injects arbitrary
user variables and conditionals into a template where textrill has seven fixed
slots and no way to pass a value in. Verified directly rather than asserted.

docs/PLAN.md -- the seven phases that follow from those two documents. Phase 1
is giving the tool its own name: every file it writes currently claims
HTML::TextToHTML v3.0 generated it, which is a false provenance claim and
distinct from licensing, where the GPLv3 credit in textrill/LICENSE is correct
and stays. Phase 2 fixes what is wrong, with corpus cases that fail before each
fix. Phase 4 is the template gap. Phase 5 is the one worth arguing about: on
homer.txt the engine recovers 39 capitalised runs as <strong> and no headings
at all, so --chunk emits one 38 KB page. Treating a short all-caps line alone
between blanks as a heading is txt2tags' inference done in reverse. Shouting
in prose is real, the caps heuristic already owns that text, and three existing
options become load-bearing for it -- so the plan requires measuring the false
positive rate over examples/ before any behaviour changes.

docs/PACKAGING.md -- the manifest's two deliberate blockers. The app-id is
io.github.example.Textrill because "example" is not a real owner and cannot be
published by accident; the domain half is still `<you>` in both Cargo.toml
files. cargo-sources.json is generated, needs flatpak-cargo-generator, which is
not installed here, so `make cargo-sources` fails honestly rather than
pretending. Generate-or-vendor is argued and left undecided.

DOCS.md -- rewritten as a register with three statuses, each decided by a test
rather than a feeling: Authoritative means code and CI check against it,
Background means it records a decision, Archived means it lives in
legacy-archive/ and is never read as current. A document with no status is the
failure mode this file exists to end, which is how TOOL-SURVEY.md came to have a
matrix that went false while still looking authoritative.

Both pandoc tarballs are tracked so the studies can be re-run offline, matching
how txt2html-3.0.tar.gz is already handled; the extracted trees stay ignored.
No code touched: fmt clean, clippy clean, 306 engine tests, 74 GUI, corpus
60/60, goldens 33/33, and `make examples` reports the Odyssey numbers above.

## 84. Record that fixing generator provenance breaks 13 goldens

Recorded 2026-10-06.

Phase 1.1 of the plan -- replacing HTML::TextToHTML v3.0 in the generator meta
with textrill -- looked like a one-line change with no behavioural effect. It is
not, and the plan now says so.

golden_check compares against the upstream Perl tfiles/good_*.html with a
byte-exact cmp. 13 of the 32 goldens contain
<meta name="generator" content="HTML::TextToHTML v3.0"/>, so correcting the
provenance breaks 13 golden comparisons immediately:

  heading1 links3 list-4 list-5 list-advanced list-custom list-styles mixed
  news robo sample umlauttest xhtml_sample

Three ways to handle it, none free: normalise the line out before cmp (keeps
coverage on all 32 stems but adds a normalisation step to the harness, which is
the shape of thing that has bitten this project before); exempt the 13 stems via
NOGOLDEN[] (no harness change, but NOGOLDEN means skipped, so a real regression
in those stems goes unseen -- and 5 of them are list cases, which is the exact
family where the two inherited list defects live); or keep the Perl string and
leave the tool misattributing itself in every file it writes.

Left unresolved in the plan rather than picked quietly, because it is a question
about what the parity tier is for rather than a consequence of fixing a string.
The 13 stems are named in the plan so a future decision cannot quietly shrink
golden coverage without it being visible.

## 85. P1.1: textrill names itself as the generator

Recorded 2026-10-06.

Every document this tool wrote began with

    <meta name="generator" content="HTML::TextToHTML v3.0"/>

which is a false provenance claim in the one field whose entire job is to say
what produced the file. It was there because that string made the port
byte-identical to the Perl reference -- the same trade that the retired plan
made everywhere, and the same trade producing the same kind of lie.

Provenance and attribution are different claims. The GPLv3 credit for
HTML::TextToHTML belongs in LICENSE, where it already is, permanently and
correctly. Putting it in the generator was never attribution; it was a false
statement about authorship of the output. textrill produced these documents, so
the meta now reads:

    <meta name="generator" content="textrill v0.1.0"/>

The version comes from env!("CARGO_PKG_VERSION") rather than a literal, so a
Cargo.toml bump cannot leave a stale string in every generated document. That
also retires the old "3.0", which had been standing in for the Perl module's
version rather than this crate's.

What it cost, measured rather than estimated
---------------------------------------------
18 differential cases and 13 goldens fail, not the 13 originally predicted. The
differential path is affected too, because a fresh Perl run names itself as
well -- a fact the earlier estimate missed because it only counted goldens.

How the divergence is handled
-----------------------------
normalize.py canonicalises exactly one line -- a meta start tag whose name
attribute is "generator" -- to a sentinel, on both sides, before any
comparison. tests/corpus/README.md gains a section on it.

The alternative was NOGOLDEN[] on the 13 stems, which is the smaller diff and
the wrong one. NOGOLDEN means *skipped*: those stems would stop being compared
at all, and five of them are list cases -- the family holding the two inherited
defects in docs/PLAN.md Phase 2. Losing golden coverage there to avoid editing
one string trades a visible divergence for an invisible hole.

Normalising excludes the line, so it cannot itself assert the value. The other
half of the gate is tests/provenance.rs, which requires the exact expected
string, fails on "TextToHTML" in any spelling, pins both tag-case branches and
HTML5 mode, checks the version against CARGO_PKG_VERSION, and runs once
through the spawned binary rather than only in-process.

Both halves were broken on purpose, per the standing rule:

  PROG reverted to HTML::TextToHTML
    -> 5/5 provenance tests fail, corpus stays 60/60 and 33/33
       (the corpus genuinely does not judge this value)

  every heading level shifted +1
    -> corpus exits 1, 11 differential and 11 golden failures,
       NORMALISED unchanged at 67

The second run is the one that matters for normalize.py: a real content
regression two thousand lines from the generator still produced 11 golden
failures, so the normaliser is not a general one quietly swallowing
differences. Its regex matches one line shape and will not touch a body
paragraph containing the word "generator"; both properties are pinned by a
table test in the commit that introduced it.

NORMALISED: 67 is printed on every corpus run including when it is zero, because
a normalisation that quietly stopped applying would leave the corpus green.

One incidental fix
------------------
encodingtest.rs asserted the old generator string in order to check that each
meta sits on its own line. It was testing line adjacency and failing for a
reason that had nothing to do with encodings; it now spells the generator from
the crate version, so it cannot go stale that way again.

Provenance now, GPLv3 credit unchanged: fmt clean, clippy 0 warnings, 311 engine
tests (306 + 5 new), 74 GUI, corpus 60/60, goldens 33/33. `make examples` on the
Odyssey reads content="textrill v0.1.0".

## 86. P1.2-1.4: textrill stops borrowing the reference's names

Recorded 2026-10-06.

Phase 1 of docs/PLAN.md completes here: the three remaining surfaces on which
this tool described itself as the Perl reference are now its own, and the
differential harness no longer leans on the reference's binary path.

Three surfaces, three fixes
---------------------------
rc file: rcfile.rs reads .textrillrc first and falls back to .txt2htmlrc only
when the preferred name is absent *in the same directory*. Deduplication is by
directory, not by path, so HOME=$PWD resolves one file rather than the same
inode twice, and precedence is unchanged: @file < home < cwd < command line.
Four unit tests pin preference, legacy fallback, the home==cwd case and
home-before-cwd ordering.

link dictionary: options.rs gains default_link_dict(home) with the same
preference (.textrill.dict, then .txt2html.dict), four unit tests behind it.
A user with only the legacy file sees no behaviour change; a user with both
gets the textrill one.

--help: the first line read "Convert text to HTML using Perl's HTML::TextToHTML
module", which is false in the one place a user reads to find out what the
program is. It now describes what the tool does and points at the README. The
epilog lists both rc names, and the Source::File / parse_args_with_rc doc
comments say "textrillrc-style file" rather than naming only the legacy one.

The GUI's two About labels and the crate docs, README, corpus README and
run.sh comments follow, so no document in the tree frames the tool as leaning
on txt2html. Where provenance belongs it stays: docs/LANDSCAPE.md keeps
"portable at source level" and the GPLv3 credit keeps HTML::TextToHTML in
textrill/LICENSE, where it is correct. The one sentence removed outright is the
README's "The compatibility work is the point", because it is no longer true --
the corpus is an oracle, not a specification, and the README now says so.

Gates, each broken on purpose before it was accepted
----------------------------------------------------
new optionstest.rs case requires "textrill" and fails on "HTML::TextToHTML" in
any spelling in --help
new provenance case requires the new first line
rcfile rc_names_prefers_textrillrc  : RC_NAMES reversed  -> 1 optionstest +
                                       2 rcfile failures, corpus unaffected
options dict_default_prefers_textrill : NAMES reversed  -> 3 of 4 fail
help_describes_textrill...          : old line restored -> fails

The fuzzer and two findings it had never been able to report
------------------------------------------------------------
tests/corpus/fuzz.py pointed at target/debug/txt2html, which stopped existing
when the binary was renamed, and run.sh/fuzz.py wrote into a RUNDIR of
/tmp/textrill-corpus. Both now default to the textrill paths.

More importantly the fuzzer could not have told a real regression from the
generator line it did not normalise: normalize.py was used by run.sh and
golden_check only. P1.1's canonicalisation is now applied by the fuzzer to
both sides through normalise_bytes(), and the summary prints
"NORMALISED: n generator meta line(s)" -- printed even when zero, because a
normalisation that quietly stopped applying would leave the fuzzer green.

That turned the generator from red noise into a signal, and the first clean
sweep reported 4 mismatches in 16 000 cases. All four were one shape:
upstream's tfiles/pre.txt contains "file:Here", the reference links it,
textrill refuses it. That is A11's scheme policy against a reference with no
policy at all -- DANGEROUS_SCHEMES in src/urlscheme.rs refuses javascript,
data, vbscript and file and unwraps the anchor keeping the text. It is Tier 2
with its own oracles (tests/urlschemetest.rs, corpus case opt_injection), so
byte comparison is the wrong oracle for this input and no byte sequence could
match.

It is removed from the claimed input domain in sanitise(), alongside the
existing non-ASCII and trailing-whitespace restrictions, and not recorded as a
known divergence: a list keyed on an output shape is the machinery this file
removed on purpose, and it would outlive its reason and start hiding real
link-handling regressions. The corpus README states the third restriction with
its reason. The three affected seeds re-ran at 0 mismatches with NORMALISED
unchanged (3718, 3754, 3708).

FUZZ_JOBS is now $(shell nproc), which is 20 here; the summary line reads
"fuzz: 8 seeds x 2000 cases, 20 at a time".

Recorded in the plan
--------------------
docs/PLAN.md marks Phase 1 items 1.2-1.4 done, adds a resolution section with
the sabotage table above, and records both fuzzer findings -- the missing
generator normalisation and the scheme-token domain restriction -- so the next
reader can see why each exists rather than discovering it by deletion.
docs/CAPABILITIES.md marks 4.5 fixed, docs/LANDSCAPE.md drops "portable at
source level". README's stale counts move with measurement: 60 corpus cases
(was 59) and 324 Rust tests (was 306; 311 after P1.1, +13 here).

Verification
------------
make verify exits 0: fmt clean on both crates, clippy 0 warnings with -D
warnings, 324 engine tests, 74 GUI tests, proptest, alloctest, corpus
PASS=60 FAIL=0 and GOLDEN 33/33, and 8 fuzz seeds x 2000 cases = 16 000
comparisons with 0 mismatches.

## 87. Gate the pandoc positioning claims, and correct Phase 5's premise

Recorded 2026-10-06.

The project can now say what it offers, so the next problem was that such
statements rot. legacy-archive/TOOL-SURVEY.md is the precedent: a feature
matrix that went false while still looking authoritative, and DOCS.md exists to
end exactly that. Claims about *another* program are the same species of
document -- they are statements about this machine, today -- so they are now
gated rather than written down once.

docs/OFFERING.md, and what it is for
------------------------------------
Nine sections: the configuration as it actually is (measured, not described);
the claim and its boundary; the measurement; the offer per audience; the build
order; the release checklist; what pandoc does better; risks; gates. It adds no
engine work -- every item it names already lives in docs/PLAN.md -- it sequences
them behind the claim, and docs/PLAN.md's Sequencing section now cross-references
that order so the two authoritative documents cannot disagree.

The claim is deliberately narrow: unmarked text, inferred structure, HTML
through the user's own template. Its "must not say" list is part of the
document rather than a private note: pandoc is not bad, we do not compete for
formats, inference is not certain, and the Perl reference is not a
specification. A positioning document that only lists strengths is the failure
mode this project already has a file to prevent.

make proof, and why it is not in verify
---------------------------------------
textrill/tests/positioning.py re-measures 21 pinned claims, exits 1 naming the
claim that moved, and exits 2 naming pandoc when the oracle is missing rather
than skipping green. It is deliberately outside `make verify`: verify must stay
runnable on a box with only Rust and perl, and a gate that sometimes cannot run
has to say so loudly.

Broken on purpose before it was believed, per the standing rule:

  option surface pinned 65 against a measured 64   -> FAIL P16, exit 1
  docx structure pinned (113, 34) vs (112, 34)     -> FAIL P19, exit 1
  PANDOC=/nonexistent/pandoc                       -> exit 2, names pandoc

The epub claim is structural (its mimetype entry), not a byte count: a zip's
bytes depend on the clock, and a claim that fails when the minute changes is
not a claim.

What the measurements found
---------------------------
pandoc has no plain-text reader in the installed 3.1.3 *or* in the 3.12 source
we ship, so the load-bearing positioning claim is checked against both. It
reads 43 formats including t2t -- txt2tags, the nearest sibling -- so the ground
held is specifically unmarked text, not text conversion.

On examples/homer.txt the two tools are 39 <strong> and 34 <br/> against 0 and
0, with pandoc merging the title block into one 3 665-character paragraph. That
was already in CAPABILITIES and LANDSCAPE; what is new is the conjunction:
textrill's HTML into `pandoc -t epub|docx|markdown` succeeds, and the structure
*we* inferred reaches the far format -- 113 bold runs and all 34 line breaks in
the .docx, `**PREFACE TO FIRST EDITION**` in markdown. Inference that dies at
the format boundary would be worth nothing, so P17-P20 gate it.

Phase 5's premise was wrong, and now says so
--------------------------------------------
The plan claimed homer.txt has "37 obvious section titles ... on their own line,
surrounded by blank lines". Measured: 40 all-caps lines, of which 24 are the
contents list (24 consecutive lines in one block, entries for sections this
extract does not contain), leaving three genuine section starts -- two PREFACEs
and a three-line title block. The false positives sit in the same shape as the
truth: two signatures and an address.

The candidate rule was therefore measured on the document that motivated it:

  alone between blanks        -> 2 fires, both signatures, 0 real titles
  whole block short-caps      -> 3 fires, 2 signatures, 1 title block
  block starting with caps    -> 6 fires, 3 true, 3 false

Every simple formulation is empty or half false, so Phase 5 restates the
candidate as a *block* hypothesis (what separates a title block, a contents run
and a signature is position, not case) and gains three items: 5.0 `--report`,
the inference counts on stderr -- report-only first, which is what a user whose
--toc came out empty needs, and what the phase's own harness needs; 5.1 the
measurement over examples/; 5.2 what `custom_heading_regexp`'s block-start
condition should be, decided rather than silently left. `-H` only firing at a
block start was found the hard way: `-H 'BOOK I'` yields 0 on homer, `-H
'PREFACE'` yields 2, and neither --help nor the README says why.

Incidental correction
---------------------
CAPABILITIES section 5 still carried "38 888 bytes out" from before P1.1: the
generator line lost six bytes when it stopped naming the Perl module. The
number, and the reason it moved, are now both on the page.

Verification
------------
make proof: 21 claims, 0 drifted. No Rust, no behaviour, no tests changed, so
make verify is unaffected by this commit.

## 88. Withdraw Phase 2.1-2.2, and make PLAN.md Sequencing authoritative

Recorded 2026-10-07.

The two list fixes were reverted after the 2.1/2.2 experiment: a Tier 2
content divergence and the fuzz differential are mutually exclusive by
design, so the plan now records them as withdrawn rather than pending.

The Sequencing section becomes the only ordering, replacing the two
silently conflicting ones — the old text claimed the orders agreed while
OFFERING's reader order skipped Phase 2's survivors and Phase 3 and could
not see that Phase 5's measurement needs Phase 7's corpus growth first.
Each open step now carries its verification check on top of the make
verify/make proof baseline, per the standing rule. OFFERING.md defers to
it.

## 89. S1: name the body-wrap template --body_template

Recorded 2026-10-07.

Phase 2.3: --template and --document_template were near-identical names
with opposite behaviour and no diagnostic, so picking the wrong one
silently produced a complete document nested inside a <body>. The body
wrap is now --body_template, making the pair read as opposites in --help;
the legacy --template spelling still works (an rc file may use it) but
prints a deprecation warning naming both poles.

Gates: optionstest asserts the alias resolves to the renamed option and
that the help names the pair and the deprecation; templatetest asserts
the alias still converts byte-identically and warns. The accepted-spelling
count moves 121 -> 122 (README and optionstest updated together).
make verify is green.

## 90. S2: document and pin the definition-list trigger

Recorded 2026-10-07.

Phase 2.4. A line that is exactly `term:` - a name of two or more word
characters, a colon, nothing after it - opens a <dl> with the name as <dt>
and the following indented block as <dd>. It has worked since the first
port and matches the reference byte for byte, but nothing told a user it
existed.

The one-line form `term : definition` stays a paragraph, deliberately:
a colon is ordinary prose and the same delimiter already starts an ordered
list (1: two, a: one), so the obvious inline form would change the meaning
of real prose and is a core-conversion content divergence - the class
withdrawn at 2.1-2.2.

Gates: the new fixture-driven 'definitions' corpus case pins the emitted
<dl> bytes and the <p> boundary against the reference. Its sabotage was
observed: with the term trigger disabled the case fails ('ref : <dl>',
'mine: <p>dpi:'). Docs also gain a Sequencing constraint that the Perl
reference is a parity oracle, not a design authority, after the decision
to present the trigger in textrill's own terms. make verify is green
(61/61 corpus, 33/33 golden, fuzz 0 mismatches).

## 91. S3: add --var name=value, filled into {{textrill:var:name}}

Recorded 2026-10-07.

## 92. S4: ship five templates selected by --template_library

Recorded 2026-10-07.

article, book, manpage, slide (whole document) and bare (body wrap) live
as files under textrill/templates/ and are embedded in the binary via
include_str, so the library works from any directory and can be copied.
Each uses only the fixed slots, so it converts with zero required
arguments and produces no silent-empty frames; bare is byte-identical
to no template at all. --template_library is mutually exclusive with the
file-template options, an unknown name is a hard error naming the
library, and the model refusals are inherited from the file-template
path (library.rs resolves to the same template_source machinery).

Gates: 4 library unit tests; templatetest +4 (every template converts a
document, unknown name is a hard error, mutual exclusion, model
refusals); option counts 67/124; P16 67. Both gates broken once on
purpose and reverted: a template without its content slot fails
validation and conversion, and an unknown name resolving to a template
fails both the unit and integration gate.

## 93. S5: emit HTML5 by default

Recorded 2026-10-07.

## 94. S5: emit HTML5 by default

Recorded 2026-10-07.

Completes the S5 change whose positioning.py half landed in c2d08b3.
Defaults become html5: true, lower_case_tags: true, xhtml: false.
--html5 and --xhtml are each other's complement and carry their mode's
tag case, so --no-html5 lands on the reference's HTML 4 with its
upper-case tags while an explicit --lower_case_tags given after the
flag still wins. do_file_start checks xhtml before html5 so a
post-construction mutation wins; the settings blob assigns the two
doctype booleans directly instead of replaying the CLI's transition
arms.

The precondition made the planned exemptions unnecessary: run.sh pins
the doctype on both sides (the reference is built with 'xhtml' => 1
placed first so CTOR/EXTRA still override, the port is passed --xhtml
ahead of its own flags) and fuzz.py puts exactly one of --xhtml /
--no-xhtml at the front of every case. With the pins in, the flip
moved no case: corpus 61/61, goldens 33/33.

Gates: provenance asserts the default document carries exactly one
<meta charset="utf-8"> and legacy mode none; html5test pins both
directions; optionstest, paratest, notest and the GUI tests follow the
new default; P8 measured 38 477 B. Sabotage observed: restoring the
old defaults turned html5test (3), provenance (2) and P8 red while the
pinned corpus stayed green.

## 95. S6: add --report, the inference counts on stderr

Recorded 2026-10-07.

P5.0. --report prints what the conversion recovered once the output is
written — bytes, headings, paragraphs, capitalised runs, line breaks —
as one key=value line on stderr. The output is byte-identical with and
without it; only stderr differs. -make examples> now reads the line
rather than grep-ping the output itself, so the CLI count and the
smoke run are one implementation and cannot drift; reporttest is the
independent gate, recounting the output with a different technique.

Counting is of the produced tags, not emission events (notes.rs drops
a <p> wrapper the engine already contributed), and is
case-insensitive so --no-html5's upper-case tags still count. The
paragraph rule is <p followed by '>' or whitespace: <p class=...>
(mailmode) counts, <pre> does not. Refused with --stream, which never
assembles a document to count; --chunk totals the run.

Gates: reporttest (9 tests) pins off-by-default, stderr-only,
byte-identical output, the recount equality across <pre>-bearing,
mailmode and upper-case inputs, chunk totals, the stream refusal, and
exactly the five numbers make examples prints for homer.txt
(38477/0/64/39/34); report unit tests pin the individual rules;
optionstest moves 67 -> 68 options, 124 -> 125 spellings; the GUI
widget coverage moves 67 -> 68; positioning P16 moves 67 -> 68; make
proof is 21 claims 0 drifted; make verify green.

Sabotage observed on purpose and reverted: a naive <p prefix failed
an_indented_block_is_not_a_paragraph and the_counts_are_what_is_in_the_output
(homer has no <pre>, so the absolute gate alone would not have caught
it), and a report sent to stdout instead of stderr failed six tests
because the output differed.

## 96. S7: grow the example corpus (eight CC0/PD documents, provenance register, counts pinned)

Recorded 2026-10-08.

## 97. S8: measure candidate heading rules over the example corpus (make measure)

Recorded 2026-10-08.

The R2a/R2b/R2c caps rules plus the setext baseline, measured against a
per-document truth set over all eight examples and pinned by tests/measure.py.
The table corrects the plan's own first pass (R2b is 2, not 3) and the verdict
is that no candidate rule is near zero false positives on real prose, so the
Phase 5 heading change stays a proposal. No behaviour change this step.

Also records the S7-aftermath documentation: the invisible-Unicode capture
notes (nbsp/zwsp provenance) and the rejected TTRPG candidates, in
examples/README.md.

## 98. S10 prep: resolve the GitHub owner; root README, LICENSE, CONTRIBUTING

Recorded 2026-10-08.

The owner decision (which blocked both Cargo.toml repository TODOs, the
Flatpak app-id, and packaging) is resolved: the repo is
github.com/youtubescholar/textrill.

- Root README.md: landing page (pitch, status, build/verify, layout map,
  fork disclaimer, licence summary); CONTRIBUTING.md: house rules, gates,
  CC0/PD corpus bar, where to start; root LICENSE: GPL-3.0-or-later,
  byte-identical to the crates'.
- repository = "https://github.com/youtubescholar/textrill" in both
  Cargo.toml files; the README's cargo install --git line follows.
- Flatpak manifest renamed io.github.example.Textrill.yml ->
  io.github.youtubescholar.Textrill.yml, app-id updated; PACKAGING.md,
  OFFERING.md, PLAN.md Phase 6/S10 and DOCS.md's decision register
  updated to record the resolution. Remaining packaging blocker is the
  generate-vs-vendor decision on cargo-sources.json.
- Gates: make fmt-check clippy test green (docs/metadata change; the
  differential suite is unaffected and runs in CI on the push).

## 99. Post-push audit: fix clippy-1.99 lint, align licence boilerplate, correct counts

Recorded 2026-10-08.

CI's floating stable (clippy 1.99) failed the first push on 9 needless
`&at` borrows in cli.rs; local toolchain updated to match so the skew
cannot hide a gate again. Differential and musl jobs had passed.

Boilerplate:
- OFL.txt and OFL-NotoSansCJK.txt headers now carry the copyright
  notices embedded in the shipped font binaries (Google LLC 2015-2021;
  Adobe 2014-2021) — the OFL 1.1 notice-travels-with-the-font rule.
- Root README Licence section now mentions the bundled OFL fonts.
- Licence prose tightened to GPL-3.0-or-later everywhere outside the
  LICENSE grant text itself (CONTRIBUTING, PLAN, CAPABILITIES clause
  10, crate README, provenance.rs comment).

Counts corrected from the tools: 365 engine tests (make test), 33
golden comparisons over 32 files (make corpus: 33/33, 0 differing).
Root README's packaging row: one remaining blocker, not two.

## 100. licence: canonical GPL-3.0 text in LICENSE, project prose in LICENSE-NOTICE

Recorded 2026-10-08.

GitHub's licensee reports NOASSERTION (74% similarity) on a LICENSE whose
preamble precedes the licence text. LICENSE is now the exact FSF GPL-3.0
text (100% match in the sidebar); LICENCE-NOTICE.md carries the grant,
the Why-GPLv3 reasoning, upstream copyright and the modifications notice.

## 101. housekeeping: consolidate docs, research, stubs; drop stray ansi2html zip

Recorded 2026-10-08.

- RUST-GUI-FINDINGS.md and template-research1/ now live under docs/.
- competition-files/ -> research/pandoc/ with explicit tarball names
  (pandoc-3.12-full / pandoc-3.12-stripped); the two txt2html reference
  archives move under research/ too.
- tests/refstub -> stubs/ (the single canonical Perl stub); root tests/
  is gone.
- ansi2html-main.zip removed: referenced only by an archived survey, no
  gate or target uses it.
- Makefile, .gitignore, CI-referenced paths, packaging skip lists,
  DOCS.md register, README layout table, LANDSCAPE/OFFERING/CAPABILITIES
  and the corpus README updated in the same change.
- make ref and make ref-large verified under the new paths.

