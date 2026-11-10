# Known limits

- Saved outputs in `.mlx` files are not restored when opening (run the script to regenerate
  them) and are not written back.
- Rich text in live scripts (bold and italic runs, links, equations, images) is shown as plain
  text.
- Functions still to implement are listed in [`docs/matlab-coverage.md`](matlab-coverage.md).
  Most of them are desktop or Simulink items, or need a toolbox that MATLAB R2023b here lacks;
  among the rest are DAE index reduction (`reduceDAEIndex`, `reduceDAEToODE`,
  `reduceRedundancies`), the sparse orderings (`amd`, `colamd`, `symamd`, `dissect`) and
  spreadsheet and XML import options.
- `subexpr` picks the common subexpression that saves the most nodes, a rule inferred from
  MATLAB's results; sums with integer coefficients (`2*x + 2*y + a`) are not matched.
- `symmatrix` keeps the matrix form only for `+`, `-`, `*`, scalar multiples and the transpose,
  and simplifies only `A + A` and `A*A`: `A*A*A` shows as `A^2*A` (MATLAB: `A^3`), and
  `inv`, `trace` and indexing return plain syms or fail.
- The Symbolic Math special functions evaluate to double precision only: `vpa` keeps
  `sinint(2)` and the like in symbolic form (`catalan` and `eulergamma` evaluate). Some
  complex-valued cases (`logint` and `igamma` of negative numbers, `ellipticPi` with `n > 1`)
  give `NaN`, and polynomials with symbolic parameters (`gegenbauerC(3, a, x)`) are equal to
  MATLAB's but grouped differently. `poles` finds the poles of rational functions only
  (not those of `tan(x)`).
- Symbolic units cover about 100 common units with the SI prefixes, out of MATLAB's larger
  list; conversion to a unit system supports SI only, `unitInfo` lists only the units
  MatLivre knows, and `simplify` does not combine terms with different units of the same
  dimension (`2*[m] + 300*[cm]`).
- The Z, Fourier and Hilbert transforms cover the common transform pairs (polynomials and
  exponentials, sines and cosines, Gaussians, Dirac and Heaviside functions); other inputs
  stay unevaluated where MATLAB finds a closed form. `iztrans` needs rational poles, and
  `smithForm` and `hermiteForm` work on integer matrices only.
- `dsolve` solves single linear equations (constant coefficients, or first order with
  variable coefficients) with initial conditions; systems and nonlinear equations are not
  solved, and the order of terms in some solutions differs from MATLAB's
  (`C1*exp(-t) + exp(t)/2`).
- Table custom properties (`addprop`) are kept and read back, but `T.Properties` shows as a
  struct rather than MATLAB's `TableProperties` object.
- `histcounts` with only a number of bins places its rounded edges by a simpler rule than
  MATLAB's, so the edges (and counts) can differ; given edges, widths, limits or the
  automatic method, they match.
- `Tiff` reads and writes uncompressed, PackBits, LZW and Deflate data without a predictor;
  JPEG-compressed and BigTIFF files are not read, and mode `'w8'` writes a classic TIFF.
- `combine` handles the `powers`, `exp`, `log` (with `IgnoreAnalyticConstraints`) and
  `sincos` targets. `sympref` keeps every preference, but of them only `HeavisideAtOrigin`
  changes results.
- Chart layouts that MATLAB fits to the screen (word clouds, bubble clouds, automatic
  `binscatter` bins) are approximated, `geobubble` draws no basemap, and graph plots place nodes like MATLAB only for the `'circle'` layout and simple `'layered'` ones (force layouts differ).
- Dithering in `rgb2ind` and `dither` is plain Floyd-Steinberg, so individual pixels can
  differ from MATLAB's; `'nodither'` matches.
- Random numbers come from MATLAB's Mersenne Twister, so `rand`, `randi` and `randperm` give
  MATLAB's values for a seed, but `randn` with the default Ziggurat transform does not (the
  `Polar` and `Inversion` transforms of `RandStream` do), and `randperm(n, k)` differs when `k`
  is more than a fifth of `n`.
- `timer` callbacks run during `pause`, `wait` and `drawnow`, but not once a script has
  finished, since there is no idle event loop.
- `settings` holds only a few of MATLAB's product settings (`matlab.fonts.codefont`), personal
  values last for the session instead of being saved, and a group lists its settings in the
  order they were added rather than in MATLAB's order.
- Sparse matrices are compressed columns from 4,096 elements up. Construction (`sparse`,
  `speye`, `spdiags`, `kron`), `A*x`, `A'`, `A+B`, `A*B`, `A\b`, `A(i,j)`, `A(i,j) = x`, `nnz`, `size`,
  `sum`, `find`, `diag`, `norm` and the iterative solvers run on them directly; other
  functions expand the matrix first, so they are limited to 2e8 elements.
- Windows is the main platform. Linux builds and runs (tested on Ubuntu 24.04, GNOME/Wayland),
  but the benchmarks and most testing are on Windows.
