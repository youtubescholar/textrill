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
make deny        # separate supply-chain gate: cargo-deny + cargo-audit
make sbom        # optional: CycloneDX SBOMs under dist/sbom
make fuzz        # optional: print the cargo-fuzz commands (needs nightly)
make bench       # optional: criterion benchmarks (measurements, not gates)
make repro       # optional: two clean CLI builds must be byte-identical
make repro-gui   # optional: the same check for the GUI
make accept-write / make examples-write   # recapture
make diff        # optional: the Perl differential corpus, as a cross-check
make metadata    # optional: desktop-file-validate + AppStream validation
make packages    # optional: deb/rpm (needs cargo-deb, cargo-generate-rpm)
```

`make deny` needs `cargo-deny`, `cargo-audit` and the RustSec database; it is
kept out of `verify` so the product gate does not fail for a missing tool.

`make verify` needs no Perl and passes with the `parity/` directory absent:
everything reference-related (the Perl tree, the stubs, the tracked tarballs,
the differential runner) lives there, outside every gate (roadmap §8.5).
`make diff` materialises the reference offline from the tracked tarball
(`make ref`; never clone or download it) and runs the differential corpus and
author goldens against it, non-gating.

## Legal

Code: GPL-3.0-or-later (`LICENSE`). Example corpus: CC0 / public domain. No CLA;
contributions are under the same terms as the project.


## Commit Message Style

Keep commit messages short and scannable:

- **Subject line**: 50 characters or less, imperative mood (e.g., "fix: resolve crash on startup")
- **Body**: Only add details if necessary. Put longer explanations in PR descriptions, not commit bodies
- **Avoid phase dumps**: Don't copy entire roadmaps/specs into commit messages. Link to issues/PRs or reference `CHANGELOG.md` instead
