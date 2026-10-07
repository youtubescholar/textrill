#!/usr/bin/env python3
# textrill — convert plain text to HTML.
#
# Copyright (C) 2026 the textrill authors.
#
# This program is free software: you can redistribute it and/or modify it
# under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.  See the LICENSE file for the full text.

"""Check the positioning claims in docs/OFFERING.md section 3.

A claim that nobody re-measures is the failure this project has already had
once: legacy-archive/TOOL-SURVEY.md carried a feature matrix that went false
while still looking authoritative, and DOCS.md exists to end that. The claims
made about pandoc are the same species of document -- they are statements about
another program, on this machine, today -- so they are gated rather than
written down once.

Every number pinned here appears in docs/OFFERING.md §3 with the command that
produced it. If this script fails, the document is stale: re-run the commands,
read what changed, and update the document in the same change. Drift is the
signal, not the noise -- a pandoc that grows a plain-text reader is a real
change to the claim, and the gate is how that becomes visible.

    python3 textrill/tests/positioning.py      # or: make proof

Requires pandoc on PATH (override with PANDOC=...), because the claims are
about pandoc. Not part of `make verify`: verify must stay runnable on a machine
with only Rust and perl installed. Exit status is 0 only if every claim still
holds.
"""

import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MINE = Path(os.environ.get("MINE") or ROOT / "textrill/target/release/textrill")
PANDOC = os.environ.get("PANDOC") or "pandoc"
TARBALL = ROOT / "competition-files/pandoc-3.12.tar.gz"
EXAMPLE = ROOT / "examples/homer.txt"

# docs/OFFERING.md §3 records the pandoc the numbers were measured against.
# A different pandoc is not automatically wrong -- it is *unreviewed*: the
# document says one thing about a machine that now says another, and somebody
# has to look at both. So a version difference fails the same way a content
# difference does, with the version in the message.
PANDOC_VERSION = "3.1.3"

# The claims themselves. Structure is (id, what the document says, expected,
# actual); a mismatch prints all four, so the report names the claim rather
# than a line number.
FIXTURE = """Shopping list
=============

Things to buy:
    milk, two pints
    eggs, a dozen

Alice said NO.
"""

results = []


def check(cid, claim, expected, actual):
    results.append((cid, claim, str(expected), str(actual), expected == actual))


def skip(cid, claim, why):
    results.append((cid, claim, "-", f"skipped: {why}", None))


def run(argv, **kw):
    return subprocess.run(argv, capture_output=True, text=True, **kw)


def paragraphs(html):
    """Paragraph count and the longest one, with tags removed.

    The measure that matters for the soft-wrap claim is the largest paragraph:
    markdown joining lines the author meant to keep apart shows up as one very
    long <p>, not as a smaller count elsewhere.
    """
    blocks = re.findall(r"<p[^>]*>(.*?)</p>", html, re.S)
    text = [re.sub(r"<[^>]+>", "", b) for b in blocks]
    return len(blocks), max((len(t) for t in text), default=0)


def main():
    if shutil.which(PANDOC) is None:
        print(
            f"positioning: {PANDOC} not found. This gate measures claims about "
            "pandoc; install it or set PANDOC= to its path.",
            file=sys.stderr,
        )
        return 2
    if not MINE.exists():
        print(f"positioning: no textrill binary at {MINE}; run `make build`",
              file=sys.stderr)
        return 2

    ver = run([PANDOC, "--version"]).stdout.splitlines()[0]
    check("P0", f"documented pandoc version ({PANDOC_VERSION})",
          f"pandoc {PANDOC_VERSION}", ver)

    # P1: pandoc has no plain-text reader. This is the load-bearing claim of
    # the whole positioning -- "pandoc converts documents, it does not read
    # prose" -- so it is checked twice: against the binary and against the
    # source we ship as research material.
    formats = run([PANDOC, "--list-input-formats"]).stdout.split()
    check("P1", "pandoc lists no plain-text input format",
          "plain absent", "plain absent" if "plain" not in formats else "plain present")
    check("P1b", "pandoc input format count", 43, len(formats))

    r = run([PANDOC, "-f", "plain", "-t", "html", str(EXAMPLE)])
    check("P2", "pandoc -f plain is rejected", True, r.returncode != 0)

    if TARBALL.exists():
        with tarfile.open(TARBALL) as tf:
            member = tf.getmember("pandoc-3.12/src/Text/Pandoc/Readers.hs")
            src = tf.extractfile(member).read().decode("utf-8", "replace")
        readers = re.findall(r'\(\s*"([a-z0-9_]+)"\s*,', src)
        check("P3", "pandoc 3.12 source registers no plain reader",
              "plain absent", "plain absent" if "plain" not in readers else "plain present")
    else:
        skip("P3", "pandoc 3.12 source registers no plain reader",
             f"{TARBALL.name} not present")

    # P4/P5: the Odyssey, converted by each tool. textrill's numbers are also
    # in docs/CAPABILITIES.md §2 and §5; pandoc's are in §5 and
    # docs/LANDSCAPE.md §2.
    t = run([str(MINE), "--infile", str(EXAMPLE), "--outfile", "-"]).stdout
    p = run([PANDOC, "-f", "markdown", "-t", "html", str(EXAMPLE)]).stdout

    t_paras, t_max = paragraphs(t)
    p_paras, p_max = paragraphs(p)
    check("P4", "textrill on homer.txt: <strong> recovered", 39, t.count("<strong>"))
    check("P5", "textrill on homer.txt: <br/> line breaks", 34, t.count("<br/>"))
    check("P6", "textrill on homer.txt: headings (none in the document)",
          0, len(re.findall(r"<h[1-6]>", t)))
    check("P7", "textrill on homer.txt: paragraphs / largest", (64, 3029),
          (t_paras, t_max))
    check("P8", "textrill on homer.txt: bytes out", 38882, len(t.encode()))

    check("P9", "pandoc -f markdown on homer.txt: <strong>", 0, p.count("<strong>"))
    check("P10", "pandoc -f markdown on homer.txt: line breaks", 0, p.count("<br"))
    check("P11", "pandoc -f markdown on homer.txt: paragraphs / largest (merged)",
          (62, 3665), (p_paras, p_max))
    check("P12", "pandoc -f markdown on homer.txt: bytes out", 38132, len(p.encode()))

    # P13: the mechanism behind P11, on input small enough to read. Markdown's
    # soft-wrap joining rule assumes a line break was not meant; on text whose
    # line structure *is* the structure, it is information loss you can see.
    with tempfile.TemporaryDirectory() as td:
        f = Path(td) / "fixture.txt"
        f.write_text(FIXTURE)
        tf_html = run([str(MINE), "--infile", str(f), "--outfile", "-"]).stdout
        pf_html = run([PANDOC, "-f", "markdown", "-t", "html", str(f)]).stdout
    check("P14", "textrill keeps indented lines apart (<br/> present)",
          True, "<br/>" in tf_html)
    joined = re.sub(r"<[^>]+>", " ", pf_html)
    merged = "Things to buy: milk, two pints eggs, a dozen"
    check("P15", "pandoc joins those lines into one paragraph",
          merged, merged if merged in joined else "(not found: no merge)")

    # P16: our own surface, which README.md and docs/CAPABILITIES.md §1 both
    # call "67 options".
    help_text = run([str(MINE), "--help"]).stdout
    opts = re.findall(r"^    [-a-zA-Z_]", help_text, re.M)
    check("P16", "textrill --help option surface", 67, len(opts))

    # P17-P20: the conjunction claim in docs/OFFERING.md §2 -- textrill reads
    # what pandoc will not, pandoc writes what we do not, so the two are the
    # front and back halves of one pipeline rather than rivals. What matters is
    # that the structure we *inferred* reaches the far format: a <strong> a
    # reader would not have written must still be bold in the .docx, and a
    # short line we turned into <br/> must still be a break.
    #
    # The epub assertion is structural (its mimetype entry), not a byte count:
    # a zip's bytes depend on the clock, and a claim that fails when the minute
    # changes is not a claim.
    with tempfile.TemporaryDirectory() as td:
        epub_path = Path(td) / "pipe.epub"
        docx_path = Path(td) / "pipe.docx"
        shell = run([str(MINE), "--title", "The Odyssey", "--infile",
                     str(EXAMPLE), "--outfile", "-"]).stdout
        e = run([PANDOC, "-f", "html", "-t", "epub", "-o", str(epub_path)],
                input=shell)
        d = run([PANDOC, "-f", "html", "-t", "docx", "-o", str(docx_path)],
                input=shell)
        md = run([PANDOC, "-f", "html", "-t", "markdown"], input=shell).stdout

        # Read both files before the temporary directory goes away.
        check("P17", "textrill HTML -> pandoc epub/docx both succeed", (0, 0),
              (e.returncode, d.returncode))
        try:
            mimetype = zipfile.ZipFile(epub_path).read("mimetype").decode()
        except Exception as exc:  # missing, or not a zip: either way not an epub
            mimetype = f"unreadable: {exc}"
        check("P18", "textrill HTML -> epub carries the epub mimetype",
              "application/epub+zip", mimetype)
        try:
            docx_xml = zipfile.ZipFile(docx_path).read("word/document.xml") \
                .decode()
            docx_counts = (docx_xml.count("<w:b"), docx_xml.count("<w:br"))
        except Exception as exc:  # missing, or not a zip
            docx_counts = f"unreadable: {exc}"
        check("P19", "inferred structure survives into docx (<w:b> bold runs, "
                     "<w:br> line breaks)", (113, 34), docx_counts)

    check("P20", "inferred <strong> survives into pandoc markdown", True,
          "**PREFACE TO FIRST EDITION**" in md)

    try:
        mine_desc = MINE.relative_to(ROOT)
    except ValueError:
        mine_desc = MINE
    print(f"positioning: docs/OFFERING.md §3 checked against {ver}, "
          f"textrill at {mine_desc}")
    failed = 0
    for cid, claim, expected, actual, ok in results:
        if ok is None:
            print(f"  SKIP {cid} {claim} ({actual})")
        elif ok:
            print(f"  OK   {cid} {claim} = {actual}")
        else:
            failed += 1
            print(f"  FAIL {cid} {claim}")
            print(f"         documented: {expected}")
            print(f"         measured:   {actual}")
    if failed:
        print(
            f"positioning: {failed} of {len(results)} claims drifted. "
            "Re-measure with the commands in docs/OFFERING.md §3 and update "
            "the document in the same change.",
            file=sys.stderr,
        )
        sys.stdout.flush()
        return 1
    print(f"positioning: {len(results)} claims, 0 drifted")
    return 0


if __name__ == "__main__":
    sys.exit(main())
