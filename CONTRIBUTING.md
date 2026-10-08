# Contributing

Small, careful, and honest. The guarantee this project stands on is that a
green run of `make verify` means the same thing on every machine, including
yours. Contributions that respect that are welcome; everything else gets
reviewed against it.

## The house rules

1. **Counts come from the tool, never from grep.** Numbers in a commit message
   or a document must be reproducible by running the target that produced them
   (`make examples`, `make measure`, `make proof`), or the change does not
   contain them.
2. **A harness change is proven by being broken.** Every gate here has shipped
   a false pass at least once (A1: a stale binary; P1: a fuzz pipeline that
   could not fail). So when you change the *harness* — the runners, the corpus
   tables, the fuzzer, the Makefile gates — demonstrate on purpose that the
   changed check can fail before trusting it. The two built-in sabotage
   commands are in `textrill/tests/corpus/README.md` (the closing rule of "Six
   ways this reported success wrongly"). Feature and behaviour work needs no
   such ritual: the differential corpus is how *its* difference is pinned.
3. **The test corpus comes first.** Change behaviour, then extend
   `textrill/tests/corpus/` so the difference is pinned by the differential
   harness and the corpus README records why.
4. **A document with no status is the failure mode.** If your change affects a
   document, update its status in `DOCS.md` in the same change.
5. **No affiliation with upstream is implied, and none is claimed.** This is a
   fork. The reference is an oracle, not a specification.

## Running the gates

```sh
make build      # the CLI, static musl
make verify     # fmt-check, clippy (-D warnings), unit/property/allocation
                #   tests, differential corpus, fixed-seed fuzz
make test-gui-rs
make proof      # the pandoc-facing claims (docs/OFFERING.md §3)
make measure    # heading-rule measurement pins (docs/PLAN.md Phase 5)
```

Full `make verify` runs the fuzzer at 2000 cases and takes a while; CI runs it
on push and a 500-case fuzz on pull requests. The oracle is materialised
offline from the tracked tarball by `make ref` — never clone or download it.

## Adding a document to `examples/`

The corpus is the public bar for what the tool does to real text, and its
licensing is a deliberate boundary:

- **CC0 or clearly public domain only.** CC-BY and CC-BY-SA candidates are
  rejected on sight and recorded as rejected in `examples/README.md`. This is
  what keeps the corpus clean alongside the GPL-3.0-or-later code.
- Record in the register: source, edition, licence basis, and the pinned
  counts. Then run `make examples` and pin the new document in
  `reporttest.rs` so the counts cannot silently drift.
- Wikisource-captured prose carries a thin CC-BY-SA transcription layer over
  the PD underlying text; if you add such a document, keep the caveat note in
  the register rather than pretending the capture is licence-free.

## Where to start

Get oriented with `DOCS.md` ("Reading order, by task"), then pick a
measurement-backed item from `docs/PLAN.md` — the roadmap is written as pick-up
able work packages. Two are always open:

- **Corpus growth.** A real, licence-clean document added and pinned is a
  complete, reviewable contribution on its own.
- **Templates and packaging.** Docs and packaging are undecided in small, named
  ways (`docs/PACKAGING.md`, `docs/PLAN.md` Phase 4); those decisions are
  welcoming first contributions.

Prefer a conversation before a wall of code: issues that propose the change and
name the measurement it will leave behind merge faster than PRs that arrive
unannounced with 2 000 lines.

## Legal

- Code: GPL-3.0-or-later (`LICENSE`).
- Example corpus: CC0 / public domain, individually recorded.
- No CLA. Contributions are made under the same terms as the project.