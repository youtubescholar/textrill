# textrill

Convert plain text to HTML, inferring structure from conventions authors
already use: indentation for lists, blank lines for paragraphs, underlines and
setext rules for headings, trailing punctuation for link labels. No markup
language is required — the input is plain text, the output is HTML through
templates you control.

`textrill` began as a Rust port of
[`HTML::TextToHTML`](https://metacpan.org/pod/HTML::TextToHTML) 3.0 and its
`txt2html` script, by Seth Golub. It has moved on from byte-parity with that
reference; the Perl original is kept as an on-demand cross-check (`make diff`).

**This is a fork.** It is not affiliated with or endorsed by the upstream
`txt2html` project.

## What it does

- Setext headings and explicit heading rules
- Bullet, ordered and definition lists from layout
- Capitalised runs → `<strong>`; short lines → `<br>`
- Tables (`--make_tables`), mail headers (`--mailmode`)
- Links from URLs, `mailto:` and a user link dictionary
- Encoding detection: BOM → UTF-16 evidence → UTF-8 → CP1252
- Output modes: `--toc`, `--section`, `--chunk`, `--extract`, `--stream`
- Templates: `--template_library article|book|manpage|slide|bare`,
  `--body_template`, `--document_template`, `--var name=value`
- HTML5 by default; `--xhtml` / `--no-html5` for the older modes

## Build and verify

Requires a Rust toolchain with `rustfmt` and `clippy`.

```sh
make build            # the CLI (single static musl binary)
printf 'Hello *world*.\n' | ./textrill/target/release/textrill
make verify           # fmt, clippy, tests, properties, acceptance — the gate
```

`make verify` needs no Perl and means the same thing here and on CI.

## Changing what the tool outputs

The gate compares every corpus case and real example against a recorded golden.
Goldens are a record of approved behaviour, not a wall. When output changes on
purpose:

```sh
make accept-write     # recapture the corpus goldens
make examples-write   # recapture the example goldens
git diff              # the diff of the recapture is the review
```

Commit the goldens with the change that recaptured them. The properties that
cannot regress (no dropped text, well-formed output, determinism, bounded
resources) are gated separately by `proptest` and `alloctest`.

## Repo layout

| Path | What it is |
|---|---|
| `textrill/` | Engine and CLI; the user contract is `textrill/README.md`. |
| `textrill-gui-rs/` | The native GUI (`egui`/`eframe`). |
| `examples/` | Real documents used as acceptance inputs, with a license record. |
| `textrill/tests/` | Unit tests, corpus and goldens. |
| `packaging/` | Flatpak manifest. |

See `CONTRIBUTING.md` for the workflow.

## Legal

Code is GPL-3.0-or-later. Example documents are CC0 / public domain
(`examples/README.md`); the GUI bundles fonts under their own licenses
(`LICENSE-NOTICE.md`). Upstream copyright is preserved:

```
Copyright 1994-2000 Seth Golub
Copyright 2002-2013 Kathryn Andersen
Copyright 2018-2019 Joao Eriberto Mota Filho
```
