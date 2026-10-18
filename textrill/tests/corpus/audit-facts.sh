#!/bin/bash
# Audit facts for the frozen acceptance goldens: what input each stem reads,
# whether that input exists, the golden's size and hash, whether the stem has an
# upstream good_*.html, and whether it is declared NOGOLDEN.
set -u
cd "$(dirname "$0")"
HERE=$PWD
source ./cases.sh

GOLD=${GOLDEN_DIR:-$HERE/../golden/corpus}
TFILES=$FIX

stems=$(for k in "${!EXTRA[@]}"; do echo "$k"; done | sort)
for stem in $stems; do
  input=${INPUT[$stem]:-}
  if [ -n "$input" ]; then
    # INPUT may be multiple files; the runner converts the first through the
    # last with one converter. Report the first for resolution.
    first=${input%%,*}
    in_sz=$([ -f "$first" ] && stat -c %s "$first" || echo "MISSING")
    in_stem_ok=""
  else
    # default: <stem>.txt in tfiles
    first="$FIX/$stem.txt"
    in_sz=$([ -f "$first" ] && stat -c %s "$first" || echo "MISSING")
  fi
  g="$GOLD/$stem.html"
  g_sz=$([ -f "$g" ] && stat -c %s "$g" || echo "NOGOLD")
  g_hash=$([ -f "$g" ] && sha256sum "$g" | cut -c1-16 || echo "-")
  good=$([ -f "$TFILES/good_$stem.html" ] && echo yes || echo no)
  ng=${NOGOLDEN[$stem]:-}
  printf '%-22s in=%-30s in_sz=%-8s gold_sz=%-8s hash=%-16s good=%-3s nogol=%s\n' \
    "$stem" "$(basename "$first")" "$in_sz" "$g_sz" "$g_hash" "$good" "$([ -n "$ng" ] && echo yes || echo no)"
done