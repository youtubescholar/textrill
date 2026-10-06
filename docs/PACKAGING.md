# Packaging

The Flatpak manifest is `packaging/io.github.example.Textrill.yml`. It is
complete and **deliberately not buildable**, for two reasons recorded below.

## The app-id is a placeholder, on purpose

```
app-id: io.github.example.Textrill
```

Flatpak requires a reverse-DNS identifier, and only the leaf is settled: the
tool is `textrill`. The domain half is the GitHub account, which is exactly the
`<you>` still sitting in the `repository = "https://github.com/<you>/textrill"`
TODO in both `Cargo.toml` files.

`example` was chosen because it is not a real owner, so the id cannot be
published by accident. Replacing it later means changing three places at once:
both `Cargo.toml` files and the manifest's filename.

## `cargo-sources.json` is generated, not committed

Both modules reference `../packaging/cargo-sources.json`, which does not exist.
It is produced by:

```sh
make cargo-sources
```

which runs `flatpak-cargo-generator` over the two lockfiles. That tool is not
installed on this host — `flatpak` and `flatpak-builder` both are — so the
target fails with the `pip install` line rather than pretending.

It is not committed because it is vendored crate metadata that only has to agree
with one lockfile, and a stale copy is a build failure nobody can read.

## Undecided: generate, or vendor

- **Generate** — needs `flatpak-cargo-generator` at build time; no large tree in
  the repository.
- **Vendor** — `cargo vendor`; no generator dependency, and the build works
  offline, at the cost of a large committed tree.

The design note prefers vendoring, because an offline reproducible build is worth
more than the tree size for a project at this stage. Not decided here.

## Module layout

Three modules: the `rust-stable` SDK extension, then the engine, then the GUI.

The GUI module cannot source only `textrill-gui-rs`. It depends on `../textrill`
by path, and cargo will not configure without the sibling present, so both
modules source the repository root and the engine builds first. The bundled Noto
fonts install from `textrill-gui-rs/assets/fonts`, which is where the binary
looks for them.

## To make it buildable

1. Get the GitHub owner; set `repository` in both `Cargo.toml` files.
2. Replace `io.github.example.Textrill` in the manifest and rename the file.
3. Decide generate-vs-vendor, then run `make cargo-sources` or `cargo vendor`.
4. `flatpak-builder build packaging/io.github.<owner>.Textrill.yml`
