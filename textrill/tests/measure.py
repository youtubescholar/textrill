#!/usr/bin/env python3
# textrill — convert plain text to HTML.
#
# Copyright (C) 2026 the textrill authors.
#
# This program is free software: you can redistribute it and/or modify it
# under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.  See the LICENSE file for the full text.

"""Compute and pin the Phase 5.1 measurement table over `examples/`.

Phase 5's candidate heading rules must be measured, not assumed (docs/PLAN.md
Phase 5): "how many candidate headings, how many false positives on prose that
is not structured". This script is that measurement. It re-derives the rule
counts from the corpus, labels each hit against a per-document truth set, and
pins every cell, so the table cannot drift the way the plan's own 2026-10-06
data point did (R2b was published as 3; measured, it is 2 — the title block's
long third line disqualifies it).

Truth labels, with provenance:

- homer.txt: the three genuine section starts PLAN.md measured (lines 38, 78,
  102 = PREFACE TO FIRST EDITION, PREFACE TO SECOND EDITION, THE ODYSSEY).
  The contents-list copies at lines 5-7 are not truth.
- gelbenhuegel.txt: the author's literal markdown headings (`^#{1,6} `
  lines); derived and asserted equal to the frozen set, so a corpus edit fails
  loudly instead of quietly moving the pins.
- every other document: 0 true headings by selection, so every hit is a false
  positive.

Rules measured (definitions mirror the engine's defaults; see src/convert.rs
and docs/PLAN.md):

- setext (baseline): the current default rule — a block whose first line is
  followed by an underline (`---` etc.). Reported by the tool itself via
  `--report`, not re-implemented here.
- R2a: a short all-caps line alone between blank lines.
- R2b: a block whose every line is short and all-caps.
- R2c: a block that starts with an all-caps line.

`table` below is the record; a changed corpus cell fails this script, exactly
like `make proof` fails on a drifted OFFERING claim. Not part of `make verify`.

    python3 textrill/tests/measure.py        # or: make measure

Exit status is 0 only if every cell still holds.
"""

import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MINE = os.environ.get("MINE") or ROOT / "textrill/target/release/textrill"

SHORT_LINE_LENGTH = 40
MIN_CAPS_LENGTH = 3

TRUTH = {
    "homer.txt": {38, 78, 102},
    "gelbenhuegel.txt": {200, 208, 214, 220, 237, 248, 257, 311, 330, 1031},
    "blake.txt": set(),
    "calli.txt": set(),
    "erya.txt": set(),
    "mohe_zhiguan_vol001.txt": set(),
    "septuagint_swete_genesis.txt": set(),
    "talmud.txt": set(),
}

# The record: (setext_hits, setext_false, r2a_hits, r2a_false, r2b_hits,
# r2b_false, r2c_hits, r2c_false) per document, as measured and frozen for S8.
# A corpus or rule edit that moves any cell fails the gate; the same way the
# plan's original R2b=3 was found stale, a silent drift here is the signal.
TABLE = {
    "homer.txt": (0, 0, 2, 2, 2, 2, 6, 3),
    "gelbenhuegel.txt": (16, 16, 0, 0, 0, 0, 0, 0),
    "blake.txt": (0, 0, 27, 27, 27, 27, 27, 27),
    "calli.txt": (0, 0, 158, 158, 158, 158, 159, 159),
    "erya.txt": (0, 0, 0, 0, 0, 0, 0, 0),
    "mohe_zhiguan_vol001.txt": (0, 0, 0, 0, 0, 0, 0, 0),
    "septuagint_swete_genesis.txt": (0, 0, 0, 0, 0, 0, 0, 0),
    "talmud.txt": (0, 0, 0, 0, 0, 0, 0, 0),
}


def iscaps(line):
    return bool(line) and bool(
        re.fullmatch(rf"[^a-z<]*[A-Z]{{{MIN_CAPS_LENGTH},}}[^a-z<]*", line)
    )


def short(line):
    return len(line) < SHORT_LINE_LENGTH


def blocks(lines):
    out, cur = [], []
    for i, l in enumerate(lines, 1):
        if not l.strip():
            if cur:
                out.append(cur)
                cur = []
        else:
            cur.append((i, l))
    if cur:
        out.append(cur)
    return out


def split(rule_hits, truth):
    starts = {b[0][0] for b in rule_hits}
    true = starts & truth
    return (len(starts), len(starts) - len(true))


def report_and_headings(path):
    r = subprocess.run(
        [str(MINE), "--infile", str(path), "--outfile", "-", "--report"],
        capture_output=True,
        text=True,
        check=True,
    )
    m = re.search(r"headings=(\d+)", r.stderr)
    hits = int(m.group(1)) if m else 0
    inner = re.findall(r"<h[1-6][^>]*>(.*?)</h[1-6]>", r.stdout, re.S)
    texts = [re.sub(r"<[^>]+>", "", t).strip() for t in inner]
    return hits, texts


def main():
    rows = []
    for name, truth in TRUTH.items():
        path = ROOT / "examples" / name
        lines = path.read_text(encoding="utf-8").split("\n")
        blk = blocks(lines)

        if name == "gelbenhuegel.txt":
            derived = {i for i, l in enumerate(lines, 1) if re.match(r"^#{1,6} ", l)}
            assert derived == truth, f"{name}: markdown headings changed {derived}"

        setext_h, setext_txt = report_and_headings(path)
        truth_txt = {lines[i - 1].strip() for i in truth if i <= len(lines)}
        setext_true = sum(1 for t in setext_txt if t in truth_txt)
        setext_f = setext_h - setext_true

        r2a = split([b for b in blk if len(b) == 1 and iscaps(b[0][1]) and short(b[0][1])], truth)
        r2b = split([b for b in blk if all(iscaps(l) and short(l) for _, l in b)], truth)
        r2c = split([b for b in blk if iscaps(b[0][1])], truth)

        row = (setext_h, setext_f, r2a[0], r2a[1], r2b[0], r2b[1], r2c[0], r2c[1])
        rows.append((name, row))
        if TABLE.get(name) != row:
            print(f"DRIFT  {name}: recorded {TABLE.get(name)} != measured {row}")
            sys.exit(1)

    print("phase 5.1 measurement over examples/  (hits/false-positives per rule)")
    print("doc                         setext     R2a        R2b        R2c      truth")
    for name, row in rows:
        cells = " ".join(f"{row[i]}/{row[i + 1]:<2}" for i in (0, 2, 4, 6))
        t = len(TRUTH[name])
        print(f"{name:<26} {cells}   {t}")
    print("verdict: no rule is near zero false positives on prose that is not")
    print("structured; every candidate heading rule stays a proposal (Phase 5.1).")


if __name__ == "__main__":
    main()