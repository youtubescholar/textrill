# Single entry point for the project's checks. Everything is one command:
#
#   make verify        fmt-check, clippy, Rust tests (engine and GUI),
#                      acceptance against the recorded outputs, real-document
#                      recorded outputs
#   make fix           what is safe to apply automatically: cargo fmt
#   make test          just the Rust tests
#   make accept        the behavioural gate: output against the recorded goldens
#   make accept-write  recapture the corpus goldens (routine; review the diff)
#   make examples      real-document recorded outputs
#   make examples-write recapture the example goldens (routine; review the diff)
#   make corpus        the Perl differential corpus (needs `make ref`)
#   make diff          the Perl differential corpus, as a non-gating cross-check
#   make scale         the 1 MB paragraph probes, timed against Perl
#
# `make verify` needs no Perl: the differential is the non-gating `make diff`,
# on demand. Individual targets build first and pass the binary path explicitly
# -- exporting MINE here means no invocation can pick up a stale binary that was
# not just built, which is how the harness once reported a false green.

TIME   ?= /usr/bin/time -f "  %es"
CARGO  ?= cargo
PYTHON ?= python3
ROOT  := $(CURDIR)
RS    := $(ROOT)/textrill
GUI_RS := $(ROOT)/textrill-gui-rs

RELEASE_BIN := $(RS)/target/release/textrill
REFDIR      ?= $(ROOT)/ref/txt2html-3.0
STUBS       ?= $(ROOT)/ref/stubs

export PERL5LIB := $(STUBS):$(REFDIR)/lib
export MINE     := $(RELEASE_BIN)

.PHONY: all verify build fmt fmt-check clippy test test-gui-rs proptest alloctest accept accept-write corpus diff scale musl corpus-musl accept-musl examples examples-write clean

all: verify

# The whole gate: lints, tests, properties, and both acceptance runners. No
# Perl anywhere in it.
verify: fmt-check clippy test test-gui-rs proptest alloctest accept examples
	@echo
	@echo "verify: OK"

# --- build -------------------------------------------------------------------

build:
	cd $(RS) && $(CARGO) build --release

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
	cd $(RS) && MINE=$(MUSL_BIN) ./tests/corpus/run.sh

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

# --- differential corpus -----------------------------------------------------

corpus: build
	cd $(RS) && ./tests/corpus/run.sh

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
# as a non-gating cross-check. Nothing depends on it. `ref` is a phony rebuild
# so a fresh clone can run this without a separate `make ref`.
.PHONY: diff
diff: ref corpus
	@echo
	@echo "diff: OK (non-gating parity cross-check)"

# --- the reference checkout ---------------------------------------------------

# The differential needs the Perl original, which is derived rather than
# tracked (`ref/` is gitignored). This target extracts the tracked tarball and
# installs the one stub module the reference `use`s at load time. Idempotent
# and offline: no network, no CPAN, no package installs -- the reference needs
# nothing but a core Perl (Getopt::ArgvFile is an optional `eval require`;
# YAML::Syck is `use`d but never called).
#
# The stub is a tracked file copied into place rather than a heredoc here, so
# its provenance and reasoning live in a file you can read and diff.
REF_TARBALL := $(ROOT)/research/txt2html-3.0.tar.gz
REF_SRC     := $(ROOT)/ref/txt2html-3.0
REF_STUB    := $(ROOT)/ref/stubs/YAML/Syck.pm
STUB_SOURCE := $(ROOT)/stubs/YAML/Syck.pm

# Phony, deliberately. A directory target that make considers "up to date" is a
# trap here: delete ref/stubs but leave ref/txt2html-3.0 and a non-phony `ref`
# does nothing at all, silently leaving the reference broken. The recipe
# is already idempotent and cheap -- a tar test and one perl -e -- so running it
# unconditionally is the honest default. Every other target in this Makefile is
# a real file, which is why this needs saying out loud.
.PHONY: ref distclean

ref:
	@test -f "$(REF_TARBALL)" || { \
	  echo "ERROR: $(REF_TARBALL) is missing; it is tracked in git." >&2; exit 1; }
	@if [ ! -f "$(REF_SRC)/lib/HTML/TextToHTML.pm" ]; then \
	  echo "ref: extracting $(notdir $(REF_TARBALL))"; \
	  mkdir -p "$(ROOT)/ref"; \
	  tar -xzf "$(REF_TARBALL)" -C "$(ROOT)/ref"; \
	else \
	  echo "ref: $(REF_SRC) already present, not re-extracting"; \
	fi
	@if [ ! -f "$(REF_STUB)" ]; then \
	  echo "ref: writing YAML::Syck stub"; \
	  mkdir -p "$(dir $(REF_STUB))"; \
	  cp "$(STUB_SOURCE)" "$(REF_STUB)"; \
	fi
	@PERL5LIB="$(STUBS):$(REFDIR)/lib" perl -e 'use HTML::TextToHTML; print "ref: reference loads OK\n"' \
	  || { echo "ERROR: the reference does not load; check the stub" >&2; exit 1; }

# The 26 MB scale fixture, split off from `ref` because it is large and only
# one test wants it. `make test` runs without it and reports the skip; this is
# for when that test is the thing being worked on.
REF_LARGE_DIR := $(ROOT)/ref/txt2html-master
LARGE_ZIP     := $(ROOT)/research/txt2html-master.zip

.PHONY: ref-large
ref-large:
	@test -f "$(LARGE_ZIP)" || { \
	  echo "ERROR: $(LARGE_ZIP) is missing; it is tracked in git." >&2; exit 1; }
	@if [ -f "$(REF_LARGE_DIR)/test2.txt" ]; then \
	  echo "ref-large: already present, not re-extracting"; \
	else \
	  echo "ref-large: extracting $(notdir $(LARGE_ZIP))"; \
	  mkdir -p "$(ROOT)/ref"; \
	  unzip -q -o "$(LARGE_ZIP)" -d "$(ROOT)/ref"; \
	fi
	@test -f "$(REF_LARGE_DIR)/test2.txt" \
	  || { echo "ERROR: test2.txt not found in $(REF_LARGE_DIR)" >&2; exit 1; }
	@echo "ref-large: fixture ready ($$(du -h "$(REF_LARGE_DIR)/test2.txt" | cut -f1))"

# Remove the derived reference. `make clean` leaves it alone on purpose: it is
# 1 MB of extracted Perl, it takes one `make ref` to rebuild, and deleting it on
# every clean is a good way to make the gate mysteriously unavailable.
# --- Flatpak packaging (draft; see packaging/README.md) --------------------------

# Generates packaging/cargo-sources.json from Cargo.lock, which the Flatpak
# manifest's two modules both consume. The file is generated rather than
# committed because it is a few thousand lines of vendored crate metadata that
# only has to agree with one lockfile, and a stale copy of it is a build failure
# nobody can read. Requires flatpak-cargo-generator (pip), which is NOT installed
# on this host. The generate-vs-vendor decision is open; see packaging/README.md.
CARGO_SOURCES := packaging/cargo-sources.json

.PHONY: cargo-sources
cargo-sources:
	@command -v flatpak-cargo-generator >/dev/null 2>&1 || \
	  { echo "ERROR: flatpak-cargo-generator is not installed."; \
	    echo "       pip install flatpak-cargo-generator"; \
	    echo "       Or vendor instead: cargo vendor (see packaging/README.md)"; exit 1; }
	@for crate in textrill textrill-gui-rs; do \
	  flatpak-cargo-generator -o $(CARGO_SOURCES) $$crate/Cargo.lock; \
	done
	@echo "wrote $(CARGO_SOURCES)"

distclean: clean
	rm -rf "$(ROOT)/ref"

# --- scale probes ------------------------------------------------------------

scale: build
	@for f in big_para big_para_crlf; do \
		in=$(RS)/tests/corpus/inputs/$$f.txt; \
		$(TIME) $(RELEASE_BIN) --infile $$in --outfile /tmp/scale_mine.html; \
		$(TIME) perl $(REFDIR)/scripts/txt2html --infile $$in --outfile /tmp/scale_ref.html; \
		cmp -s /tmp/scale_mine.html /tmp/scale_ref.html \
			&& echo "  $$f: byte-identical" || echo "  $$f: DIFFER"; \
	done

clean:
	cd $(RS) && $(CARGO) clean
