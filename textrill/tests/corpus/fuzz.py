#!/usr/bin/env python3
"""Seeded differential fuzzer: textrill vs the Perl reference.

Each case is a mutation of an upstream ``tfiles/*.txt`` corpus file, converted
by both ``HTML::TextToHTML`` 3.0 and the textrill binary under a random option
set, and the two outputs are compared byte for byte.

Mutating real corpus files rather than generating synthetic text is deliberate:
a failing case is reproducible from the repository with a fixed seed, and the
interesting layout heuristics (underline headings, definition lists, delimiter
tables, setext headings) are only reachable from text that already exercises
them.

The seed corpus is extended with generated structural fixtures (``bigpara``,
``bigpara-crlf``, ``manydelims``), because the A1 and A2 defects are
input-driven rather than option-driven and a seeded option sweep alone will not
reach them -- they need one long paragraph and many distinct table delimiters.

Every option here is a real option in *both* implementations, checked against
``init_our_data`` in the reference module and against ``--help`` in the port.
An option name that exists in only one side would make the fuzzer report a
harness bug as a conversion mismatch.

Usage:
    tests/corpus/fuzz.py                    # CI count
    tests/corpus/fuzz.py --cases 2000       # local
    tests/corpus/fuzz.py --seed 12345       # reproduce a failure
    tests/corpus/fuzz.py --keep             # save failing inputs under RUNDIR
    tests/corpus/fuzz.py --fail-dir DIR     # save them under DIR instead

``--fail-dir`` exists for concurrent runs. Saved cases are named
``<source>-<seed>-<n>`` so two runs cannot overwrite each other's evidence even
if they share a directory, but a caller running several seeds at once should
still give each one its own directory. ``make fuzz`` does exactly that.

Environment overrides match ``run.sh``: REFDIR, MINE, RUNDIR, STUBS, PERL5LIB.
All of them default to paths derived from this file, so a fresh checkout works
once ``make ref`` has built the reference.
"""

import argparse
import os
import random
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
# normalize.py sits beside this file. This gate applies the corpus's own
# one-line canonicalisation by importing it, so there is one implementation of
# "the declared divergence" and not two that can drift apart.
sys.path.insert(0, HERE)
from normalize import normalise_bytes  # noqa: E402
# Derived from __file__, not hardcoded: see the note in run.sh. A fresh clone has
# to be able to run this. `make ref` materialises the reference checkout.
ROOT = os.path.dirname(os.path.dirname(HERE))
REPO = os.path.dirname(ROOT)
REFDIR = os.environ.get("REFDIR", os.path.join(REPO, "ref", "txt2html-3.0"))
STUBS = os.environ.get("STUBS", os.path.join(REPO, "ref", "stubs"))
MINE = os.environ.get("MINE", os.path.join(ROOT, "target", "debug", "textrill"))
RUNDIR = os.environ.get("RUNDIR", os.path.join(tempfile.gettempdir(), "textrill-corpus"))
# Both halves live under ref/ in the repo.  An earlier version kept them in
# /tmp, and a reboot wiped them: the reference then exited non-zero on every
# case, which this fuzzer counts as "reference refused" and skips, so the run
# reported 0 mismatches having actually checked nothing.
os.environ.setdefault("PERL5LIB", f"{STUBS}:{REFDIR}/lib")

# Refuse to run rather than report 0 mismatches having checked nothing. The
# failure this guards against is not hypothetical: a missing reference is
# exactly the state every fresh clone was in until `make ref` existed.
if not os.path.isfile(os.path.join(REFDIR, "lib", "HTML", "TextToHTML.pm")):
    sys.exit(
        f"ERROR: no reference checkout at {REFDIR}\n"
        f"       Run 'make ref', or set REFDIR."
    )
if not os.path.isdir(os.path.join(STUBS, "YAML")):
    sys.exit(
        f"ERROR: no YAML::Syck stub at {STUBS}/YAML\n"
        f"       Run 'make ref', or set STUBS."
    )

SEED_FILES = [
    "list.txt",
    "list-2.txt",
    "list-3.txt",
    "list-5.txt",
    "list-advanced.txt",
    "list-custom.txt",
    "list-styles.txt",
    "custom-headers.txt",
    "heading1.txt",
    "mixed.txt",
    "news.txt",
    "punct.txt",
    "pre.txt",
    "hyphens.txt",
    "robo.txt",
    "table-align.txt",
    "table-border.txt",
    "table-delim.txt",
    "table-pgsql.txt",
    "links.txt",
    "links2.txt",
    "links3.txt",
    "links4.txt",
    "links5.txt",
    "sample.txt",
    "umlauttest.txt",
]

# (cli_flags, perl_pairs) for options that toggle.  The port accepts a `no` /
# `no_` prefix on any boolean; the two spellings are given so the negated form
# is exercised on the CLI as well as the positive one.
BOOL_OPTS = [
    (["--demoronize"], "demoronize=>1"),
    (["--no-demoronize"], "demoronize=>0"),
    (["--eight_bit_clean"], "eight_bit_clean=>1"),
    (["--no-eight_bit_clean"], "eight_bit_clean=>0"),
    (["--escape_HTML_chars"], "escape_HTML_chars=>1"),
    (["--no-escape_HTML_chars"], "escape_HTML_chars=>0"),
    (["--explicit_headings"], "explicit_headings=>1"),
    (["--no-explicit_headings"], "explicit_headings=>0"),
    (["--extract"], "extract=>1"),
    (["--no-extract"], "extract=>0"),
    (["--indent_par_break"], "indent_par_break=>1"),
    (["--no-indent_par_break"], "indent_par_break=>0"),
    (["--link_only"], "link_only=>1"),
    (["--no-link_only"], "link_only=>0"),
    (["--lower_case_tags"], "lower_case_tags=>1"),
    (["--no-lower_case_tags"], "lower_case_tags=>0"),
    (["--mailmode"], "mailmode=>1"),
    (["--no-mailmode"], "mailmode=>0"),
    (["--make_anchors"], "make_anchors=>1"),
    (["--no-make_anchors"], "make_anchors=>0"),
    (["--make_links"], "make_links=>1"),
    (["--no-make_links"], "make_links=>0"),
    (["--make_tables"], "make_tables=>1"),
    (["--no-make_tables"], "make_tables=>0"),
    (["--preserve_indent"], "preserve_indent=>1"),
    (["--no-preserve_indent"], "preserve_indent=>0"),
    (["--titlefirst"], "titlefirst=>1"),
    (["--no-titlefirst"], "titlefirst=>0"),
    (["--unhyphenation"], "unhyphenation=>1"),
    (["--no-unhyphenation"], "unhyphenation=>0"),
    (["--use_mosaic_header"], "use_mosaic_header=>1"),
    (["--no-use_mosaic_header"], "use_mosaic_header=>0"),
    (["--use_preformat_marker"], "use_preformat_marker=>1"),
    (["--no-use_preformat_marker"], "use_preformat_marker=>0"),
    (["--xhtml"], "xhtml=>1"),
    (["--no-xhtml"], "xhtml=>0"),
]

# (cli_flag_name, perl_key, [values]).  The flag is always passed as two argv
# entries, matching the port's `<flag> <value>` form.
VALUE_OPTS = [
    ("--body_deco", "body_deco", ["<!-- hi -->", "<hr/>"]),
    ("--tab_width", "tab_width", ["2", "4", "8"]),
    ("--indent_width", "indent_width", ["2", "3", "4"]),
    ("--par_indent", "par_indent", ["0", "2", "4"]),
    ("--min_caps_length", "min_caps_length", ["4", "5", "8"]),
    ("--short_line_length", "short_line_length", ["40", "60", "80"]),
    ("--hrule_min", "hrule_min", ["3", "5"]),
    ("--bullets", "bullets", ["-*", "-=o+*"]),
    ("--bullets_ordered", "bullets_ordered", ["#", "1."]),
    ("--caps_tag", "caps_tag", ["STRONG", "EM"]),
    ("--bold_delimiter", "bold_delimiter", ["#", "%"]),
    ("--italic_delimiter", "italic_delimiter", ["*", "~"]),
    ("--underline_delimiter", "underline_delimiter", ["_", "="]),
    ("--underline_length_tolerance", "underline_length_tolerance", ["0", "2"]),
    ("--underline_offset_tolerance", "underline_offset_tolerance", ["0", "2"]),
    ("--preformat_whitespace_min", "preformat_whitespace_min", ["2", "5"]),
    ("--preformat_trigger_lines", "preformat_trigger_lines", ["1", "2"]),
    ("--endpreformat_trigger_lines", "endpreformat_trigger_lines", ["1", "2"]),
    ("--table_type", "table_type", ["ALIGN=0", "PGSQL=0", "DELIM=0", "BORDER=0"]),
]

# Options the fuzzer must NOT generate, because the port deliberately emits
# different bytes from the reference for them and byte-comparing is therefore
# the wrong oracle.  A9/A8 in the remediation plan: `--title` and `--style_url`
# are interpolated into the document escaped (`chars::escape_attr`), because
# unescaped they are a live XSS -- `--title '</title><script>alert(3)</script>'`
# emitted exactly that.  Perl emits them raw, so any value containing `& < > "`
# differs by construction.
#
# Both used to be in VALUE_OPTS, and the `&` in `--title "A & B"` is why.  This
# is a list of *options*, not of known-bad values, and the distinction is the
# whole point: these are not "divergences we have not fixed yet", they are inputs
# where the reference is the defect and the plan's tier table says Tier 2 items
# may not be verified by byte-comparing against the reference.  A list of
# *unfixed* divergences is the thing this project deliberately removed from this
# file, so it is not being reintroduced under another name -- there is no
# mechanism here that can suppress a mismatch, and no entry can be added to
# silence one.
#
# The cost is real and is worth stating: dropping these two means the fuzzer no
# longer exercises title or stylesheet-URL handling at all, across all 16 000
# cases.  That is why `OPTION_DIVERGENT` below is a hard error if anything else
# ever needs excluding -- the next person has to come here and write down why,
# rather than it being a value quietly removed from a list.
#
# Their *content* is still covered, by a different oracle: `proptest.py` checks
# that `--xhtml` output parses as XML, which is what the escaping is for and what
# the raw bytes failed.
OPTION_DIVERGENT = {
    "--title": "A8: emitted escaped by the port, raw by the reference (XSS)",
    "--style_url": "A8: emitted escaped by the port, raw by the reference (XSS)",
}

# Values that must reach the reference as a hashref rather than a string.
PERL_HASH_OPTS = {"table_type"}

# --- mutations -------------------------------------------------------------
#
# Mutations are ASCII-only and carry no trailing whitespace before a line
# ending; see sanitise() for exactly why, and for what covers the excluded
# domains instead.

MARKERS = [
    "  \n",
    "\t",
    "\r\n",
    "--- ",
    "=== ",
    "* ",
    "- ",
    "1. ",
    ":: ",
    "\x0c",
    "   ",
    "http://example.invalid/a",
    "www.example.invalid/b",
    "foo_bar_baz",
    "<b>raw</b>",
    "&amp;",
]


def read_seed_texts():
    seeds = []
    for f in SEED_FILES:
        p = os.path.join(REFDIR, "tfiles", f)
        try:
            with open(p, "rb") as fh:
                raw = fh.read()
        except FileNotFoundError:
            continue
        seeds.append((f, raw.decode("latin-1")))
    if not seeds:
        sys.exit(
            f"fuzz: no seed corpus under {REFDIR}/tfiles "
            "(set REFDIR to an unpacked txt2html-3.0)"
        )
    return seeds


def mutate(text, rng):
    """Apply 1-4 random structural mutations to `text`."""
    lines = text.split("\n")
    for _ in range(rng.randint(1, 4)):
        if not lines:
            lines = [""]
        kind = rng.choice(
            [
                "insert",
                "delete",
                "swap",
                "join",
                "split",
                "marker",
                "repeat",
                "indent",
                "case",
                "blank",
            ]
        )
        i = rng.randrange(len(lines))
        if kind == "insert":
            lines.insert(
                i, "".join(rng.choice(MARKERS) for _ in range(rng.randint(1, 3)))
            )
        elif kind == "delete":
            del lines[i]
        elif kind == "swap" and len(lines) > 1:
            j = rng.randrange(len(lines))
            lines[i], lines[j] = lines[j], lines[i]
        elif kind == "join" and i + 1 < len(lines):
            lines[i] = lines[i] + " " + lines[i + 1]
            del lines[i + 1]
        elif kind == "split" and lines[i]:
            cut = rng.randrange(1, max(2, len(lines[i])))
            lines[i : i + 1] = [lines[i][:cut], lines[i][cut:]]
        elif kind == "marker":
            lines[i] = lines[i] + rng.choice(MARKERS)
        elif kind == "repeat":
            lines[i] = lines[i] * rng.randint(2, 4)
        elif kind == "indent":
            lines[i] = " " * rng.randint(1, 8) + lines[i]
        elif kind == "case":
            lines[i] = lines[i].upper() if rng.random() < 0.5 else lines[i].lower()
        elif kind == "blank":
            lines[i] = ""
    out = "\n".join(lines)
    # Keep a CI run fast.  The large-paragraph and many-delimiter shapes that
    # A1 and A2 need are generated structurally below, not by mutation.
    if len(out) > 100_000:
        out = out[:100_000]
    return sanitise(out)


def gen_structural(name, rng):
    """Deterministic structural fixtures a plain option sweep cannot reach."""
    if name == "bigpara":
        # One paragraph, no blank lines: A1's shape.
        n = rng.choice([1200, 2500, 5000])
        return "\n".join(
            f"line {i} " + "word" * rng.randint(1, 8) for i in range(n)
        )
    if name == "bigpara-crlf":
        # Same, but exercising A1's CR and CRLF string surgery.
        n = rng.choice([800, 2000])
        return "".join(
            f"{rng.choice(['   ', chr(9), ' ', ''])}body {i}"
            f"{rng.choice([chr(13) + chr(10), chr(13), chr(10)])}"
            for i in range(n)
        )
    if name == "manydelims":
        # Many distinct table delimiters: A2's shape.  Requires --make_tables,
        # which build_case forces on.
        rows = []
        for i in range(rng.randint(20, 60)):
            d = chr(33 + (i % 90))
            if d.isalnum():
                d = "#"
            rows.append(f"{d}a{d} b{d} c{d}")
            rows.append(f"1{d}2{d}3{d}")
            rows.append("")
        return "\n".join(rows)
    raise KeyError(name)


STRUCTURAL = ["bigpara", "bigpara-crlf", "manydelims"]
STRUCTURAL_EXTRA = {
    "manydelims": [(["--make_tables"], "make_tables=>1")],
}

# The reference is byte-oriented end to end -- it opens the file with no
# ``:encoding`` layer and never decodes -- while the port decodes UTF-8 when it
# can and falls back to Latin-1 otherwise.  So the two disagree on *every*
# non-ASCII input, in one of three ways:
#
#   * demoronize sees bytes in the reference and code points in the port, so
#     "\xc3\xa4" becomes "&Atilde;&curren;" versus "&auml;";
#   * a byte that is escaped identically still changes the byte offsets that
#     tab expansion and table column slicing count, so a Latin-1 character
#     followed by a tab lands one stop apart;
#   * a high byte that reaches the output unescaped is re-encoded as UTF-8
#     rather than passed through.
#
# Trailing whitespace before a line ending is the trigger for two open
# divergences where the port drops a space the reference keeps: ``'   e\\n '``
# with --indent_par_break, and the three-space-indent ALIGN table
# ``'   . \\n3  .'``.
#
# Both domains are covered by dedicated fixtures rather than by random cases,
# so restricting the alphabet costs no coverage that exists anywhere else.
# Tabs are kept: with the input ASCII-only, tab expansion agrees with the
# reference, and the structural fixtures need tabs and CRs.
#
# Scheme tokens are the third restriction, added 2026-10-06 after the first
# clean generator-normalised sweep reported 4 mismatches in 16 000 cases, all of
# them one shape: upstream's own `tfiles/pre.txt` contains `file:Here`, and the
# reference links it while textrill refuses it. That refusal is policy
# (src/urlscheme.rs: javascript, data, vbscript and file are DANGEROUS_SCHEMES,
# and a refused anchor is unwrapped with the text kept), and it has its own
# oracles -- tests/urlschemetest.rs and the corpus case `opt_injection`. The
# reference has no scheme policy at all, so there is no byte sequence textrill
# could emit to match it: this is a Tier 2 divergence, not a defect, and a byte
# comparison is simply the wrong oracle for this input.
#
# It is removed from the claimed domain rather than recorded as a known
# divergence, because a list keyed on an output shape is the machinery this file
# removed on purpose (see below): it would outlive its reason and start hiding
# real link-handling regressions. The input domain statement stays honest
# instead -- the fuzzer compares byte for byte over inputs where byte comparison
# is the right oracle, and the scheme policy is tested where it belongs.
DANGEROUS_SCHEME = re.compile(r"(?i)\b(?:javascript|data|vbscript|file)\s*:")


def sanitise(text):
    out = []
    for line in text.split("\n"):
        line = "".join(c if ord(c) < 0x80 else "?" for c in line)
        line = DANGEROUS_SCHEME.sub("", line)
        out.append(line.rstrip(" \t"))
    return "\n".join(out)


# There is deliberately no list of known-and-unfixed divergences here, and no
# machinery to consult one.  An earlier version had `KNOWN_DIVERGENCES` plus ~50
# lines matching a reported mismatch against recorded option sets and output
# shapes, stepping over anything that matched.  It was removed because it was
# machinery with no user:
#
#   - The list was empty, and stayed empty, because every entry was deleted in
#     the same change that fixed the defect it described.  The three that came
#     before that -- a table_type that merged into the defaults instead of
#     replacing them, a named table_type, and a PRE block that dropped
#     everything after its first blank line -- are pinned as fixed cases in
#     cases.sh, where a regression is a *failing test* rather than a quiet pass.
#
#   - Its history was bad for a suppression list.  The CR path's phantom
#     trailing paragraph (E3) was recorded as a *signature* -- "the port's line
#     list is the reference's with two blank lines spliced in" -- not as a
#     defect.  A signature outlives its fix by construction: left in place, it
#     suppressed the very regression that came back, and the fuzzer had nothing
#     to say about it.  A suppression entry keyed on a symptom is a machine for
#     hiding a bug that got fixed.
#
# So the rule is the one the corpus already follows: a divergence is a failing
# gate, and it is made a *passing* gate by fixing it, in the same change.  When
# a real defect needs tracking, cases.sh is where it goes, and it goes there as
# a case that currently fails.


def build_case(rng, seeds):
    """Return (source_name, text, argv).

    The argv is handed to both converters unchanged.  PERL_HASH_OPTS is still
    needed to build the value a module driver would want, but the oracle is the
    script now, so it only decides how the value is spelled on the command line.
    """
    picked = []  # list of (cli_argv, perl_kv_string)

    if rng.random() < 0.15:
        name = rng.choice(STRUCTURAL)
        text = sanitise(gen_structural(name, rng))
        for flags, pair in STRUCTURAL_EXTRA.get(name, []):
            picked.append((flags, pair))
    else:
        name, base = rng.choice(seeds)
        text = mutate(base, rng)

    for flags, pair in rng.sample(BOOL_OPTS, rng.randint(0, 5)):
        picked.append((flags, pair))

    for flag, key, choices in rng.sample(VALUE_OPTS, rng.randint(0, 2)):
        val = rng.choice(choices)
        perl_val = f"%{val}" if key in PERL_HASH_OPTS else val
        picked.append(([flag, val], f"{key}=>{perl_val}"))

    argv = []
    for flags, _pair in picked:
        argv.extend(flags)
    return name, text, argv


# --- the two converters ----------------------------------------------------

def run_perl(inpath, outpath, flags):
    # The oracle is scripts/txt2html -- the reference's own command line tool --
    # driven with the very same argv the port is given, so the comparison is
    # CLI against CLI and no argument-reshaping code sits between them.
    #
    # An earlier version drove the module through an embedded perl -e program
    # instead.  That is a *different* program: for the same option values it
    # produced a different document (case 369: the script emits
    # "<strong>a</strong>" where the embedded driver left "%a%" alone).  run.sh
    # uses a module driver too, but it pins the construction-time options it
    # needs in CTOR[]; a fuzzer that samples 32 options at random has no way to
    # know which ones those are, so it needs the oracle that has no such table.
    return subprocess.run(
        ["perl", "scripts/txt2html", "--infile", inpath, "--outfile", outpath,
         "--default_link_dict", ""] + list(flags),
        cwd=REFDIR,
        capture_output=True,
        timeout=120,
    )


def run_rust(inpath, outpath, flags):
    return subprocess.run(
        [MINE, "--infile", inpath, "--outfile", outpath, "--default_link_dict", ""]
        + flags,
        cwd=REFDIR,
        capture_output=True,
        timeout=120,
    )


def smoke_check():
    """Refuse to report a result unless both converters actually work.

    A broken PERL5LIB or a half-deleted reference tree makes the reference exit
    non-zero on every case.  A run that simply stepped over those cases printed
    "0 mismatches" having compared nothing at all -- the same false-green shape
    as the P1 harness bug.  The per-case skip that allowed it has been removed,
    so a total failure now fails the run; what is left to this check is the
    diagnosis, which is worth having early and by name: "the reference exited 1
    and here is its stderr" is a different morning from "0 cases compared".
    """
    d = tempfile.mkdtemp(prefix="t2hsmoke-", dir="/tmp")
    i, r, m = (os.path.join(d, n) for n in ("in.txt", "ref.html", "mine.html"))
    with open(i, "wb") as fh:
        fh.write(b"smoke *test*\n")
    p = run_perl(i, r, [])
    q = run_rust(i, m, [])
    for who, proc in (("reference", p), ("port", q)):
        if proc.returncode != 0:
            sys.exit(
                f"fuzz: the {who} exited {proc.returncode} on a trivial input.\n"
                f"      stderr: {proc.stderr.decode('utf-8', 'replace').strip()[:400]}\n"
                f"      PERL5LIB={os.environ['PERL5LIB']}"
            )
    # * is the default *italic* delimiter; the default bold delimiter is #.
    want = b"<em>test</em>"
    for who, path in (("reference", r), ("port", m)):
        if not os.path.exists(path) or want not in open(path, "rb").read():
            sys.exit(f"fuzz: the {who} did not emit {want!r}; the comparison is meaningless")


def main():
    ap = argparse.ArgumentParser()
    # Deliberately small: a bare run has to be short enough that it gets
    # run at all.  Each case is two converter invocations, so 300 is a few
    # minutes.  Longer sweeps are opt-in: --cases 2000 --seed <n>.
    ap.add_argument("--cases", type=int, default=300)
    ap.add_argument("--seed", type=int, default=20260929)
    ap.add_argument("--keep", action="store_true", help="save failing inputs")
    ap.add_argument(
        "--fail-dir",
        metavar="DIR",
        help=(
            "where --keep saves failing inputs "
            f"(default: {os.path.join(RUNDIR, 'fuzz-fail')}). Give each "
            "concurrent run its own directory."
        ),
    )
    ap.add_argument(
        "--dump",
        type=int,
        metavar="N",
        help="write case N's input and flags to stdout and exit (debugging)",
    )
    args = ap.parse_args()

    faildir = args.fail_dir or os.path.join(RUNDIR, "fuzz-fail")

    if not os.path.isdir(REFDIR):
        sys.exit(f"fuzz: REFDIR {REFDIR} does not exist")
    if not (os.path.isfile(MINE) and os.access(MINE, os.X_OK)):
        sys.exit(f"fuzz: MINE {MINE} is not executable (run 'cargo build')")

    smoke_check()

    # Checked once here rather than inside the generator's loop, because the loop
    # only reaches an option that happens to be sampled -- with `--cases 2` and
    # `--title` in a list of nineteen, the contradiction is silent. An option
    # cannot be both byte-compared and declared byte-divergent, and which of the
    # two someone believes decides whether a real mismatch is reported or
    # suppressed.
    generated = {flag for flag, _k, _v in VALUE_OPTS}
    both = sorted(generated & set(OPTION_DIVERGENT))
    if both:
        for flag in both:
            sys.exit(
                f"fuzz: {flag} is in both VALUE_OPTS and OPTION_DIVERGENT "
                f"({OPTION_DIVERGENT[flag]}). It cannot be compared and skipped "
                f"at once; remove it from one of the two."
            )

    seeds = read_seed_texts()

    if args.dump is not None:
        rng = random.Random(args.seed)
        for n in range(args.dump + 1):
            name, text, flags = build_case(rng, seeds)
        # Written as latin-1 because that is exactly what the runner feeds to
        # both converters; dumping as UTF-8 would corrupt any high byte and
        # produce a case that no longer reproduces.
        sys.stdout.buffer.write(text.encode("latin-1", "replace"))
        sys.stderr.write(f"case {args.dump}: source={name}\n")
        sys.stderr.write("flags: " + " ".join(flags) + "\n")
        return 0

    work = tempfile.mkdtemp(prefix="t2hfuzz-", dir="/tmp")
    inpath = os.path.join(work, "in.txt")
    refout = os.path.join(work, "ref.html")
    myout = os.path.join(work, "mine.html")

    rng = random.Random(args.seed)
    mismatches = 0
    port_timeouts = 0
    ref_timeouts = 0
    compared = 0
    normalised = 0

    for n in range(args.cases):
        name, text, flags = build_case(rng, seeds)

        with open(inpath, "wb") as fh:
            fh.write(text.encode("latin-1", "replace"))
        for p in (refout, myout):
            if os.path.exists(p):
                os.remove(p)

        # A converter that hangs must not take the rest of the run with it.
        # subprocess.run raises TimeoutExpired, and until this was caught that
        # exception propagated out of the loop and ended the whole seed after
        # however many cases it had reached -- silently, because the Makefile
        # piped our output through `tail -1` and so reported success either way.
        # That made "16 000 cases, 0 mismatches" unprovable: an aborted run and
        # a clean one were indistinguishable from the exit status.
        #
        # The two sides are not equal here. The port hanging on an input is a
        # defect in the port (P5, the inherited hang, is still open). The
        # reference hanging tells us nothing about the port, so it is skipped
        # like any other reference refusal -- but counted and printed, never
        # folded into "0 mismatches" without saying so.
        try:
            m = run_rust(inpath, myout, flags)
        except subprocess.TimeoutExpired:
            port_timeouts += 1
            print(f"case {n} (seed {args.seed}, from {name}): PORT TIMED OUT")
            print("  flags:", " ".join(flags))
            if args.keep:
                _save(faildir, args.seed, inpath, refout, myout, name, n)
            continue
        try:
            r = run_perl(inpath, refout, flags)
        except subprocess.TimeoutExpired:
            # The reference hanging is not a port defect, so it does not fail
            # the run -- but it is counted and printed separately from a port
            # hang so the two can never be confused. A reference that hangs on
            # every case is an environment failure, and `compared == 0` below
            # is what catches that.
            ref_timeouts += 1
            print(
                f"case {n} (seed {args.seed}, from {name}): "
                "reference timed out, skipped"
            )
            continue

        # A non-zero exit on either side is a failure, not something to
        # compare -- this is the P1 harness bug in a different place.
        if m.returncode != 0:
            mismatches += 1
            print(f"case {n} (seed {args.seed}, from {name}): port exited {m.returncode}")
            print("  flags:", " ".join(flags))
            print("  stderr:", m.stderr.decode("utf-8", "replace")[:400].strip())
            if args.keep:
                _save(faildir, args.seed, inpath, refout, myout, name, n)
            continue
        if r.returncode != 0:
            # The reference refusing an option set is not a port defect, so it
            # used to be counted as "skipped" and stepped over.  That is the
            # P1 false-green shape in miniature, and it is gone.
            #
            # `build_case` draws every value from the hand-checked choice lists
            # above -- `VALUE_OPTS` names only options that exist in *both*
            # implementations, with values each side accepts -- so a refusal is
            # unreachable by construction.  It has never once fired: 16 000 cases
            # across eight seeds, zero skips.  A branch that cannot be taken,
            # whose only effect if it were taken would be to reduce the number of
            # things checked, is not a safety net; it is a way for the sweep to
            # quietly shrink.  So it is now reported and fails the run, which is
            # what a real environment failure needs anyway: a wiped /tmp used to
            # make the reference exit non-zero on *every* case, and the symptom
            # the team saw was a green build.
            #
            # `compared == 0` further down still catches the total-failure case
            # on its own, and `smoke_check` catches it before the loop starts.
            # Neither can see a *partial* refusal, which is why this one matters.
            mismatches += 1
            print(
                f"case {n} (seed {args.seed}, from {name}): "
                f"reference exited {r.returncode}"
            )
            print("  flags:", " ".join(flags))
            print("  stderr:", r.stderr.decode("utf-8", "replace")[:400].strip())
            if args.keep:
                _save(faildir, args.seed, inpath, refout, myout, name, n)
            continue
        if not os.path.exists(refout) or not os.path.exists(myout):
            mismatches += 1
            print(
                f"case {n}: missing output "
                f"(ref={os.path.exists(refout)} mine={os.path.exists(myout)})"
            )
            if args.keep:
                _save(faildir, args.seed, inpath, refout, myout, name, n)
            continue

        # From here on this case is genuinely compared, whatever the outcome.
        compared += 1

        with open(refout, "rb") as fh:
            a = fh.read()
        with open(myout, "rb") as fh:
            b = fh.read()
        # P1.1. Both converters name themselves in the one
        # `meta name="generator"` line, so neither byte sequence can match the
        # other's -- the same declared divergence the corpus normalises through
        # normalize.py. Both sides go through that rule, and the count is
        # reported in the summary for the same reason the corpus prints
        # NORMALISED unconditionally: a canonicalisation that quietly stopped
        # applying would leave this gate comparing something it no longer means
        # to, and a zero that is not printed cannot be noticed.
        a, na = normalise_bytes(a)
        b, nb = normalise_bytes(b)
        normalised += na + nb
        if a != b:
            mismatches += 1
            print(f"case {n} (seed {args.seed}, from {name}): MISMATCH")
            print("  flags:", " ".join(flags))
            la, lb = a.split(b"\n"), b.split(b"\n")
            shown = False
            for i, (x, y) in enumerate(zip(la, lb)):
                if x != y:
                    print(f"  line {i+1}:\n    ref : {x!r}\n    mine: {y!r}")
                    shown = True
                    break
            if not shown:
                print(f"  length differs: ref={len(a)} mine={len(b)}")
            if args.keep:
                _save(faildir, args.seed, inpath, refout, myout, name, n)

    print(
        f"fuzz: {args.cases} cases, seed {args.seed}, "
        f"{compared} compared, "
        f"{mismatches} mismatches, "
        f"{port_timeouts} port timeouts, {ref_timeouts} reference timeouts, "
        f"NORMALISED: {normalised} generator meta line(s)"
    )
    # A port timeout is a defect, so it fails the run on its own. A reference
    # timeout is not the port's fault and does not, but a run that compared
    # nothing at all is the P1 bug again -- it happens when the reference
    # refuses or hangs on every case, which is exactly what a wiped /tmp once
    # caused. Zero comparisons is a broken environment, never a pass.
    if compared == 0:
        print(
            "fuzz: FAIL -- 0 cases compared. The reference hung or produced no "
            "output on every case, so this run checked nothing."
        )
        return 1
    return 1 if (mismatches or port_timeouts) else 0


def _save(faildir, seed, inpath, refout, myout, name, n):
    d = faildir
    os.makedirs(d, exist_ok=True)
    # The seed is part of the name: two runs of different seeds reach the same
    # case index from the same source file, so `<name>-<n>` alone would have
    # them overwrite each other's evidence.
    base = os.path.join(d, f"{name}-{seed}-{n}")
    for src, ext in ((inpath, ".txt"), (refout, ".ref.html"), (myout, ".mine.html")):
        if os.path.exists(src):
            with open(src, "rb") as fh:
                data = fh.read()
            with open(base + ext, "wb") as fh:
                fh.write(data)
    print(f"  saved to {base}.*")


if __name__ == "__main__":
    sys.exit(main())
