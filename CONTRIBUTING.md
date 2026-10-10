# Contributing

Small and practical. The one commitment: a green `make verify` means the same
thing on every machine.

## Workflow

1. Change the code, add or adjust a test that names the behaviour, then
   recapture the affected goldens:

   ```sh
   make verify
   make accept-write    # if corpus output changed
   make examples-write  # if example output changed
   git diff             # the recapture diff is the review
   ```

   Commit the goldens with the change. `proptest` and `alloctest` gate the
   non-negotiables: no dropped text, well-formed output, determinism, bounded
   resources.

2. If you change the harness (runners, corpus tables, Makefile gates),
   demonstrate once that the changed check can fail before trusting it.

## House rules

- A new input shape earns a corpus case in `textrill/tests/corpus/tfiles/`,
  recorded in the goldens like any output change.
- Corpus documents must be CC0 or public domain. CC-BY / CC-BY-SA are rejected
  on sight and recorded as rejected in `examples/README.md`.
- Keep documentation small. Prefer the code, the tests and `--help`.
- This is a fork. The reference is a cross-check, not a specification.

## Running the gates

```sh
make verify      # fmt, clippy (-D warnings), engine + GUI tests, proptest,
                 #   alloctest, acceptance
make accept-write / make examples-write   # recapture
make diff        # optional: the Perl differential corpus, as a cross-check
```

`make verify` needs no Perl. `make diff` materialises the reference offline from
the tracked tarball (`make ref`; never clone or download it) and runs the
differential corpus and author goldens against it, non-gating.

## Legal

Code: GPL-3.0-or-later (`LICENSE`). Example corpus: CC0 / public domain. No CLA;
contributions are under the same terms as the project.
