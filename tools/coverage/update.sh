#!/usr/bin/env bash
# Regenerates docs/matlab-coverage.md: checks every in-scope MATLAB function name
# (tools/coverage/scope.tsv, taken from the MATLAB R2023b installation) against the release
# matlivre-cli, marks names that only exist on unmerged claude/* branches, and prints the
# names that became implemented since the last run.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cli="$root/target/release/matlivre-cli.exe"
[ -x "$cli" ] || cli="$root/target/release/matlivre-cli"

# Names matlivre does not know.
awk -F'\t' '{printf "if ~exist(\x27%s\x27), disp(\x27%s\x27); end\n", $2, $2}' "$here/scope.tsv" > "$tmp/check.m"
"$cli" "$tmp/check.m" | tr -d '\r' | sort -u > "$tmp/missing.txt"

# Names provided by branches not merged into the current HEAD.
: > "$tmp/branch.txt"
for b in $(git -C "$root" for-each-ref --format='%(refname)' 'refs/heads/claude/*'); do
  git -C "$root" merge-base --is-ancestor "$b" HEAD && continue
  for f in $(git -C "$root" ls-tree -r --name-only "$b" crates/livre-lang/src | grep -E 'builtins_|_classes|polyshape'); do
    git -C "$root" show "$b:$f" | awk '/pub const NAMES/,/\];/'
  done
done | grep -oE '"[A-Za-z0-9_]+"' | tr -d '"' | sort -u > "$tmp/branch.txt"

awk -f "$here/classify.awk" "$tmp/missing.txt" "$here/scope.tsv" |
  awk -F'\t' -v OFS='\t' 'NR==FNR{b[$1]=1; next} {if ($3=="todo" && ($2 in b)) $3="branch"; print}' "$tmp/branch.txt" - > "$tmp/classified.tsv"

# Newly implemented since the previous checklist.
doc="$root/docs/matlab-coverage.md"
if [ -f "$doc" ]; then
  grep -oE '^- \[ \] `[^`]+`' "$doc" | sed -E 's/.*`(.*)`/\1/' | sort -u > "$tmp/was_todo.txt"
  awk -F'\t' '$3=="done"{print $2}' "$tmp/classified.tsv" | sort -u > "$tmp/done.txt"
  comm -12 "$tmp/was_todo.txt" "$tmp/done.txt" > "$tmp/new.txt"
  echo "Newly implemented: $(wc -l < "$tmp/new.txt")"
  tr '\n' ' ' < "$tmp/new.txt"; echo
fi

awk -v date="$(date +%Y-%m-%d)" -f "$here/report.awk" "$tmp/classified.tsv" > "$doc"
cut -f3 "$tmp/classified.tsv" | sort | uniq -c
