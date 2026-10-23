# Single entry point for the project's checks. Everything is one command:
#
#   make verify        fmt-check, clippy, Rust tests (engine and GUI),
#                      acceptance against the recorded outputs, real-document
#                      recorded outputs
#   make fmt           what is safe to apply automatically: cargo fmt
#   make test          just the Rust tests
#   make accept        the behavioural gate: output against the recorded goldens
#   make accept-write  recapture the corpus goldens (routine; review the diff)
#   make examples      real-document recorded outputs
#   make examples-write recapture the example goldens (routine; review the diff)
#   make corpus        the Perl differential corpus (needs `make ref`)
#   make diff          the Perl differential corpus, as a non-gating cross-check
#   make scale         the 1 MB paragraph probes, timed against Perl
#   make appimage      a local (non-portable) AppImage of the GUI and CLI
#   make metadata      validate the desktop file and AppStream metainfo
#   make deb / rpm     native packages (needs cargo-deb / cargo-generate-rpm)
#   make publish-crate publish the engine crate (needs a crates.io token)
#
# `make verify` needs no Perl, and passes with `parity/` absent: everything
# Perl-related -- the reference tree, the stubs, the tracked tarballs and the
# differential runner -- lives in the non-gating `parity/` directory (roadmap
# §8.5) and is reached only through the `corpus`/`diff`/`scale`/`ref` wrappers
# below. Individual targets build first and pass the binary path explicitly
# -- exporting MINE here means no invocation can pick up a stale binary that was
# not just built, which is how the harness once reported a false green.

CARGO  ?= cargo
PYTHON ?= python3
ROOT  := $(CURDIR)
RS    := $(ROOT)/textrill
GUI_RS := $(ROOT)/textrill-gui-rs

RELEASE_BIN := $(RS)/target/release/textrill
export MINE := $(RELEASE_BIN)

.PHONY: all verify build gui run-gui fmt fmt-check clippy test test-gui-rs proptest alloctest accept accept-write corpus diff scale ref ref-large musl corpus-musl accept-musl examples examples-write appimage metadata deb rpm packages publish-crate cargo-sources cargo-sources-check clean distclean

all: verify

# The whole gate: lints, tests, properties, and both acceptance runners. No
# Perl anywhere in it.
verify: fmt-check clippy test test-gui-rs proptest alloctest accept examples
	@echo
	@echo "verify: OK"

# --- build -------------------------------------------------------------------

build:
	cd $(RS) && $(CARGO) build --release

# The GUI is a separate crate with its own ~300-crate eframe tree. `make build`
# only builds the CLI (what `verify` needs), so these build and run the GUI on
# demand. `run-gui` execs the freshly built binary, so it is never stale.
gui:
	cd $(GUI_RS) && $(CARGO) build --release

run-gui: gui
	$(GUI_RS)/target/release/textrill-gui

# --- static musl build -------------------------------------------------------

# The CLI's distribution claim is a single static binary that runs on any Linux,
# including Alpine and other musl distros where a glibc build does not. That
# claim is only as good as a target that actually builds it -- and a target that
# builds but silently produces different bytes would look green. So `musl`
# builds and asserts the result really is static; `accept-musl` then runs the
# reference-free acceptance against the static binary. `corpus-musl` does the
# same against the Perl reference, for a parity cross-check on the shipping
# binary. `rustup target add $(MUSL_TARGET)` is the only prerequisite: this
# crate has no C dependencies, so the target's self-contained musl links it
# without musl-gcc.
#
# Deliberately not part of `verify`: most dev machines have no musl target
# installed, and a gate that fails for a missing toolchain rather than a defect
# is the kind people learn to skip. CI runs it (`.github/workflows/ci.yml`).
MUSL_TARGET ?= x86_64-unknown-linux-musl
MUSL_BIN    := $(RS)/target/$(MUSL_TARGET)/release/textrill

musl:
	cd $(RS) && $(CARGO) build --release --target $(MUSL_TARGET)
	@file "$(MUSL_BIN)" | grep -q 'static' \
	  || { echo "ERROR: $(MUSL_BIN) is not a static binary" >&2; exit 1; }

# MINE is passed on the command line, not exported, so it overrides the glibc
# path this Makefile exports above.
accept-musl: musl
	cd $(RS) && MINE=$(MUSL_BIN) ./tests/corpus/accept.sh

# The parity cross-check on the static binary. Optional, like `make diff`: it
# needs the Perl reference and is not part of any gate.
corpus-musl: musl
	$(MAKE) -C parity corpus MINE=$(MUSL_BIN)

# --- rust --------------------------------------------------------------------

fmt:
	cd $(RS) && $(CARGO) fmt
	cd $(GUI_RS) && $(CARGO) fmt

fmt-check:
	cd $(RS) && $(CARGO) fmt --check
	cd $(GUI_RS) && $(CARGO) fmt --check

# Fails, not warns. This target deliberately only warned, on the reasoning that
# a lint gate that is always red gets ignored. That reasoning was right about
# the 74 warnings it was written next to and wrong about what to do with them:
# 09d13d9 cleared them, and a gate that reports without stopping anyone is not a
# gate. The four remaining `#[allow]`s are reviewed exceptions with the reason
# next to them, not a backlog.
# The native GUI crate is linted in debug, not release: it pulls the ~300-crate
# eframe tree and lints do not depend on optimisation. The engine stays release.
clippy: build
	cd $(RS) && $(CARGO) clippy --release --all-targets -- -D warnings
	cd $(GUI_RS) && $(CARGO) clippy --all-targets -- -D warnings

test: build
	cd $(RS) && $(CARGO) test --release

# The native GUI crate. The tests are headless (`egui_kittest` drives the
# widget tree directly), so this needs no display.
test-gui-rs:
	cd $(GUI_RS) && $(CARGO) test

# --- properties and resource bounds ------------------------------------------

# Guarantees the tool owes its user: no data loss, well-formed output,
# determinism, and no panic on hostile input. The reference is never invoked
# here, which is the point.
proptest: build
	cd $(RS) && $(PYTHON) tests/proptest.py

# Allocation budgets via a counting global allocator. Byte-comparison cannot
# see a resource defect. Known-open budgets are printed, not silenced; see
# KNOWN_OPEN in the test. --test-threads=1 is belt-and-braces: the test
# serialises itself, so `cargo test --release` runs it green at any thread
# count.
alloctest: build
	cd $(RS) && $(CARGO) test --release --test alloctest -- --nocapture --test-threads=1

# --- Perl parity (delegates to parity/; see parity/README.md) ----------------
# The differential harness itself is not in this Makefile any more: the
# reference, the stubs and the Perl runner live under parity/, outside every
# gate. These are thin wrappers so the familiar command names keep working.

corpus:
	$(MAKE) -C parity corpus

# --- acceptance ---------------------------------------------------------------

# The behavioural gate: every corpus case converted and compared against the
# recorded goldens, plus the upstream author goldens as an independent check.
# No Perl, no `make ref`.
#
# The goldens are a record of approved behaviour, not a wall. To change what
# the tool outputs: change the code, run `make accept-write`, and review the
# git diff of the goldens you just recaptured -- the diff is the review. The
# properties that must never regress (no dropped text, well-formed output,
# determinism, resource bounds) are gated separately by `proptest`/`alloctest`
# and keep passing whatever the bytes say.
accept: build
	cd $(RS) && MINE=$(RELEASE_BIN) ./tests/corpus/accept.sh

# Recapture the corpus goldens. Routine. Commit the result with the change
# that motivated it so the two cannot separate.
.PHONY: accept-write
accept-write: build
	cd $(RS) && MINE=$(RELEASE_BIN) ./tests/corpus/accept.sh --write

# --- real-document recorded outputs ------------------------------------------
# The corpus uses short synthetic inputs; `examples/` holds documents a person
# actually wrote -- plain prose, nothing that announces its structure (homer.txt
# is 37 KB of Odyssey running prose whose section titles are bare capitals).
# tests/examples.py re-converts each one and compares byte-for-byte against
# tests/golden/examples/<name>.html, so a regression in layout inference shows
# up as a byte diff, not as a shrug.
#
# The counts printed are the tool's own `--report`, which `tests/reporttest.rs`
# keeps honest by recounting the output with a second implementation.
examples: build
	cd $(RS) && MINE=$(RELEASE_BIN) $(PYTHON) tests/examples.py

# Recapture the example goldens. Same policy as `accept-write`: routine, and
# reviewed as the git diff of the recapture.
.PHONY: examples-write
examples-write: build
	cd $(RS) && MINE=$(RELEASE_BIN) $(PYTHON) tests/examples.py --write

# The parity oracle, on demand. `make verify` never invokes Perl; this target
# re-runs the differential corpus and the author goldens against the reference
# as a non-gating cross-check. Nothing depends on it.
diff:
	$(MAKE) -C parity diff

# --- the reference checkout (parity/) ----------------------------------------

# The reference tree, the stubs and the tracked upstream archives all live in
# parity/ now; these wrappers keep the documented command names working.
ref:
	$(MAKE) -C parity ref

ref-large:
	$(MAKE) -C parity ref-large
# Remove the derived reference. `make clean` leaves it alone on purpose: it is
# 1 MB of extracted Perl, it takes one `make ref` to rebuild, and deleting it on
# every clean is a good way to make the gate mysteriously unavailable.
# --- Flatpak packaging (see packaging/README.md) ---------------------------------

# Generates one packaging/cargo-sources-<crate>.json per crate, from that
# crate's Cargo.lock. The manifest's two modules each consume their own file:
# the two lockfiles pin different versions of shared crates (e.g. bit-set), so
# a single shared file could not satisfy both `--locked` builds. A checked-in
# snapshot keeps an offline Flatpak build free of Python, and
# `cargo-sources-check` fails when a lockfile change left it stale. Requires
# flatpak-cargo-generator (pip) on PATH; see packaging/README.md.
CARGO_SOURCES := packaging/cargo-sources-textrill.json \
                 packaging/cargo-sources-textrill-gui.json
APP_ID := io.github.youtubescholar.Textrill

.PHONY: cargo-sources
cargo-sources:
	@command -v flatpak-cargo-generator >/dev/null 2>&1 || \
	  { echo "ERROR: flatpak-cargo-generator is not installed."; \
	    echo "       pip install flatpak-cargo-generator"; \
	    echo "       Or vendor instead: cargo vendor (see packaging/README.md)"; exit 1; }
	flatpak-cargo-generator -o packaging/cargo-sources-textrill.json textrill/Cargo.lock
	flatpak-cargo-generator -o packaging/cargo-sources-textrill-gui.json textrill-gui-rs/Cargo.lock
	@echo "wrote $(CARGO_SOURCES)"

# The generated files are checked in so a Flatpak build needs no Python. Run
# this after a lockfile change; the release workflow regenerates them again.
cargo-sources-check: cargo-sources
	@git diff --exit-code -- $(CARGO_SOURCES) \
	  || { echo "ERROR: $(CARGO_SOURCES) were stale; commit the regenerated files" >&2; exit 1; }

# --- desktop metadata ---------------------------------------------------------

# Not part of `verify`: these live in desktop-specific packages, and a gate
# that fails for a missing tool is the kind people learn to skip. CI and the
# release preflight install them and run this target.
metadata:
	desktop-file-validate packaging/appimage/$(APP_ID).desktop
	appstreamcli validate --no-net --strict packaging/$(APP_ID).metainfo.xml
	@test -f packaging/appimage/$(APP_ID).png || { echo "ERROR: missing PNG icon" >&2; exit 1; }
	@test -f packaging/appimage/$(APP_ID).svg || { echo "ERROR: missing SVG icon" >&2; exit 1; }

# --- native distro packages ---------------------------------------------------

# cargo-deb and cargo-generate-rpm read their metadata from the crate Cargo.toml
# and write under each crate's target/. `make packages` is the local equivalent
# of the release workflow's Linux package step, not part of `verify`.
deb: build gui
	cd $(RS) && $(CARGO) deb
	cd $(GUI_RS) && $(CARGO) deb

rpm: build gui
	cd $(RS) && $(CARGO) generate-rpm
	cd $(GUI_RS) && $(CARGO) generate-rpm

packages: deb rpm

# Manual and irreversible: publishing a version is one-way. The release
# workflow does this on a tag only when CARGO_REGISTRY_TOKEN is configured.
publish-crate:
	cd $(RS) && $(CARGO) publish --locked

# --- AppImage (see packaging/appimage/build.sh) -------------------------------

# A local, self-contained bundle of the GUI with the CLI engine beside it. Not a
# substitute for the Flatpak: the AppImage inherits this host's glibc, so it
# runs on distributions at least as new as the build host. The script builds the
# release binaries, assembles the AppDir, and fetches appimagetool once; the
# artifact lands under build/ (gitignored). Deliberately not part of `verify`:
# it needs network on first run and produces an artifact, not a test result.
appimage:
	packaging/appimage/build.sh

distclean: clean
	$(MAKE) -C parity distclean

# --- scale probes (parity/) --------------------------------------------------

scale:
	$(MAKE) -C parity scale

clean:
	cd $(RS) && $(CARGO) clean
