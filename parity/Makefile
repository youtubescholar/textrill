# The Perl parity harness (roadmap §8.5).
#
# Everything in this directory serves one purpose: auditing the Rust port
# against the original Perl `HTML::TextToHTML`. It is NOT part of any gate.
# `make verify` at the top level never enters this directory, never runs Perl,
# and passes with `parity/` absent; nothing in the Rust roadmap depends on it.
#
# Targets (usually invoked as `make diff` / `make ref` / `make scale` at the
# top level, which delegate here):
#
#   make ref          extract the tracked reference tarball offline + the stub
#   make ref-large    extract the 26 MB scale fixture (only `scale`/one test)
#   make corpus       the differential corpus: Perl reference vs Rust port
#   make diff         ref + corpus: the on-demand parity cross-check
#   make scale        the 1 MB paragraph probes, timed against Perl
#   make distclean    drop the derived reference tree
#
# Offline by construction: the reference comes from the tracked tarballs in
# this directory. No network, no CPAN, no package installs.

ROOT      := $(CURDIR)/..
RS        := $(ROOT)/textrill
CARGO     ?= cargo
TIME      ?= /usr/bin/time -f "  %es"

REFDIR      ?= $(CURDIR)/ref/txt2html-3.0
STUBS       ?= $(CURDIR)/ref/stubs
REF_TARBALL := $(CURDIR)/txt2html-3.0.tar.gz
REF_SRC     := $(CURDIR)/ref/txt2html-3.0
REF_STUB    := $(CURDIR)/ref/stubs/YAML/Syck.pm
STUB_SOURCE := $(CURDIR)/stubs/YAML/Syck.pm
LARGE_ZIP     := $(CURDIR)/txt2html-master.zip
REF_LARGE_DIR := $(CURDIR)/ref/txt2html-master

RELEASE_BIN := $(RS)/target/release/textrill
export PERL5LIB := $(STUBS):$(REFDIR)/lib
export MINE     := $(RELEASE_BIN)

.PHONY: all corpus ref ref-large diff scale distclean build

all: diff

build:
	cd $(RS) && $(CARGO) build --release

# --- differential corpus -----------------------------------------------------

# Runs only the Perl half (parity/corpus/run.sh); the shared case table,
# fixtures and normaliser stay in textrill/tests/corpus so both runners see
# one definition of each case.
corpus: build
	$(CURDIR)/corpus/run.sh

# --- the reference checkout ---------------------------------------------------

# The differential needs the Perl original, derived rather than re-derived by
# hand: this target extracts the tracked tarball and installs the one stub
# module the reference `use`s at load time. Idempotent and offline: no network,
# no CPAN, no package installs -- the reference needs nothing but a core Perl
# (Getopt::ArgvFile is an optional `eval require`; YAML::Syck is `use`d but
# never called). The stub is a tracked file copied into place rather than a
# heredoc here, so its provenance and reasoning live in a file you can read
# and diff.
#
# Phony, deliberately. A directory target that make considers "up to date" is
# a trap here: delete ref/stubs but leave ref/txt2html-3.0 and a non-phony
# `ref` does nothing at all, silently leaving the reference broken. The recipe
# is already idempotent and cheap -- a tar test and one perl -e -- so running
# it unconditionally is the honest default.
ref:
	@test -f "$(REF_TARBALL)" || { \
	  echo "ERROR: $(REF_TARBALL) is missing; it is tracked in git." >&2; exit 1; }
	@if [ ! -f "$(REF_SRC)/lib/HTML/TextToHTML.pm" ]; then \
	  echo "ref: extracting $(notdir $(REF_TARBALL))"; \
	  mkdir -p "$(CURDIR)/ref"; \
	  tar -xzf "$(REF_TARBALL)" -C "$(CURDIR)/ref"; \
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
.PHONY: ref-large
ref-large:
	@test -f "$(LARGE_ZIP)" || { \
	  echo "ERROR: $(LARGE_ZIP) is missing; it is tracked in git." >&2; exit 1; }
	@if [ -f "$(REF_LARGE_DIR)/test2.txt" ]; then \
	  echo "ref-large: already present, not re-extracting"; \
	else \
	  echo "ref-large: extracting $(notdir $(LARGE_ZIP))"; \
	  mkdir -p "$(CURDIR)/ref"; \
	  unzip -q -o "$(LARGE_ZIP)" -d "$(CURDIR)/ref"; \
	fi
	@test -f "$(REF_LARGE_DIR)/test2.txt" \
	  || { echo "ERROR: test2.txt not found in $(REF_LARGE_DIR)" >&2; exit 1; }
	@echo "ref-large: fixture ready ($$(du -h "$(REF_LARGE_DIR)/test2.txt" | cut -f1))"

# The parity oracle, on demand. `make verify` never invokes Perl; this target
# re-runs the differential corpus against the reference as a non-gating
# cross-check. Nothing depends on it. `ref` is phony so a fresh clone can run
# this without a separate `make ref`.
diff: ref corpus
	@echo
	@echo "diff: OK (non-gating parity cross-check)"

# --- scale probes ------------------------------------------------------------

# The point is the timing. The byte compare keeps it honest: the two tools must
# be converting the same bytes, or a timing advantage over a differently-shaped
# run is meaningless. That compare has to use the modes the corpus pins, not
# the defaults: textrill's default doctype is deliberately HTML5 while the
# reference defaults to XHTML, and the generator meta line names each tool.
# `--extract` drops both (no prolog, no generator line), and `--xhtml` pins
# the one serialization the two share byte-for-byte. The corpus proves this
# combination is byte-exact (`huge_paragraph`, `huge_paragraph_crlf`).
scale: build
	@for f in big_para big_para_crlf; do \
		in=$(RS)/tests/corpus/inputs/$$f.txt; \
		$(TIME) $(RELEASE_BIN) --extract --xhtml --infile $$in --outfile /tmp/scale_mine.html; \
		$(TIME) perl $(REFDIR)/scripts/txt2html --extract --xhtml --infile $$in --outfile /tmp/scale_ref.html; \
		cmp -s /tmp/scale_mine.html /tmp/scale_ref.html \
			&& echo "  $$f: byte-identical" || echo "  $$f: DIFFER"; \
	done

# Remove the derived reference. `make clean` leaves it alone on purpose: it is
# 1 MB of extracted Perl, it takes one `make ref` to rebuild, and deleting it
# on every clean is a good way to make the harness mysteriously unavailable.
distclean:
	rm -rf "$(CURDIR)/ref"
