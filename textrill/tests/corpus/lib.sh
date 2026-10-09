#!/bin/bash
# Shared case-table guards for the two corpus runners:
#
#   run.sh     — the Perl differential (the parity oracle)
#   accept.sh  — reference-free acceptance against frozen self-goldens
#
# Both source `cases.sh` and both have exactly the same failure mode if the
# tables are malformed: a case that is written but never runs, so it can never
# fail. That check lives here, in one place, rather than being copied into the
# second runner and left to drift. A guard that is duplicated is a guard that
# can disagree with itself, which is the whole class of bug these functions
# exist to catch (P20, P21).
#
# The functions read the arrays `cases.sh` declares (EXTRA, CLI, INPUT, GOLDEN,
# NOGOLDEN); they take no table argument because bash arrays are global.

# P20. The full run iterates "${!EXTRA[@]}" and reads CLI[$stem] for each, so
# the two arrays have to name the same cases. They do today -- 61 and 61 -- but
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
duplicate_key_check() {
  # P21. `alignment_check` compares key *sets*, so it cannot see a key that was
  # assigned twice: by the time it runs, the second assignment has already won
  # and the array looks perfectly consistent. That is not a hypothetical. A8 added
  # a new `pre_explicit_blank` case without noticing the stem was taken, and the
  # older case it displaced vanished -- silently, because both happened to agree
  # with the reference, so the corpus stayed at 47/47 with one case never running.
  # A case that does not run cannot fail, which is the P2 shape one level down.
  #
  # So the check has to run against the *source text*, where both assignments are
  # still visible, not against the sourced arrays where one has been lost. Any
  # array a case is defined in, not just CLI[]: a duplicate EXTRA[] or INPUT[] is
  # the same silent loss.
  local f line arr stem seen dups
  dups=0
  for f in "$@"; do
    seen=$(grep -oE '^(CLI|EXTRA|INPUT|GOLDEN|NOGOLDEN)\[[A-Za-z0-9_-]+\]=' "$f" \
           | sort | uniq -d)
    if [ -n "$seen" ]; then
      while read -r line; do
        [ -n "$line" ] || continue
        arr="${line%%[*}"; stem="${line#*[}"; stem="${stem%%]*}"
        echo "ALIGN: ${arr}[${stem}] is assigned more than once in $(basename "$f")"
        echo "      bash keeps the last assignment, so an earlier case for this"
        echo "      stem never runs -- and the run still reports a clean pass."
        dups=$((dups + 1))
      done <<< "$seen"
    fi
  done
  if [ "$dups" -gt 0 ]; then
    echo "ALIGN: $dups duplicate case key(s); every one is a case that silently"
    echo "      stopped running. Give each case its own stem."
    return 1
  fi
  return 0
}

# Compare the key sets of CLI[] and EXTRA[], and -- when the differential runner
# passes the directory holding the author goldens -- hold every NOGOLDEN[]
# suppression to its own claim. The NOGOLDEN half is skipped with an empty
# argument, because a reference-free runner has no `good_*.html` set to check the
# entry against; the suppression is a statement about the *differential*, so only
# the differential can judge it.
alignment_check() {
  local goldendir="${1:-}"
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
  [ -n "$goldendir" ] || return 0
  # A NOGOLDEN entry is a suppression, and a suppression that suppresses nothing
  # is a lie in a file whose whole job is being believed. Each one has to name
  # a case that exists, and it has to be honest about *which* oracle it is
  # standing in for.
  #
  # Two kinds, distinguished by the reason text itself:
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
        if [ -f "$goldendir/good_$stem.html" ]; then
          echo "ALIGN: NOGOLDEN['$stem'] claims no golden exists but one does"
          bogus=1
        fi
        ;;
      *)
        if [ ! -f "$goldendir/good_$stem.html" ]; then
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
