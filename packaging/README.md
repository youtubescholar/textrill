# Packaging

The Flatpak manifest is `packaging/io.github.youtubescholar.Textrill.yml`.
App-id: `io.github.youtubescholar.Textrill` (reverse-DNS; the domain half is
the GitHub owner, which matches `repository` in both `Cargo.toml` files).
The runtime is `org.freedesktop.Platform` `26.08`. The manifest builds
end to end on a host with `flatpak-builder`, with the `.desktop`, AppStream
metainfo and icons now installed from the repository. What remains before
publishing is the Flathub submission itself (including a store screenshot), and
the signing/notarisation follow-ups for the other platforms.

## Desktop metadata

Three files are shared by every desktop-integrated install:

- `appimage/io.github.youtubescholar.Textrill.desktop` — launcher, `%f` open
  support and the `text/plain` MIME association;
- `io.github.youtubescholar.Textrill.metainfo.xml` — AppStream component data,
  including the release entry and both binaries;
- `appimage/io.github.youtubescholar.Textrill.{png,svg}` — the 256 px and
  scalable icons.

The Flatpak's `textrill-metadata` module installs them under `/app/share`; the
AppImage puts them in its AppDir; the deb/rpm metadata in each `Cargo.toml`
lists the same files. Validate them with:

```sh
make metadata             # desktop-file-validate + appstreamcli validate --strict
```

`make metadata` is not part of `make verify` because it needs desktop-specific
tools that the core gate deliberately does not. CI and the release preflight
install them and run it.

The metainfo has no screenshot yet: Flathub wants at least one, and that is an
external submission task rather than a build input.

## `cargo-sources-*.json` are generated snapshots

Each module references its own file, produced by:

```sh
make cargo-sources        # needs: pip install flatpak-cargo-generator
```

The files are checked in so a Flatpak build works offline without Python. The
release workflow regenerates them before building, so a lockfile change cannot
produce a stale vendored tree in a release. `make cargo-sources-check`
regenerates and fails if the checked-in copies differ; run it after changing
either lockfile.

`flatpak-cargo-generator` must be on `PATH`. If it is installed into a
virtualenv (as it is on the development host, at
`/home/vicpu/Downloads/.venv/bin/flatpak-cargo-generator`), call it with that
environment active or invoke the binary directly; `make cargo-sources` fails its
`command -v` guard otherwise.

One file per crate, not one shared file, because the crates are not a
workspace: `textrill/Cargo.lock` and `textrill-gui-rs/Cargo.lock` pin
different versions of shared crates (for example `bit-set` 0.8 vs 0.10), and a
single vendored set could not satisfy both `--locked` builds.

The manifest is written for the generate path: each module lists its JSON as a
**bare filename** (`- cargo-sources-textrill.json`), which makes
`flatpak-builder` expand the generated JSON into the module's source list.
Writing it as `type: file` instead would only *copy* the JSON and the offline
build would fail with `no matching package named ... found`. Switching to
`cargo vendor` later would need the manifests' `sources:` rewritten and the
vendored tree committed; the trade (offline-at-build vs a large committed tree)
is still open.

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
`CARGO_HOME=/run/build/<module>/cargo` and `CARGO_NET_OFFLINE=true`.

The bundled Noto fonts are compiled into the GUI with `include_bytes!`
(`textrill-gui-rs/src/fonts.rs`), so nothing is installed alongside the
binary and the sandbox needs no system fonts.

A third, `textrill-metadata`, module installs the launcher, metainfo and icons.
It is separate because the build modules skip `packaging/` (which holds this
manifest and the generated source lists), while the metadata files must still
enter the build.

## File dialogs and the sandbox

The GUI opens and saves through two paths:

- the desktop's own chooser, via `rfd`'s `xdg-portal` backend (the D-Bus
  `org.freedesktop.portal.FileChooser`); and
- a built-in file browser drawn in egui (`textrill-gui-rs/src/file_browser.rs`).

Flatpak always allows the `org.freedesktop.portal.*` interfaces, so no portal
`--talk-name` is needed and no OpenURI portal is requested — the app never
opens a URI.

The manifest grants **no `--filesystem=home`**. The `FileChooser` portal returns
access to the single file the user picked (through the document portal), so
neither the chooser nor the conversion needs broad home access. That keeps the
sandbox narrow, which is also what Flathub prefers over static filesystem
permissions.

Because that means the built-in browser would have no home access to browse, the
GUI **does not offer it inside the Flatpak**: it detects the sandbox
(`/.flatpak-info`, as GLib does) and always uses the portal, whose dialog works
there (`textrill-gui-rs/src/dialogs.rs`, `src/app.rs`). The portal is also more
reliable than the host-filesystem probe that decides the default on a native
install: inside the sandbox the host's `*.portal` declarations are not visible,
so the probe cannot be trusted to see a dialog that is in fact available.

The CLI ships in the same sandbox. As with any Flatpak CLI, it reads the paths
it is given rather than browsing the home directory; users who want a wider
view can grant it with `flatpak run --filesystem=...`.

On a native install the portal may be unable to service `FileChooser`:
compositor backends such as `xdg-desktop-portal-wlr` (Sway/wlroots) and
`xdg-desktop-portal-hyprland` do not implement it, and without `zenity` as an
`rfd` fallback a request returns nothing — indistinguishable from a cancel, so
Open/Save would silently do nothing. The app therefore probes for a backend
that declares `FileChooser` (`dialogs::system_dialog_available`) and, when none
is found, starts with the built-in browser instead. On a native install the View
menu exposes a "Use built-in file browser" toggle to override either way, and the
choice is remembered; inside the Flatpak the toggle is withheld, since there the
browser has no home access to browse. The built-in browser needs no new
dependencies — only `std` and `egui` — and uses the process's normal filesystem
access (the whole home directory on a native install).

The built-in browser is also the fallback when `rfd` finds no portal and no
`zenity`; that is the case it exists to fix. `rfd` is kept (its only added
crates are `rfd` itself and `pollster`); its `xdg-portal` backend `dlopen`s
`libdbus-1.so.3` at runtime, so it adds no link-time library and no GTK.

## Building

```sh
make cargo-sources        # once, or after a lockfile change
make metadata             # needs desktop-file-utils and appstream
flatpak-builder --force-clean build packaging/io.github.youtubescholar.Textrill.yml
```

`build/` is the `flatpak-builder` output directory and `.flatpak-builder/` its
shared state; both are git-ignored. The finished app is at
`build/files/bin/textrill-gui` (and `.../textrill` for the CLI). To produce the
single-file bundle the release ships:

```sh
flatpak-builder --force-clean --repo=build/flatpak-repo build/flatpak \
  packaging/io.github.youtubescholar.Textrill.yml
flatpak build-bundle build/flatpak-repo textrill-gui-x86_64.flatpak \
  io.github.youtubescholar.Textrill
```

## AppImage (local, not portable)

```sh
make appimage              # or: packaging/appimage/build.sh
```

This builds the release binaries, assembles an AppDir, and packages
`build/appimage/textrill-gui-x86_64.AppImage` with both the GUI and the CLI.
The AppDir carries the `.desktop` file, metainfo and both icons, so a desktop
that integrates the AppImage can find them.

It is a convenience for running the app on this host, **not** a substitute for
the Flatpak. The AppImage inherits the build host's glibc: a local build on
Ubuntu 24.04 has a glibc 2.39 floor. The release workflow builds on Ubuntu
22.04 for a lower floor, but a genuinely portable AppImage would have to be
built inside a pinned old-glibc container. That remains out of scope; the
Flatpak is the portable Linux artifact.

The AppImage pieces live under `packaging/appimage/`
(`AppRun` is AppImage-only):

- `io.github.youtubescholar.Textrill.desktop`
- `io.github.youtubescholar.Textrill.svg` and the rendered `.png`
- `AppRun`
- `build.sh`

The script fetches `appimagetool` once into `build/appimage/tools/` (network on
first run only) and runs it with `--appimage-extract-and-run`, so no FUSE is
required.

## Debian and RPM packages

`make deb` and `make rpm` use `cargo-deb` and `cargo-generate-rpm`. Their
metadata is in each crate's `Cargo.toml`; the GUI packages install the same
launcher, metainfo and icons as the Flatpak, while the CLI package stays a
single binary plus its README. Build both with:

```sh
cargo install cargo-deb cargo-generate-rpm
make packages
```

The `.deb` files land under each crate's `target/debian/`, the `.rpm` files
under `target/generate-rpm/`. Runtime dependencies are discovered from the
built binaries (`$auto` for deb, the built-in `ldd` pass for rpm).

## Release pipeline

`.github/workflows/release.yml` runs on a `v*` tag. Preflight rejects a tag
that does not match both crate versions and validates the desktop metadata.
The Linux job re-runs `make verify`, the static musl acceptance, then builds the
AppImage, deb and rpm packages; separate jobs build the Flatpak, Windows and
macOS binaries and publish everything with a `SHA256SUMS.txt` on the GitHub
release. The engine crate is published to crates.io when
`CARGO_REGISTRY_TOKEN` is configured (the binaries still ship without it).

CI (`.github/workflows/ci.yml`) runs `make verify`, the musl acceptance and
`make metadata` on every push and pull request.

## Still missing for distribution

- Flathub submission and at least one screenshot in the metainfo.
- Windows code signing and macOS notarisation (the binaries are unsigned).
- A truly portable AppImage built against a pinned old glibc (the Flatpak is
  the portable Linux option for now).
