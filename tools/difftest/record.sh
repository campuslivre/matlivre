#!/usr/bin/env bash
# Record MATLAB's snapshots of the difftest cases, over SSH, into
# tests/matlab/expected/<release>/<case>.json (the release is read from the MATLAB that ran them).
#
#   tools/difftest/record.sh                 record every case under tests/matlab/cases
#   tools/difftest/record.sh tests/matlab/cases/basic/workspace.m ...
#   tools/difftest/record.sh --release R2023b ...   refuse to record unless MATLAB is that release
#
# One MATLAB session runs all the cases; difftest_run.m resets the state between them.
# Configuration is the same as tools/matlab-remote.sh (MATLAB_SSH_HOST, MATLAB_REMOTE_DIR,
# MATLAB_REMOTE_BIN, MATLAB_REMOTE_OS = windows|posix, default posix); see AGENTS.local.md.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"

want_release=""
if [ "${1:-}" = "--release" ]; then want_release="$2"; shift 2; fi
host="${MATLAB_SSH_HOST:?set MATLAB_SSH_HOST (see AGENTS.local.md)}"
rdir="${MATLAB_REMOTE_DIR:-/tmp}"
bin="${MATLAB_REMOTE_BIN:-matlab}"
os="${MATLAB_REMOTE_OS:-posix}"

if [ $# -gt 0 ]; then cases=("$@"); else
  mapfile -t cases < <(cd "$root" && find tests/matlab/cases -name '*.m' | sort)
fi
[ ${#cases[@]} -gt 0 ] || { echo "no cases"; exit 1; }

quiet() { grep -v -e 'post-quantum' -e 'store now' -e 'openssh.com/pq' || true; }
rsh() { ssh "$host" "$@" 2> >(quiet >&2); }
case "$rdir" in /*) scp_dir="$rdir" ;; *) scp_dir="/$rdir" ;; esac

work="dt_$$"
if [ "$os" = windows ]; then
  wdir_win="${rdir//\//\\}\\$work"
  rsh "mkdir \"$wdir_win\"" >/dev/null
  cleanup() { rsh "rmdir /s /q \"$wdir_win\"" >/dev/null 2>&1 || true; }
else
  rsh "mkdir -p \"$rdir/$work\"" >/dev/null
  cleanup() { rsh "rm -rf \"$rdir/$work\"" >/dev/null 2>&1 || true; }
fi
trap cleanup EXIT
rwork="$rdir/$work"

local_tmp="$(mktemp -d)"
trap 'cleanup; rm -rf "$local_tmp"' EXIT

# Send the harness and the cases (c001.m, c002.m, ...), plus a driver that runs them in order.
scp -q "$here/difftest_run.m" "$host:$scp_dir/$work/difftest_run.m"
driver="$local_tmp/dt_driver.m"
{
  echo "here = fileparts(mfilename('fullpath')); addpath(here);"
  echo "fid = fopen(fullfile(here,'release.txt'),'w'); fprintf(fid,'%s',version('-release')); fclose(fid);"
} > "$driver"
i=0
for c in "${cases[@]}"; do
  i=$((i+1)); id=$(printf 'c%03d' "$i")
  scp -q "$root/$c" "$host:$scp_dir/$work/$id.m"
  echo "try, cd(tempdir); difftest_run(fullfile(here,'$id.m'), fullfile(here,'$id.json')); catch ME, disp(['FAILED $id: ' ME.message]); end" >> "$driver"
done
scp -q "$driver" "$host:$scp_dir/$work/dt_driver.m"

# The script's own folder is the current folder while it runs, so difftest_run.m is found.
rsh "\"$bin\" -batch \"run('$rwork/dt_driver.m')\""

scp -q "$host:$scp_dir/$work/release.txt" "$local_tmp/release.txt"
release="R$(tr -d '\r\n' < "$local_tmp/release.txt")"
if [ -n "$want_release" ] && [ "$want_release" != "$release" ]; then
  echo "MATLAB on $host is $release, not $want_release: nothing recorded" >&2; exit 1
fi
echo "Recording against MATLAB $release"

i=0
for c in "${cases[@]}"; do
  i=$((i+1)); id=$(printf 'c%03d' "$i")
  rel="${c#tests/matlab/cases/}"; rel="${rel%.m}"
  out="$root/tests/matlab/expected/$release/$rel.json"
  mkdir -p "$(dirname "$out")"
  if scp -q "$host:$scp_dir/$work/$id.json" "$out" 2>/dev/null; then
    tr -d '\r' < "$out" > "$out.tmp" && mv "$out.tmp" "$out"
    echo "  $rel"
  else
    echo "  $rel: no snapshot produced" >&2
  fi
done
