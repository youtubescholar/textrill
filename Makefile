# Single entry point for the project's checks. Everything is one command:
#
#   make verify      fmt-check, clippy, Rust tests (engine and GUI), the
#                    differential corpus, fuzzer
#   make fix         what is safe to apply automatically: cargo fmt
#   make test        just the Rust tests
#   make corpus      the Perl differential corpus
#   make scale       the 1 MB paragraph probes, timed against Perl
#
# Individual targets build first and pass the binary path explicitly. The
# corpus runner defaults MINE to target/debug/textrill, and a stale debug
# binary is what made it report a false green twice during A1; exporting MINE
# from here means no invocation can pick up a binary that was not just built.

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

# The seeds are fixed so a run is reproducible. 424242 reaches the input shape
# that E3 lived in -- CR-terminated blank sequences -- so it stays in the list
# as a regression seed, but it no longer carries a known divergence: E3 is fixed
# and KNOWN_DIVERGENCES in fuzz.py is empty. 99 is the seed that found the
# delimiter retry bug. Any divergence found from here on must be diagnosed, and
# either fixed or re-added with a precise signature.
FUZZ_SEEDS ?= 99 424242 20260929 7 31337 555 90210 1
FUZZ_CASES ?= 2000
# P19: the seeds run concurrently. 8 x 2000 cases took ~99 min one at a time on
# a 20-core machine, and a gate nobody runs is not a gate. Eight at a time is
# ~13 min, which is short enough that `make verify` can actually include it.
# Lower FUZZ_JOBS on a smaller box; the number is echoed at the start of a run.
FUZZ_JOBS ?= 8
# Each seed gets its own failure directory. Two seeds reach the same case index
# from the same source file, so a shared directory would have them overwrite each
# other's evidence -- see fuzz.py --fail-dir.
FUZZ_FAILDIR ?= $(RS)/tests/corpus/fuzz-fail

.PHONY: all verify build fmt fmt-check clippy test test-gui-rs proptest alloctest corpus fuzz scale musl corpus-musl clean

all: verify

verify: fmt-check clippy test test-gui-rs proptest alloctest corpus fuzz
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
# builds and asserts the result really is static; `corpus-musl` then runs the
# differential gate against the static binary. `rustup target add
# $(MUSL_TARGET)` is the only prerequisite: this crate has no C dependencies, so
# the target's self-contained musl links it without musl-gcc.
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
# path this Makefile exports above. The warning in run.sh about MINE being older
# than the sources cannot fire here: `musl` depends on nothing stale because it
# is a phony rebuild, and the static target's sources are the same files.
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

# The native GUI crate. The Python/PySide6 front end it replaced is archived in
# legacy-archive/. The tests are headless (`egui_kittest` drives the widget tree
# directly), so this needs no display.
test-gui-rs:
	cd $(GUI_RS) && $(CARGO) test

# --- properties and resource bounds (P12) ------------------------------------

# Guarantees the tool owes its user regardless of what the reference does: no
# data loss, well-formed XHTML, determinism, and no panic on hostile input. The
# reference is never invoked here, which is the point -- it is the oracle for
# Tier 1 only, and it is broken on non-ASCII input.
proptest: build
	cd $(RS) && $(PYTHON) tests/proptest.py

# Allocation budgets via a counting global allocator. Byte-comparison cannot see
# a resource defect: A2 leaks while producing correct output, so the corpus passes.
# Known-open budgets are printed, not silenced; see KNOWN_OPEN in the test.
# --test-threads=1 is belt-and-braces, not what makes this correct. The budgets
# read process-global counters, so until commit 4f48dbd the four tests raced each
# other and this flag was the only reason this entry point was honest while
# `make test` was intermittently red. alloctest.rs now serialises itself, and
# `cargo test --release` runs it green at any thread count.
alloctest: build
	cd $(RS) && $(CARGO) test --release --test alloctest -- --nocapture --test-threads=1

# --- differential corpus and fuzzer ------------------------------------------

corpus: build
	cd $(RS) && ./tests/corpus/run.sh

# Each seed's exit status must reach make. This target used to end the fuzz.py
# invocation in `| tail -1` to print just the summary line, and a pipeline
# reports the status of its *last* command -- so `tail` exited 0 whatever the
# fuzzer did, and `make fuzz` was incapable of failing. A run that crashed, or
# found a mismatch, or aborted early, was reported as a pass by `make verify`.
# That is the P1 false-green shape in a third place, and it made the plan's
# "16 000 cases, 0 mismatches" unprovable.
#
# So: capture each seed's output and status, print the summary line as before,
# print the whole log if the seed failed, and fail the target if any seed did.
# The seeds run concurrently (P19). Each writes its status to a file, and the
# statuses are read back in seed order afterwards, so the report is stable
# regardless of which seed finishes first. Deliberately NOT collecting status
# through `wait $pid`: a seed that dies before it can write its status file
# would then be silently absent rather than reported as a failure, which is
# the exact bug class this target exists to rule out (P14). A missing status
# file is a failure, and so is a seed whose log has no summary line -- a run
# that printed nothing has proved nothing.
fuzz: build
	@rc=0; \
	logdir=$$(mktemp -d) || exit 1; \
	trap 'rm -rf "$$logdir"' EXIT; \
	echo "fuzz: $(words $(FUZZ_SEEDS)) seeds x $(FUZZ_CASES) cases, $(FUZZ_JOBS) at a time"; \
	launched=0; \
	for seed in $(FUZZ_SEEDS); do \
		( \
			cd $(RS)/tests/corpus && $(PYTHON) fuzz.py \
				--seed "$$seed" --cases $(FUZZ_CASES) \
				--fail-dir '$(FUZZ_FAILDIR)/seed-'$$seed; \
			echo $$? >"$$logdir/$$seed.status"; \
		) >"$$logdir/$$seed.log" 2>&1 & \
		launched=$$((launched+1)); \
		if [ "$$launched" -ge $(FUZZ_JOBS) ]; then wait; launched=0; fi; \
	done; \
	wait; \
	for seed in $(FUZZ_SEEDS); do \
		if [ ! -f "$$logdir/$$seed.status" ]; then \
			printf 'seed %-10s NO STATUS -- died before it could report\n' "$$seed"; \
			rc=1; \
			continue; \
		fi; \
		st=$$(cat "$$logdir/$$seed.status"); \
		last=$$(tail -1 "$$logdir/$$seed.log" 2>/dev/null); \
		printf 'seed %-10s %s\n' "$$seed" "$$last"; \
		case "$$last" in \
			"fuzz: "*) ;; \
			*) echo "  --- no 'fuzz:' summary line; treating as failure ---"; rc=1 ;; \
		esac; \
		if [ "$$st" -ne 0 ]; then \
			echo "  --- seed $$seed FAILED (exit $$st), full output: ---"; \
			sed 's/^/  /' "$$logdir/$$seed.log"; \
			echo "  --- failing inputs, if kept: $(FUZZ_FAILDIR)/seed-$$seed ---"; \
			rc=1; \
		fi; \
	done; \
	if [ $$rc -ne 0 ]; then echo "fuzz: FAILED"; exit 1; fi; \
	echo "fuzz: OK"

# --- the reference checkout (P22) ---------------------------------------------

# The differential corpus compares this port against the Perl original, so the
# gate needs that original. It is not in version control -- `ref/` is gitignored
# because it is derived -- which until now meant a fresh clone had no way to
# produce it: the two tarballs are tracked but nothing extracted them, and the
# stub module the reference `use`s at load time existed only on the machine that
# wrote it. Every other default in this Makefile was that machine's absolute
# path, so `make corpus` on a new checkout had nothing to run against.
#
# This target closes that: extract the tracked archive, then write the one stub
# the reference needs. It is idempotent and offline -- no network, no CPAN, no
# package installs. The reference turns out to need nothing but a core Perl:
# Getopt::ArgvFile is an optional `eval require`, and YAML::Syck is `use`d at
# load time but never called.
#
# The stub is a tracked file copied into place rather than a heredoc here, so
# its provenance and reasoning live in a file you can read and diff.
REF_TARBALL := $(ROOT)/txt2html-3.0.tar.gz
REF_SRC     := $(ROOT)/ref/txt2html-3.0
REF_STUB    := $(ROOT)/ref/stubs/YAML/Syck.pm
STUB_SOURCE := $(ROOT)/tests/refstub/YAML/Syck.pm

# Phony, deliberately. A directory target that make considers "up to date" is a
# trap here: delete ref/stubs but leave ref/txt2html-3.0 and a non-phony `ref`
# does nothing at all, silently reinstating the unreproducible gate. The recipe
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
LARGE_ZIP     := $(ROOT)/txt2html-master.zip

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
# --- Flatpak packaging (draft; see REMEDIATION-PLAN.md) ------------------------

# Generates packaging/cargo-sources.json from Cargo.lock, which the Flatpak
# manifest's two modules both consume. The file is generated rather than
# committed because it is a few thousand lines of vendored crate metadata that
# only has to agree with one lockfile, and a stale copy of it is a build failure
# nobody can read. Requires flatpak-cargo-generator (pip), which is NOT installed
# on this host; the app-id is also still a placeholder. Both are recorded in the
# plan rather than papered over here.
CARGO_SOURCES := packaging/cargo-sources.json

.PHONY: cargo-sources
cargo-sources:
	@command -v flatpak-cargo-generator >/dev/null 2>&1 || \
	  { echo "ERROR: flatpak-cargo-generator is not installed."; \
	    echo "       pip install flatpak-cargo-generator"; \
	    echo "       Or vendor instead: cargo vendor (see the plan's preference"; \
	    echo "       note on offline dependencies)."; exit 1; }
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
