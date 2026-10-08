# Rust GUI toolkit findings and development sequencing

Status: findings recorded 2026-10-02; status reviewed 2026-10-06. Background
document for the native GUI — see `DOCS.md` for the register and what is
authoritative. The remediation plan this used to name as a companion is retired
and lives at `legacy-archive/REMEDIATION-PLAN.md`; the feature-gap survey is
also archived, its matrix having gone stale.

This document exists because the Phase 6 native GUI rewrite stalled on a
toolkit choice that looked settled. The cost was not the code — it was the
assumption. The findings below are the ones we paid for, recorded so the next
project does not pay for them again.

## 1. TL;DR

- **Decision: the Phase 6 GUI is written in Rust with `egui`/`eframe`.** The
  toolkit is pure Rust, has first-class headless widget testing, and removes the
  C++/MOC/`build.rs` surface entirely.
- The earlier Qt 6 decision was correct *as a target* but the specific binding we
  picked was not viable here. See §3.
- **Sequencing rule: validate the GUI toolkit with a throwaway spike before
  freezing any interface, and make the spike fail on the *integration* path (a
  window that opens and a widget test that runs), not on a `cargo add`.**
- **Keep the GUI a separate crate.** This protects the CLI's static/musl build
  and keeps the engine dependency-clean.

## 2. Environment this was tested on

| Item | Value |
|---|---|
| OS | Ubuntu 24.04 |
| Rust | 1.98.1 |
| Qt (system) | 6.4.2 (`qt6-base-dev`, `qt6-declarative-dev`) |
| Graphics | Mesa 25.2.8, software GL (llvmpipe), Vulkan loader + lavapipe (`libvulkan_lvp.so`) |
| Display for smoke runs | `xvfb-run`, `QT_QPA_PLATFORM=offscreen` for Qt |

## 3. What actually happened with Qt

Qt 6 itself is installed and healthy. The problem is the Rust bindings.

### 3.1 cxx-qt 0.10.0 is not usable in this configuration

`cxx-qt` is the KDAB binding and the one the plan originally named. On the
current toolchain (syn 2.0.119 / 3.0.6, cxx 1.0.202) a bridge module containing
**any** `extern "RustQt"` QObject fails to compile. For example:

```text
error: expected one of `!`, `(`, `+`, `::`, `<`, `>`, or `as`, found `/`
error: non-foreign item macro in foreign item position: include
```

Root cause: `cxx-qt-gen` emits `include !(< QtCore / QObject >);` into the
generated Rust. `include!(<...>)` is accepted by **cxx's own parser**, but
`#[cxx_qt::bridge]` first runs the input through `syn::ItemMod`, and `syn` cannot
parse an angle-bracket path in macro position. The crate's own `test_inputs`
exercise the generator `Parser` directly and bypass the macro, so the breakage
is invisible to its test suite.

Independent findings from the same spike:

- `include!(<QtWidgets/QPushButton>)` **does not work** in
  `#[cxx_qt::bridge]`; the string form `include!("QtWidgets/QPushButton")` is
  required. (The angle form works only in a plain `#[cxx::bridge]`.)
- Only **one `include!` per extern block** is accepted.
- A `#[qobject]` type must be declared in exactly **one** block; declaring it in
  both `C++` and `C++Qt` blocks errors with `defined multiple times`.
- `impl cxx_qt::Constructor<()> for T` only applies to **Rust-defined** QObjects
  (`extern "RustQt"`). Existing Qt classes such as `QPushButton` get **no
  constructor** from Rust, so every widget class needs a hand-written C++
  factory shim.
- Static methods such as `QApplication::exec` cannot be expressed as free
  functions in the block form tested.

Upstream documentation also describes cxx-qt as "designed for teams already
living in C++", and its README notes the API is pre-1.0 and changes frequently.
It is a reasonable choice for embedding Rust in an existing C++ codebase; it is
a poor fit for a Rust-first GUI.

### 3.2 Qt Bridge for Rust (`qtbridge`) looks like the right Qt tool — but needs Qt 6.10+

`qtbridge` 0.3.0 is Qt's official Rust bridge. Its API is the cleanest of any
option examined:

```rust
QApp::new()
    .register::<Backend>()
    .load_qml(include_bytes!("qml/Main.qml"))
    .run();
```

No `build.rs`, no hand-written C++. It is CXX-Qt-compatible and reuses
cxx-qt-lib's basic types.

**Blocker:** the upstream README states QtBridge currently requires **Qt 6.10 or
higher**; Ubuntu 24.04 ships Qt 6.4.2. Qt 6.10 would have to be installed out of
band (`aqtinstall`, ~2 GB) and Flathub runtimes do not ship it, which pushes the
problem into packaging. This should be re-evaluated if the project ever moves to
a distro or runtime with Qt ≥ 6.10.

### 3.3 The other Qt-adjacent options

- **`qmetaobject-rs`** — QML-only, and passively maintained; its author moved to
  Slint and recommends it. Not suitable for a new project.
- **`rust-qt` / generated bindings** — direct, unsafe, non-idiomatic, and not
  maintained. Not considered.

## 4. The pure-Rust options

| Toolkit | Version | Model | Headless testing | Notes |
|---|---|---|---|---|
| `egui`/`eframe` | 0.36.2 | immediate mode | `egui_kittest` + `kittest` (AccessKit) | Chosen |
| Slint | 1.18.1 | retained, declarative | `i-slint-backend-testing` | Runner-up |
| `iced` | 0.14.0 | retained, Elm-style | weaker testing story | Not chosen |
| `gtk4` | 0.11.5 | retained | manual / lower-level | Adds GTK system dep |

### 4.1 Why egui/eframe was chosen

1. **The GUI is a 54-option settings panel with a live preview and 32 acceptance
   tests.** Testability dominates. `egui_kittest` exposes the widget tree through
   AccessKit, so tests find controls by their visible label with no display:

   ```rust
   use egui_kittest::kittest::Queryable;
   let harness = egui_kittest::Harness::new_ui(panel);
   harness.get_by_label("Fancy headers");
   harness.get_by_label("Encoding:");
   ```

   This was verified working headlessly on this machine.
2. **It removes the entire C++/MOC/`build.rs` failure class**, which is exactly
   where the previous weeks went.
3. It is a consistent fit for a Rust-first rewrite and needs no Qt at all.
4. Immediate mode is a good match for a form that re-renders on every option
   change.

### 4.2 Why Slint was the runner-up

Slint is retained-mode and declarative, closer in spirit to QML, and is actively
developed by the author of `qmetaobject-rs`. It is the better choice if the UI
later needs complex custom widgets or animation. It was not chosen because the
AccessKit-based `egui_kittest` harness is a more direct fit for the acceptance
suite and because the immediate-mode model maps cleanly onto the existing
"options panel emits settings, preview re-renders" contract.

## 5. Dependency and packaging analysis

### 5.1 Counts

Measured with `cargo generate-lockfile` / `cargo tree` on 2026-10-02:

| Configuration | Lock packages |
|---|---|
| `eframe` default (includes `wgpu` + `accesskit`) | 417 |
| `eframe` with `glow` (OpenGL) + `accesskit` | 365 |
| `eframe` with `glow`, **no** OS `accesskit` bridge | 298 |

Observations:

- The default `eframe` pulls **both** `wgpu` and `glow`; choosing one removes a
  large subtree.
- `egui` depends on `accesskit` for its own accessibility tree regardless of
  features. The `accesskit` *feature* adds the OS bridge
  (`accesskit_unix` → `atspi`/D-Bus, ~67 crates). Our acceptance tests do not
  need the OS bridge, so it can be omitted and re-added if screen-reader support
  becomes a requirement.
- No OpenSSL, no Rustls, no GTK/GDK/WebKit, no async runtime (Tokio/async-std).
  `atspi`/D-Bus appears only via the optional accessibility bridge.

Recommended feature set:

```toml
[dependencies]
egui = "0.36"
eframe = { version = "0.36", default-features = false, features = [
    "default_fonts", "glow", "wayland", "x11",
] }

[dev-dependencies]
egui_kittest = "0.36"
kittest = "0.3"
```

### 5.2 System libraries

`ldd` on a built `eframe` binary shows **no non-trivial system libraries linked
at build time** — only libc/libgcc/libm. `winit`, `glutin`, and `wgpu` load
libGL/libEGL/libX11/libwayland-client/libxkbcommon/libvulkan dynamically at
runtime.

By design we use `glow` (OpenGL/EGL). It therefore needs a working GL stack:
Mesa plus a software rasterizer (llvmpipe) where no GPU is present. Both were
verified present here, and a windowed run under `xvfb-run` with
`LIBGL_ALWAYS_SOFTWARE=1` stayed alive with no errors for the full test window.

### 5.3 Flatpak / distro

- `org.freedesktop.Platform` and the KDE/GNOME runtimes already carry Mesa and
  the usual GL/EGL/Wayland/X11/xkbcommon libraries, so an `eframe` app needs no
  unusual runtime extension. This is **simpler** than shipping Qt, because there
  is no Qt version to match against the runtime.
- Fonts are bundled, so there is no runtime font dependency: `default_fonts`
  (Ubuntu-Light, a monochrome emoji face, Hack) plus Noto Sans Regular
  (Greek/Cyrillic) and Noto Sans CJK TC (the full pan-CJK repertoire), appended
  by the app as fallbacks. See §5.5 and §6 item 14 for the size and the SC/TC
  choice.
- If the OS accessibility bridge is enabled later, Flatpak needs
  `--talk-name=org.a11y.Bus`.
- Distro packaging is equally straightforward: the binary's only hard runtime
  requirement is a GL stack, which every desktop provides.

### 5.4 musl and static builds

`winit`/`glutin` depend on runtime `dlopen` of GL/Wayland/X11 libraries and are
not a good candidate for a fully static musl binary. This is **not** a problem
as long as the split is respected:

- **CLI** → may continue to target `x86_64-unknown-linux-musl` (Phase 5 work).
- **GUI** → a separate crate, built against glibc, shipped as a Flatpak or a
  normal desktop package.

Do **not** put `eframe` in the same crate/feature graph as the static CLI build.

### 5.5 Licence, privacy and fingerprinting audit

Run 2026-10-03 against the GUI crate's resolved graph
(`cargo metadata --all-features`, which includes dev-dependencies; 318 packages).

**Licences are all permissive or GPL-3.0-compatible.** No package is
copyleft-only, AGPL, SSPL or non-free. The classes present:

- MIT and/or Apache-2.0 (the large majority), plus `Zlib`,
  `BSD-2/3-Clause`, `ISC`, `0BSD`, `BSL-1.0`, `Unlicense`, `Unicode-3.0`.
- `rfd` (the file chooser) is `MIT`; `pollster` (its blocking adapter) is
  `Apache-2.0 OR MIT`.
- `r-efi` (`MIT OR Apache-2.0 OR LGPL-2.1-or-later`) and `self_cell`
  (`Apache-2.0 OR GPL-2.0-only`) are dual-licensed; the permissive branch is
  taken (default Cargo selection), which is compatible with our GPL-3.0-or-later.
- `epaint_default_fonts` is `(MIT OR Apache-2.0) AND OFL-1.1 AND
  Ubuntu-font-1.0` — the `default_fonts` feature bundles an emoji font (OFL) and
  Ubuntu-Light (Ubuntu font licence), both redistributable. Distros that prefer
  system fonts can drop `default_fonts`, at the cost of egui having no built-in
  glyphs.
- The bundled `NotoSans-Regular.ttf` and `NotoSansCJKtc-Regular.otf`
  (`textrill-gui-rs/assets/fonts/`, with `OFL.txt` and `OFL-NotoSansCJK.txt`)
  are OFL-1.1. They add no crate to the graph. `Noto Sans` carries
  Latin/Greek/Cyrillic; `Noto Sans CJK TC` carries the full pan-CJK repertoire
  (CJK Unified Ideographs + Ext-A, kana, Hangul, Bopomofo). See §6 item 14 for
  why `TC` and what it costs.
- GPL-3.0-or-later: only our own two crates.

**No network, telemetry or fingerprinting.** `cargo tree` contains no
`reqwest`/`hyper`/`ureq`/`curl`/`openssl`/`rustls`/`tokio`/`async-std`. Our own
source has no `std::net`, no `Command`, no `std::fs`/`std::env` reads (the
engine's `Options::default()` reads `HOME` for the default link dictionary — the
same local configuration the CLI already does — and `convert_text` is
in-memory). The GUI crate now carries `#![forbid(unsafe_code)]`.

The file chooser is local-only. With the `xdg-portal` feature, `rfd` asks the
session's desktop portal over D-Bus for a path and returns just that path; it
does not copy or move files, and we set `can_create_directories(false)` so a
mistyped save name cannot grow a directory tree. If the portal is unavailable
`rfd` falls back to running `zenity` as a dialog-only process — the one place a
dependency can spawn a program, and it only ever receives filter/name strings.

Two dependencies pull data that looks sensitive but is not:

- `gethostname` (via `x11rb`, which `winit` and `arboard` use) is called for the
  local X11 connection's auth family and, with the `resource_manager` feature,
  to locate `$HOME/.Xdefaults-<hostname>` (X resources). It is used to *find*
  local files and authenticate to the local X server; nothing is transmitted off
  the machine, and the value never leaves `x11rb`.
- `getrandom` (via `ahash`, for HashMap seed randomisation) and `uuid` (via
  `accesskit`, for widget node IDs) only produce in-process random identifiers.

**Build-time probes.** `pkg-config` and `cc` appear as build-dependencies
(`x11-dl`, `wayland-sys`, `smithay-client-toolkit`; `cc` for the wayland
backend). They locate X11/Wayland headers if present, but the runtime link is
`dlopen`, as `ldd` confirms (only libc/libgcc/libm). The hard runtime
requirement remains just a GL stack.

## 6. Verified facts (reproducible)

The following were confirmed by running them, not by reading docs:

1. `pkg-config --modversion Qt6Widgets Qt6Core Qt6Qml Qt6Quick Qt6QuickControls2`
   all report `6.4.2`.
2. `egui_kittest::Harness::new_ui` finds widgets by label headlessly
   (AccessKit), with no display. Test passed.
3. A minimal `eframe` app builds and runs under `xvfb-run` +
   `LIBGL_ALWAYS_SOFTWARE=1`, surviving a 20-second window with an empty log.
4. `eframe 0.36` changed the `App` trait: `fn update(...)` is gone; the required
   method is `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)`,
   with an optional `fn logic(...)`. Code samples from earlier versions will not
   compile.
5. `kittest` must match the version expected by `egui_kittest` (0.3 for
   `egui_kittest` 0.36); a mismatched `kittest` fails with `method not found in
   Node`.
6. `cxx-qt` 0.10 exhibits the failures listed in §3.1.
7. `qtbridge` 0.3.0 resolves on crates.io; its upstream README states a Qt 6.10+
   requirement.
8. Whole-UI zoom is built into `egui` 0.36 and works with no extra code:
   `Options::zoom_with_keyboard` defaults to `true`, and Ctrl/⌘ + `=`/`+`, `-`,
   `0` change `Context::zoom_factor` in 0.1 steps, clamped to 0.2–5.0.
   `egui::gui_zoom::{zoom_in, zoom_out, zoom_menu_buttons}` are public for a
   menu or toolbar. Confirmed by `the_ui_zooms_with_the_keyboard` and
   `the_display_controls_are_exposed`.
9. OS DPI scaling is automatic and separate from user zoom. On X11, winit
   0.30.13 reads `Xft/DPI` (xsettings, value ÷ 1024); on Wayland it reports the
   output scale. egui then computes `pixels_per_point = zoom_factor *
   native_pixels_per_point`, so a HiDPI panel and a 96 DPI projector are already
   reconciled before the user touches anything, and user zoom multiplies on top.
10. `eframe` persists window geometry and egui memory (which includes
    `zoom_factor`) only with the non-default `persistence` feature, which pulls
    in `egui/persistence`, `egui-winit/serde`, `serde` and `ron`. That feature is
    deliberately left off; the app persists the geometry itself (item 15). The
    default window is winit's 800×600; a first run asks for 900×720 with a
    360×300 minimum.
11. `Context::send_viewport_cmd` requests a repaint. Sending the same `Title`
    every frame therefore keeps the UI permanently awake and makes
    `egui_kittest::Harness::run` fail with "exceeded max_steps". The title is now
    sent only when it changes.
12. `rfd` 0.17's synchronous `FileDialog::pick_file`/`save_file` return
    `Option<PathBuf>` and block the caller until the portal answers. Because that
    would hang headless tests, the app asks a `Chooser` trait object (default
    `NativeChooser`) so a test stub can answer `None`/a path. Only two packages
    were added (`rfd`, `pollster`); `percent-encoding`, `libc` and `log` were
    already in the graph. The `wayland` feature is left off because the dialog
    is not parented.
13. The GUI's command line is hand-parsed (`src/args.rs`) rather than pulling in
    `clap`, keeping parity with `app.py`: `[file]`, `--xhtml`/`--no-xhtml`,
    `--tables`, `--version`, `-h`/`--help`. `--version` prints and exits before
    any window is created; an unknown flag or a second file exits 2.
14. `egui` renders only the fonts it is given and never reads the OS font
    configuration, so "the system will supply CJK" is false without extra work.
    The app bundles Noto Sans Regular (0.57 MB) and Noto Sans CJK TC Regular
    (15.7 MB), both OFL-1.1, appended behind the stock faces.
    - **Coverage:** the `CJK TC` file is the full pan-CJK repertoire, not a
      Traditional-only subset — verified with `fc-query`: Traditional-only
      codepoints (國門說體龍), kana (あカ), Hangul (한), and Bopomofo (ㄅ) are all
      present. Japanese and Korean text therefore do not tofu.
    - **SC vs TC:** `egui` applies no OpenType language features, so one default
      shape set must be chosen. SC and TC have identical coverage; they differ
      only in the default glyph shape for codepoints shared across regions
      (直, 骨, 者…). `TC` was chosen so Traditional-default shapes are used.
      Bundling both is pointless: both contain every codepoint, so the second is
      never consulted.
    - **Cost:** the CJK font dominates the app — stripped binary 28.6 MB (from
      12.9 MB), ~15.7 MB compressed (≈ Flatpak download), up from ~4.2 MB. This
      is in line with comparable self-contained Rust GUIs (e.g. LACT: 10.2 MB
      download / 27.4 MB installed) and far below Electron editors
      (150–230 MB). Dropping the CJK font is the one big size lever.
    The engine converts CJK correctly regardless; this is display-only.
15. Window geometry is persisted by the app, not by `eframe` (item 10), and only
    as **normal size + maximized flag + zoom** — never a position. A stored
    position is the one value that can strand a window off-screen (and Wayland
    forbids clients setting one at all), so it is not written. The facts behind
    that choice:
    - `ViewportInfo::inner_rect` is `None` on Wayland, so the size is read from
      `Context::viewport_rect()` instead, which is available on every backend.
    - `ViewportBuilder::with_inner_size` is in egui points *before* zoom; on
      creation `egui-winit` multiplies it by `zoom_factor`, and
      `with_clamp_size_to_monitor_size` defaults to `true`, so an oversized
      stored value is clamped to the monitor. The stored size is therefore
      `viewport_rect().size() * zoom_factor`, which is invariant under a change
      of zoom.
    - Applying `Context::set_zoom_factor` *after* creation changes
      `pixels_per_point` but not the OS window size, which is why zoom is applied
      in the `run_native` creation closure, before the first frame.
    - A window created flush with the work area cannot be resized, so the stored
      size is the last *normal* size even while maximized.
    - Every value is clamped on read (`360×300` … monitor, zoom `0.5` … `4.0`),
      and an unreadable `window-size`/`zoom` falls back to the default, so a
      hand-edited or corrupt file can never stop the window from opening.
    - A single `View → Reset window size and zoom` command is the escape hatch;
      it clears the stored keys and has a keyboard route (`Ctrl/Cmd+Alt+0`) that
      does not depend on any widget being on-screen. The toolbar uses
      `horizontal_wrapped`, so at a high zoom it folds instead of clipping
      controls — the Reflow half of WCAG 1.4.10.
    - The `[General]` section is written to a sibling temporary file and renamed,
      so a crash or a second instance cannot leave a truncated config.

## 7. How to sequence this better next time

These are general, not egui-specific.

1. **Spike the integration path, not the dependency.**
   A successful `cargo add` proves nothing. The spike is only done when a window
   opens and a widget test runs. Budget half a day for this *before* freezing an
   interface such as `SURFACE.md`.
2. **Verify the published crate version, not the documentation.**
   cxx-qt's docs and generator tests described a syntax the released macro could
   not compile. The README for `qtbridge` and the published `0.3.0` may also
   differ; check the artifact.
3. **Check the version of the system dependency against the binding's
   requirement early.** `qtbridge` needed Qt 6.10; the OS has 6.4.2. Discover
   that in hour one, not after committing to QML.
4. **Prefer the tool whose test surface matches the deliverable.** For a form
   with an acceptance suite, headless widget querying is worth more than a nicer
   declarative language.
5. **Keep the UI crate separate from the engine and CLI.** It isolates system
   dependencies, protects static/musl builds, and keeps the engine testable
   without a display.
6. **Decide the boundary before porting, but expect to revise it once.**
   `SURFACE.md` froze QtWidgets widget mappings before the toolkit was proven.
   Freeze the *behaviour* (options, signals, threading contract) early; defer
   the *widget mapping* until the toolkit spike passes.
7. **Record the decision and its evidence in-repo** (this file, plus a line in
   the plan) so the next contributor does not relitigate it from the docs.
8. **Keep throwaway spikes out of the tree.** All work behind these findings
   lived in `/tmp/opencode/`; the repository gained only documentation.

## 8. Consequences for textrill Phase 6

- The frozen behavioural contract (option inventory, generation counter, queue
  drop, max-two-workers, panic capture) was ported from `textrill-gui/SURFACE.md`
  as `egui_kittest` acceptance tests rather than as a re-drawn widget mapping.
- A native GUI crate (`textrill-gui-rs`) depending only on `egui`/`eframe` and
  the engine crate was added, and the Python/PySide6 GUI and the `pyo3` layer
  were retired to `legacy-archive/` once its acceptance suite passed.
- ~~Port the 32 acceptance tests onto `egui_kittest`/`kittest`, querying by label.~~ Done: 74 native GUI tests.
- ~~Remember the window geometry.~~ Done: normal size, maximized flag and zoom
  are persisted (never position); see §6 item 15 for the policy and the escape
  hatch.
- Keep the CLI's musl/static build path free of GUI dependencies.
- The `qt6-base-dev` / `qt6-declarative-dev` packages are no longer required by
  the project and can be removed from the build prerequisites.
