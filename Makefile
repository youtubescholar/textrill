# Single entry point for the project's checks. Everything is one command:
#
#   make verify      fmt-check, clippy, Rust tests, differential corpus,
#                    fuzzer, GUI suite
#   make fix         what is safe to apply automatically: cargo fmt
#   make test        just the Rust tests
#   make corpus      the Perl differential corpus
#   make scale       the 1 MB paragraph probes, timed against Perl
#
# Individual targets build first and pass the binary path explicitly. The
# corpus runner defaults MINE to target/debug/txt2html, and a stale debug
# binary is what made it report a false green twice during A1; exporting MINE
# from here means no invocation can pick up a binary that was not just built.

TIME   ?= /usr/bin/time -f "  %es"
CARGO  ?= cargo
PYTHON ?= python3
ROOT  := $(CURDIR)
RS    := $(ROOT)/txt2html-rs
GUI   := $(ROOT)/txt2html-gui
VENV  ?= $(ROOT)/.venv

RELEASE_BIN := $(RS)/target/release/txt2html
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

.PHONY: all verify build fmt fmt-check clippy test proptest alloctest corpus fuzz gui scale clean

all: verify

verify: fmt-check clippy test proptest alloctest corpus fuzz gui
	@echo
	@echo "verify: OK"

# --- build -------------------------------------------------------------------

build:
	cd $(RS) && $(CARGO) build --release

# --- rust --------------------------------------------------------------------

fmt:
	cd $(RS) && $(CARGO) fmt

fmt-check:
	cd $(RS) && $(CARGO) fmt --check

# Warns rather than fails: the crate is not clippy-clean yet and a lint gate
# that is always red gets ignored, which is worse than no gate. Flip to
# `-- -D warnings` once the cleanup lands.
clippy: build
	cd $(RS) && $(CARGO) clippy --release --all-targets

test: build
	cd $(RS) && $(CARGO) test --release

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

fuzz: build
	@for seed in $(FUZZ_SEEDS); do \
		printf 'seed %-10s ' "$$seed"; \
		cd $(RS)/tests/corpus && $(PYTHON) fuzz.py --seed "$$seed" --cases $(FUZZ_CASES) \
			| tail -1; \
	done

# --- scale probes ------------------------------------------------------------

scale: build
	@for f in big_para big_para_crlf; do \
		in=$(RS)/tests/corpus/inputs/$$f.txt; \
		$(TIME) $(RELEASE_BIN) --infile $$in --outfile /tmp/scale_mine.html; \
		$(TIME) perl $(REFDIR)/scripts/txt2html --infile $$in --outfile /tmp/scale_ref.html; \
		cmp -s /tmp/scale_mine.html /tmp/scale_ref.html \
			&& echo "  $$f: byte-identical" || echo "  $$f: DIFFER"; \
	done

# --- GUI ---------------------------------------------------------------------

# The extension is reinstalled first: the GUI imports the compiled module, not
# the crate, so without this the suite tests whatever maturin last built.
gui: build
	cd $(RS) && $(VENV)/bin/maturin develop --release
	cd $(GUI) && $(VENV)/bin/python -m unittest discover -s tests

clean:
	cd $(RS) && $(CARGO) clean
