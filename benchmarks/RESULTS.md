# Benchmark: MATLAB vs Octave vs matlivre

`bench.m` (with `bench_fib.m`) runs unchanged in all three engines. Each case reports the
best of 3 runs (`tic`/`toc`), and a checksum so the engines can be checked against each other:
**all 12 checksums agree across MATLAB, Octave and matlivre.**

- Machine: Intel Core i5-10600K (6 cores / 12 threads), 24 GB RAM, Windows 11 Pro.
- MATLAB R2023b Update 3 (`matlab -batch`), GNU Octave 11.3.0 (`octave-cli`),
  matlivre release build (`matlivre-cli`). Runs were sequential, not concurrent.
- Run date: 2026-10-28.

| Case | MATLAB (ms) | Octave (ms) | matlivre (ms) | matlivre vs Octave | matlivre vs MATLAB |
|---|---:|---:|---:|---|---|
| scalar_loop (2M iterations) | 6.26 | 2,419.39 | 1,923.65 | 1.3x faster | 307.1x slower |
| while_branch (Collatz, 2.9M steps) | 100.41 | 12,413.43 | 8,006.99 | 1.6x faster | 79.7x slower |
| fib_recursive (fib(20)) | 0.94 | 132.84 | 49.40 | 2.7x faster | 52.7x slower |
| array_growth (100k appends) | 7.68 | 406.98 | 106.40 | 3.8x faster | 13.9x slower |
| indexed_assign (1M writes) | 7.20 | 2,202.50 | 1,154.99 | 1.9x faster | 160.5x slower |
| vector_math (2M elements) | 143.35 | 122.10 | 209.58 | 1.7x slower | 1.5x slower |
| matmul_300 | 0.77 | 0.55 | 7.12 | 13.0x slower | 9.2x slower |
| linsolve_300 | 0.63 | 1.61 | 9.57 | 6.0x slower | 15.3x slower |
| sort_1e6 | 63.65 | 18.92 | 97.83 | 5.2x slower | 1.5x slower |
| string_build (20k appends) | 230.25 | 260.31 | 1,879.67 | 7.2x slower | 8.2x slower |
| struct_fields (500k updates) | 165.86 | 4,658.75 | 682.11 | 6.8x faster | 4.1x slower |
| cell_ops (100k writes + reads) | 25.47 | 608.51 | 260.21 | 2.3x faster | 10.2x slower |

Geometric mean over the 12 cases: matlivre is **1.2x slower than Octave** and **16.1x slower
than MATLAB**.

## Reading the results

- **Interpreted code (loops, calls, indexing, structs, cells): matlivre beats Octave in all 7
  cases (1.3x–6.8x)** but is far behind MATLAB, whose execution engine compiles loops to
  machine code (13x–307x). matlivre is a tree-walking interpreter; every scalar is a heap
  allocated array, and every variable access is a hash lookup.
- **Library kernels: MATLAB and Octave call optimized BLAS/LAPACK (MKL / OpenBLAS) and tuned
  sorting**, so `matmul`, `linsolve` and `sort` are 5x–15x faster than matlivre's plain Rust
  loops. Elementwise math is within 1.5x–1.7x.
- **`string_build`** is matlivre's weakest case: chars are stored as `f64` arrays, so each
  append copies and converts the whole string.

## Where the first run went wrong (now fixed)

The first matlivre run exposed three problems, fixed before these numbers were taken:
indexed assignment (`x(k) = v`), cell writes and reads (`c{k}`) and struct field updates copied
the whole variable each time (quadratic loops; `indexed_assign` did not finish in 10 minutes),
and `[s t]` with `s = ''` raised an error where MATLAB ignores 0×0 empties.

## Next steps for performance

1. A scalar fast path (no heap allocation for 1×1 doubles) and slot-indexed variables instead of
   hash lookups — the interpreter-overhead cases are dominated by these.
2. Link a BLAS/LAPACK (e.g. via the `faer` crate or OpenBLAS) for `*`, `\`, `inv`, `eig`.
3. Use `sort_unstable_by` with NaN handling, and a byte/UTF-16 char representation.
4. Longer term: compile hot loops (bytecode or a JIT), as MATLAB does.

## Reproduce

```bash
run_bench.bat
```

It runs the three engines one after another and prints this table (`compare.ps1`).
