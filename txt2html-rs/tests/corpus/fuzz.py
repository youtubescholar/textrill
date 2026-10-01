#!/usr/bin/env python3
"""Seeded differential fuzzer: this port vs the Perl reference.

Each case is a mutation of an upstream ``tfiles/*.txt`` corpus file, converted
by both ``HTML::TextToHTML`` 3.0 and the Rust binary under a random option set,
and the two outputs are compared byte for byte.

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

Environment overrides match ``run.sh``: REFDIR, MINE, RUNDIR, PERL5LIB.
"""

import argparse
import os
import random
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REFDIR = os.environ.get("REFDIR", "/home/vicpu/build/ref/txt2html-3.0")
MINE = os.environ.get("MINE", "/home/vicpu/build/txt2html-rs/target/debug/txt2html")
RUNDIR = os.environ.get("RUNDIR", "/tmp/opencode/corpus")
# Both halves live under ref/ in the repo.  An earlier version kept them in
# /tmp, and a reboot wiped them: the reference then exited non-zero on every
# case, which this fuzzer counts as "reference refused" and skips, so the run
# reported 0 mismatches having actually checked nothing.
os.environ.setdefault(
    "PERL5LIB", f"/home/vicpu/build/ref/stubs:{REFDIR}/lib")

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
    ("--style_url", "style_url", ["ascii.css", "s.css"]),
    ("--title", "title", ["Fuzz", "A & B", "1. Intro"]),
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
def sanitise(text):
    out = []
    for line in text.split("\n"):
        line = "".join(c if ord(c) < 0x80 else "?" for c in line)
        out.append(line.rstrip(" \t"))
    return "\n".join(out)


# Divergences that are known, understood and not yet fixed.  Each entry must
# pin the *output shape* as well as the options, so a second and different bug
# landing on the same options still gets reported; only an exact repeat of a
# known divergence is stepped over, and it is counted and printed.
#
# Every entry needs a minimal reproduction here so that whoever picks it up does
# not have to re-derive it.
#
# Currently empty, which is the state to keep it in.  The three that used to
# live here were all fixed and are now pinned as fixed cases in cases.sh
# (table_type_replace, table_type_named, pre_explicit_blank): a
# table_type set that merged into the defaults instead of replacing them, and
# an explicit-quote PRE block that dropped everything after its first blank
# line.  If a sweep starts printing "N known", something listed here has
# regressed or a new bug matches a recorded shape exactly.
# Divergences that are known, understood, and still unfixed.  Each entry keeps
# the fuzzer quiet about one defect so that it can find the next one; removing an
# entry is part of fixing the defect it describes.
#
# This list is empty because its only occupant, the CR path's phantom trailing
# paragraph, is fixed (E3).  The entry described a signature -- "the port's line
# list is the reference's with two blank lines spliced in" -- rather than the
# defect, so leaving it in place after the fix would have suppressed the same
# regression the moment it came back, and the fuzzer would have had nothing to
# say about it.  A recurrence should be reported, not suppressed: add an entry
# when a divergence is diagnosed but not yet fixed, and delete it in the same
# change that fixes it.
KNOWN_DIVERGENCES: list[dict] = []



def first_difference(a, b):
    """Return (ref_line, mine_line) at the first differing line, else None."""
    la, lb = a.split(b"\n"), b.split(b"\n")
    for x, y in zip(la, lb):
        if x != y:
            return x, y
    if len(la) != len(lb):
        return la[min(len(la), len(lb)) :][0:1] or b"", lb[min(len(la), len(lb)) :][0:1] or b""
    return None


def _contains(haystack, needle):
    n = len(needle)
    return any(tuple(haystack[i : i + n]) == needle for i in range(len(haystack) - n + 1))


def _is_extra_blank_lines(a, b, n):
    """True when the port's lines are the reference's with n blanks spliced in."""
    la, lb = a.split(b"\n"), b.split(b"\n")
    if len(lb) != len(la) + n:
        return False
    return any(la[:i] + [b""] * n + la[i:] == lb for i in range(len(la) + 1))


def known_divergence(flags, a, b):
    for k in KNOWN_DIVERGENCES:
        # Each entry in "requires" is a contiguous run of argv that has to be
        # present somewhere, in any order and with any other options around it.
        if not all(_contains(flags, r) for r in k["requires"]):
            continue
        if "extra_blank_lines" in k:
            if _is_extra_blank_lines(a, b, k["extra_blank_lines"]):
                return True
        elif first_difference(a, b) == (k["ref"], k["mine"]):
            # The exact first differing line is the discriminator, so a
            # different bug landing on the same options is still reported.
            return True
    return False


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

PERL_DRIVER = r"""
use HTML::TextToHTML;
my ($in, $out, @pairs) = @ARGV;
my %o = (infile => [$in], outfile => $out, default_link_dict => "");
for my $kv (@pairs) {
    my ($k, $v) = split(/=>/, $kv, 2);
    # A leading % marks a value that has to be built into a hashref; this is
    # how the reference stores table_type (init_our_data sets it to a hashref,
    # and scripts/txt2html gets one from Getopt::Long's n% spec).
    if ($v =~ /^%(.*)$/s) {
        my %h;
        for my $pair (split(/,/, $1)) {
            my ($kk, $vv) = split(/=/, $pair, 2);
            $h{$kk} = $vv;
        }
        $o{$k} = \%h;
    }
    else {
        $o{$k} = $v;
    }
}
# Options go to the constructor and txt2html() is called with no arguments,
# exactly as scripts/txt2html does (new(%{$args_ref}) then txt2html()).  This is
# not cosmetic: the reference derives lower_case_tags from xhtml and loads the
# link dictionary during construction, so passing options to txt2html() instead
# produces a different document for the same option values.  run.sh's Perl
# driver passes them to txt2html() and compensates with CTOR[]; here there is
# no such table, so the CLI's own shape is the one to mirror.
my $c = HTML::TextToHTML->new(%o);
$c->txt2html();
"""


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

    A case where the reference exits non-zero is counted as "the reference
    refused this option set" and skipped.  That is right for a genuinely
    invalid option set and catastrophic for a broken PERL5LIB: every case gets
    skipped and the run prints "0 mismatches" having compared nothing at all --
    the same false-green shape as the P1 harness bug.  So prove both sides
    convert a known input before the loop starts.
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
    skipped = 0
    known = 0
    port_timeouts = 0
    ref_timeouts = 0
    compared = 0

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
            skipped += 1
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
            # The reference refusing an option set is not a port defect.
            skipped += 1
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
        if a != b and known_divergence(flags, a, b):
            known += 1
            if args.keep:
                _save(faildir, args.seed, inpath, refout, myout, name, n)
            continue
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

    timeouts = port_timeouts + ref_timeouts
    print(
        f"fuzz: {args.cases} cases, seed {args.seed}, "
        f"{compared} compared, "
        f"{mismatches} mismatches, {known} known, "
        f"{skipped} skipped (reference refused), "
        f"{timeouts} timed out (port {port_timeouts}, reference {ref_timeouts})"
    )
    # A port timeout is a defect, so it fails the run on its own. A reference
    # timeout is not the port's fault and does not, but a run that compared
    # nothing at all is the P1 bug again -- it happens when the reference
    # refuses or hangs on every case, which is exactly what a wiped /tmp once
    # caused. Zero comparisons is a broken environment, never a pass.
    if compared == 0:
        print(
            "fuzz: FAIL -- 0 cases compared. The reference refused or hung on "
            "every case, so this run checked nothing."
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
