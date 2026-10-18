# Textrill — Phased Improvement Plan

> Status: **working plan, pre-decision.** Written to be acted on phase by phase.
> The user contract stays `textrill/README.md`; the framing stays `Scopev1.md`.
> This file is the *what next and in what order*, with exit criteria per phase.

Owner: the maker (sole user and maintainer for now). Version control: local
git (`origin` = `/home/vicpu/build2/forges/textrill.git`), upstream GitHub
`youtubescholar/textrill`.

---

## 0. Guiding principles (do not violate)

These hold across every phase. A phase that breaks one is redesigned, not
shipped.

1. **Pure Rust, dependency-free engine.** Engine, CLI and GUI stay Rust end to
   end — no Python, no Qt, no Node, no server runtime. The **engine crate grows
   no dependencies**: it keeps its current three (`fancy-regex`,
   `regex-syntax`, `memchr`) and nothing more. A new format the engine must read
   (e.g. a pack manifest) gets a small parser we own, not a new crate.
2. **Determinism and content preservation are the product.** No phase may
   silently change conversion output. Any intended output change is recaptured
   (`make accept-write` / `make examples-write`) and reviewed as the diff.
3. **No network, ever, in the tool.** Textrill does not connect to the internet —
   not in conversion, not in the GUI, not in the CLI core. Portfolio features
   are local and file-based; sharing is a file you copy, not a service it calls.
   Anything that fetches, if it ever exists, is a *separate* tool the user runs,
   never the engine or the app.
4. **The Perl port is a separate pipeline.** The Rust crate is the product and
   owns all forward behaviour. The Perl reference, the differential harness and
   the byte-parity modes (`--xhtml`, `--no-html5`) are isolated as a frozen,
   optional parity concern (see §8.5) — they do not define the product and do
   not constrain the Rust design.
5. **Small, auditable, test-gated.** `make verify` is the gate and means the
   same thing on every machine. New behaviour earns a named test; the properties
   (no dropped text, well-formed, deterministic, bounded) never regress.
6. **Options, not modes; presets are data.** The engine is a set of composable
   options. A "feature" is usually an option (or a curated bundle of them), not
   a new code path.
7. **The GUI is thin over the engine.** Widget metadata comes from
   `textrill::cli::SPECS`; the panel cannot drift from `--help` or offer a value
   the engine rejects. Templates stay plain HTML + fixed slots — no template
   language, no control flow, no JavaScript in output.

---

## 1. Current state (baseline, 2026-10)

- **Engine** (`textrill/src/`): layout-inference converter, 72 options / 130
  spellings, HTML5 default with `--xhtml`/`--no-html5` parity, streaming,
  sectioning/TOC/chunk, citations & glossary, 4-way encoding detection.
- **CLI**: single static musl binary; rc/@file configuration.
- **GUI** (`textrill-gui-rs/`, egui/eframe): live source→HTML editor, a
  generated options panel (`options_panel.rs`), preset bar (`presets.rs`), 4
  WCAG-AAA themes (`theme.rs`), zoom, focus mode, native dialogs, remembered
  geometry.
- **Tests**: 426 engine + 148 GUI tests, byte-golden corpus + examples,
  `proptest`, `alloctest`, link-integrity.
- **Templates today**: **five compiled-in templates** embedded with
  `include_str!` in `textrill/src/library.rs`, plus user file paths via
  `--body_template` / `--document_template`. There is **no discovery, no
  metadata, no install/remove, no sharing, and no preview** for templates — this
  is the headline gap.

### The delivery gap, stated plainly

To *add* a template today you edit `library.rs` and recompile. To *share* one
you email an `.html` file and a README. To *find* one you read the docs. That is
the "delivery system" to replace.

---

## 2. Phase overview

| Phase | Theme | Ships | Gate |
|---|---|---|---|
| 0 | Version control & local forge | repo, `.gitignore`, local bare remote | first push |
| 1 | UI/UX foundation | real layout, split panes, app bar, empty state, status bar | GUI suite green |
| 2 | Template delivery v1 | discovery, pack format, validation, `templates` commands, manager UI | pack tests + golden-stable |
| 3 | Template sharing (offline) | local catalogs, install/search/verify, integrity | catalog tests |
| 4 | Preview & editing | native subset preview, outline, problems panel, recents/session | GUI suite green |
| 5 | Option & preset ergonomics | user presets, `{{textrill:lang}}`, changed badges | engine tests + goldens |
| 6 | Distribution | Flatpak complete, AppImage releases, Windows/macOS, crates.io | artifacts build |
| 7 | Hardening & sustainability | fuzz, cargo-deny, reproducible builds, benchmarks | deny/fuzz green |

Phases 1–3 are the core ask ("better UI", "better template delivery"). 4–7
compound it. 0 is done in this session (see §3).

---

## 3. Phase 0 — Version control & local forge

**Goal.** A real repository with history and a push target, so every later phase
is a reviewable change.

**Done this session**
- `git init -b main`, local identity, `.gitignore` (excludes `target/`, `build/`,
  `*.AppImage`, vendored `_build/`, editor/OS noise).
- Initial commit of the source tree (415 files); `target/` and `build/` excluded.
- Local bare "GitHub": `/home/vicpu/build2/forges/textrill.git`, added as
  `origin`, `main` pushed and tracking.

**Remaining**
- [ ] Decide the real GitHub story: keep `origin` local and add a second remote
      `github` (`git remote add github https://github.com/youtubescholar/textrill.git`),
      or point `origin` at GitHub once `gh auth login` is done.
- [ ] Decide whether the large `research/*.tar.gz` (HandBrake 13 MB, qBittorrent
      11 MB, pandoc) belong in history or in an external store. Keep for now;
      revisit before the first public push to avoid rewriting history later.
- [ ] Add `make verify` as the documented pre-commit routine; consider a
      `pre-push` hook that runs `make verify`.

**Exit criteria.** `git log` exists; `git push` to a remote succeeds;
`make verify` runs from a clean checkout with `target/` absent.

---

## 4. Phase 1 — UI/UX foundation overhaul

**Goal.** The window stops being "a text box with controls below it" and
becomes a real editor: a resizable source/preview layout, a coherent app bar, a
purposeful empty state, and a status bar — all still one `egui::Ui` pass so
`egui_kittest` keeps driving the same code.

**Why now.** Everything else (template manager, preview, presets) needs places
to live. Build the shell first.

**Current shape to change** (`textrill-gui-rs/src/app.rs:1412` `draw`): one
vertical stack — menu bar → toolbar → preset bar → file strip → summary →
Title → Input (capped at `available_height * 0.4`) → separator → HTML. The two
panes cannot be resized, split, or shown side by side.

### Workstreams

1. **Layout engine** — replace the vertical stack with a **resizable source/HTML
   split** driven from `draw`: a bare-`Ui` horizontal layout with a custom drag
   divider between Source and Preview, and the split ratio persisted in
   `settings.rs` alongside window/zoom. (Deliberately *not*
   `TopBottomPanel`/`SidePanel`/`CentralPanel`: those attach to `Context` and
   cannot run inside `draw(&mut self, ui)`, the seam `egui_kittest` drives — see
   the resolved note in §14.)
2. **View modes** — tabs or a segmented control for `Source`, `HTML`,
   `Preview` (Phase 4 fills Preview; Phase 1 lands the slot), and `Outline`.
   Keyboard-first: `Ctrl+1/2/3`.
3. **App bar** — the title field and primary actions (Convert, Save HTML) move
   into a compact top bar; the menu bar stays for discoverability.
4. **Empty state / onboarding** — when the document is empty, show a landing
   panel: **Open file · Paste text · Try the example · Browse templates**. The
   template card links into Phase 2.
5. **Status bar** — a bottom `TopBottomPanel`: status message, inference
   summary (reuse `--report` numbers), cursor line/col, encoding, and the
   `*`/`+` dirty marks (today only in the title/strip).
6. **Design-system pass** — a modest `egui` visual refresh: spacing/rounding
   tokens, an icon set for toolbar buttons (text fallback), consistent heading
   hierarchy, and a review that all four themes in `theme.rs` stay WCAG AAA with
   the new surfaces. Add a **Compact / Comfortable** density option.
7. **Command palette** — `Ctrl/Cmd+K` fuzzy list over menu actions, recent
   files, templates, and preset switching. Pure egui; a big usability win for a
   sparse UI.

### Files
`app.rs` (layout split), new `layout.rs` / `statusbar.rs` / `command_palette.rs`,
`theme.rs` (tokens), `settings.rs` (split ratio, view mode, density),
`tests/acceptance.rs` (query the new widgets by label).

### Acceptance criteria
- Source and preview panes are independently resizable and the ratio persists.
- View modes switch with mouse and keyboard; `Ctrl+1/2/3` tested.
- Empty state appears exactly when `doc.text.is_empty()` and its buttons work.
- Status bar shows the report counts, decoding, and dirty marks.
- All four themes pass an automated contrast check; no new unsafe; `#![forbid(unsafe_code)]` intact.
- GUI suite green; no engine output change (goldens untouched).

### Risks
- `egui` API churn (`0.36`); pin and upgrade deliberately.
- Accessibility: keep every new control reachable at 320 px and 200 % zoom
  (the existing WCAG 1.4.10 discipline).

---

## 5. Phase 2 — Template delivery system, v1 (local)

**Goal.** Templates become **discoverable, packaged, validated, installable and
sharable data** — without touching the conversion contract. A shipped template
and a user pack template remain, under the hood, exactly a
`--template_library` name or a `--body_template` / `--document_template` path.

**Why now.** This is the explicit request and the biggest product gap. It needs
the Phase 1 shell for its UI.

### 5.1 Discovery (search path)

Define a precedence-ordered set of template roots; the first match wins and
collisions are reported, never silent:

1. `--template_dir DIR` (repeatable) — explicit, highest priority.
2. `./.textrill/templates/` — project-local, committable.
3. `$XDG_DATA_HOME/textrill/templates` (`~/.local/share/textrill/templates`).
4. `$XDG_DATA_DIRS/textrill/templates` (`/usr/local/share/…`, `/usr/share/…`).
5. **Compiled-in shipped set** (`library.rs`) — always present, lowest priority.

A template is addressed by `pack-id:template-name` when names collide; a bare
name resolves only if unambiguous.

### 5.2 Pack format (a directory, or an archive of one)

```
academic-book/
  template.toml        # manifest (required)
  book.html            # one or more templates
  preview.png          # optional card thumbnail
  style.css            # optional, referenced by the template
  README.md            # optional
  LICENSE              # required if manifest license != shipped set
```

Manifest (`template.toml`). The engine stays dependency-free, so we ship a
**small reader we own** for a documented **strict subset of TOML**: top-level
`key = value`, arrays of tables (`[[templates]]`, `[[templates.vars]]`), and
string / integer / boolean / string-array values. Anything outside the subset is
a **hard error naming the line**, never a silent misread — so a manifest written
as full TOML fails loudly rather than being half-understood:

```toml
schema = 1
id = "org.example.academic-book"     # reverse-DNS, globally unique
name = "Academic Book"
version = "1.2.0"                    # semver
description = "A print-ready academic book skeleton."
authors = ["A. Scholar <a@example.org>"]
license = "CC0-1.0"
tags = ["book", "print", "academic"]

[[templates]]
file = "book.html"
model = "document"                   # "document" | "body"
slots = ["content", "toc", "head", "header", "citations", "glossary"]
preview = "preview.png"

[[templates.vars]]
name = "author"
required = false
```

### 5.3 Validation (fail loud, before output)

A pack validator, reusing `template::validate` (`textrill/src/template.rs:66`):
- manifest parses; `schema` known; `id`/`version` well-formed;
- every declared `slots` entry is a known slot; the file actually contains
  `{{textrill:content}}`;
- declared `vars` match the `{{textrill:var:*}}` used;
- `model` ∈ {body, document}; files exist; no path escapes the pack root; no
  symlinks out of tree; no executables;
- license recorded (a pack with no license is refused, matching the corpus rule
  in `CONTRIBUTING.md`).

Validation runs on load and again before install; the GUI shows the first
problem per template inline.

### 5.4 CLI surface

Keep the option grammar; add two options and one additive command group.

- `--template_dir DIR` (repeatable): add a search root (option, so it can live
  in `.textrillrc`).
- `--list_templates`: print discovered templates — `id · name · model · source`
  (shipped/local/installed). Machine-readable `--list_templates=json` for tests.
- **`textrill templates <verb>`** — an additive subcommand detected before
  option parsing so all existing invocations are unchanged:
  - `list [--tag T] [--json]`
  - `inspect <id>[:<template>]` — manifest, slots, vars, source path
  - `new <name>` — scaffold a pack (manifest + minimal valid template)
  - `install <PATH>` — from a **pack directory** or a **deterministic
    uncompressed `.tar`** on disk (no URL; no `.tar.gz`/`.zip` in core — see the
    archive decision below)
  - `remove <id>` — user-installed packs only
  - `pack <dir>` / `unpack <archive>` — deterministic **uncompressed `.tar`**
    (sorted entries, fixed mtime); `unpack` reads the same
  - `validate <dir|archive>` — the 5.3 checks

**Archive decision (resolved, §14).** Core ships **directory packs plus a
deterministic uncompressed `.tar`** only. gzip/zip need DEFLATE, which a
dependency-free engine would have to hand-roll — a large, security-sensitive
surface. Users (or the optional companion, §6) unpack `.tar.gz`/`.zip` with
`tar`/`unzip` first. `pack`/`unpack` stay byte-exact round-trips without
pulling in an inflate implementation.

`--template_library NAME` now resolves across discovered packs as well as the
shipped set; the shipped five keep working unchanged.

### 5.5 GUI Template Manager

A new window (`templates_window.rs`), reached from the app bar and the empty
state:

- **Catalog** — card grid/list: name, description, tags, model badge,
  source badge (shipped / local / installed), version, and a thumbnail.
  Search box + tag filter + sort; keyboard navigable.
- **Use** — selects the template (`template_library` for a document/whole-page
  model, or the equivalent body/document option), live-updating the preview.
- **Duplicate** — copy a shipped/pack template into
  `~/.local/share/textrill/templates/<mine>/` for editing.
- **New / Edit** — open a template in an editor with **slot autocomplete** and
  live validation (missing `content`, unknown slot, undeclared var → inline red
  with a fix hint).
- **Reveal on disk**, **Export pack** (deterministic archive), **Remove** (user
  packs only).
- **Install** — drop a pack folder/archive (drag-and-drop or a file picker);
  validation result shown before it is accepted. No URL field: the app does not
  fetch.

### Files
New `textrill/src/pack/{mod,manifest,catalog,archive}.rs`,
`textrill/src/library.rs` (merge discovered + shipped), `textrill/src/cli.rs`
(new options), a `templates` command entry in `main.rs`, GUI
`templates_window.rs`, `options_panel.rs` (Template source radio gains
"Discovered"), `presets.rs` (presets may reference pack ids). Tests:
`tests/packtest.rs`, `tests/templatetest.rs` extensions, GUI acceptance.

### Acceptance criteria
- A pack dropped into `./.textrill/templates/` is discovered, listed, validated
  and selectable, with no flags.
- `--template_dir` composes and beats the default roots; collisions are named.
- Installing a pack from a directory and from a deterministic archive both work
  offline; a pack with a missing `content` slot is refused before any output.
- The shipped five still convert byte-identically (goldens untouched).
- Pack round-trip: `pack` then `unpack` reproduces the tree byte-for-byte
  (deterministic archive test).
- GUI manager lists, filters, previews and applies a template end to end.

### Risks / decisions
- **Decided: no new dependency.** The manifest reader is ours and covers a
  documented subset; unsupported syntax is a hard error, not a silent guess.
- Naming: reject all bare-name collisions rather than guessing a priority the
  user cannot see.
- Archive extraction must defend against traversal/symlink/executable payloads.

---

## 6. Phase 3 — Template sharing (offline, file-based)

**Goal.** People can share whole *libraries* of template packs without emailing
one `.html` at a time — while Textrill itself **never touches the network**. The
tool reads catalogs the user already has on disk; how those files arrived (git,
a browser, a shared drive, email) is the user's business, not the tool's.

**Why this shape.** "No internet in the tool" rules out the usual
fetch-a-registry design. The sharing model is therefore *git-native and
file-based*: a catalog is a directory of packs plus an index file; the user
obtains it however they move files, and the tool installs from it locally.

### Workstreams

1. **Catalog format** — a `catalog` is a directory with a `catalog.toml` index
   (parsed by our own reader, §5.2) listing packs: `id`, all `version`s,
   `sha256` per archive, `min_textrill`, `license`, `tags`, `description`. No
   server, no JSON dependency.
2. **Local sources** — `--catalog DIR` (repeatable, option so it can live in
   `.textrillrc`) registers a catalog directory. A canonical layout
   (`textrill-templates/` git repo with `catalog.toml` + `packs/`) is documented
   so a community repo is just a clone.
3. **Commands** — `templates search <query> --catalog DIR`,
   `templates install <id>[@ver] --catalog DIR` (resolves an archive *already on
   disk*), `templates verify <id>` (hash check), `templates sources` (list
   registered catalogs), `templates update` (re-scan catalogs, no fetching).
4. **Integrity** — SHA-256 of each archive verified against `catalog.toml`;
   optional detached signature; a mismatch is a hard error. Nothing is installed
   unverified by default.
5. **Versioning** — semver; a pack declares `min_textrill`; incompatible packs
   are listed with a reason, never crash.
6. **Self-hosting / a "public registry"** — if a central library is wanted, it
   is simply a public git repo of packs and a generated `catalog.toml`. Users
   `git clone` it (or use the optional small companion below) and point
   `--catalog` at it. The repo is the registry; no service is required.

### Optional companion (out of core, not in the engine or the GUI)

If users want "fetch", provide a **separate**, tiny helper — a shell script or a
small `git`-based tool (`textrill-catalog`) — that clones/pulls a catalog repo.
It is deliberately *not* part of the textrill binary and *not* wired into the
app, so the product keeps its no-network property. Decision deferred; the
in-core story is complete without it.

### Files
`textrill/src/pack/catalog.rs` (index read/verify), `textrill/src/pack/archive.rs`,
`textrill/src/cli.rs` (new options), docs `docs/templates.md` (catalog layout and
how to publish one via git), GUI "Catalogs" tab in `templates_window.rs` (lists
local catalogs, installs from them).

### Acceptance criteria
- With a catalog dir on disk, `search`/`install`/`verify` work fully offline.
- Tampering with an archive fails `verify` with a hash mismatch.
- `install` refuses a pack whose `min_textrill` is unmet, naming the reason.
- **No code path in the engine or GUI performs network I/O** (auditable: no
  HTTP/TLS/socket dependency anywhere in the tree).

### Risks
- Sharing without fetching UX is less convenient; the companion helper and clear
  docs are the mitigation.
- Key management for signing is a policy decision; ship hash-only first,
  signatures opt-in.

---

## 7. Phase 4 — Preview & editing experience

**Goal.** Answer "what does this actually look like?" and "where is the
problem?" inside the app.

### Workstreams
1. **Rendered preview — a native egui subset renderer (pure Rust, no deps).**
   egui cannot render HTML, and we will not embed a browser engine (`wry`/
   WebKitGTK is a large C dependency and against the GUI's deliberate no-GTK
   stance) nor add a network path. Instead, draw an **honest subset** of the
   produced HTML natively: headings, paragraphs, lists (ul/ol/dl), `strong`/
   `em`/`code`, blockquotes, `<pre>`, tables, links, and the `<header>`/TOC
   landmarks — using the engine's own structure, themed by `theme.rs`. It is not
   pixel-identical to a browser and says so; it is a fast, offline, deterministic
   *preview*. A **local** escape hatch (`Open in browser`) writes the exact
   bytes to a temp file and hands them to the OS handler — that is a local
   program launch, **not** a network request.
2. **Outline.** Draw the document structure (headings + anchors + TOC) as an
   `egui` tree from the engine's section data — natively rendered, no HTML.
   Clicking scrolls the source to the heading.
3. **Diff view (preview persistence).** A "rendered vs. previous" toggle to see
   what a settings change did to the output.
4. **Problems panel.** Parse engine errors (they already name `file:line` for
   option files, and issue lines for content) into a list; clicking a problem
   jumps to the location. "No output was written because…" becomes actionable.
5. **Recents & session.** A recent-files list (`File` menu + command palette),
   and optional restore of the last document and options on launch (extends the
   existing `settings.rs` / `window_state.rs`).

### Acceptance criteria
- The native preview reflects the same structure as the produced HTML (headings,
  lists, emphasis, links, tables) for every example document.
- `Open in browser` writes the exact bytes that "Save HTML" would write and
  performs no network I/O.
- Outline entries match the produced headings and anchors; clicking navigates.
- A deliberately broken document lists its problems and no output is produced.
- Session restore is opt-in and cannot clobber an unsaved buffer silently.

---

## 8. Phase 5 — Option & preset ergonomics

**Goal.** Make the engine's real surface (72 options) pleasant without hiding
it.

### Workstreams
1. **User presets.** Promote `presets.rs`'s compiled-in `PRESETS` to also read
   `$XDG_CONFIG_HOME/textrill/presets.toml`: a named bundle of options (and a
   template pack id), surfaced as GUI buttons and as a CLI `--preset NAME`
   (expanded early in option precedence, below the command line and above rc).
   A preset is still *only options*; a "Modified" marker shows divergence.
2. **`{{textrill:lang}}` slot.** Closes the known gap (`Scopev1.md` §2.2): make
   `--lang` reach templates so shipped templates stop hard-coding
   `<html lang="en">`. Update the shipped five to use the slot (golden
   recapture, reviewed).
3. **Var defaults.** Pack manifests may give `vars` a `default`, so a pack
   template works with zero `--var` while remaining overridable.
4. **Options panel polish.** A "changed from default" badge and a per-option
   reset; grouping already exists (`CATEGORIES`); add search over *values* and a
   "show only changed" filter. Presets become the curated entry point.
5. **Engine modularisation (behavior-preserving).** `convert.rs` is **4,866
   lines in one file** today. It holds, in order: encoding/decode plus the
   single-byte tables (LATIN1/CP1252/CP1251/CP1253/KOI8R), BOM/UTF-16 sniffing
   and CR chopping; the action bitflags/constants; the `Converter` struct and
   its ~2,900-line impl (tag emission, hrules, mail headers/quotes, paragraphs,
   lists, tables, preformat, headings, caps/delimiters, links, template/page
   assembly, TOC/pager, chunking, streaming); free helpers (regex predicates,
   table-shape detectors, byte slicing); the streaming `ParagraphReader` and
   `split_blank_lines`; a `ReplAll` regex trait; and ~940 lines of in-file tests.
   Because we are **free to drift from Perl**, take the *clean seams, not the
   Perl function names*. Proposed layout:

   | New module | Moves |
   |---|---|
   | `convert/decode.rs` | encoding detection, single-byte tables, BOM/UTF-16 sniff, CR chop |
   | `convert/tags.rs` | action flags, `get_tag`/`close_tag`/`tagline` |
   | `convert/paragraph.rs` | `paragraph`, `shortline`, mail header/quote |
   | `convert/lists.rs` | `listprefix`/`startlist`/`endlist`/`continuelist`/`liststuff` |
   | `convert/tables.rs` | every `table_*` / `is_*table` |
   | `convert/preformat.rs` | `is_preformatted`, `preformat`, explicit markers |
   | `convert/headings.rs` | `heading`, `anchor_*`, `is_heading`, `custom_heading`, `unhyphenate` |
   | `convert/inline.rs` | `caps`, `do_delim`, `delim_replace`, `delim_loop`, `apply_links` |
   | `convert/pages.rs` | `apply_template`, `build_head`, chunking, TOC/pager, `finish_*` |
   | `convert/stream.rs` | `ParagraphReader`, `paragraph_records`, `split_blank_lines` |
   | `convert/tests.rs` | the in-file tests |

   Zero output change; the byte-goldens are the proof. Do this only after
   Phase 1 so GUI churn has landed.

### Acceptance criteria
- A user preset applied via GUI and via `--preset` produce identical options.
- `--lang` flows into shipped and pack templates; goldens recaptured once,
  reviewed, and the `{{textrill:lang}}` behaviour tested.
- `convert.rs` split changes no golden byte and no test.
- Options panel changed-badges match `cli::get_opt` vs. defaults exactly.

### 8.5 Perl parity as a separate pipeline (new)

**Decision.** The Rust crate is the product. The Perl `HTML::TextToHTML`
heritage stops shaping the architecture. Rather than deleting it (it is a real
asset for auditing), **isolate** it:

- Move `ref/` (the reference tarball), the stubs, and the differential harness
  (`make ref`, `make diff`, `make scale`'s Perl timing) into a clearly separated
  concern — a top-level `parity/` directory with its own README, or a separate
  `textrill-parity` crate — with its **own non-gating CI job**.
- Recast `--xhtml` / `--no-html5` as a **frozen compatibility layer**: document
  them as "reproduce the Perl reference's skeleton, for porting and audit", not
  as part of the product's identity. They keep their byte-parity tests; they no
  longer constrain new features.
- The main `make verify` gate stays Perl-free (it already is). Parity work is
  opt-in and can lag without blocking the Rust roadmap.
- Long-term option: expose parity as a Cargo feature (`parity`) so the default
  build is lean and the reference behaviour is available on demand.

**Acceptance criteria**
- No Rust roadmap item depends on Perl or on the `ref/` tree.
- `make verify` passes with `parity/` absent.
- `--xhtml`/`--no-html5` output remains byte-identical to today (goldens pin it),
  but their tests live in the parity pipeline.

---

## 9. Phase 6 — Distribution & packaging

**Goal.** A thing people can install without a toolchain.

### Workstreams
1. **Flatpak, finished.** Add the missing desktop metadata the engine README
   already flags: `.desktop`, AppStream metainfo, and an icon (the AppImage tree
   already has an SVG/PNG to reuse); wire them into
   `packaging/io.github.youtubescholar.Textrill.yml`; submit to Flathub.
2. **AppImage releases.** The build script exists
   (`packaging/appimage/build.sh`, `make appimage`); add a release job that
   produces the AppImage and uploads it.
3. **Desktop distro packages.** `cargo-deb` / `cargo-generate-rpm` (or a
   Debian `debian/` dir) for the CLI and GUI.
4. **Windows & macOS.** `eframe`/`rfd` already support both; add a CI matrix
   that builds the GUI and CLI, with signing/notarisation as a follow-up.
5. **crates.io.** Publish the engine crate (the CLI is already installable with
   `cargo install --git`); a library release makes embedding trivial.
6. **CI.** A GitHub Actions workflow: `make verify` on Linux (with the musl
   acceptance), plus release artifacts per tag. Keep `make verify` the single
   source of truth so CI cannot drift from local.

### Acceptance criteria
- A Flathub-style install runs the GUI and CLI with the same output as the musl
  binary.
- Tagged release produces AppImage + Flatpak + deb/rpm + Windows/macOS binaries.
- `cargo install textrill` works from crates.io.

---

## 10. Phase 7 — Hardening & sustainability

**Goal.** Keep the trust story true as the surface grows.

### Workstreams
1. **Supply chain.** `cargo-deny` (licenses, advisories, duplicate bans) and
   `cargo-audit` in `make verify` (or a separate `make deny` gate). Record an
   SBOM per release.
2. **Fuzzing.** `cargo-fuzz` targets for the converter and for pack/archive
   parsing (the new untrusted-input surface from Phase 2/3).
3. **Reproducible builds.** Pin the toolchain, verify byte-identical release
   binaries across two builds.
4. **Benchmarks.** `criterion` for large-document conversion; keep the
   `alloctest` resource guarantees as hard gates, not just measurements.
5. **Docs.** A short `docs/templates.md` (pack authoring + catalogs), an issue
   template, and a release checklist. Keep the "small docs" house rule.

### Acceptance criteria
- `make verify` (or `make deny`) fails on a known-bad dependency.
- A fuzz corpus runs clean for a fixed budget in CI.
- Two clean builds of a tag are byte-identical.

---

## 11. Sequencing & dependencies

```
Phase 0 (done) ──► Phase 1 (UI shell) ──┬─► Phase 2 (template delivery v1) ──► Phase 3 (sharing/catalog)
                                        ├─► Phase 4 (preview/editing)
                                        └─► Phase 5 (options/presets) ──► Phase 6 (distribution) ──► Phase 7 (hardening)
```

- Phases 2 and 4 both need Phase 1's shell.
- Phase 3 depends on Phase 2's pack/manifest format being frozen.
- Phase 5's `convert.rs` split should wait until GUI churn pauses.
- Phase 6 can start in parallel once the GUI stabilises.

---

## 12. Metrics of success

- **Delivery:** a new template is shared, installed and selected with no
  recompile and no source edit; `templates list` finds it.
- **UI:** a first-time user reaches a converted document via the empty state
  without opening the options panel; the source/preview split is resizable and
  remembered.
- **Trust:** every guarantee in `Scopev1.md` §2.1 still holds and is gated; no
  golden byte changes except via a reviewed recapture.
- **Reach:** installable on Linux (Flatpak/AppImage/deb) and, later, Windows and
  macOS.

---

## 13. Explicit non-goals (unchanged)

Still out of scope, restated so phases do not drift: no markup language, no
WYSIWYG page layout, no OCR, no HTML→text reverse converter, no document
manager/CMS, no template *language* (loops/conditionals/includes), no AI/model,
no network anywhere in the tool (not just the conversion path). See
`Scopev1.md` §1.4, §6.2.

---

## 14. Open questions (need the maker's call)

**Resolved**

- **Engine dependencies** — *dependency-free.* The engine keeps its current
  three crates and grows nothing; the pack-manifest reader is ours (a documented
  strict subset, hard-error on anything outside it). *(was Q3)*
- **Rendered preview** — *native egui subset renderer*, pure Rust, no webview,
  no network; `Open in browser` is a local escape hatch only. *(was Q4)*
- **`convert.rs` split** — *yes, do it*, with the module map in §8, and drift
  from Perl's internal shape is welcome. *(was Q5)*
- **Perl parity** — *separate pipeline* (§8.5); the Rust crate is the product
  and no roadmap item depends on Perl. *(new)*
- **Template archives** — *directory packs + deterministic uncompressed `.tar`
  only* in core; no `.tar.gz`/`.zip` (DEFLATE) in the dependency-free engine.
  Users or the optional companion unpack compressed archives first. *(new)*
- **Phase 1 layout** — a *bare-`Ui` horizontal splitter with a drag handle*,
  not egui `TopBottomPanel`/`SidePanel`/`CentralPanel`: panels attach to
  `Context` and cannot live inside `draw(&mut self, ui)`, which is the seam all
  58 acceptance tests drive via `egui_kittest`. The roadmap's original "panels"
  wording is corrected; the test harness is not reworked. *(new)*

**Still open**

1. **Local forge semantics** — is `/home/vicpu/build2/forges/textrill.git` the
   intended "local GitHub", or should I stand up a local Forgejo/Gitea and/or
   wire the real GitHub remote now?
2. **Sharing authority** — should there be a single canonical
   `textrill-templates` catalog repo, or catalog-agnostic from the start
   (self-hosting first-class)? And is the optional out-of-core fetching helper
   (§6) wanted at all?
3. **Large research tarballs** — keep the HandBrake/qBittorrent/pandoc archives
   in git history or move them out before the first public push?

---

### Revision log

- v1 — first phased plan. Baseline recorded; phases 0–7; template delivery
  (Phase 2) and registry (Phase 3) specified against the current
  `library.rs`/`template.rs` code; UI overhaul (Phase 1) specified against the
  current `app.rs` stack.
- v2 — recorded the maker's decisions: engine stays dependency-free (hand-rolled
  subset manifest reader); the tool **never uses the network** (Phase 3 recast
  as offline, file/git-based catalog sharing; preview becomes a native egui
  subset renderer); `convert.rs` modularisation specified with a real module map;
  Perl parity isolated as a separate, non-gating pipeline (§8.5).
