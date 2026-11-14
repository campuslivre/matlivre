# Working on MatLivre

Notes for coding agents (and people) working in this repository.

## Rules

- **MATLAB compatibility is required.** Semantics, display format and error messages follow
  MATLAB R2023b. Record the expected output by running code in MATLAB
  (`"C:\Program Files\MATLAB\R2023b\bin\matlab" -batch script`) and test against it.
- **Clean room.** Learn behavior from MATLAB's documentation and observable output only. Never
  read or copy MathWorks source code (`.m` files under the MATLAB installation included), and
  never base code on MATLAB's DLLs, so the project stays legal. Never open, copy or translate
  Octave's source either. Octave is a comparison target for benchmarks only.
- **Starting a toolbox.** List its functions from MATLAB's documentation in
  `tools/coverage/documented.tsv` first, then implement from that list.
- **Do not modify the user's `.mlx` files.** Work on copies.
- **Run heavy work one at a time** (builds, test runs, MATLAB sessions, benchmarks): the
  machine is shared and benchmark numbers are only valid on an idle CPU.

## After implementing functions

1. Add tests under `crates/livre-lang/tests/all/` (one module per area, listed in `main.rs`)
   with expected output recorded from MATLAB.
2. Time the new functions against MATLAB and Octave with a script that runs in all three,
   without overwriting the tracked files in `benchmarks/results/`. Name any case where
   matlivre is slower in the commit message.
3. Commit the implementation.
4. Refresh the coverage checklist: `bash tools/coverage/update.sh`, then commit
   "Refresh the coverage checklist".
5. **Update `README.md`** in the same change: the implemented-function count (from
   `docs/matlab-coverage.md`) wherever it appears, the feature lists under "The language" and
   "The app" when a new area is covered, and "Known limits" when a listed gap is closed.
6. Push, for maintainers only. Contributors submit a pull request from their fork.

## Layout

| Path | Contents |
|---|---|
| `crates/livre-lang` | Interpreter, builtins, figure model; tests in `tests/` |
| `crates/livre-doc` | `.mlx` / `.m` document model |
| `crates/livre-app` | Desktop app (egui), plot rendering |
| `docs/matlab-coverage.md` | Every MATLAB function and its status (generated) |
| `benchmarks/` | Language, cold-start and live script benchmarks |
| `tools/` | Coverage generator, README screenshots |
