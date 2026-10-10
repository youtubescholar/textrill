#!/usr/bin/env python3
# textrill - convert plain text to HTML.
#
# Copyright (C) 2026 the textrill authors.
#
# This program is free software: you can redistribute it and/or modify it
# under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.  See the LICENSE file for the full text.

"""Reference-free acceptance for the real-document examples.

The differential corpus (run.sh) and the reference-free case runner (accept.sh)
convert short synthetic inputs. These are the opposite: documents a person
actually wrote, no markup language, nothing that announces its structure. There
is no oracle for "did you read this document the way a person would" -- which is
exactly why the record here is a *frozen* one. Each example's output is captured
to `tests/golden/examples/<name>.html` with `--report` added, and this script
re-converts and compares every run. A layout-inference regression that moves the
headings now shows up as a byte diff instead of a shrug.
"""

import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MINE = os.environ.get("MINE") or ROOT / "textrill/target/release/textrill"
EXAMPLES = ROOT / "examples"
GOLDEN = ROOT / "textrill/tests/golden/examples"
HERE = ROOT / "textrill/tests/corpus"

REPORT_LINE = re.compile(r"textrill:\s+report\s+bytes=\d+.*")
REPORT_SUFFIX = re.compile(r"^textrill:\s+report\s+")


def normalise(path: Path) -> None:
    """Apply the corpus's one canonicalisation (the generator meta), so a
    capture does not go stale the way the crate version bumps. Same rule, same
    file the corpus uses: one copy of a normaliser, or they drift."""
    subprocess.run(
        [sys.executable, str(HERE / "normalize.py"), str(path)],
        check=True,
        capture_output=True,
    )


def main(argv):
    write = "--write" in argv
    failures = []
    files = sorted(EXAMPLES.glob("*.txt"))
    if not files:
        print("no examples in examples/", file=sys.stderr)
        return 1
    for f in files:
        out = GOLDEN / (f.stem + ".html")
        tmp = Path(os.environ.get("TMPDIR", "/tmp")) / ("textrill-example-" + f.stem + ".html")
        try:
            r = subprocess.run(
                [str(MINE), "--infile", str(f), "--outfile", str(tmp), "--report"],
                capture_output=True,
                text=True,
            )
        except FileNotFoundError:
            print(f"FAIL  {f.name}: no textrill binary at {MINE}", file=sys.stderr)
            return 1
        if r.returncode != 0:
            print(f"FAIL  {f.name}: converter exited {r.returncode}")
            failures.append(f.name)
            tmp.unlink(missing_ok=True)
            continue
        rep = (r.stderr or "").strip()
        if not REPORT_LINE.match(rep):
            print(f"FAIL  {f.name}: no report line ({rep!r})")
            failures.append(f.name)
            tmp.unlink(missing_ok=True)
            continue
        assert tmp.is_file(), f"{tmp} was not written"
        normalise(tmp)
        if write:
            GOLDEN.mkdir(parents=True, exist_ok=True)
            tmp.replace(out)
            print(f"wrote {out.name}")
        else:
            if not out.is_file():
                print(f"FAIL  {f.name}: no golden at tests/golden/examples/{out.name}")
                failures.append(f.name)
            elif tmp.read_bytes() != out.read_bytes():
                print(f"FAIL  {f.name}: differs from golden/examples/{out.name}")
                failures.append(f.name)
            else:
                print(f"OK    {f.name}")
        tmp.unlink(missing_ok=True)
        print(f"      {REPORT_SUFFIX.sub('', rep)}")
    if failures:
        print(f"golden: {len(files) - len(failures)}/{len(files)} passed, failing:"
              f" {', '.join(failures)}")
        return 1
    print(f"examples: {len(files)}/{len(files)} frozen outputs reproduced")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))