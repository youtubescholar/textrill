# `parity/` — the non-gating Perl parity harness

Everything reference-related in one place: the original Perl `HTML::TextToHTML`
(3.0), the `YAML::Syck` stub it needs at load time, the tracked upstream
archives, and the differential corpus runner. This is the isolation decision
in roadmap §8.5 — the Rust crate is the product, the Perl heritage is an audit
asset, and **no gate depends on any of it**.

## The rules

- `make verify` at the top level never enters this directory, never runs
  Perl, and passes with `parity/` absent.
- Nothing in the Rust roadmap depends on `parity/`. Parity work is opt-in and
  may lag without blocking anything.
- Offline by construction: the reference is extracted from the tracked
  tarballs here. No network, no CPAN, no package installs — a core Perl is
  the whole dependency.
- `--xhtml` and `--no-html5` are a **frozen compatibility layer**: they
  reproduce the reference's document skeleton byte for byte for porting and
  audit. They keep their golden pins but no longer constrain new features.

## Layout

| Path | What |
|---|---|
| `parity/corpus/run.sh` | the differential runner (Perl reference vs Rust port) |
| `parity/ref/txt2html-3.0/` | the extracted reference (derived; `make ref`) |
| `parity/ref/txt2html-master/` | the 26 MB scale fixture (derived; `make ref-large`) |
| `parity/ref/stubs/YAML/Syck.pm` | the installed stub (derived from `stubs/`) |
| `parity/stubs/YAML/Syck.pm` | the tracked stub source, with its provenance |
| `parity/txt2html-3.0.tar.gz` | the tracked reference archive |
| `parity/txt2html-master.zip` | the tracked 26 MB fixture archive |

Shared with the Rust side, and deliberately **not** here: the case table
(`textrill/tests/corpus/cases.sh`), the fixtures, the author goldens and
`normalize.py`. Both runners read exactly those files, so a case has one input
set and the reference-free acceptance gate can run with this directory
deleted.

## Running it

From the repository root, so the familiar names keep working:

```sh
make ref     # extract the reference + stub (offline, idempotent)
make corpus  # the differential corpus
make diff    # ref + corpus: the on-demand parity cross-check
make scale   # the 1 MB paragraph probes, timed against Perl
```

Or directly: `parity/corpus/run.sh [case-name]`.

## CI

When CI lands (roadmap Phase 6) this directory belongs in its **own non-gating
job**: a failure here reports drift from the Perl reference, never a broken
build. The main `make verify` job must stay Perl-free and must not check this
directory out at all.
