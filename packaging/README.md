# Packaging

The Flatpak manifest is `packaging/io.github.youtubescholar.Textrill.yml`.
App-id: `io.github.youtubescholar.Textrill` (reverse-DNS; the domain half is
the GitHub owner, which matches `repository` in both `Cargo.toml` files).

The manifest is complete but not buildable yet, for one reason:

## `cargo-sources.json` is generated, not committed

Both modules reference `../packaging/cargo-sources.json`, produced by:

```sh
make cargo-sources        # needs: pip install flatpak-cargo-generator
```

It is not committed because it is vendored crate metadata that only has to
agree with one lockfile, and a stale copy is a build failure nobody can read.

**Undecided: generate, or `cargo vendor`.** Generate needs
`flatpak-cargo-generator` at build time and keeps the repo small; vendoring
needs no generator and builds offline at the cost of a large committed tree.
Vendoring is the better fit here (offline reproducible builds matter more
than tree size), but it is not decided.

## Module layout

Three modules: the `rust-stable` SDK extension, then the engine, then the
GUI. The GUI module sources the repository root rather than only
`textrill-gui-rs`, because the crate depends on `../textrill` by path; the
engine therefore builds first. The bundled Noto fonts install from
`textrill-gui-rs/assets/fonts`, where the binary looks for them.

## To make it buildable

1. Decide generate-vs-vendor, then run `make cargo-sources` or `cargo vendor`.
2. `flatpak-builder build packaging/io.github.youtubescholar.Textrill.yml`
