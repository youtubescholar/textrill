# textrill

Convert plain text to HTML.

`textrill` converts plain text to HTML, inferring structure from the
conventions text authors already use: indentation for lists, blank lines for
paragraphs, underlines and setext rules for headings, trailing punctuation for
link labels, and so on. No markup language is required or assumed.

It began as a Rust port of
[`HTML::TextToHTML`](https://metacpan.org/pod/HTML::TextToHTML) 3.0 and its
`txt2html` script, originally written by Seth Golub, and it has been restored
to byte-parity with it across the upstream test corpus — the reference-free
acceptance suite is now the gate, with the Perl reference demoted to an
optional cross-check (`make diff`).

**This is a fork.** It is not endorsed by, and carries no affiliation with, the
upstream `txt2html` project or its authors.

## Status

Early. `0.1.0`. The conversion engine is pinned by reference-free acceptance —
a frozen output for every one of the 61 corpus cases, the 33 upstream
`good_*.html` goldens, and all 8 real-document examples — with the Perl
differential still available as `make diff`, a non-gating cross-check. The CLI
ships as a single static musl binary; encoding detection, sectioning/TOC/chunk,
citations and glossary, templates, reporting, and a native GUI are implemented
and tested. What remains (Flatpak, distro packaging, the planned harness
phases) is tracked in `docs/PLAN.md`.

## Build and verify

Prerequisites: a Rust toolchain (`rustup default stable` with `rustfmt` and
`clippy`). That is the only mandatory tooling. The Perl reference used by the
optional `make diff` cross-check is built offline from the tracked tarball and
needs no CPAN, but `make verify` does not touch it.

Try it:

```sh
make build            # the CLI (single static musl binary)
printf 'Hello *world*.\n' | ./textrill/target/release/textrill
```

The gates you need as a contributor:

```sh
make build      # the CLI
make verify     # fmt, clippy, unit/property/allocation tests, plus reference-free
                #   acceptance: frozen corpus+examples outputs and upstream goldens
make diff       # optional: the Perl differential + fuzzer, as a cross-check
make proof      # the pandoc-facing claims (docs/OFFERING.md)
make measure    # the heading-rule measurements (docs/PLAN.md Phase 5)
```

`make verify` is reproducible — the same command means the same thing on a
GitHub runner and on your machine, which is the guarantee the project stands on.
CI runs the same set on every push, so sending a PR does not require running it
locally first. `make diff` runs the 8 fuzz seeds at 2 000 cases and takes
roughly a quarter of an hour; it is deliberate that nothing schedules it. See
`textrill/tests/corpus/README.md`, including "Six ways this reported success
wrongly", before changing the harness.

## Repo layout

| Path | What it is |
|---|---|
| `textrill/` | The engine and CLI. The user-facing contract is `textrill/README.md`. |
| `textrill-gui-rs/` | The native GUI (`egui`/`eframe`). A frozen behavioural contract lives in `legacy-archive/SURFACE.md`. |
| `docs/` | The authoritative plan: `PLAN.md`, `CAPABILITIES.md`, `LANDSCAPE.md`, `OFFERING.md`, `PACKAGING.md` — plus the GUI findings and template research. |
| `DOCS.md` | The document register: exactly one status per document, decided by a test. Start here for orientation. |
| `examples/` | Real documents the tool is exercised and measured against, with a provenance register. |
| `research/` | Source archives for the offline studies: pandoc 3.12 (full + stripped) and the txt2html reference tar/zip. |
| `stubs/` | The canonical Perl stub `make ref` installs into the derived `ref/` tree. |
| `packaging/` | The Flatpak manifest; one remaining blocker (generate-vs-vendor for `cargo-sources.json`). |
| `legacy-archive/` | Superseded work, kept for the decisions it records. |

## Contributing

The project is small and wants to stay that way. The gates run on every PR and
are the product: **break one on purpose before trusting it.** How to contribute,
what the licence bar for corpus documents is, and where to start are in
[`CONTRIBUTING.md`](CONTRIBUTING.md).

## Licence

GPL-3.0-or-later. The corpus of example documents is CC0 / public domain and is
kept that way deliberately — see `examples/README.md`. The GUI bundles Noto Sans
and Noto Sans CJK TC under the SIL Open Font License 1.1; the notices travel
with the fonts in `textrill-gui-rs/assets/fonts/`.

Upstream `txt2html` licenses itself "under the same terms as Perl itself"
(Artistic License 1.0 or the GPL); this fork is a derivative work distributed
under the GPL branch of that grant. The reasoning is in
[`LICENSE-NOTICE.md`](LICENSE-NOTICE.md).

Upstream copyright is preserved:

```
Copyright 1994-2000 Seth Golub
Copyright 2002-2013 Kathryn Andersen
Copyright 2018-2019 Joao Eriberto Mota Filho
```