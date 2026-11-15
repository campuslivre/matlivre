#!/usr/bin/env bash
# Run a MATLAB script on a remote machine (e.g. a VM) over SSH and print its output.
#
#   tools/matlab-remote.sh script.m > expected.txt
#
# Configuration (environment, or see AGENTS.local.md for this machine's values):
#   MATLAB_SSH_HOST    ssh host or alias (required), e.g. matlab-vm
#   MATLAB_REMOTE_DIR  scratch dir on the remote, forward slashes (default: /tmp)
#   MATLAB_REMOTE_BIN  MATLAB executable on the remote (default: matlab)
#   MATLAB_REMOTE_OS   windows | posix: the remote's shell, for cleanup (default: posix)
#                      (only the windows setup has been tested)
set -euo pipefail

host="${MATLAB_SSH_HOST:?set MATLAB_SSH_HOST (see AGENTS.local.md)}"
dir="${MATLAB_REMOTE_DIR:-/tmp}"
bin="${MATLAB_REMOTE_BIN:-matlab}"
script="${1:?usage: matlab-remote.sh script.m}"

name="matlivre_$$_$(basename "$script")"
case "$dir" in /*) scp_dir="$dir" ;; *) scp_dir="/$dir" ;; esac  # Windows: /C:/Users/me
scp -q "$script" "$host:$scp_dir/$name"
# -batch runs the script and exits non-zero on error. The remote file is removed afterwards.
# (Newer OpenSSH clients print key-exchange warnings on stderr; drop them.)
rc=0
ssh "$host" "\"$bin\" -batch \"run('$dir/$name')\"" 2> >(grep -v -e 'post-quantum' -e 'store now' -e 'openssh.com/pq' >&2) || rc=$?
if [ "${MATLAB_REMOTE_OS:-posix}" = windows ]; then
  winpath="${dir//\//\\}\\$name"
  ssh "$host" "del \"$winpath\"" >/dev/null 2>&1 || true
else
  ssh "$host" "rm -f \"$dir/$name\"" >/dev/null 2>&1 || true
fi
exit $rc
