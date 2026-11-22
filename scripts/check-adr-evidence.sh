#!/usr/bin/env bash
# Fails if a file in specs/adr/ADR-013-evidence/ is not linked from the index in
# section 5 of ADR-013, or the index links a file that does not exist.
set -euo pipefail
cd "$(dirname "$0")/.."
adr=specs/adr/ADR-013-logic-compiler-and-vm.md
dir=specs/adr/ADR-013-evidence

# Links of the form (ADR-013-evidence/<file>.md) in section 5 only.
linked=$(awk '/^## 5\. /{s=1; next} /^## /{s=0} s' "$adr" \
  | grep -o '(ADR-013-evidence/[^)]*\.md)' | sed 's/^(ADR-013-evidence\///; s/)$//' | sort || true)
present=$(find "$dir" -maxdepth 1 -type f -name '*.md' -printf '%f\n' | sort)

fail=0
while IFS= read -r f; do
  [ -z "$f" ] && continue
  if ! grep -qxF "$f" <<<"$linked"; then
    echo "NOT IN THE INDEX: $dir/$f is not linked from section 5 of $adr" >&2; fail=1
  fi
done <<<"$present"
while IFS= read -r f; do
  [ -z "$f" ] && continue
  if [ ! -f "$dir/$f" ]; then
    echo "NO SUCH FILE: the index of section 5 links $dir/$f" >&2; fail=1
  fi
done <<<"$linked"
dups=$(sort <<<"$linked" | uniq -d)
if [ -n "$dups" ]; then echo "LINKED TWICE: $dups" >&2; fail=1; fi

if [ "$fail" -ne 0 ]; then exit 1; fi
echo "ADR-013 evidence: $(grep -c . <<<"$present") files, all linked from the index, every link resolves"
