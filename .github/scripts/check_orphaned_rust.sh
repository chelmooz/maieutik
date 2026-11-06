#!/usr/bin/env bash
set -euo pipefail

# Resolve workspace members (absolute paths)
mapfile -t members < <(cargo metadata --format-version 1 --no-deps \
  | jq -r '.packages[].manifest_path' \
  | xargs dirname | sort -u)

[ ${#members[@]} -gt 0 ] || { echo "::error::cargo metadata returned no packages"; exit 1; }

fail=0
while IFS= read -r f; do
  # Resolve to absolute path for comparison
  f_abs=$(git rev-parse --show-toplevel)/$f
  dir=$(dirname "$f_abs")
  found=0
  for m in "${members[@]}"; do
    [[ "$m" == "$dir" || "$dir" == "$m"* ]] && found=1 && break
  done
  if [ $found -eq 0 ]; then
    echo "::error file=$f::Orphaned .rs file: not in any workspace member"
    fail=1
  fi
done < <(git ls-files '*.rs')

if [ $fail -eq 1 ]; then
  echo "::error::Found orphaned .rs files"
  exit 1
fi

echo "::notice::All .rs files belong to workspace members"
exit 0