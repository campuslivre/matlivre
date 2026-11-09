# Local environment notes (copy to ../AGENTS.local.md, which is untracked)

Facts about your machine that agents should not rediscover. Fill in what applies.

## Host
- Machine, OS, RAM; whether MATLAB is installed locally (path).
- Where `cargo` is (if not on PATH).

## Remote MATLAB (only if MATLAB is not local)
- SSH alias (`~/.ssh/config` entry), guest OS, user, MATLAB path and release (R2023b expected).
- Environment for `tools/matlab-remote.sh` and `tools/difftest/record.sh`:
  `MATLAB_SSH_HOST`, `MATLAB_REMOTE_DIR` (forward slashes), `MATLAB_REMOTE_BIN`,
  `MATLAB_REMOTE_OS` (`windows` or `posix`; only windows has been tested).
- License notes (host-ID licenses break if the VM's MAC or disk changes).

## Log
- Dated notes of what you set up or found.
