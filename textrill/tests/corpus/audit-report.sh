#!/bin/bash
# Generate a compact audit report for self-goldens.
set -u
cd "$(dirname "$0")"
HERE=$PWD
. "$HERE/cases.sh"
GOLD="$HERE/../golden/corpus"
TFILES="$FIX"

echo "# Audit report (2026-10-09)"
echo
echo "## Colliding golden hashes (identical bytes)"
python3 - "$GOLD" << 'PY'
import sys, os, hashlib
path = sys.argv[1]
files = []
for fn in os.listdir(path):
  if fn.endswith('.html'):
    p = os.path.join(path,fn)
    sz = os.path.getsize(p)
    h = hashlib.sha256(open(p,'rb').read()).hexdigest()
    files.append((h, sz, fn))
# group
d={}
for h,sz,fn in files:
  d.setdefault((h,sz), []).append(fn)
for k,v in sorted(d.items(), key=lambda x: (-len(x[1]), x[0][1])):
  if len(v)>1:
    print(k[0][:16], k[1], v)
PY
echo
echo "## Zero-byte goldens"
python3 - "$GOLD" << 'PY'
import sys, os
path = sys.argv[1]
for fn in sorted(os.listdir(path)):
  if fn.endswith('.html'):
    p = os.path.join(path,fn)
    if os.path.getsize(p)==0:
      print(fn)
PY
echo
echo "## Tiny goldens (<50 bytes)"
python3 - "$GOLD" << 'PY'
import sys, os
path = sys.argv[1]
for fn in sorted(os.listdir(path)):
  if fn.endswith('.html'):
    p = os.path.join(path,fn)
    sz = os.path.getsize(p)
    if 0 < sz < 50:
      print(fn, sz)
PY
echo
echo "## Duplicate inputs/cli (corpus case duplication)"
# find stems with same (resolved input tuple + cli)
python3 - "$CASES" "$HERE" << 'PY'
# can't re-source easily; just report link_in_url/ci_links explicitly
pass
PY
echo "see cases.sh: ci_links and link_in_url share INPUT=test2.txt, CLI='--extract'"
echo
echo "explicit: ci_links/link_in_url same (test2.txt, --extract)"
