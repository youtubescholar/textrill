# Corpus tests

`run.sh` converts every `tfiles/*.txt` of the upstream distribution with both
the Perl reference (`HTML::TextToHTML` 3.0) and this Rust port, then compares
the two outputs byte for byte.

```sh
cargo build
tests/corpus/run.sh          # all cases
tests/corpus/run.sh sample   # a single case
```

Environment overrides:

| variable    | default                                                 |
|-------------|---------------------------------------------------------|
| `REFDIR`    | `/home/vicpu/build/ref/txt2html-3.0`                    |
| `MINE`      | `/home/vicpu/build/txt2html-rs/target/debug/txt2html`   |
| `RUNDIR`    | `/tmp/opencode/corpus`                                  |
| `PERL5LIB`  | `/home/vicpu/build/ref/stubs:$REFDIR/lib`                |

Both halves of the default `PERL5LIB` live under `ref/` in the repo on purpose.
`ref/stubs/YAML/Syck.pm` is a stub for a module `TextToHTML.pm` `use`s at load
time but never calls; the real `YAML::Syck` is long superseded and not
installable on a current perl. Keeping these in `/tmp` was tried and a reboot
wiped them mid-run — see "Two ways this reports success wrongly" below.

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

Current status: **46/46 cases byte-identical**, and 29 of the 31 upstream
goldens reproduce byte for byte (the other two are the `NOGOLDEN` cases above).
`tests/paratest.rs` additionally mirrors `t/10para.t`, `t/25handles.t`,
`t/30sample.t`, `t/50xsample.t` and `t/70bugs.t` from the reference
distribution for the string-level API.

## Two ways this reports success wrongly

Both were live bugs in this harness, and both produce a green run while
comparing nothing, so they are worth stating rather than just fixing.

1. **Both sides fail identically.** If `PERL5LIB` is wrong, every perl
   invocation exits non-zero and writes nothing, so comparing two absent files
   would be vacuous. The runner clears its output directories first, fails any
   case where either converter exited non-zero, and additionally runs a smoke
   conversion before the loop: a broken environment now stops the run with the
   perl error on stderr instead of 40 misleading "port regression" failures.
2. **A stale port binary.** The runner warns when `$MINE` is older than the
   sources, because a stale binary passes cases the current code would fail.
   The default is `target/debug/txt2html`, so **`cargo build --release` on its
   own leaves the corpus testing a stale debug binary** — which will happily
   report 46/46 for code that does not even compile into it. Either `cargo
   build` as well, or point the run at the release binary explicitly:

   ```sh
   MINE=$PWD/target/release/txt2html tests/corpus/run.sh
   ```

## Fuzzer

`fuzz.py` is a seeded differential fuzzer over the same pair of converters. It
builds each case by taking a seed from `tfiles/`, mutating it, and choosing a
random subset of the real options, then compares the two outputs strictly.

```sh
tests/corpus/fuzz.py                          # 300 cases, default seed
tests/corpus/fuzz.py --cases 2000 --seed 7    # longer sweep
tests/corpus/fuzz.py --cases 2000 --keep      # save failing inputs
tests/corpus/fuzz.py --seed 7 --dump 41 >case.txt   # case 41's input, to hand
                                                    # to minimise.py
```

The default is small on purpose: each case is two converter invocations, and a
gate nobody runs is not a gate. Longer sweeps are opt-in.

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
before a line ending, for two documented reasons.

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
the Latin-1 fallback and come out byte-identical. Fixing the encoding story is
P7, and the fuzzer's exclusion is not meant to paper over it.

**Trailing whitespace** is the trigger for two open divergences where the port
drops a space the reference keeps: `'   e\n '` with `--indent_par_break`, and
the three-space-indent ALIGN table `'   . \n3  .'`.

Tabs are *kept*: with ASCII-only input, tab expansion agrees with the
reference, and the structural fixtures need tabs and CRs.

### Known divergences

`KNOWN_DIVERGENCES` lists divergences that are understood and not yet fixed.
An entry pins the *output shape* as well as the required options, so a second
and different bug landing on the same options is still reported; only an exact
repeat is stepped over, and repeats are counted and printed. If a sweep starts
printing "N known", something has regressed or a new bug matches a recorded
shape.

| options | reference | port |
|---|---|---|
| `--xhtml --make_anchors` | 3 newlines in `<body>` | 5 |

```sh
printf '\r\n\r\n\n' | txt2html --xhtml --make_anchors \
  --preserve_indent --no-use_mosaic_header --no-titlefirst
```

CR-only lines leave two stray blank lines in the body. It is very narrow:
`\r\n`, `\r`, `\n`, `\r\n\r\n`, `\n\n` and `a\r\n\r\n\r` all agree, so
this is paragraph-boundary accounting in the CR handling rather than a general
"CRs are mishandled" one. The entry is matched structurally — "the port's line
list is the reference's with two blank lines spliced in" — because the first
differing line is reported to depend entirely on what follows the CRs: the same
defect has shown up as a missing `</body>`, as a missing `<p>-</p>` and as a
missing `<h1>`. Nothing is ever actually missing, so a structural match cannot
hide a content-loss bug.

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
