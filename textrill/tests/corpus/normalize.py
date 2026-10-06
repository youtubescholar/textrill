#!/usr/bin/env python3
"""Canonicalise the generator meta line in a converted document.

Why this exists
---------------
`textrill` names itself as the generator in the `meta` it writes. The Perl
reference names itself. Those strings cannot both be present in a byte
comparison, and before P1.1 they were identical only because the port was
claiming a false provenance:

    reference: <meta name="generator" content="HTML::TextToHTML v3.0"/>
    textrill:  <meta name="generator" content="textrill v0.1.0"/>

Correcting that made the port fail 18 differential cases and 13 golden
comparisons. The alternative -- keeping the Perl string -- means every document
textrill writes states that the Perl module produced it.

So this rewrites the generator meta to a fixed sentinel before comparison, on
both sides. Everything else stays byte-exact.

What this deliberately is NOT
----------------------------
It is not a general normaliser, and it must not grow into one. A general
normaliser would quietly stop catching real divergences, which is the exact
failure this harness exists to prevent. It matches one line shape and nothing
else: a `meta` start tag whose `name` attribute is `generator`, in either
case, with either quoting. It will not touch a body paragraph that happens to
contain the word "generator".

Coverage is not lost
--------------------
The alternative to this file was `NOGOLDEN[]` on the 13 affected stems. That
suppresses the comparison entirely, so a real regression in those stems -- five
of which are list cases, the family that contains two known inherited defects --
would go unseen. Normalising one line keeps all 33 goldens compared.

The value is asserted, not ignored
----------------------------------
Normalising the line means the corpus no longer checks *which* generator is
named. That is checked instead, positively, by `tests/provenance.rs`, which
requires the exact expected string and fails if `HTML::TextToHTML` reappears in
the meta. So the two halves together are still a complete gate: this file
proves content parity with the reference, that test proves correct provenance.

Usage: normalize.py FILE...   (rewrites in place; prints how many lines changed)
"""

import re
import sys

# One shape only: a meta start tag carrying a name attribute of "generator".
# Deliberately not `.*generator.*` -- see the module docstring.
GENERATOR_META = re.compile(
    r"""^(\s*)<meta\b[^>]*?\bname\s*=\s*["']?generator["']?[^>]*>\s*$""",
    re.IGNORECASE,
)

SENTINEL = '<meta name="generator" content="NORMALISED"/>'


def normalise(path):
    """Rewrite generator meta lines in `path`. Returns the number replaced."""
    with open(path, "rb") as handle:
        raw = handle.read()

    text = raw.decode("utf-8", "surrogateescape")
    # splitlines(keepends=True) so the trailing newline state of the file is
    # preserved exactly; this file is compared against a reference, so a
    # normaliser that adds or drops a final newline changes the comparison.
    out = []
    replaced = 0
    for line in text.splitlines(keepends=True):
        body = line.rstrip("\r\n")
        if GENERATOR_META.match(body):
            indent = GENERATOR_META.match(body).group(1)
            ending = line[len(body):]
            out.append(indent + SENTINEL + ending)
            replaced += 1
        else:
            out.append(line)

    if replaced:
        with open(path, "wb") as handle:
            handle.write("".join(out).encode("utf-8", "surrogateescape"))
    return replaced


def main(argv):
    if not argv:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    total = 0
    for path in argv:
        try:
            total += normalise(path)
        except FileNotFoundError:
            # A missing file is the converter's failure to report, not ours to
            # paper over. The caller checks exit status separately, and cmp.py
            # reports MISSING for an absent side.
            continue
    print(total)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
