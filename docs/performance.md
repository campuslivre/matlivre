# Performance

All measurements on the same machine: Intel Core i5-10600K (6 cores / 12 threads), 24 GB RAM,
Windows 11 Pro. MATLAB R2023b Update 3, GNU Octave 11.3.0, Scilab 2025.0.0, MatLivre release build. Programs
were run one after another, never concurrently.

## Cold start

Time from launching the program until it has run a line of code. For the desktop apps this is
the time until you can work in them.

| Program | First launch | Warm launch |
|---|---:|---:|
| **MatLivre app** (window up, ready to run) | **0.97 s** | **0.31 s** |
| **MatLivre CLI** (runs a script) | **0.03 s** | **0.07 s** |
| GNU Octave CLI (`--eval`) | 2.56 s | 0.35 s |
| MATLAB desktop (runs a startup command) | — | 3.90 s |
| MATLAB `-batch` | 24.30 s | 6.84 s |

"First launch" is the first start in a session (MATLAB's was a true cold start: 24 s before
it ran anything); "warm launch" is the median of three more starts, with the program's files
already in the disk cache. MATLAB's desktop was measured after its batch runs, so only its warm
time is shown. The Octave GUI is not in the table: its first start stops at a setup wizard.
Script: [`benchmarks/coldstart.ps1`](benchmarks/coldstart.ps1).

## Live scripts

[`benchmarks/livescript/bench_live.mlx`](benchmarks/livescript/bench_live.mlx) is a typical
analysis notebook: eight sections with matrix algebra, a loop, statistics, a table, string
processing and four figures (a line plot, a surface with a colorbar, a histogram and a 2x2
grid of subplots). The time covers running every section, capturing the outputs and drawing
the figures.

| Engine | First run | Warm run |
|---|---:|---:|
| **MatLivre** | **0.14 s** | **0.11 s** |
| MATLAB Live Editor (`executeAndSave`) | 14.30 s | 4.91 s |
| MATLAB, same code as a plain `.m` script | 1.17 s | 0.37 s |
| GNU Octave | cannot run live scripts | |

MatLivre is about **43x faster** than MATLAB's Live Editor on a warm run and **100x faster** on
the first run in a session. MATLAB runs the same code as a plain script in 0.37 s, so most of
its live script time is the Live Editor itself: evaluating section by section, capturing each
output and figure, and writing them into the file. MatLivre's time covers loading the `.mlx`,
running every section, capturing the outputs and rendering the four figures (it does not write
outputs back into the file). Warm runs are the best of three in an already running program.
Scripts: [`benchmarks/livescript`](benchmarks/livescript).

**Does Octave run live scripts?** No. GNU Octave has no live script support: it cannot open or
run `.mlx` files, and it has no live editor with inline output. The closest it offers is `%%`
code sections in plain `.m` files in its editor, with output going to the command window and
figures to separate windows.

## Language and numerics

[`benchmarks/bench.m`](benchmarks/bench.m) runs unchanged in MATLAB, Octave and MatLivre; Scilab
runs a line-by-line port, [`benchmarks/bench.sce`](benchmarks/bench.sce) (Scilab's syntax differs).
Each case is the best of 3 runs and reports a checksum, and **all 12 checksums agree in all four
engines**. Run date: 2026-11-07 (MatLivre column re-measured after the allocator change).

| Case | MATLAB (ms) | Octave (ms) | Scilab (ms) | MatLivre (ms) | MatLivre vs MATLAB | vs Scilab |
|---|---:|---:|---:|---:|---|---|
| scalar_loop (2M iterations) | 4.57 | 2,267.82 | 1,662.67 | 2.69 | 1.7x faster | 619x faster |
| while_branch (Collatz, 2.9M steps) | 79.62 | 11,231.69 | 70,510.38 | 14.30 | 5.6x faster | 4,929x faster |
| fib_recursive (fib(20)) | 0.96 | 123.42 | 70.68 | 0.06 | 15.8x faster | 1,159x faster |
| array_growth (100k appends) | 6.83 | 377.48 | 211.68 | 1.18 | 5.8x faster | 179x faster |
| indexed_assign (1M writes) | 2.93 | 1,996.90 | 879.51 | 2.36 | 1.2x faster | 373x faster |
| vector_math (2M elements) | 107.40 | 92.81 | 180.59 | 10.65 | 10.1x faster | 17.0x faster |
| matmul_300 | 0.68 | 0.55 | 0.19 | 0.33 | 2.1x faster | 1.8x slower |
| linsolve_300 | 0.53 | 1.54 | 0.48 | 0.59 | 1.1x slower | 1.2x slower |
| sort_1e6 | 59.09 | 14.82 | 185.38 | 6.97 | 8.5x faster | 26.6x faster |
| string_build (20k appends) | 207.04 | 249.86 | 491.80 | 35.30 | 5.9x faster | 13.9x faster |
| struct_fields (500k updates) | 144.11 | 4,309.09 | 1,841.38 | 0.65 | 223x faster | 2,846x faster |
| cell_ops (100k writes + reads) | 21.54 | 561.40 | 349.36 | 6.70 | 3.2x faster | 52.1x faster |

Geometric mean: **5.5x faster than MATLAB, 83x faster than Octave, 83x faster than Scilab**.
MatLivre is faster than MATLAB in 11 of 12 cases, than Octave in all 12 and than Scilab in 10.
Where it is behind: `linsolve_300` (MATLAB by 13%, Scilab by 25%) and `matmul_300` (Scilab, which
uses Intel MKL, by 1.7x). Cases under 1 ms vary by tens of percent
between runs. The suite is small and favours MatLivre's native loop compilation; it does not
measure toolbox breadth or plotting. Details and the per-case Octave comparison are in
[`benchmarks/RESULTS.md`](benchmarks/RESULTS.md).

## Sparse matrices and large vectorized operations

[`benchmarks/bench_sparse.m`](benchmarks/bench_sparse.m) runs in MATLAB R2023b and MatLivre, and
[`bench_sparse.sce`](benchmarks/bench_sparse.sce) is its Scilab version (best of 3, checksums
agree; Scilab has no `spdiags`, implicit expansion or `pcg`, so those parts are written out).
Sparse matrices of 4,096 elements or more are held as compressed columns, so they cost
O(nonzeros) and can be far larger than memory would allow densely: a 1,000,000-unknown
tridiagonal system is built in about 55 ms and solved in about 60 ms (MATLAB: 23 ms for the
solve). Operations without a native sparse version expand the matrix, run, and compress the
result again, and fail with "Out of memory" when the dense form would not fit. Smaller sparse
matrices are dense arrays with a sparse flag. On large dense arrays MatLivre is ahead.

| Case | MATLAB (ms) | Scilab (ms) | MatLivre (ms) | vs MATLAB | vs Scilab |
|---|---:|---:|---:|---|---|
| sparse tridiagonal solve, n = 3,000 | 0.07 | 0.32 | 0.13 | 1.7x slower | 2.5x faster |
| 2-D Poisson solve, 2,500 unknowns | 1.69 | 87.5 | 1.52 | 1.1x faster | 57.5x faster |
| 200 sparse matrix-vector products, n = 3,000 | 2.10 | 6.99 | 4.09 | 1.9x slower | 1.7x faster |
| 20 builds with `sparse(i,j,s,n,n)`, n = 2,000 | 2.57 | 3.10 | 2.25 | 1.1x faster | 1.4x faster |
| conjugate gradients on the Poisson matrix | 3.02 | 2.87 | 4.16 | 1.4x slower | 1.5x slower |
| elementwise math, 1e7 elements | 139.8 | 485.2 | 31.9 | 4.4x faster | 15.2x faster |
| `cumsum`, 1e7 elements | 45.6 | 59.3 | 37.4 | 1.2x faster | 1.6x faster |
| `find` on a logical mask, 1e7 | 25.4 | 183.6 | 17.6 | 1.4x faster | 10.4x faster |
| broadcast arithmetic, 2000 x 2000 | 10.2 | 41.2 | 12.3 | 1.2x slower | 3.3x faster |
| row and column reductions, 2000 x 2000 | 8.18 | 102.6 | 8.19 | about equal | 12.5x faster |
| `sort`, 4e6 elements | 66.7 | 334.2 | 35.3 | 1.9x faster | 9.5x faster |
| `M*M`, 2000 x 2000 | 126.2 | 78.5 | 59.2 | 2.1x faster | 1.3x faster |

Square sparse systems of order 100 and up are solved with a sparse LU (Markowitz pivoting, taken
from the SolverLivre project) with a residual check and a fallback to the dense solver, and the
iterative solvers and the sparse products (`A*x`, `x'*A`) multiply with compressed columns,
which are built once per matrix and kept until it changes. Chains of elementwise operators on large
arrays (`exp(-x.^2) .* sin(20*x) + sqrt(x+1)`), comparisons and logical operators, rows and
columns expanded over a matrix (`M .* r + c - M`) and `find(x > a & x < b)` are evaluated fused,
in cache-sized blocks on several threads, so only the result is written to memory (`find` never
builds the logical array); reductions, `cumsum`, `max` and `min` work on the arrays in place and
in parallel when they are large, and `sum(A(:))` does not copy `A`.

## Other common operations

[`benchmarks/bench_more.m`](benchmarks/bench_more.m) times 34 everyday operations (linear algebra
factorizations, FFT, convolution, lookups, sorting, strings, function handles, structs and maps)
in MATLAB R2023b and MatLivre, best of 3, checksums recorded. MatLivre is faster in 9 of them,
within 3.7x of MATLAB in all of them, and 1.03x slower on the geometric mean; a selection:

| Case | MATLAB (ms) | MatLivre (ms) | MatLivre vs MATLAB |
|---|---:|---:|---|
| `rand(1,1e7)` and `randn(1000)` | 67.51 | 253.1 | 3.7x slower |
| `[Q,R] = qr`, 300 x 300 | 1.45 | 3.73 | 2.6x slower |
| `eig`, symmetric 300 x 300 | 2.45 | 4.56 | 1.9x slower |
| `fft`, 2^20 points | 6.62 | 10.00 | 1.5x slower |
| `chol`, 300 x 300 | 0.24 | 0.37 | 1.5x slower |
| `svd`, 300 x 300 | 5.55 | 8.25 | 1.5x slower |
| `[s,i] = sort`, 1e6 | 35.37 | 53.76 | 1.5x slower |
| `ismember`, 1e5 in 1e5 | 3.38 | 4.37 | 1.3x slower |
| `[L,U,P] = lu`, 300 x 300 | 0.62 | 0.70 | 1.1x slower |
| `unique`, 1e6 elements | 48.56 | 46.04 | 1.1x faster |
| `inv`, 300 x 300 | 2.17 | 1.88 | 1.2x faster |
| `mean`, `std`, `median`, `var`, 1e6 | 14.07 | 11.03 | 1.3x faster |
| `cellfun` with an anonymous function, 1e5 | 161.5 | 47.04 | 3.4x faster |
| `expm`, 150 x 150 | 3.44 | 0.93 | 3.7x faster |
| `strjoin` and `strsplit`, 2e4 words | 14.27 | 3.51 | 4.1x faster |
| `num2str` in a loop, 1e4 | 147.0 | 19.52 | 7.5x faster |
| 20,000 calls of a user function | 0.64 | 0.08 | 8.2x faster |
| `containers.Map`, 5,000 inserts and lookups | 71.77 | 8.13 | 8.8x faster |

## Why it is fast

- **Native code for loops and functions.** `for`/`while` loops and user functions over scalars,
  array elements, cell elements and struct fields are compiled to machine code with Cranelift.
  Everything else is interpreted, with identical behavior (differential tests run every case
  both ways).
- **Multithreaded linear algebra** through `faer`: blocked matrix products and LU
  factorization.
- **Parallel elementwise math and sorting** on large arrays (rayon).
- **Multithreaded FFT** of long vectors (four-step transform with the transposes folded into the
  passes, half-length transform for real input) and real-arithmetic `expm`.
- **A retaining allocator** (mimalloc): large arrays are not handed back to the OS and faulted in again
  on the next operation, which made products, solves and whole-array operations 25 to 35% faster.
- **No copies where MATLAB semantics allow it**: in-place appends (`x(end+1) = v`,
  `s = [s v]`), in-place element writes, operands read by reference.
- **A small runtime**: no JVM, no desktop framework to boot; the app is one native process.
