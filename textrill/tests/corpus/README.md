# Corpus tests

The behavioural harness. Two runners share the recipes in `cases.sh` and the
fixtures tracked at `tfiles/` and `workflows/`:

- **`accept.sh` — the gate.** Converts every case with textrill alone and
  compares the output against the recorded golden (`tests/golden/corpus/*.html`:
  `SELF`), then checks the upstream author goldens (`good_*.html` in `tfiles/`:
  `AUTHOR`) on top, honouring `NOGOLDEN[]`. `make accept` runs it; no Perl
  needed. A case that cannot run, or a golden that differs, is a failure —
  never a skip.
- **`run.sh` — the differential cross-check.** Convert the same cases with the
  Perl reference and compare byte for byte. Advisory since the golden set
  became the gate: `make diff` runs it on demand.

The fixture inputs come from the txt2html distribution and the encoding study;
`tfiles/PROVENANCE.md` records where each one came from and how to refresh it.

## The gate

```sh
make accept                        # all cases
tests/corpus/accept.sh definitions # one case
tests/corpus/accept.sh --write     # (re)capture every SELF golden
tests/corpus/accept.sh --write definitions
```

A full run prints `SELF: 61/61` and `AUTHOR: 33/33`. The goldens are a record
of approved behaviour, not a wall: **changing output means running
`--write` (or `make accept-write`), and reviewing the git diff of what you
recaptured. The diff is the review.** Commit the goldens with the change that
recaptured them. The properties that can never regress no matter what the
bytes say — no dropped text, well-formed output, determinism, bounded
resources — are gated separately by `make proptest` and `make alloctest`.

Author goldens (`AUTHOR`) are upstream's own expected files. Where the port
deliberately differs from one, the case is declared `NOGOLDEN[stem]` in
`cases.sh` *with a reason* — encoding cases whose oracle is
`tests/encodingtest.rs`, the URL-scheme policy whose oracle is
`tests/urlschemetest.rs`, the non-1 ordered list whose oracle is the self-golden
for `list-start`, and two upstream-golden quirks (custom headers and a trailing
newline). A `NOGOLDEN` entry without a stated reason, or naming a case with no
golden, is a lie the runner checks for and refuses.

## What the harness normalises

Exactly one line shape: a `<meta name="generator" …>` line, canonicalised to a
sentinel on both sides before comparison. textrill names itself, the reference
names itself, so the one line can never agree; the exact value textrill emits
is pinned by `tests/provenance.rs`. Every run prints `NORMALISED: <n>` so a
normalisation that quietly stopped applying would show. `normalize.py` is the
single implementation; `run.sh` and `examples.py` call that file rather than
growing their own copy.

There is no other normalisation — changing `caps_tag` to `B` instead of
`STRONG` is still 6 differential mismatches.

## The differential cross-check (`make diff`)

```sh
make ref          # extract the reference from the tracked tarball (offline)
make diff         # the differential corpus and author goldens
tests/corpus/run.sh          # all cases
tests/corpus/run.sh sample   # one case
```

Both halves of the doctype are pinned so no case reads a default: the
reference driver passes `xhtml => 1, @ctor` at construction and the port gets
`--xhtml` before its own flags. `cases.sh` entries: `CTOR[]`/`EXTRA[]` for the
Perl side, `CLI[]` for the Rust side, `INPUT[]` for a multi-file input,
`NOGOLDEN[]` as above. `lib.sh` runs `alignment_check` (the two table sides
must name the same cases, and every `NOGOLDEN` entry is honest about which
oracle it stands in for) and `duplicate_key_check` (a stem assigned twice in
the source text) before the loop — a case that is not in both tables does not
run, and a case that does not run cannot fail.

A case the port deliberately diverges on starts its `NOGOLDEN` reason with
`differential must fail:`; `run.sh` then requires the mismatch rather than
tolerating it, and fails if the port ever starts matching the reference again.
That covers `list-advanced`, `multi-file`, `list-start` and `list-sibling` for
the list-shape divergences (4.1/4.2), alongside the XSS, URL-scheme and
encoding cases.

## Proving the harness can fail

The gate has shipped a false pass before — six different ways, all of them an
exit status being green while something went unchecked. If you **change the
harness**, break it on purpose and confirm the changed check exits non-zero
before trusting it:

```sh
MINE=/tmp/stub-that-writes-garbage tests/corpus/accept.sh   # must fail
printf "\nCLI[typo]='--xhtml'\n" >> tests/corpus/cases.sh    # tables disagree;
git checkout -- tests/corpus/cases.sh                        # ALIGN: must fail
```

Behaviour changes need no such ritual: the recapture diff is the review.

## What this harness can and cannot judge

- `accept.sh` and `run.sh` are strict byte-identity against their respective
  oracles. That is the right strictness for the ASCII inputs and the
  documented output format.
- The reference is broken on genuine UTF-8 input (it decodes Latin-1), so
  non-ASCII cases belong in the golden/encoding set, where the port agrees
  with the author's golden, not in a byte-identity differential case.

The differential fuzzer that used to run here has been retired. It mutated the
`tfiles/` corpus at random and required byte-identity with the reference, which
the deliberate list-shape divergences (4.1/4.2) make impossible by
construction; once it reported hundreds of expected mismatches per seed it
obscured real ones. The properties it was meant to protect are covered by
`proptest` and `alloctest`, and each divergence is pinned by a named case
above.