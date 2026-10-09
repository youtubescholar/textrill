# Oracle archaeology — which oracle found which bug

Written 2026-10-08. Status: **Background** (see `DOCS.md`). It records
evidence about finished work and justifies a decision; nothing is checked
against it.

The question this file answers: **for every bug this project has found in
itself, which oracle found it, and would a reference-free oracle have found
it?** The answer decides how much of the Perl differential harness earns its
keep.

## The four oracles

| oracle | what it is | can it judge |
|---|---|---|
| **Differential** | byte-compare against Perl `HTML::TextToHTML` 3.0 (`run.sh`, `fuzz.py`) | *same / different* — never *better / worse* |
| **Golden** | a frozen expected output: upstream author's `good_*.html`, or a pinned self-golden | identical to the frozen bytes only |
| **Property** | an invariant that holds whatever Perl does: no data loss, XML well-formedness, determinism, no panic, resource bounds, exit code (`proptest.py`, `alloctest.rs`) | *correct on its own terms* |
| **Truth set** | a hand-authored expectation for one document (heading counts in `measure.py`, decoded code points in `encodingtest.rs`) | the specific authored claim |

A differential oracle's ceiling is `diff`: it can say *this differs*, never
*this side is right*. That single limitation is the whole story below.

These four cut across, rather than replace, the three tiers in
`legacy-archive/REMEDIATION-PLAN.md` § Compatibility policy: Tier 1 is differential and
golden, Tier 2 is golden, truth and property, Tier 3 is property and truth. The
finer split is used here only to ask *which oracle found which bug*.

## The table

"Found by" is taken from `legacy-archive/ADVERSARIAL-FINDINGS.md` (the A-series)
and `legacy-archive/REMEDIATION-PLAN.md` §E1–E3, which state their own
provenance explicitly.

| # | Finding | Found by | Frozen golden? | Property? | Correct output knowable without Perl? |
|---|---|---|---|---|---|
| A2 | leak in `ascii_re_cached` (`Box::leak`) | attack pass, allocator/RSS | no | **yes** — resource bound (P12) | n/a, output is correct |
| A1 | >500 KB paragraph panics (backtrack limit + `$` lookaround) | attack pass, crash probing | no | **yes** — no-panic / termination | no |
| A1b | the hang behind A1 (`\B`/`(?<!delim)` in `do_delim`) | attack pass, after A1 | no | **yes** — termination / resource | no |
| A3 | `tab_width=0` divide-by-zero panic | attack pass, option boundary | no | **yes** — no-panic | no |
| A3b | huge `tab_width` aborts/hangs | attack pass, option boundary | no | **yes** — resource bound | no |
| A4 | GUI swallows `PanicException` | real Qt event loop | no | reference-free (visibility) | no |
| A5 | GUI re-encodes non-UTF-8 on save | attack pass, round-trip | no | **yes** — data loss / round-trip | no; Perl is the defect here |
| A6 | superseded conversions never cancelled | attack pass, latency | no | **yes** — resource / latency | no |
| A7 | save creates directories on a typo | attack pass | no | behavioral, no property | n/a |
| A8 | `--title` / `--style_url` XSS | attack pass | no | **yes** — XML well-formedness (P12) | no; escape is a declared choice |
| A9 | missing input exits 0 | attack pass | no | **yes** — exit-code property | no |
| A10 | unbounded `re_cache` | code review only | no | not demonstrated | no |
| E1 | `--table_type` merged into defaults | **differential fuzzer** | no | no | POD *documents* replacement; Perl found it |
| E2 | explicit-quote `<pre>` dropped text after blank line | **differential fuzzer** | no | **yes** — no data loss (P12) | no, once loss is noticed |
| E3 | CR path leaves two extra blank lines | **differential fuzzer** | no | no — well-formed, lossless, deterministic | **yes, only Perl** |
| — | `delim_retry` (`#` pair across `</p><p>`) | **differential fuzzer**, seed 99 | no | no obvious invariant | **yes, only Perl** |
| — | non-ASCII delimiter tested as bytes (`ééwordé`) | differential test surfaced it | no | no | **yes, only Perl** |
| P5 | inherited hang (dictionary pattern) | differential fuzzer timeout | no | **yes** — termination | n/a, Perl hangs too |
| P6 | ~2× slower; prefilter off for `\b` | benchmark | no | **yes** — resource bound | n/a |
| P7.1/P7.4 | CP1252 / CJK / UTF-16 decoding | non-ASCII gap probe | **yes** — author's `good_utf8.html` + `encodingtest.rs` | **yes** — decoded code points, no C1 | no; **Perl is the defect** |
| P22 | invalid user regexp panics | property / no-panic | no | **yes** — no-panic | no |
| P1–P21 | the harness's own false-green bugs | **sabotage** (break the gate on purpose) | n/a | n/a | no |

## What the table says

**The attack pass and the property suite cover the crash class.** Every
A-series finding — panic, hang, leak, XSS, exit code — is reachable by an
oracle that never runs Perl: a crash probe, an allocation counter, an XML
parser, an exit-status check. `ADVERSARIAL-FINDINGS.md` says so itself: that
pass "did not audit the transformation logic against Perl" and found none of
E1–E3.

**The differential fuzzer's unique contribution is small and now frozen.**
Its exclusive discoveries are the *silent semantic divergences*: E3, the
`delim_retry` boundary case, and the non-ASCII delimiter predicate. E1 is
borderline — the POD documents the replacement semantics, so a spec-derived
test could have found it; Perl is where the *discovery* came from. E2 was also
a differential discovery, but P12's no-data-loss property would have caught it
on its own.

So among the correctness divergences, **three** justify Perl as a *discovery*
device, and each is now pinned as a corpus case or regression test — the
frozen artifact does the future work, not Perl.

**Perl is sometimes the wrong oracle.** On genuine UTF-8 input Perl emits
mojibake and spurious `<sup>TM</sup>/<em>` markup where the author's own
`good_utf8.html` is correct. On CP1252/CJK/UTF-16 the reference either cannot
produce the right output or produces the bytes for a browser to guess. A gate
that demanded parity there would have reverted correct behaviour.

**The differential approach is not myopic; the oracle set was.** A single
oracle structurally cannot see a class of bugs — that is
`REMEDIATION-PLAN.md`'s own conclusion, and P12 (the property suite) is the
fix that was implemented.

## What parity was actually for, and why it can end

Byte-parity served four jobs: (1) *bootstrap conformance* — prove the port
understood the original; (2) *regression* — freeze behaviour so a refactor
cannot drift; (3) *discovery* — find divergences the author never enumerated;
(4) *identity*. Jobs 1 and 3 are spent — the port understands the original, and
the three semantic discoveries are frozen. Job 2 is now done better by
self-goldens and properties, which cannot rot into a false pass the way a
suppressed comparison can. Job 4 was always wrong: naming itself `textrill` and
emitting its own generator string already broke parity on purpose.

This is the same shape as a **bootstrap compiler**. `rustc` was written in
OCaml until it could compile itself; Go's `gc` was written in C; OpenBSD forked
from NetBSD and then stopped tracking it. The ancestor is used to *build* the
descendant, then retired from the build.

## Consequence

The logical consequence is that Perl need not **gate** the build. It stays a
useful **advisor** and **seed source** — its `tfiles/*.txt` are good inputs —
but the assertions that decide pass/fail can all be reference-free. The cost is
not code: it is that inference correctness loses its only independent discovery
device, so the truth corpus has to be authored and reviewed by hand. That is
the honest price of standing on its own.

Whether and when to act on this is a plan item, not this document's job. Any
cut-over — retiring `ref/`, the `YAML::Syck` stub, or the `perl` CI job —
belongs in `docs/PLAN.md`.
