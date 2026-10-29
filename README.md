# matlivre

A clean-room, proof-of-concept live script environment for a MATLAB-compatible language,
written in Rust, with a Windows 11 look modeled on
[ModernWinForms](../../CS/ModernWinForms).

![overview](docs/overview.png)

## Run

```bash
cargo run -p livre-app --release
```

Optionally pass a file: `cargo run -p livre-app --release -- path\to\script.mlx`.
On Windows, `build.bat` builds and `run.bat [file]` builds and starts the app.

The release build `target\release\matlivre.exe` is standalone: the C runtime is linked
statically (`.cargo/config.toml`), so it depends only on DLLs that ship with Windows.
Opens and saves `.mlx` (live script packages) and `.m` (plain scripts with `%%` sections).

| Shortcut | Action |
|---|---|
| Ctrl+Enter | Run current section |
| Ctrl+Shift+Enter | Run section and advance |
| F5 | Run all |
| Ctrl+Alt+Enter | Insert section break |
| Ctrl+N / Ctrl+O / Ctrl+S | New / Open / Save |

Clicking the gutter bar left of a section runs that section.

## Layout

| Crate | Purpose |
|---|---|
| `crates/livre-lang` | Lexer, parser, interpreter, N-D values, MATLAB display, ~400 builtins, classdef, datetime/duration, tables, Symbolic Math, figure model |
| `crates/livre-doc` | Live script document model; `.mlx` (OPC/ZIP + WordprocessingML) and `.m` reader/writer |
| `crates/livre-app` | eframe/egui desktop app: custom title bar, command bar, document editor, inline/right outputs, plots, workspace browser, light/dark skins |

Code runs on a dedicated engine thread that owns the base workspace and streams outputs back
to the UI (text, values, figures, warnings, errors), so the UI never blocks and **Stop** works
between statements.

## Clean-room process

[`docs/SPEC-live-script.md`](docs/SPEC-live-script.md) is the behavioral specification the
implementation was written from. It was produced by inspecting **data files and observable
behavior only** — the `.mlx` packages shipped with MATLAB R2023b (via REA artifact extraction)
and the public Live Editor behavior. No MathWorks source code was read or copied. Each spec
item is tagged as observed, inferred, or a design decision.

## Tests

```bash
cargo test --workspace
```

`livre-doc/tests/matlab_samples.rs` round-trips the live scripts shipped with a local MATLAB
install when one is present.

## Language

MATLAB compatibility is a requirement: behavior follows MATLAB's documented semantics and
display (spec section 5). Supported:

- N-D arrays of every type, with implicit expansion and `end`/`:` indexing.
- `double`, `single`, `logical`, `char` and `int8`…`uint64` with MATLAB's saturation and class
  combination rules; complex numbers (`3 + 4i`, complex-valued `sqrt`/`log`/`roots`, …).
- String arrays, cell arrays, struct arrays (`s(2).f`, `[s.f]`), function handles.
- `classdef` classes and function files from the script's folder: properties (defaults,
  `Constant`, `Dependent` with `get.` methods), methods, `Static` methods, constructors,
  inheritance with `obj@Super(...)`, handle vs value semantics, operator overloading.
- `datetime`, `duration`, `calendarDuration` (arithmetic, components, `between`, ranges, time
  axes in plots) and `table` (indexing, Properties, sorting, CSV `readtable`/`writetable`).
- Symbolic Math: `syms`, exact rationals, `diff`, `int`, `limit`, `taylor`, `solve`
  (polynomials and linear systems), `simplify`, `expand`, `factor`, `collect`, `subs`,
  `matlabFunction`, `fplot`, symbolic matrices.

Try `examples/features.m` (it uses `examples/Point.m`).

## Proof-of-concept limits

- Known language gaps are listed in the spec (section 6): `int64` beyond 2^53, IANA time zones
  and daylight saving, object arrays, property `set.` methods and validation, events,
  enumerations, packages, regular expressions, and symbolic integration/solving beyond the
  documented rules.
- Live controls inside `.mlx` files load as plain code (their current value) and are not
  interactive; saved outputs in `output.xml` are not restored (re-run to regenerate) and are
  not written.
- Rich text formatting (bold/italic runs, links, equations, images) is flattened to plain text.
- Windows is the only tested platform; the stack (egui/winit/glow) is cross-platform.
