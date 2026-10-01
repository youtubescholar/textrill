#!/bin/bash
# Corpus runner: per-file isolated comparison reference(Perl) vs port(Rust).
set -u
# Directory holding this script, so regression fixtures in tests/corpus/inputs
# can be referenced by absolute path from cases.sh.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REFDIR="${REFDIR:-/home/vicpu/build/ref/txt2html-3.0}"
MINE="${MINE:-/home/vicpu/build/txt2html-rs/target/debug/txt2html}"
RUNDIR="${RUNDIR:-/tmp/opencode/corpus}"
# Both halves of PERL5LIB live under ref/ in the repo.  An earlier version kept
# the reference module tree and the YAML::Syck stub in /tmp, and a machine
# reboot wiped them mid-run: every case still "passed" because both converters
# failed identically and cmp.py compared two empty files.
_refdir_lib="$REFDIR/lib"
# Clear the run directories: a stale output from an earlier run would otherwise
# be compared by cmp.py and could report PASS for a case that just crashed.
rm -rf "$RUNDIR/ref" "$RUNDIR/mine"
mkdir -p "$RUNDIR"/ref "$RUNDIR"/mine
export PERL5LIB="${PERL5LIB:-/home/vicpu/build/ref/stubs:$_refdir_lib}"

# Guard against comparing a stale binary: it silently "passes" cases that the
# current sources would fail, which is how the link-in-URL regression got
# through once already.
_mine_src=/home/vicpu/build/txt2html-rs/src
if [ -d "$_mine_src" ] && [ -x "$MINE" ]; then
  if [ -n "$(find "$_mine_src" "$_mine_src/../src" -name '*.rs' -newer "$MINE" 2>/dev/null | head -1)" ]; then
    echo "WARNING: $MINE is older than the sources in $_mine_src." >&2
    echo "         Run 'cargo build' (or set MINE) or these results are meaningless." >&2
  fi
fi

# Prove the reference actually runs before comparing anything.  A missing
# PERL5LIB entry makes every perl invocation exit non-zero, which the per-case
# exit check does catch -- but only as 40 separate failures that look like port
# regressions.  Fail once, here, with the real cause instead.
if [ -n "${T2H_SKIP_SMOKE:-}" ]; then
  :
else
  _smoke_in="$RUNDIR/smoke.txt"
  _smoke_out="$RUNDIR/smoke.html"
  printf 'smoke *test*\n' > "$_smoke_in"
  rm -f "$_smoke_out"
  if ! (cd "$REFDIR" && perl scripts/txt2html --infile "$_smoke_in" \
          --outfile "$_smoke_out" --default_link_dict "") >"$RUNDIR/smoke.err" 2>&1; then
    echo "ERROR: the reference will not run.  PERL5LIB=$PERL5LIB" >&2
    sed 's/^/  | /' "$RUNDIR/smoke.err" >&2
    exit 2
  fi
  if ! grep -q '<em>test</em>' "$_smoke_out" 2>/dev/null; then
    echo "ERROR: the reference ran but produced unexpected output." >&2
    sed 's/^/  | /' "$_smoke_out" >&2
    exit 2
  fi
  rm -f "$_smoke_in" "$_smoke_out" "$RUNDIR/smoke.err"
fi

cat > "$RUNDIR"/cmp.py <<'PYEOF'
import sys
def lines(p):
    try:
        with open(p, 'rb') as f:
            return [l.decode('utf-8','replace').replace('\n','').replace('\r','')
                    for l in f.read().splitlines(keepends=True)]
    except FileNotFoundError:
        return None
def main():
    a, b = sys.argv[1], sys.argv[2]
    la, lb = lines(a), lines(b)
    if la is None or lb is None:
        print(f"MISSING {a if la is None else b}"); return
    bad = 0
    for i,(x,y) in enumerate(zip(la,lb)):
        if x != y:
            bad += 1
            if bad <= 6:
                print(f"  line {i+1}:\n    ref : {x!r}\n    mine: {y!r}")
    if len(la) != len(lb):
        print(f"  LINE-COUNT ref={len(la)} mine={len(lb)}")
    print(f"  {'PASS' if bad==0 and len(la)==len(lb) else f'FAIL ({bad} diff lines)'}")
main()
PYEOF

run_case() {
  local stem="$1"; local extra="$2"; local cli="$3"
  CASE_ERR=""
  # Remove this case's outputs up front: cmp.py would otherwise compare a file
  # left over from an earlier run and could report PASS for a case that just
  # failed.  Belt and braces with the rm -rf at start-up, because run.sh can
  # also be invoked for a single stem against an existing RUNDIR.
  rm -f "$RUNDIR/ref/$stem.html" "$RUNDIR/mine/$stem.html"
  # INPUT may hold a comma-separated list to test shared converter state
  local -a infiles=()
  IFS=',' read -r -a infiles <<< "${INPUT[$stem]:-$stem.txt}"
  local t2h_in="" mine_infile=()
  for f in "${infiles[@]}"; do
    # An absolute path is used as-is, so a regression fixture can live in the
    # port's own tree instead of being dropped into the reference distribution.
    local path="tfiles/$f"
    case "$f" in /*) path="$f" ;; esac
    t2h_in="${t2h_in:+$t2h_in,}$path"
    mine_infile+=(--infile "$path")
  done
  # DICT holds extra link-dictionary files, comma separated.  This is how the
  # --links_dictionaries option is exercised, which is the one behavioural fix
  # 2.51 -> 3.0 shipped.
  local -a dicts=()
  IFS=',' read -r -a dicts <<< "${DICT[$stem]-}"
  local t2h_dict="" mine_dict=()
  for d in "${dicts[@]}"; do
    [ -n "$d" ] || continue
    t2h_dict="${t2h_dict:+$t2h_dict,}$d"
    mine_dict+=(--links_dictionaries "$d")
  done
  # Perl driver via env vars: no shell interpolation of the option list.
  (cd "$REFDIR" && \
    T2H_IN="$t2h_in" T2H_OUT="$RUNDIR/ref/$stem.html" \
    T2H_DICT="$t2h_dict" \
    T2H_CTOR="${CTOR[$stem]-}" T2H_EXTRA="$extra" \
    perl -MHTML::TextToHTML -e '
      my @ctor = &{ eval "sub { $ENV{T2H_CTOR} }" }();
      my @in = split(/,/, $ENV{T2H_IN});
      my @dicts = grep { length } split(/,/, $ENV{T2H_DICT});
      my %t = (infile => [@in], outfile => $ENV{T2H_OUT},
               default_link_dict => "", &{ eval "sub { $ENV{T2H_EXTRA} }" }());
      my $c = HTML::TextToHTML->new(@ctor);
      $c->args(links_dictionaries => \@dicts) if @dicts;
      $c->txt2html(%t);
    ' 2>"$RUNDIR/ref/$stem.err")
  local ref_rc=$?
  [ "$ref_rc" -eq 0 ] || CASE_ERR+="reference exited $ref_rc: $(head -c 200 "$RUNDIR/ref/$stem.err" | tr '\n' ' ')"
  local -a cliargs=()
  eval "cliargs=($cli)"
  (cd "$REFDIR" && "$MINE" "${mine_infile[@]}" "${mine_dict[@]}" --outfile "$RUNDIR/mine/$stem.html" --default_link_dict "" "${cliargs[@]}" 2>"$RUNDIR/mine/$stem.err")
  local mine_rc=$?
  # A non-zero exit is a failure even if an output file happens to be present:
  # the engine may have written a partial document before giving up, and the
  # comparison must not be allowed to pass on it.
  if [ "$mine_rc" -ne 0 ]; then
    CASE_ERR+="port exited $mine_rc: $(head -c 200 "$RUNDIR/mine/$stem.err" | tr '\n' ' ')"
  fi
}

# Second, independent check: compare the port's output against the golden the
# upstream 3.0 test suite shipped, where one exists for this stem.  The
# differential comparison above is the primary methodology -- comparing against
# freshly generated Perl catches divergence, and this catches an option set that
# is merely self-consistent, because a wrong pattern (or a wrong option list)
# applied to both sides still produces matching output.  A case listed in
# NOGOLDEN[] is skipped with a reason.
golden_check() {
  local stem="$1"
  [ -n "${NOGOLDEN[$stem]+x}" ] && { echo "  GOLDEN skipped (${NOGOLDEN[$stem]})"; return; }
  local g="$REFDIR/tfiles/good_$stem.html"
  [ -f "$g" ] || { echo "  GOLDEN none"; return; }
  if LC_ALL=C cmp -s "$g" "$RUNDIR/mine/$stem.html"; then
    echo "  GOLDEN pass"
  else
    echo "  GOLDEN FAIL (differs from tfiles/good_$stem.html)"
    diff "$g" "$RUNDIR/mine/$stem.html" | head -6 | sed 's/^/    /'
    GOLDEN_FAILS+=("$stem")
  fi
  GOLDEN_N=$((GOLDEN_N + 1))
}

# shellcheck disable=SC1091
. "$(dirname "$0")/cases.sh"

# P20. The full run iterates "${!EXTRA[@]}" and reads CLI[$stem] for each, so
# the two arrays have to name the same cases. They do today -- 46 and 46 -- but
# that was a fact about the file, not something the harness checked, and the way
# it breaks is the P2 shape exactly: a case is written, wired up on one side, and
# never runs. It fails silently in the direction that matters, because a case
# that does not run cannot fail.
#
# The two directions are not symmetric, which is why this cannot be a count
# comparison. A stem in CLI[] but not EXTRA[] is skipped without a word.
# A stem in EXTRA[] but not CLI[] trips `set -u` on "${CLI[$stem]}" and kills
# the script mid-run -- loud, but only because of an unrelated line of shell,
# and the message names a variable rather than a case. So both directions are
# checked by name.
alignment_check() {
  local only_cli only_extra n=0
  mapfile -t only_cli < <(
    comm -23 <(printf '%s\n' "${!CLI[@]}"   | sort) <(printf '%s\n' "${!EXTRA[@]}" | sort)
  )
  mapfile -t only_extra < <(
    comm -13 <(printf '%s\n' "${!CLI[@]}"   | sort) <(printf '%s\n' "${!EXTRA[@]}" | sort)
  )
  for stem in "${only_cli[@]}"; do
    echo "ALIGN: '$stem' is in CLI[] but not EXTRA[] -- it would never run"
    n=$((n+1))
  done
  for stem in "${only_extra[@]}"; do
    echo "ALIGN: '$stem' is in EXTRA[] but not CLI[] -- it would die on set -u"
    n=$((n+1))
  done
  if [ "$n" -gt 0 ]; then
    echo "ALIGN: $n case(s) declared on one side only"
    return 1
  fi
  # A NOGOLDEN entry is a suppression, and a suppression that suppresses nothing
  # is a lie in a file whose whole job is being believed. Each one has to name a
  # case that exists and that actually has a golden to skip.
  # A NOGOLDEN entry is a suppression, and a suppression that suppresses nothing
  # is a lie in a file whose whole job is being believed. Each one has to name a
  # case that exists, and it has to be honest about *which* oracle it is
  # standing in for.
  #
  # Two kinds, distinguished by NOGOLDEN_REASON[], which the entry must use:
  #
  #   * "golden differs for a stated reason" -- the case HAS a golden, the
  #     reference does not match it, and the reason says why. The differential
  #     comparison against the reference is still the oracle and still runs.
  #   * "differential must fail: <reason>" -- the case has NO golden, because
  #     upstream ships none, and the port deliberately diverges from the
  #     reference, so a byte comparison cannot be the oracle at all. The entry
  #     has to say so explicitly, and the differential comparison for that case
  #     is then expected to fail rather than being silently tolerated.
  #
  # Without the second kind, "opt_injection" would have had to be written as a
  # golden-shaped NOGOLDEN entry to get past the check below, which is exactly
  # the lie this guard exists to prevent: a case whose divergence is deliberate
  # and total has no golden to skip, and saying it does would be untrue.
  local bogus=0 stem
  for stem in "${!NOGOLDEN[@]}"; do
    if [ -z "${EXTRA[$stem]+x}" ]; then
      echo "ALIGN: NOGOLDEN['$stem'] names a case that does not exist"
      bogus=1
      continue
    fi
    case "${NOGOLDEN[$stem]}" in
      "differential must fail:"*)
        if [ -f "$REFDIR/tfiles/good_$stem.html" ]; then
          echo "ALIGN: NOGOLDEN['$stem'] claims no golden exists but one does"
          bogus=1
        fi
        ;;
      *)
        if [ ! -f "$REFDIR/tfiles/good_$stem.html" ]; then
          echo "ALIGN: NOGOLDEN['$stem'] skips a case that has no golden"
          echo "      (if the port deliberately diverges from the reference for"
          echo "       this case, start the reason with 'differential must fail:')"
          bogus=1
        fi
        ;;
    esac
  done
  [ "$bogus" -eq 0 ] || return 1
  return 0
}

if [ "$#" -gt 0 ]; then
  stem="$1"
  extra="${EXTRA[$stem]-}"; cli="${CLI[$stem]-}"
  run_case "$stem" "$extra" "$cli"
  echo "== $stem =="
  [ -n "$CASE_ERR" ] && echo "  ERROR: $CASE_ERR"
  res=$(python3 "$RUNDIR/cmp.py" "$RUNDIR/ref/$stem.html" "$RUNDIR/mine/$stem.html")
  echo "$res"
  # GOLDEN_N is only initialised in the full-run branch below, and golden_check
  # increments it, so a single-stem invocation over a stem that has a golden
  # died on `set -u` with "GOLDEN_N: unbound variable" -- after printing PASS
  # and "GOLDEN pass", which is a nasty way to fail. Single-stem mode was
  # therefore unusable for 28 of the 46 cases and nothing caught it, because
  # no gate invokes it.
  GOLDEN_N=0; GOLDEN_FAILS=()
  golden_check "$stem"
  # Same exit contract as the full run below: a single-stem invocation is still
  # a gate, and must be able to fail.
  rc=0
  [ -n "$CASE_ERR" ] && rc=1
  echo "$res" | grep -q PASS || rc=1
  [ "${#GOLDEN_FAILS[@]}" -gt 0 ] && rc=1
  # Checked here too, because the single-stem path is how a case is developed
  # and the misalignment is exactly the kind of thing that happens while writing
  # one. Cheap, and it names the case rather than dying on a variable.
  alignment_check || rc=1
  exit "$rc"
else
  pass=0; fail=0; failnames=()
  GOLDEN_FAILS=(); GOLDEN_N=0
  # Before the loop, not after: a case that never runs is a case that cannot
  # fail, so an alignment problem has to be reported before the run is summarised
  # as PASS=46/46, not appended to a report that already looks like a pass.
  #
  # And on failure the loop is not run. A misaligned table cannot produce a
  # meaningful PASS count -- the count is the number of cases that ran, so
  # reporting one here is reporting on a subset while implying it is the whole.
  # Continuing would also turn one fault into two errors, since the EXTRA-only
  # case then trips `set -u` on "${CLI[$stem]}" a few lines below.
  if ! alignment_check; then
    echo
    echo "PASS=0 FAIL=0"
    echo "  run aborted: the case tables disagree, so no count would mean anything"
    exit 1
  fi
  for stem in "${!EXTRA[@]}"; do
    extra="${EXTRA[$stem]}"; cli="${CLI[$stem]}"
    run_case "$stem" "$extra" "$cli"
    echo "== $stem =="
    res=$(python3 "$RUNDIR/cmp.py" "$RUNDIR/ref/$stem.html" "$RUNDIR/mine/$stem.html")
    [ -n "$CASE_ERR" ] && echo "  ERROR: $CASE_ERR"
    echo "$res"
    golden_check "$stem"
    # A case declared `differential must fail:` has no golden and diverges from
    # the reference *on purpose* -- the reference is the defect, which is the
    # whole of Tier 2. So a mismatch is the required outcome, not a failure, and
    # counting it as one would make the fix impossible to land. What is still
    # checked, and what would be a real failure, is the two things that can go
    # wrong in the other direction: the port not running at all, and the port
    # matching the reference (which would mean the deliberate escaping had
    # quietly stopped happening).
    case "${NOGOLDEN[$stem]-}" in
      "differential must fail:"*)
        if [ -n "$CASE_ERR" ]; then
          fail=$((fail+1)); failnames+=("$stem (converter error)")
        elif echo "$res" | grep -q PASS; then
          fail=$((fail+1)); failnames+=("$stem (matches the reference; the declared divergence is gone)")
        else
          pass=$((pass+1))
        fi
        continue
        ;;
    esac
    if [ -z "$CASE_ERR" ] && echo "$res" | grep -q PASS; then
      pass=$((pass+1))
    else
      fail=$((fail+1)); failnames+=("$stem")
    fi
  done
  echo
  echo "PASS=$pass FAIL=$fail"
  printf '  %s\n' "${failnames[@]}"
  echo "GOLDEN: $((GOLDEN_N - ${#GOLDEN_FAILS[@]}))/$GOLDEN_N compared, ${#GOLDEN_FAILS[@]} differing"
  if [ "${#GOLDEN_FAILS[@]}" -gt 0 ]; then
    printf '  differing: %s\n' "${GOLDEN_FAILS[*]}"
  fi
  # The counters above are for humans. Without this the script ends on a
  # successful `echo` and reports success to `make corpus` and `make verify`
  # even when every case failed: demonstrated with a stub converter that exits
  # 0 and writes wrong output, which produced "PASS=0 FAIL=46" and exit 0.
  #
  # That made the Tier 1 invariant -- 46/46 and 29/29, restated in this plan
  # after nearly every item -- unenforced. It is the same false-green shape as
  # the P1 per-case bug, one level up: P1 made a crashed case count as a pass,
  # this made every crashed case count as a pass without anyone counting.
  if [ "$fail" -gt 0 ] || [ "${#GOLDEN_FAILS[@]}" -gt 0 ]; then
    exit 1
  fi
  exit 0
fi