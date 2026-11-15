# Working on MatLivre

Notes for coding agents (and people) working in this repository.

## Rules

- **MATLAB compatibility is required.** Semantics, display format and error messages follow
  MATLAB R2023b. Record the expected output by running code in MATLAB
  (`"C:\Program Files\MATLAB\R2023b\bin\matlab" -batch script`) and test against it. If MATLAB is
  not installed on this machine, run it on a remote machine over SSH with
  `tools/matlab-remote.sh script.m`; `AGENTS.local.md` (untracked) describes this machine's setup.
- **Clean room.** Learn behavior from MATLAB's documentation and observable output only. Never
  read or copy MathWorks source code (`.m` files under the MATLAB installation included).
- **Do not modify the user's `.mlx` files.** Work on copies.
- **Run heavy work one at a time** (builds, test runs, MATLAB sessions, benchmarks): the
  machine is shared and benchmark numbers are only valid on an idle CPU.

## After implementing functions

1. Add tests under `crates/livre-lang/tests/all/` (one module per area, listed in `main.rs`)
   with expected output recorded from MATLAB. For behaviour you can express as a plain script,
   prefer a differential case (next section).
2. Commit the implementation.
3. Refresh the coverage checklist: `bash tools/coverage/update.sh`, then commit
   "Refresh the coverage checklist".
4. **Update `README.md`** in the same change: the implemented-function count (from
   `docs/matlab-coverage.md`) wherever it appears, the feature lists under "The language" and
   "The app" when a new area is covered, and "Known limits" when a listed gap is closed.
5. Push.

## Differential cases (`.m` scripts compared with MATLAB)

A case is a plain script in `tests/matlab/cases/<area>/<name>.m`. `tools/difftest/difftest_run.m`
runs it from a clean state (in MATLAB and in MatLivre alike) and writes a JSON snapshot: command
window log, the error that stopped it, `lastwarn`, every workspace variable (class, size, exact
bits, display) and the open figures' graphics tree. MATLAB's snapshots are recorded per release in
`tests/matlab/expected/<release>/` and committed; `cargo test -p livre-lang --test all difftest`
runs each case in MatLivre and diffs it against them.

- Record: `tools/difftest/record.sh [--release R2023b] [cases...]` (needs MATLAB, local or over
  SSH as in `AGENTS.local.md`). It refuses to record if MATLAB is not the release you named.
- `MATLIVRE_TARGET_RELEASE` picks the release compared against (default `R2023b`). A case with no
  snapshot for it is listed as not recorded, not failed.
- **Any difference from MATLAB fails the test.** There is no pending or skip marker: a failing
  case is behaviour still to implement. Do not weaken a case, the harness or the comparison to
  make it pass; fix MatLivre.
- The command-window text (`log` and each variable's display) is compared too. The test runs the
  interpreter with `Interpreter::command_window = true`, which shows results as `matlab -batch`
  does (`x =`, blank line, `     5`, blank line) instead of the Live Editor style (`x = 5`) the
  app uses. Types `display_value_cw` (`display.rs`) does not cover yet fall back to the Live
  Editor style and so fail until they are added.
- The comparer drops only what is not behaviour: line endings, MATLAB's `<a href>` link markup
  and warning bold markers, call-stack lines, trailing blanks. Figure windows' display is left
  out (it shows the screen-dependent window position).
- Cases must not depend on time, random numbers without `rng`, paths or handle numbers.

## Layout

| Path | Contents |
|---|---|
| `crates/livre-lang` | Interpreter, builtins, figure model; tests in `tests/` |
| `crates/livre-doc` | `.mlx` / `.m` document model |
| `crates/livre-app` | Desktop app (egui), plot rendering |
| `docs/matlab-coverage.md` | Every MATLAB function and its status (generated) |
| `benchmarks/` | Language, cold-start and live script benchmarks |
| `tests/matlab/` | Differential cases (`cases/`) and MATLAB's recorded snapshots (`expected/<release>/`) |
| `tools/` | Coverage generator, README screenshots, `difftest/` harness, `matlab-remote.sh` |
