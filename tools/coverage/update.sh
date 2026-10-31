#!/usr/bin/env bash
# Regenerates docs/matlab-coverage.md: checks every function of the MATLAB R2023b and Symbolic
# Math Toolbox function references (documented.tsv), key classes (classes.tsv) and the
# undocumented public functions of the toolbox folders (scope.tsv) against the release
# matlivre-cli, marks names provided by unmerged claude/* branches, and prints the names
# implemented since the last run. See coverage.pl for the scope rules.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cli="$root/target/release/matlivre-cli.exe"
[ -x "$cli" ] || cli="$root/target/release/matlivre-cli"

# Every name, then the ones MatLivre's exist() does not know.
{ cut -f3 "$here/documented.tsv"; cut -f2 "$here/classes.tsv"; cut -f2 "$here/scope.tsv"; } | tr -d '\r' | sort -u > "$tmp/names.txt"
awk '{gsub(/\x27/, "\x27\x27"); printf "if ~exist(\x27%s\x27), disp(\x27%s\x27); end\n", $0, $0}' "$tmp/names.txt" > "$tmp/check.m"
"$cli" "$tmp/check.m" | tr -d '\r' | grep -v '^Error' | sort -u > "$tmp/missing.txt" || true

# Names provided by branches not merged into the current HEAD.
for b in $(git -C "$root" for-each-ref --format='%(refname)' 'refs/heads/claude/*'); do
  git -C "$root" merge-base --is-ancestor "$b" HEAD && continue
  for f in $(git -C "$root" ls-tree -r --name-only "$b" crates/livre-lang/src); do
    git -C "$root" show "$b:$f" | awk '/pub const NAMES/,/\];/'
  done
done | { grep -oE '"[A-Za-z0-9_.]+"' || true; } | tr -d '"' | sort -u > "$tmp/branch.txt"

perl "$here/coverage.pl" "$here" "$tmp/missing.txt" "$tmp/branch.txt" "$root/docs/matlab-coverage.md"
