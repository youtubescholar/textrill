#!/bin/bash
# Reference-free acceptance: compare textrill's output to frozen self-goldens.
#
# This is the replacement for the Perl differential as the *gate*. The
# differential (run.sh) can only say "different from Perl", never "wrong"; a
# frozen golden is a reviewed expectation, and it needs no Perl, no reference
# checkout and no CP437/CP1252 guessing on the other side.
#
# Two independent expectations are checked, where each exists:
#
#   SELF   tests/golden/corpus/<stem>.html -- the port's own reviewed output,
#          captured with --write and committed. This is what makes an option set
#          that is merely self-consistent fail: a wrong pattern applied to both
#          sides of a differential still matches; a frozen expectation does not.
#   AUTHOR tests/corpus/tfiles/good_<stem>.html -- the upstream author's own
#          expected output, a tracked copy, hand-written rather than frozen from
#          the port. Where both exist they are two genuinely different oracles.
#
# Exit status is non-zero if any case errors, any self-golden differs, or any
# author golden differs. A case that cannot run is a failure, not a skip: the
# whole point of building this was to stop the false green where both sides fail
# identically and the comparison "passes".
#
# Usage:
#   tests/corpus/accept.sh                 # all cases, compare
#   tests/corpus/accept.sh <stem>          # one case, compare
#   tests/corpus/accept.sh --write         # (re)capture every self-golden
#   tests/corpus/accept.sh --write <stem>  # (re)capture one
#
# --write is the *capture* half of the golden workflow. A captured file is not
# believed until it has been reviewed in `git diff` and the differential has
# been run beside it; that review is the gate, not the capture.

set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
MINE="${MINE:-$ROOT/target/debug/textrill}"
# The frozen port outputs. Distinct from tests/corpus/tfiles, which holds the
# upstream inputs and the author goldens.
GOLDEN="${GOLDEN:-$HERE/../golden/corpus}"
GOLDEN_AUTHOR="${GOLDEN_AUTHOR:-$HERE/tfiles}"
RUNDIR="${RUNDIR:-${TMPDIR:-/tmp}/textrill-accept}"
CASES="${CASES:-$HERE/cases.sh}"

WRITE=0
if [ "${1:-}" = "--write" ]; then
  WRITE=1
  shift
fi

if [ ! -x "$MINE" ]; then
  echo "ERROR: no textrill binary at $MINE" >&2
  echo "       Run 'cargo build' (or set MINE)." >&2
  exit 1
fi

rm -rf "$RUNDIR"
mkdir -p "$RUNDIR" "$GOLDEN"

# cases.sh sets FIX/WF from $HERE; source it, then the shared guards.
# shellcheck source=tests/corpus/cases.sh
# shellcheck disable=SC1091
. "$CASES"
# shellcheck source=tests/corpus/lib.sh
# shellcheck disable=SC1091
. "$HERE/lib.sh"

# SELF_GOLDEN_N and AUTHOR_GOLDEN_N count comparisons, not cases: a case can
# have neither, either, or both, and the summary has to describe what actually
# ran, not how many stems the table holds.
SELF_FAILS=(); SELF_N=0
AUTHOR_FAILS=(); AUTHOR_N=0

run_port() {
  local stem="$1" cli="$2"
  CASE_ERR=""
  # A leftover output from an earlier run would otherwise be compared and could
  # report a PASS for a case whose conversion just failed. Belt and braces with
  # the rm -rf at start-up, so the script is also correct for a single stem.
  rm -f "$RUNDIR/$stem.html"
  local -a infiles=()
  IFS=',' read -r -a infiles <<< "${INPUT[$stem]:-$stem.txt}"
  local -a mine_infile=()
  for f in "${infiles[@]}"; do
    # An absolute path is used as-is; otherwise it is an upstream fixture under
    # the tracked tests/corpus/tfiles.
    local path="$FIX/$f"
    case "$f" in /*) path="$f" ;; esac
    mine_infile+=(--infile "$path")
  done
  local -a dicts=()
  IFS=',' read -r -a dicts <<< "${DICT[$stem]-}"
  local -a mine_dict=()
  for d in "${dicts[@]}"; do
    [ -n "$d" ] || continue
    mine_dict+=(--links_dictionaries "$d")
  done
  local -a cliargs=()
  eval "cliargs=($cli)"
  # Run from the scratch directory: with every path above absolute, cwd must not
  # be able to change the result, and a known-empty cwd is the way to prove it.
  (cd "$RUNDIR" && "$MINE" "${mine_infile[@]}" "${mine_dict[@]}" \
     --outfile "$RUNDIR/$stem.html" --default_link_dict "" --xhtml \
     "${cliargs[@]}" 2>"$RUNDIR/$stem.err")
  local rc=$?
  if [ "$rc" -ne 0 ]; then
    CASE_ERR="port exited $rc: $(head -c 200 "$RUNDIR/$stem.err" | tr '\n' ' ')"
  fi
  # Canonicalise the one line the port is allowed to differ on from any older
  # capture: the generator meta, which names the crate version. See normalize.py.
  # Applied to the fresh output before every comparison and to nothing else.
  python3 "$HERE/normalize.py" "$RUNDIR/$stem.html" >/dev/null 2>&1
}

self_check() {
  local stem="$1"
  local g="$GOLDEN/$stem.html"
  if [ "$WRITE" -eq 1 ]; then
    cp "$RUNDIR/$stem.html" "$g"
    echo "  SELF written"
    SELF_N=$((SELF_N + 1))
    return
  fi
  if [ ! -f "$g" ]; then
    echo "  SELF FAIL (no self-golden at golden/corpus/$stem.html)"
    SELF_FAILS+=("$stem (missing)")
    SELF_N=$((SELF_N + 1))
    return
  fi
  if [ -n "$CASE_ERR" ]; then
    echo "  SELF not compared (the converter errored)"
    SELF_FAILS+=("$stem (converter error)")
    SELF_N=$((SELF_N + 1))
    return
  fi
  if LC_ALL=C cmp -s "$g" "$RUNDIR/$stem.html"; then
    echo "  SELF pass"
  else
    echo "  SELF FAIL (differs from golden/corpus/$stem.html)"
    diff "$g" "$RUNDIR/$stem.html" | head -6 | sed 's/^/    /'
    SELF_FAILS+=("$stem")
  fi
  SELF_N=$((SELF_N + 1))
}

# The author golden is the *independent* half: hand-written upstream, not frozen
# from the port. A NOGOLDEN[] entry means the author golden legitimately cannot
# match -- custom-headers2 (shared-converter state) and pre2 (a trailing newline
# upstream's own comparison strips). Those are skipped with the reason, exactly
# as the differential skips them.
author_check() {
  local stem="$1"
  [ -n "${NOGOLDEN[$stem]+x}" ] && { echo "  AUTHOR skipped (${NOGOLDEN[$stem]})"; return; }
  local g="$GOLDEN_AUTHOR/${GOLDEN[$stem]:-good_$stem.html}"
  [ -f "$g" ] || { echo "  AUTHOR none"; return; }
  if [ -n "$CASE_ERR" ]; then
    echo "  AUTHOR not compared (the converter errored)"
    AUTHOR_FAILS+=("$stem (converter error)")
    AUTHOR_N=$((AUTHOR_N + 1))
    return
  fi
  local gn="$RUNDIR/author-$stem.html"
  cp "$g" "$gn"
  python3 "$HERE/normalize.py" "$gn" >/dev/null 2>&1
  AUTHOR_N=$((AUTHOR_N + 1))
  if LC_ALL=C cmp -s "$gn" "$RUNDIR/$stem.html"; then
    echo "  AUTHOR pass"
  else
    echo "  AUTHOR FAIL (differs from tfiles/$(basename "$g"))"
    diff "$gn" "$RUNDIR/$stem.html" | head -6 | sed 's/^/    /'
    AUTHOR_FAILS+=("$stem")
  fi
}

one_case() {
  local stem="$1"
  run_port "$stem" "${CLI[$stem]}"
  echo "== $stem =="
  [ -n "$CASE_ERR" ] && echo "  ERROR: $CASE_ERR"
  self_check "$stem"
  author_check "$stem"
}

if [ "$#" -gt 0 ]; then
  stem="$1"
  if [ -z "${EXTRA[$stem]+x}" ]; then
    echo "ERROR: no such case '$stem'" >&2
    exit 2
  fi
  # The same table guards as the differential: developing one case is where a
  # misaligned or duplicated key is introduced, so they must fire here too.
  rc=0
  duplicate_key_check "$CASES" || rc=1
  alignment_check "" || rc=1
  one_case "$stem"
  [ -n "$CASE_ERR" ] && rc=1
  [ "${#SELF_FAILS[@]}" -gt 0 ] && rc=1
  [ "${#AUTHOR_FAILS[@]}" -gt 0 ] && rc=1
  exit "$rc"
fi

# Full run. Table guards first: a case that does not run cannot fail, and the
# count below is only meaningful once every declared case actually runs.
if ! duplicate_key_check "$CASES"; then
  echo
  echo "SELF=0 AUTHOR=0  run aborted: duplicate case keys"
  exit 1
fi
if ! alignment_check ""; then
  echo
  echo "SELF=0 AUTHOR=0  run aborted: the case tables disagree"
  exit 1
fi

for stem in "${!EXTRA[@]}"; do
  one_case "$stem"
done

echo
echo "SELF: $((SELF_N - ${#SELF_FAILS[@]}))/$SELF_N compared, ${#SELF_FAILS[@]} differing"
if [ "${#SELF_FAILS[@]}" -gt 0 ]; then
  printf '  differing: %s\n' "${SELF_FAILS[*]}"
fi
echo "AUTHOR: $((AUTHOR_N - ${#AUTHOR_FAILS[@]}))/$AUTHOR_N compared, ${#AUTHOR_FAILS[@]} differing"
if [ "${#AUTHOR_FAILS[@]}" -gt 0 ]; then
  printf '  differing: %s\n' "${AUTHOR_FAILS[*]}"
fi

rc=0
[ "${#SELF_FAILS[@]}" -gt 0 ] && rc=1
[ "${#AUTHOR_FAILS[@]}" -gt 0 ] && rc=1
exit "$rc"
