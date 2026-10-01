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
# P19: the seeds run concurrently. 8 x 2000 cases took ~99 min one at a time on
# a 20-core machine, and a gate nobody runs is not a gate. Eight at a time is
# ~13 min, which is short enough that `make verify` can actually include it.
# Lower FUZZ_JOBS on a smaller box; the number is echoed at the start of a run.
FUZZ_JOBS ?= 8
# Each seed gets its own failure directory. Two seeds reach the same case index
# from the same source file, so a shared directory would have them overwrite each
# other's evidence -- see fuzz.py --fail-dir.
FUZZ_FAILDIR ?= $(RS)/tests/corpus/fuzz-fail

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
