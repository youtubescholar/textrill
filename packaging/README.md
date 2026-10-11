# Packaging

The Flatpak manifest is `packaging/io.github.youtubescholar.Textrill.yml`.
App-id: `io.github.youtubescholar.Textrill` (reverse-DNS; the domain half is
the GitHub owner, which matches `repository` in both `Cargo.toml` files).

The manifest is complete but not buildable yet, for one reason:

## `cargo-sources-*.json` are generated, not committed

Each module references its own file, produced by:

```sh
make cargo-sources        # needs: pip install flatpak-cargo-generator
```

One file per crate, not one shared file, because the crates are not a
workspace: `textrill/Cargo.lock` and `textrill-gui-rs/Cargo.lock` pin
different versions of shared crates (for example `bit-set` 0.8 vs 0.10), and a
single vendored set could not satisfy both `--locked` builds.

They are not committed because they are vendored crate metadata that only has
to agree with a lockfile, and a stale copy is a build failure nobody can read.

**Undecided: generate, or `cargo vendor`.** Generate needs
`flatpak-cargo-generator` at build time and keeps the repo small; vendoring
needs no generator and builds offline at the cost of a large committed tree.
Vendoring is the better fit here (offline reproducible builds matter more
than tree size), but it is not decided.

## Module layout

Two modules, no toolchain module. The Rust compiler comes from the
`org.freedesktop.Sdk.Extension.rust-stable` SDK extension declared as
`sdk-extensions`, and `/usr/lib/sdk/rust-stable/bin` is prepended to `PATH`
for every module. Engine first, then the GUI: the GUI depends on `../textrill`
by path, so the CLI has to build before the GUI configures, and both are
installed so the single artifact ships both.

The GUI module sources the repository root rather than only
`textrill-gui-rs`, because the crate depends on `../textrill` by path. Both
modules build by `--manifest-path` (there is no workspace `Cargo.toml` at the
repo root) and offline against the generated vendored sources via
`CARGO_HOME=/run/build/<module>/cargo`.

The bundled Noto fonts are compiled into the GUI with `include_bytes!`
(`textrill-gui-rs/src/fonts.rs`), so nothing is installed alongside the
binary and the sandbox needs no system fonts.

## To make it buildable

1. Decide generate-vs-vendor, then run `make cargo-sources` or `cargo vendor`.
2. `flatpak-builder build packaging/io.github.youtubescholar.Textrill.yml`

## Still missing for distribution

There is no `.desktop` file, AppStream metainfo, or icon in the tree yet. The
manifest builds a runnable app, but Flathub (and desktop-integrated installs
generally) require those; add them before publishing.
