# textrill

Convert plain text to HTML.

`textrill` converts plain text to HTML, inferring structure from the
conventions text authors already use: indentation for lists, blank lines for
paragraphs, underlines and setext rules for headings, trailing punctuation for
link labels, and so on. No markup language is required or assumed.

It began as a Rust port of
[`HTML::TextToHTML`](https://metacpan.org/pod/HTML::TextToHTML) 3.0 and its
`txt2html` script, originally written by Seth Golub, and the two can still be
compared byte for byte — the differential harness against the Perl reference is
the only thing that gets to say "behaves like upstream".

**This is a fork.** It is not endorsed by, and carries no affiliation with, the
upstream `txt2html` project or its authors.

## Status

Early. `0.1.0`. The conversion engine is byte-verified against the reference
across a 61-case differential corpus and 33 upstream goldens; the CLI ships as
a single static musl binary; encoding detection, sectioning/TOC/chunk,
citations and glossary, templates, reporting, and a native GUI are implemented
and tested. What remains (Flatpak, distro packaging, the planned harness
phases) is tracked in `docs/PLAN.md`.

## Build and verify

```sh
make build      # the CLI, static musl
make verify     # fmt, clippy, unit/property/allocation tests, corpus, fuzz
make proof      # the pandoc-facing claims (docs/OFFERING.md)
make measure    # the heading-rule measurements (docs/PLAN.md Phase 5)
```

The differential oracle is built from a tracked tarball (`make ref`), offline
and identical everywhere, so a green run means the same thing on this machine,
a GitHub runner, and yours. See `textrill/tests/corpus/README.md` — including
"Six ways this reported success wrongly" — before changing the harness.

## Repo layout

| Path | What it is |
|---|---|
| `textrill/` | The engine and CLI. The user-facing contract is `textrill/README.md`. |
| `textrill-gui-rs/` | The native GUI (`egui`/`eframe`). A frozen behavioural contract lives in `legacy-archive/textrill-gui/SURFACE.md`. |
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