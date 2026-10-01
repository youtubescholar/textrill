# txt2html — adversarial findings

> **Scope note.** This pass judged the port against the Perl module as
> the specification. That remains right for ASCII input, but on genuine UTF-8
> input Perl is the defect — see "Compatibility policy" in
> `REMEDIATION-PLAN.md`, which also records that this evidence pass reached
> **2 of 65** upstream `tfiles` and no non-ASCII case at all.

Status: 2026-09-29. Result of an attack pass over `txt2html-rs` and
`txt2html-gui`. Companion to `REMEDIATION-PLAN.md` and `TOOL-SURVEY.md`.

Every claim below was reproduced against the built binary or the real Qt event
loop. Where a suspected issue turned out **not** to be a problem, that is
recorded too — section 6 is as important as section 2, because it closes off
work that would otherwise be wasted.

Severity is about the *product as shipped*: a CLI in a pipeline and a desktop
app that opens files a user downloaded. No findings require a network attacker.

> **Scope, 2026-09-30.** This pass attacked the engine's *failure modes* — panics,
> leaks, hangs, panics-not-caught — and did not audit the *transformation
> logic* against Perl. It therefore found none of E1–E3, which are correctness
> divergences found later by the promoted differential fuzzer, including one
> silent content loss reachable with default options. "Not listed here" means
> "not examined", not "correct". See `REMEDIATION-PLAN.md` §0.1.

> **Status, 2026-10-01. Every High and Medium finding here is fixed.** This
> document is kept as the point-in-time record of the pass and its severities
> are *as assessed then* — read §7's recommended order as history, not as a
> to-do list. The remediation plan owns current state; the mapping is:
>
> | Finding | Plan item | State |
> |---|---|---|
> | S1 leak | A2 | **done** — and the severity was wrong. Retention is bounded near ~0.3 MB per process, not 142 MB and not unbounded; the plan's original 1.3 GB figure was a direct-call benchmark artefact. |
> | S2 >500 KB panic | A1 | **done** |
> | S3 `tab_width=0` panic | A3 | **done** |
> | S4 GUI swallows panics | A4 | **done** |
> | S5 large `tab_width` | A3 | **done** — four options deliberately left unbounded, measured |
> | S6 non-UTF-8 corruption | A5 | **done** |
> | S7 no cancellation | A6 | **done** |
> | S8 `--title` unescaped | A8 | **open** — sole owner of the 30 known-open `proptest` checks |
> | S9 missing input exits 0 | A9 | **open** |
> | S10 `mkdir` on save | A7 | **done** |
> | S11 unbounded `re_cache` | A10 | **open** — still code-review only, not demonstrated |

## 1. Summary

| # | Finding | Severity | Reachable from |
|---|---|---|---|
| S1 | ~1 MB text file leaks 142 MB, unbounded | **High** | requires `--make_tables` (off by default) |
| S2 | Paragraph over ~500 KB panics the engine | **High** | any input, default options |
| S3 | `tab_width=0` panics (modulo by zero) | **High** | CLI, **and the GUI spinbox** |
| S4 | GUI silently swallows every Rust panic | **High** | GUI, amplifies S2/S3 |
| S5 | `tab_width` ≥ 10⁶ hangs or aborts the process | Medium | CLI only (GUI caps at 999) |
| S6 | GUI corrupts non-UTF-8 files on save | Medium | GUI |
| S7 | GUI never cancels superseded conversions | Low–Medium | GUI, documents > ~250 KB |
| S8 | `--title` / `--style_url` not escaped (XSS) | Low | CLI/GUI, inherited from Perl |
| S9 | Missing input file exits 0 with empty output | Low | CLI, inherited from Perl |
| S10 | `write_text_file` creates directories on a typo | Low | GUI |
| S11 | Unbounded `re_cache` (code review only) | Low | not demonstrated |

Three of these (S1, S2, S3) are new defects, not previously documented. S2 and
S4 together are the worst outcome in the set: **open an ordinary large text file
in the GUI and the preview dies silently with no error shown.**

## 2. Crashes and denial of service

### S1. Document-controlled memory leak, ~133× amplification — HIGH

`links::ascii_re_cached` (`links.rs:153-172`) caches compiled regexes in a
thread-local map and hands out `&'static Regex` by **leaking the box**:

```rust
let re: &'static Regex = Box::leak(Box::new(
    Regex::new(&translate_pattern(pat)).expect("valid pattern"),
));
```

The map is capped at 128 entries and cleared when full (`links.rs:164-165`), but
`Box::leak` is never undone — clearing the map drops the references, not the
memory. The 128 cap therefore bounds the *map*, not the *leak*.

The pattern is built from the **document**, at `convert.rs:2422`:

```rust
let dc = links::ascii_re_cached(&format!("[{}]", &delim));
```

where `delim` is the first non-alphanumeric character of the candidate table's
first row (`convert.rs:2406`). So a document containing delimiter tables that
each use a *different* delimiter character leaks one permanently-unreachable
compiled regex per table.

Measured, CLI, `--make_tables`, no other options. Note this flag is **not** on
by default (`options.rs:112` sets `make_tables: false`), so the leak is not
reachable with stock options; it is reachable in any run that has opted into
table recognition:

| distinct delimiters | input size | peak RSS |
|---|---|---|
| 100 | 2 KB | 6 916 KB |
| 800 | 21 KB | 10 436 KB |
| 3 000 | 92 KB | 20 180 KB |
| 30 000 | 1 065 KB | **142 440 KB** |

Linear in the delimiter count, not saturating at 128. Against a ~6.9 MB trivial-
file baseline that is ~4.5 KB leaked per distinct delimiter, so a 10 MB file
approaches 1.3 GB. The 30 000 case also takes 13.6 s, so this is a time cost as
well as a memory cost.

In the GUI this is worse than a spike: the memory is never reclaimed for the
lifetime of the process, so opening such a file repeatedly in one session
accumulates.

Fix: stop leaking. Store owned `Regex` values in the map and return a reference
into the map, or key the cache on a bounded set of known patterns. The simplest
correct version is to not cache dynamic patterns at all — the set of delimiters
in practice is tiny, and recompiling one small pattern per table is cheaper than
leaking it.

### S2. Any paragraph over ~500 KB panics the engine — HIGH

`fancy-regex` 0.14's `Regex::replace`/`replace_all` are thin wrappers that
unpack a fallible call — `fancy-regex-0.14.0/src/lib.rs:1073`:

```rust
pub fn replacen<'t, R: Replacer>(&self, text: &'t str, limit: usize, rep: R) -> Cow<'t, str> {
    self.try_replacen(text, limit, rep).unwrap()
}
```

Its own doc comment says: *"Will panic if any errors are encountered. Use
`try_replacen`, which this function unwraps, if you want to handle errors."*

The port calls the panicking variants on whole-paragraph data in several
places — `convert.rs:1764`, `convert.rs:1766`, `convert.rs:1470`, and
`links.rs:230,236,242`. Only one of them is actually reachable at this
threshold, which matters because it makes the fix a two-line change rather than
an audit.

**Root cause, isolated: `convert.rs:1764` only.** The paragraph length is not by
itself the problem. `expand_ascii_escapes` (`links.rs:60-69`) rewrites every `$`
outside a character class into the *lookahead* `(?=\n?$)`:

```rust
if c == '$' {
    if in_class { out.push('$'); } else { out.push_str(r"(?=\n?$)"); }
    continue;
}
```

`fancy-regex` can only use its fast automaton when no lookaround is present, so
this translation alone moves the pattern off the fast path and onto the
backtracking engine, where a long haystack exhausts the default 1 000 000-step
budget. `convert.rs:1766` is unaffected because `^` is not rewritten.

Verified directly, same 1 049 999-byte / 25 000-line paragraph, same two
patterns, `fancy-regex` 0.14.0:

| pattern | result |
|---|---|
| `(?s)[ \t]*\x0D$` (raw, as written in the source) | ok |
| `(?s)[ \t]*\x0D(?=\n?$)` (after translation) | **panic** |
| `(?s)^[ \t]*\x0D` (line 1766) | ok |

So the panicking call is exactly `convert.rs:1764`, and the other
panicking-variant sites are latent rather than currently reachable. The fix and
its 660 000-case equivalence check are in `REMEDIATION-PLAN.md` A1.

**Aftermath, and a correction to the diagnosis above.** The two CR patterns were
only half of it. With them replaced by string surgery the same paragraph no
longer panicked and no longer *finished* either: it hung, indefinitely, at the
same ~500 KB scale. The cause is the same mechanism in `do_delim` rather than
`process_para` — `expand_ascii_escapes` turns every `\B` into a lookaround
alternation, and Perl's delimiter patterns are full of `\B` and `(?<!delim)`,
so all of them run on fancy-regex's backtracking VM.

The original finding above blamed the *bold* branch, on the evidence that
disabling bold made the case finish. That was wrong: `--italic_delimiter=`
finishing just means the next delimiter (`*`, the default) was the one hanging
all along. Perl renders the same input in about 925 ms. Full diagnosis, the
table of which patterns blow up, and the fix are in `REMEDIATION-PLAN.md` A1b.

The general lesson, since it is the second time in this project: a regex whose
only unusual feature is a lookaround added by `expand_ascii_escapes` is not
merely slow, it is a cliff, and the cliff is at paragraph length. Every
`\B`, `(?<!...)` and `(?=...)` reaching a pattern compiled through
`ascii_re_cached` is a latent instance of the same defect, and the fix is to
stop putting the condition in the regex where possible — not to raise
`backtrack_limit`, which only moves the crash to a stall.

**Threshold, deterministic, 3/3 runs each.** The trigger is *paragraph* length,
not file size and not line length:

| one paragraph of | result |
|---|---|
| 450 999 bytes | ok |
| 532 999 bytes | **panic** |

Proof that it is structural rather than volumetric — the same order of magnitude
of text, arranged differently:

| input | size | result |
|---|---|---|
| 2.05 MB, no blank lines (one paragraph) | 2 049 999 B | **panic** |
| 1.05 MB, split into 1 250 paragraphs | 1 049 998 B | ok |

Perl converts the 2 MB single paragraph in ~1 s with no error, so this is a port
regression, not inherited behaviour.

Also triggers on: a single line over ~500 KB, and about 1 000 levels of list
indentation. Both are the same root cause. `--link_only` on a 2 MB line hangs
rather than panicking, same family.

This is reachable from **plain input with default options** — a chapter with no
blank lines, a minified file, a long log, a wide CSV row. No option needs to be
set and nothing adversarial is required.

Fix: use `try_replacen` (and `try_replace`) at the four sites and treat
`BacktrackLimitExceeded` as "this pattern gave up on this text" — which for
these particular patterns means leaving the text unchanged, since they only chop
stray carriage returns and HTML entities. Alternatively raise the backtrack limit
via `RegexBuilder::backtrack_limit` and keep the error handling.

### S3. `tab_width=0` panics with modulo by zero — HIGH

`convert.rs:1791-1797`:

```rust
while let Some(tab) = line.find('\t') {
    let tw = self.opts.tab_width;
    let spaces = " ".repeat(tw - ((tab) % tw));   // <-- divides by tw
    line.replace_range(tab..tab + 1, &spaces);
}
```

`set_int` (`cli.rs:375`, cast at `cli.rs:386`) clamps with `v.max(0)`, so a **negative** value is
safely floored at 0 — and 0 then divides here. `--tab_width 0` on any input
containing a tab:

```
thread 'main' panicked at src/convert.rs:1794:34:
attempt to calculate the remainder with a divisor of zero
```

**Reachable from the GUI.** `OptionSpec.minimum` (`optionspanel.py:107-112`)
returns 0 for every option except the two `preformat_*_lines` ones, so
`tab_width = 0` is a legal spin-box value the user can select with the mouse.

Fix: `tw.max(1)`, and validate in `Options::deal_with_options` so the value is
rejected up front rather than at conversion time.

### S5. Large `tab_width` hangs or aborts the process — MEDIUM

Same code. `set_int` applies **no upper bound** — `v.max(0) as usize` passes
`i64::MAX` straight through — and the result is fed to `" ".repeat(tw)`:

| input | `tab_width` | result |
|---|---|---|
| one tab | 100 000 | ok |
| one tab | 1 000 000 | hangs, killed at 20 s |
| one tab | 9 223 372 036 854 775 807 | **rc=134, allocation abort** |
| nested list | 9 223 372 036 854 775 807 | **rc=134, allocation abort** |

Perl handles `tab_width => 1000000` in well under a second, so this is also a
regression. `rc=134` is `SIGABRT` — an abort, not a catchable error, so unlike
S2/S3 it cannot be handled higher up the stack.

The expansion is also quadratic in the wrong place: `line.find('\t')` rescans
the whole line from the start after every replacement, so a line with *k* tabs
costs O(k²·tw). Measured at `tab_width=10000`: 10 tabbed lines 0.11 s, 100
tabbed lines >30 s.

CLI-only in practice — the GUI spin box caps at 999 (`optionspanel.py:115-120`),
which is below the ~10⁶ threshold. Fix: clamp `tab_width` (and `indent_width`,
`par_indent`) to a sane maximum, reject out-of-range values with a message, and
rewrite the expansion as a single left-to-right pass.

## 3. GUI

### S4. The GUI cannot catch a Rust panic — HIGH

`worker.py:48-53`:

```python
try:
    html = txt2html.convert(self.text, self.options)
    error = ""
except Exception:  # keep the GUI alive whatever the input is
    html = ""
    error = traceback.format_exc(limit=3)
```

pyo3's `PanicException` inherits from `BaseException`, **not** `Exception`
(verified: `isinstance(e, Exception)` is `False`). So the handler never runs.
Observed through the real `QThreadPool`:

```
thread '<unnamed>' panicked at src/convert.rs:1794:34:
attempt to calculate the remainder with a divisor of zero
Error calling Python override of QRunnable::run(): Traceback (most recent call last):
  File ".../worker.py", line 49, in run
    html = txt2html.convert(self.text, self.options)
pyo3_runtime.PanicException: attempt to calculate the remainder with a divisor of zero
```

Consequences:
1. The exception escapes `QRunnable::run()`. PySide6 6.11.2 reported it and kept
   running (exit 0), so this is not an immediate crash on this version — but it
   is a Qt-level "error calling Python override", not a handled condition, and
   that path is not guaranteed to be survivable across PySide6 versions.
2. `self.sink.finished.emit(...)` at `worker.py:55` is **never reached**, so the
   main window never gets a completion signal. Measured: of two queued
   conversions, the healthy one delivered (`gen=2 html_len=285`) and the
   panicking one delivered **nothing**.
3. The user therefore sees a **permanently stale preview and no error message**,
   because the only path that surfaces an error is the handler that never runs.
4. A raw Rust `thread '<unnamed>' panicked` line is written to stderr, which in
   a GUI launch is invisible.

This corrects `REMEDIATION-PLAN.md` P4, which states "In the GUI it does not
crash (`worker.py:51` catches it)". It does not catch it. The same P4 applies to
any bad regexp typed into the list editor at `optionspanel.py:135`, so P4's GUI
regression test is needed more than the plan realises.

**S2 + S4 combined is the headline finding:** opening a ~600 KB single-paragraph
text file with default options gives

```
delivered: NONE - preview stuck, no error shown
```

Fix: catch `BaseException` (or explicitly `pyo3_runtime.PanicException`), and add
a `finally` so the signal is always emitted. Better still, fix S2 and S3 at the
source so the panic cannot happen; the GUI handler is defence in depth.

### S6. Saving a non-UTF-8 file corrupts it — MEDIUM

`files.py:38` defaults to `encoding="utf-8"`, and `mainwindow.py:376` ("Save
text…") and `:421` ("Save HTML") both call it without passing the encoding the
file was read with. `read_text_file` reads UTF-8-or-Latin-1, so a CP1252 file
loads fine and is then written back transcoded:

```
original bytes : b'Caf\xe9 \x97 na\xefve \x93quotes\x94 5 < 6\n'
after GUI save : b'Caf\xc3\xa9 \xc2\x97 na\xc3\xafve \xc2\x93quotes\xc2\x94 5 < 6\n'
```

The `0x80`–`0x9F` bytes became C1 control characters, so the file is both
re-encoded and semantically wrong. This is the concrete instance of plan item
P7.5. Fix: have `read_text_file` return `(text, encoding)` and pass the encoding
through to `write_text_file`.

### S7. Superseded conversions are never cancelled — LOW–MEDIUM

`Converter.cancel_pending()` (`worker.py:84-86`) exists and is **dead code** —
`grep -rn cancel_pending` finds only the definition. `convert_now`
(`mainwindow.py:277-281`) calls `self.converter.convert(...)` directly, which
appends to the `QThreadPool` queue with no bound. `schedule_convert` coalesces
only within its 300 ms window.

Once one conversion takes longer than ~600 ms, the 2-thread pool (2 threads ×
1/1.25 s = 1.60 jobs/s) cannot keep up with the 300 ms debounce (3.33 jobs/s),
and the backlog grows for as long as the user keeps typing. Every queued `_Job`
holds its own copy of the document (`worker.py:37`).

Measured with a 793 KB document (one conversion = 1.25 s), 10 jobs queued over
3.0 s of typing:

```
after typing stopped, the remaining queue took 3.4s to drain (10/10 done)
total 6.4s of wall clock for 1.25s of useful work; peak RSS 61 MB
```

So roughly 5× wasted conversion work, ~20 MB of queued document copies, and a
status bar reading "converting…" for seconds after the user stopped typing. The
generation check at `mainwindow.py:285` discards stale *results* but never skips
stale *work*. Fix: call `cancel_pending()` in `convert_now` before queueing, and
set a pool expiry timeout.

### S10. Save silently creates directories — LOW

`files.py:41-42` does `target.parent.mkdir(parents=True, exist_ok=True)`. A typo
in the save dialog creates the whole tree. Verified: saving to
`newtree/a/b/c/out.html` created three directories. Surprising rather than
dangerous, but a save dialog should not be inventing filesystem layout — a
failed save should be an error, not a new tree.

## 4. Inherited from Perl — real, but not port defects

These reproduce byte-identically in `HTML::TextToHTML` 3.0. Fixing them is a
declared deviation, so they need a decision, not just a patch.

### S8. `--title` and `--style_url` are not escaped — LOW

```
$ txt2html --title '</title><script>alert(3)</script>'
<title></title><script>alert(3)</script></title>

$ txt2html --style_url 'x.css" onload="alert(4)'
<link rel="stylesheet" type="text/css" href="x.css" onload="alert(4)"/>
```

Both are working XSS, and Perl produces the same bytes. Severity is low because
these are *option values*, which the invoking user chose — not document content.
The important negative result is that the document path is safe:
`--titlefirst` takes its title from the file and **is** escaped, so a malicious
`.txt` file cannot inject through it (verified against Perl too).

Worth escaping anyway, since the tool's whole output is normally served as a
page, and a `~/.txt2htmlrc` (once P11 lands) is a plausible injection vector for
a shared or dotfile-managed environment.

### S9. Missing input file exits 0 with empty output — LOW

```
$ txt2html --infile /nonexistent/nope.txt --outfile out.html
Could not open /nonexistent/nope.txt
$ echo $?
0
$ wc -c out.html
0 out.html
```

Perl behaves identically. In a Makefile, CI job, or shell pipeline this reads as
success and silently produces an empty document. `--outfile` to an unwritable
path *is* handled correctly (exit 1 with a message), so the inconsistency is
within the tool itself. A non-zero exit for an unreadable input is a small,
well-justified deviation.

## 5. Code-review observations, not demonstrated

### S11. `self.re_cache` is unbounded

`convert.rs:160-168` caches into a per-converter `HashMap` with **no size cap**,
unlike `ascii_re_cached`'s 128. The patterns reaching it from input-derived data
(`convert.rs:1646,1652,1658` interpolate a delimiter; `:451,453,459` interpolate
bullet/term strings) are drawn from the user's `bullets` options, which are
finite, so I could not construct unbounded growth. Worth a cap for symmetry and
as insurance, but I have not shown a problem.

### `re_cache` and `PanicException` both deserve a note on error style

`self.re` and `re_i` (`convert.rs:160-175`) `panic!` on an uncompilable pattern.
That is correct today, because the only caller-supplied patterns arrive through
`custom_heading_regexp`, which plan P4 already proposes to validate up front.
Once S2/S3 are fixed the same discipline should be applied to the `table_re`
`.unwrap()` at `convert.rs:2323`.

## 6. Verified *not* vulnerable

Recorded so this work is not repeated.

**No ReDoS.** `fancy_regex`'s default 1 000 000-step backtrack limit defeats
catastrophic backtracking. 14 classic evil patterns
(`^(a+)+$`, `^(a|a)*$`, `^(.*a){20}$`, `^([a-z]+)+$`, `(a+)+b`, `a*a*a*a*a*b`,
`a+a+`, and the same set through the link-dictionary path) against inputs of
200 / 2 000 / 20 000 `a`s: **every run under 70 ms, none exceeded 3 s.** The
limit that causes S2 is also what makes ReDoS a non-issue. The one exception is
S2 itself, which is the limit being *exceeded* and then unwrapped.

**Document text cannot inject HTML.** `<script>alert(1)</script>`,
`<img src=x onerror=alert(2)>`, bare `&` and `"` in body text are all correctly
escaped under default options. `titlefirst` is escaped (above).

**Nasty bytes are all safe**, exit 0 and no panic: NUL byte, invalid UTF-8
(`FF FE FA`), overlong encoding (`C0 80`), surrogate (`ED A0 80`), 4-byte emoji,
U+202E right-to-left override, CR-only line endings, and a file with no trailing
newline. `read_any_file` (`convert.rs:18-24`) maps invalid bytes to U+0000–U+00FF
lossily but safely.

**No stack overflow.** 100 000 levels of list indentation is fine. The failure at
~1 000 levels is S2's backtrack limit, not recursion.

**The thread-local regex cache is correct.** `ascii_re_cached` uses
`thread_local!` + `RefCell` with a bounded map, so the GUI's two concurrent worker
threads do not race. The *memory* problem is S1, not a data race.

**`body_deco` malformed output is faithful.** `--body_deco FOO` yields
`<bodyFOO>`; Perl 3.0 produces the same bytes. Not a port defect.

**Option abbreviation absence is deliberate and safe** (see `TOOL-SURVEY.md` §3).

## 7. Recommended order

1. **S2** — highest reachability: default options, ordinary input, kills the
   preview. Fix `convert.rs:1764` with the two string-surgery helpers verified in
   `REMEDIATION-PLAN.md` A1 (do *not* switch call sites to `try_*`: only this one
   is reachable, and `try_*` merely converts the panic into a slow error on
   exactly the large inputs that are already expensive). Add a corpus fixture of
   a ~600 KB paragraph with CRs and CRLFs mixed in.
2. **S1** — remove the `Box::leak`; add a fixture with many distinct delimiters
   run with `--make_tables`, plus an RSS assertion, since nothing else will catch
   a regression here.
3. **S3** and **S5** together — clamp numeric options at parse time, which fixes
   the modulo panic, the allocation abort, and the quadratic expansion in one
   place, and gives the GUI spin boxes correct bounds for free.
4. **S4** — catch `BaseException` and emit the signal from a `finally`. Cheap, and
   it converts every future engine bug from "silent dead preview" into a visible
   error.
5. **S6** then **S7** — GUI correctness and responsiveness.
6. **S8**, **S9**, **S10** — decide whether each is worth a declared deviation
   from Perl.

Regression status **as of this pass (2026-09-29)**: `cargo test --release` 21/21
(5 link + 7 options + 9 convert), GUI `unittest` 28/28, upstream Perl suite
102/102 assertions across the 7 functional `.t` files (`t/20tfiles.t` alone
accounts for 50 of them). No source file was modified.

For current numbers see `REMEDIATION-PLAN.md` §0, which had reached 45/45 Rust
tests and 45/45 GUI tests by 2026-10-01.

The remediation plan for all of the above is the A1–A10 addendum at the end of
`REMEDIATION-PLAN.md`.
