# Benchmark: MATLAB vs Octave vs MatLivre

`bench.m` (with `bench_fib.m`) runs unchanged in all three engines. Each case reports the
best of 3 runs (`tic`/`toc`), and a checksum so the engines can be checked against each other:
**all 12 checksums agree across MATLAB, Octave and MatLivre.**

- Machine: Intel Core i5-10600K (6 cores / 12 threads), 24 GB RAM, Windows 11 Pro.
- MATLAB R2023b Update 3 (`matlab -batch`), GNU Octave 11.3.0 (`octave-cli`),
  matlivre release build (`matlivre-cli`). Runs were sequential, not concurrent.
- Run date: 2026-10-29.

| Case | MATLAB (ms) | Octave (ms) | MatLivre (ms) | MatLivre vs Octave | MatLivre vs MATLAB |
|---|---:|---:|---:|---|---|
| scalar_loop (2M iterations) | 4.92 | 2,265.87 | 2.62 | 865x faster | 1.9x faster |
| while_branch (Collatz, 2.9M steps) | 102.22 | 11,742.56 | 13.86 | 847x faster | 7.4x faster |
| fib_recursive (fib(20)) | 0.66 | 128.88 | 0.06 | 2,222x faster | 11.3x faster |
| array_growth (100k appends) | 8.43 | 402.17 | 1.42 | 283x faster | 5.9x faster |
| indexed_assign (1M writes) | 4.01 | 2,115.03 | 3.15 | 671x faster | 1.3x faster |
| vector_math (2M elements) | 121.78 | 127.25 | 86.36 | 1.5x faster | 1.4x faster |
| matmul_300 | 1.81 | 0.51 | 0.41 | 1.3x faster | 4.5x faster |
| linsolve_300 | 0.61 | 1.85 | 0.73 | 2.5x faster | 1.2x slower |
| sort_1e6 | 58.93 | 17.77 | 9.36 | 1.9x faster | 6.3x faster |
| string_build (20k appends) | 201.63 | 264.22 | 29.53 | 8.9x faster | 6.8x faster |
| struct_fields (500k updates) | 170.42 | 4,568.02 | 0.68 | 6,767x faster | 252x faster |
| cell_ops (100k writes + reads) | 31.25 | 581.45 | 17.70 | 33x faster | 1.8x faster |

Geometric mean over the 12 cases: MatLivre is **64.8x faster than Octave** and **4.7x faster
than MATLAB**. MatLivre wins 11 of 12 cases against MATLAB; `linsolve_300` is 1.2x behind.

MATLAB timings vary by roughly ±30% between runs on small cases (e.g. `matmul_300` measured
0.77 ms in the previous run and 1.81 ms in this one).

## What made the difference

- **Native compilation of loops and functions** (`src/jit.rs`, Cranelift). `for`/`while` loops
  over scalar doubles and logicals, elements of numeric arrays (`x(k)`, `A(i,j)`, `x(end+1)`),
  scalar cell elements (`c{k}`) and scalar struct fields (`p.x`) compile to machine code, as do
  user functions called with scalar doubles, including recursion. Anything else (display,
  strings, possibly complex results such as `sqrt` of a negative, whole-array use) stays
  interpreted, so behaviour is unchanged. Differential tests (`tests/jit.rs`) run every snippet
  with compilation on and off and require identical output, errors and workspace.
  `MATLIVRE_NOJIT=1` turns compilation off.
- **Linear algebra through `faer`**: blocked, multithreaded matrix products (one thread per
  physical core) and a blocked LU for `\`, `inv` and `det` from order 24.
- **Parallel elementwise math** (rayon) from 32k elements, and `x.^2` as `x.*x`.
- **Parallel unstable sort** when only sorted values are requested (sort stays stable when
  `-0` and `0` both occur or indices are requested).
- **In-place append** for `s = [s expr]` instead of copying the whole array each time.
- **Amortized `x(end+1) = v`**, in-place cell element writes, and operands of binary operators
  read by reference instead of copied.

## Reproduce

```bash
run_bench.bat
```

It runs the three engines one after another and prints this table (`compare.ps1`).
