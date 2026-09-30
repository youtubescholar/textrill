#!/usr/bin/env python3
"""Minimise a fuzz mismatch to a small reproducible input.

Not part of the test suite: this is a debugging aid for reducing a failing
fuzz case (tests/corpus/fuzz.py --dump N) to something small enough to read.

    tests/corpus/minimise.py 519 --no-escape_HTML_chars --no-demoronize ...
"""
import os
import subprocess
import sys

REFDIR = os.environ.get("REFDIR", "/home/vicpu/build/ref/txt2html-3.0")
MINE = os.environ.get("MINE", "/home/vicpu/build/txt2html-rs/target/debug/txt2html")
TMP = "/tmp/opencode/minimise"
os.makedirs(TMP, exist_ok=True)
env = dict(os.environ, PERL5LIB=os.environ.get(
    "PERL5LIB", f"/home/vicpu/build/ref/stubs:{REFDIR}/lib"))


def differs(text, flags):
    p = os.path.join(TMP, "in.txt")
    r = os.path.join(TMP, "r.html")
    m = os.path.join(TMP, "m.html")
    with open(p, "wb") as fh:
        fh.write(text)
    for out in (r, m):
        if os.path.exists(out):
            os.remove(out)
    for out, cmd in (
        (r, ["perl", "scripts/txt2html", "--infile", p, "--default_link_dict", ""]),
        (m, [MINE, "--infile", p, "--default_link_dict", ""]),
    ):
        subprocess.run(
            cmd + flags + ["--outfile", out],
            cwd=REFDIR, env=env, capture_output=True, timeout=120)
    if not (os.path.exists(r) and os.path.exists(m)):
        return False
    with open(r, "rb") as fh:
        a = fh.read()
    with open(m, "rb") as fh:
        b = fh.read()
    return a != b


def minimise(text, flags):
    assert differs(text, flags), "input does not reproduce"
    # line-level
    lines = text.split(b"\n")
    changed = True
    while changed:
        changed = False
        i = 0
        while i < len(lines):
            trial = lines[:i] + lines[i + 1:]
            if differs(b"\n".join(trial), flags):
                lines = trial
                changed = True
            else:
                i += 1
    text = b"\n".join(lines)
    # character-level, repeated until stable
    changed = True
    while changed:
        changed = False
        i = 0
        while i < len(text):
            for n in (64, 16, 4, 1):
                if i + n > len(text):
                    continue
                trial = text[:i] + text[i + n:]
                if differs(trial, flags):
                    text = trial
                    changed = True
                    break
            else:
                i += 1
    return text


def main():
    case = sys.argv[1]
    flags = sys.argv[2:]
    with open(case, "rb") as fh:
        text = fh.read()
    small = minimise(text, flags)
    print(f"minimal input: {len(text)} -> {len(small)} bytes")
    print("flags:", " ".join(flags))
    print("repr:", repr(small))
    with open("/tmp/opencode/minimal.txt", "wb") as fh:
        fh.write(small)
    # Show the first differing line pair for the minimal input.
    p = "/tmp/opencode/minimal.txt"
    r = "/tmp/opencode/minim-r.html"
    m = "/tmp/opencode/minim-m.html"
    for out, cmd in (
        (r, ["perl", "scripts/txt2html", "--infile", p, "--default_link_dict", ""]),
        (m, [MINE, "--infile", p, "--default_link_dict", ""]),
    ):
        subprocess.run(cmd + flags + ["--outfile", out],
                       cwd=REFDIR, env=env, stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL)
    with open(r, "rb") as fh:
        a = fh.read().split(b"\n")
    with open(m, "rb") as fh:
        b = fh.read().split(b"\n")
    for i, (x, y) in enumerate(zip(a, b)):
        if x != y:
            print(f"line {i+1}:\n  ref : {x!r}\n  port: {y!r}")
            break
    else:
        print(f"lengths differ: ref={sum(map(len, a))} port={sum(map(len, b))}")


if __name__ == "__main__":
    main()
