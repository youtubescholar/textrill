# Packaging

The Flatpak manifest is `packaging/io.github.youtubescholar.Textrill.yml`.
App-id: `io.github.youtubescholar.Textrill` (reverse-DNS; the domain half is
the GitHub owner, which matches `repository` in both `Cargo.toml` files).
The runtime is `org.freedesktop.Platform` `26.08`. The manifest builds
end to end on a host with `flatpak-builder`; what remains before publishing is
the metadata listed at the bottom.

## `cargo-sources-*.json` are generated, not committed

Each module references its own file, produced by:

```sh
make cargo-sources        # needs: pip install flatpak-cargo-generator
```

`flatpak-cargo-generator` must be on `PATH`. If it is installed into a
virtualenv (as it is on the development host, at
`/home/vicpu/Downloads/.venv/bin/flatpak-cargo-generator`), call it with that
environment active or invoke the binary directly; `make cargo-sources` fails its
`command -v` guard otherwise.

One file per crate, not one shared file, because the crates are not a
workspace: `textrill/Cargo.lock` and `textrill-gui-rs/Cargo.lock` pin
different versions of shared crates (for example `bit-set` 0.8 vs 0.10), and a
single vendored set could not satisfy both `--locked` builds.

They are not committed because they are vendored crate metadata that only has
to agree with a lockfile, and a stale copy is a build failure nobody can read.

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
flatpak-builder --force-clean build packaging/io.github.youtubescholar.Textrill.yml
```

`build/` is the `flatpak-builder` output directory and `.flatpak-builder/` its
shared state; both are git-ignored. The finished app is at
`build/files/bin/textrill-gui` (and `.../textrill` for the CLI).

## AppImage (local, not portable)

```sh
make appimage              # or: packaging/appimage/build.sh
```

This builds the release binaries, assembles an AppDir, and packages
`build/appimage/textrill-gui-x86_64.AppImage` with both the GUI and the CLI.
It is a convenience for running the app on this host, **not** a substitute for
the Flatpak and **not** portable: the AppImage inherits the build host's glibc
(Ubuntu 24.04, glibc 2.39), so it runs only on distributions at least as new as
the build host. A portable one would have to be built inside an older-glibc
container; that is deliberately out of scope while the app is not released.

The assets under `packaging/appimage/` are the pieces a desktop-integrated
install needs, and the `.desktop` file and icon there are the same ones the
Flatpak manifest should install (`packaging/appimage/AppRun` is AppImage-only):

- `io.github.youtubescholar.Textrill.desktop`
- `io.github.youtubescholar.Textrill.svg` and the rendered `.png`
- `AppRun`
- `build.sh`

The script fetches `appimagetool` once into `build/appimage/tools/` (network on
first run only) and runs it with `--appimage-extract-and-run`, so no FUSE is
required.

## Still missing for distribution

AppStream metainfo (`io.github.youtubescholar.Textrill.metainfo.xml`) is still
missing, and neither the Flatpak manifest nor the AppImage installs the
`.desktop` file and icon into the runtime. Flathub requires all three; the
`.desktop` and icon now exist under `packaging/appimage/` and only need wiring
into the manifest, but the metainfo has to be written before publishing.
