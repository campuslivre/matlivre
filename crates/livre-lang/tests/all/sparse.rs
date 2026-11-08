//! Sparse matrices against MATLAB R2023b (Live Editor output and `disp` of the results).

use livre_lang::{Interpreter, OutputKind};

fn shown(src: &str) -> Vec<String> {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    it.take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Value { text, .. } => Some(text),
            OutputKind::Text(t) => Some(t),
            OutputKind::Warning(w) => Some(format!("Warning: {w}")),
            OutputKind::Error(e) => Some(format!("Error: {e}")),
            _ => None,
        })
        .collect()
}

#[test]
fn displays_and_propagates_like_matlab() {
    assert_eq!(
        shown(concat!(
            "S = sparse([1 2 3], [1 3 2], [4 5 6])\nF = full(S)\nI = speye(3)\ns1 = S(2, 3)\ne = sparse(3, 3)\n",
            "D = spdiags([1 2 3; 4 5 6; 7 8 9], [-1 0 1], 3, 3)\nnz = nnz(S)\nv = nonzeros(S)'\n",
            "t = [issparse(S * 2), issparse(S + 1), issparse(S .* rand(3)), issparse(S * ones(3)), issparse(S'), issparse(S + S)]\n",
            "b = S \\ [1; 2; 3]\nc = class(S)\nL = sparse(logical([1 0; 0 1]))\n",
            "a = sparse([1 2], [1 2], [0.5 -1.25])\nb = sparse(10, 12, 7)\nc = sparse([1 12], [1 3], [1e6 2])\nd = sparse([1 2], [1 1], [pi 100.5])\n",
            "f = sparse([1 2],[1 2],[1 2]) * 0.1\nA = delsq(numgrid('S', 4))\n",
        )),
        [
            "S = \n\n   (1,1)        4\n   (3,2)        6\n   (2,3)        5",
            "F = 3×3\n\n     4     0     0\n     0     0     5\n     0     6     0",
            "I = \n\n   (1,1)        1\n   (2,2)        1\n   (3,3)        1",
            "s1 = \n\n   (1,1)        5",
            "e = \n\n   All zero sparse: 3x3",
            "D = \n\n   (1,1)        2\n   (2,1)        1\n   (1,2)        6\n   (2,2)        5\n   (3,2)        4\n   (2,3)        9\n   (3,3)        8",
            "nz = 3",
            "v = 1×3\n\n     4     6     5",
            "t = 1×6 logical array\n\n   1   0   1   0   1   1",
            "b = 3×1\n\n    0.2500\n    0.5000\n    0.4000",
            "c = 'double'",
            "L = 2x2 sparse logical array\n\n   (1,1)      1\n   (2,2)      1",
            "a = \n\n   (1,1)       0.5000\n   (2,2)      -1.2500",
            "b = \n\n  (10,12)       7",
            "c = \n\n   (1,1)        1000000\n  (12,3)              2",
            "d = \n\n   (1,1)       3.1416\n   (2,1)     100.5000",
            "f = \n\n   (1,1)       0.1000\n   (2,2)       0.2000",
            "A = \n\n   (1,1)        4\n   (2,1)       -1\n   (3,1)       -1\n   (1,2)       -1\n   (2,2)        4\n   (4,2)       -1\n   (1,3)       -1\n   (3,3)        4\n   (4,3)       -1\n   (2,4)       -1\n   (3,4)       -1\n   (4,4)        4",
        ]
    );
}

#[test]
fn test_matrices_and_trees_match_matlab() {
    assert_eq!(
        shown(concat!(
            "disp(numgrid('S', 5))\ndisp(numgrid('L', 6))\ndisp(numgrid('C', 7))\ndisp(numgrid('A', 9))\n",
            "A = delsq(numgrid('S', 5));\ndisp(etree(A))\ndisp(symrcm(A))\ndisp(symbfact(A)')\n",
            "disp(colperm(sparse([1 0 1; 1 1 1; 0 0 1])))\n[xx, yy] = treelayout([0 1 1 2 2]); disp(xx); disp(yy)\n",
            "disp(sprank(sparse([1 1 0; 0 0 0; 1 1 0])))\ndisp(nzmax(sparse(3,3))); disp(nzmax(speye(3)))\n",
            "[B, d] = spdiags(sparse([1 2 0; 3 4 5; 0 6 7])); disp(B); disp(d')\n",
            "disp(full(ichol(sparse([4 1 0; 1 4 1; 0 1 4]))))\n[Lu, Uu] = ilu(sparse([4 1 0; 1 4 1; 0 1 4])); disp(full(Lu)); disp(full(Uu))\n",
        )),
        [
            "     0     0     0     0     0\n     0     1     4     7     0\n     0     2     5     8     0\n     0     3     6     9     0\n     0     0     0     0     0\n",
            "     0     0     0     0     0     0\n     0     1     3     5     9     0\n     0     2     4     6    10     0\n     0     0     0     7    11     0\n     0     0     0     8    12     0\n     0     0     0     0     0     0\n",
            "     0     0     0     0     0     0     0\n     0     1     4     7    12    17     0\n     0     2     5     8    13    18     0\n     0     3     6     9    14    19     0\n     0     0     0    10    15    20     0\n     0     0     0    11    16    21     0\n     0     0     0     0     0     0     0\n",
            "     0     0     0     0     0     0     0     0     0\n     0     0     6    10    12    14    16     0     0\n     0     1     7     0     0     0    17    20     0\n     0     2     0     0     0     0     0    21     0\n     0     3     0     0     0     0     0    22     0\n     0     4     0     0     0     0     0    23     0\n     0     5     8     0     0     0    18    24     0\n     0     0     9    11    13    15    19     0     0\n     0     0     0     0     0     0     0     0     0\n",
            "     2     3     4     5     6     7     8     9     0\n",
            "     1     4     2     7     5     3     8     6     9\n",
            "     3\n     4\n     4\n     4\n     4\n     4\n     3\n     2\n     1\n",
            "     2     1     3\n",
            "    0.5000    0.3750    0.7500    0.2500    0.5000\n",
            "    0.7500    0.5000    0.2500    0.2500    0.2500\n",
            "     2\n",
            "     1\n",
            "     3\n",
            "     3     1     0\n     6     4     2\n     0     7     5\n",
            "    -1     0     1\n",
            "    2.0000         0         0\n    0.5000    1.9365         0\n         0    0.5164    1.9322\n",
            "    1.0000         0         0\n    0.2500    1.0000         0\n         0    0.2667    1.0000\n",
            "    4.0000    1.0000         0\n         0    3.7500    1.0000\n         0         0    3.7333\n",
        ]
    );
}

#[test]
fn iterative_solvers_report_like_matlab() {
    let out = shown(concat!(
        "x = pcg([4 1; 1 3], [1; 2])\n[x2, fl] = gmres([4 1; 1 3], [1; 2])\n",
        "A = delsq(numgrid('S', 5)); b = ones(size(A,1),1);\nx = pcg(A, b);\nx = pcg(A, b, 1e-10, 2);\n",
        "r = [];\nfor s = {'gmres', 'bicgstab', 'minres', 'cgs', 'bicg', 'qmr', 'lsqr'}, [y, f] = feval(s{1}, A, b); r(end+1) = norm(A*y - b) < 1e-8 && f == 0; end, disp(all(r))\n",
    ));
    assert_eq!(out[0], "pcg converged at iteration 2 to a solution with relative residual 0.\n");
    assert_eq!(out[1], "x = 2×1\n\n    0.0909\n    0.6364");
    assert_eq!(out[2], "x2 = 2×1\n\n    0.0909\n    0.6364");
    assert_eq!(out[3], "fl = 0");
    assert_eq!(out[4], "pcg converged at iteration 3 to a solution with relative residual 0.\n");
    assert_eq!(
        out[5],
        "pcg stopped at iteration 2 without converging to the desired tolerance 1e-10\nbecause the maximum number of iterations was reached.\nThe iterate returned (number 2) has relative residual 0.091.\n"
    );
    assert_eq!(out[6], "   1\n");
}

/// `A\b` on sparse matrices of order 100 and up goes through the sparse LU; results (recorded
/// from MATLAB R2023b) must match, including the singular case that falls back to the dense path.
#[test]
fn sparse_solve_matches_matlab() {
    let src = concat!(
        "n = 200; e = ones(n,1); A = spdiags([-e 2*e -e], -1:1, n, n); x0 = (1:n)'; x = A \\ (A*x0);\n",
        "fprintf('T1 %d %.10g\\n', norm(x - x0) < 1e-6, sum(x));\n",
        "m = 12; I = speye(m); T = spdiags([-ones(m,1) 2*ones(m,1) -ones(m,1)], -1:1, m, m); P = kron(I,T)+kron(T,I);\n",
        "F = [ones(m*m,1) (1:m*m)']; X = P \\ F;\n",
        "fprintf('T2 %d %.10g %.10g %d\\n', norm(P*X - F, 'fro') < 1e-8, sum(X(:,1)), sum(X(:,2)), issparse(X));\n",
        "n = 150; Q = sparse(1:n, [n 1:n-1], 1:n) + sparse(1:n-1, 2:n, 0.5, n, n);\n",
        "y = Q \\ ones(n,1); fprintf('T3 %d %.10g\\n', norm(Q*y - ones(n,1)) < 1e-8, sum(y));\n",
        "n = 120; B = spdiags([-e(1:n) 2*e(1:n) -e(1:n)], -1:1, n, n); B(7,:) = 0;\n",
        "z = B \\ ones(n,1); fprintf('T4 %d %d\\n', any(isinf(z)) || any(isnan(z)), numel(z));\n",
        "n = 130; D = speye(n) * 1e-14 + sparse(1:n-1, 2:n, 1e-14, n, n); u = D \\ ones(n,1); fprintf('T5 %d %.6g\\n', norm(D*u - ones(n,1)) < 1e-6, sum(u));\n",
    );
    let out: Vec<String> = shown(src).into_iter().map(|s| s.trim_end().to_string()).collect();
    assert_eq!(
        out,
        [
            "T1 1 20100",
            "T2 1 984.7456035 71394.05625 0",
            "T3 1 5.243594236",
            "Warning: Matrix is singular to working precision.",
            "T4 1 120",
            "T5 1 6.5e+15",
        ]
    );
}

/// Iterative solvers on a 1,600-unknown Poisson matrix (stored as compressed columns inside the
/// solver): flags, iteration counts and solutions as in MATLAB R2023b.
#[test]
fn iterative_solvers_on_a_larger_sparse_system_match_matlab() {
    let src = concat!(
        "m = 40; I = speye(m); T = spdiags([-ones(m,1) 2*ones(m,1) -ones(m,1)], -1:1, m, m); P = kron(I,T)+kron(T,I); f = ones(m*m,1);\n",
        "[u,fl,rr,it] = pcg(P, f, 1e-8, 500); fprintf('pcg %d %d %d %.6g\\n', fl, it, norm(P*u-f) < 1e-5, sum(u));\n",
        "[u,fl,rr,it] = bicgstab(P, f, 1e-8, 500); fprintf('bicgstab %d %.1f %d %.6g\\n', fl, it, norm(P*u-f) < 1e-5, sum(u));\n",
        "[u,fl,rr,it] = gmres(P, f, 30, 1e-8, 40); fprintf('gmres %d %d %d %d %.6g\\n', fl, it(1), it(2), norm(P*u-f) < 1e-5, sum(u));\n",
        "[u,fl,rr,it] = lsqr(P, f, 1e-8, 500); fprintf('lsqr %d %d %d\\n', fl, it, norm(P*u-f) < 1e-5);\n",
    );
    let out: Vec<String> = shown(src).into_iter().map(|s| s.trim_end().to_string()).collect();
    assert_eq!(out, ["pcg 0 74 1 99117.6", "bicgstab 0 53.5 1 99117.6", "gmres 0 6 30 1 99117.6", "lsqr 0 267 1"]);
}

/// Products with a sparse matrix use cached compressed columns; the cache must not survive a
/// change of the matrix (element assignment, in a loop or not) and must agree with the dense path.
#[test]
fn sparse_products_follow_changes_to_the_matrix() {
    let src = concat!(
        "n = 100; A = spdiags([-ones(n,1) 2*ones(n,1) -ones(n,1)], -1:1, n, n); v = (1:n)';\n",
        "y1 = A*v; d1 = norm(y1 - full(A)*v); fprintf('P1 %d\\n', d1 < 1e-9);\n",
        "A(3,7) = 5; y2 = A*v; fprintf('P2 %d %d\\n', norm(y2 - full(A)*v) < 1e-9, norm(y2 - y1) > 1);\n",
        "for k = 1:3, A(k,k+2) = k; y3 = A*v; if norm(y3 - full(A)*v) > 1e-9, disp('bad loop'); end, end\n",
        "z = v'*A; fprintf('P3 %d\\n', norm(z' - full(A)'*v) < 1e-9);\n",
        "B = A; B(1,1) = 100; fprintf('P4 %d %d\\n', norm(A*v - full(A)*v) < 1e-9, norm(B*v - full(B)*v) < 1e-9);\n",
        "x = A \\ v; fprintf('P5 %d\\n', norm(A*x - v) < 1e-8); A(2,2) = 9; x = A \\ v; fprintf('P6 %d\\n', norm(A*x - v) < 1e-8);\n",
        "C = A*[v 2*v]; fprintf('P7 %d\\n', norm(C - full(A)*[v 2*v]) < 1e-9);\n",
    );
    let out: Vec<String> = shown(src).into_iter().map(|s| s.trim_end().to_string()).collect();
    assert_eq!(out, ["P1 1", "P2 1 1", "P3 1", "P4 1 1", "P5 1", "P6 1", "P7 1"]);
}

/// Large variables passed to numeric builtins are lent, not copied; they must be back afterwards,
/// also when the call fails, and other arguments that read the same variable must still see it.
#[test]
fn large_variables_survive_builtin_calls() {
    let src = concat!(
        "n = 100; A = spdiags([-ones(n,1) 2*ones(n,1) -ones(n,1)], -1:1, n, n); b = ones(n,1);\n",
        "k = nnz(A); s = size(A); t = issparse(A); m = norm(A*b - A*b); fprintf('L1 %d %d %d %d %d\\n', k, s(1), s(2), t, m);\n",
        "[x, fl] = pcg(A, b, 1e-10, 200); fprintf('L2 %d %d\\n', norm(A*x - b) < 1e-8, nnz(A));\n",
        "c = sum(A(:)) + sum(sum(A)); d = max(A, [], 'all'); fprintf('L3 %d %d %d\\n', c, d, exist('A'));\n",
        "try, r = norm(A, 'bogus'); catch, end\n",
        "fprintf('L4 %d %d\\n', nnz(A), numel(A));\n",
        "w = norm(A*b - b, 1) + norm(b - A*b, 1); fprintf('L5 %d\\n', nnz(A));\n",
        "z = dot(b, b) + trace(A) + nnz(A); fprintf('L6 %d %d\\n', z, nnz(A));\n",
        "B = A; fprintf('L7 %d\\n', nnz(B) + nnz(A));\n",
        "A(1, 1) = 7; fprintf('L8 %d %d\\n', nnz(A), A(1, 1));\n",
    );
    let out: Vec<String> = shown(src).into_iter().map(|s| s.trim_end().to_string()).collect();
    assert_eq!(out, ["L1 298 100 100 1 0", "L2 1 298", "L3 4 2 1", "L4 298 10000", "L5 298", "L6 598 298", "L7 596", "L8 298 7"]);
}

/// Sparse matrices of 4096 elements or more are held as compressed columns; every operation that
/// runs on them natively must agree with the same computation on dense arrays.
#[test]
fn compressed_sparse_matrices_agree_with_dense_arrays() {
    let src = concat!(
        "n = 100; e = ones(n,1); T = spdiags([-e 2*e -e], -1:1, n, n); D = 2*eye(n) - diag(ones(n-1,1),1) - diag(ones(n-1,1),-1);\n",
        "x = (1:n)'; b = sin(x);\n",
        "ok = @(a, b) norm(full(a) - full(b), 1) < 1e-9 * (1 + norm(full(b), 1));\n",
        "fprintf('C1 %d %d %d %d\\n', issparse(T), nnz(T), ok(T, D), isequal(size(T), [n n]));\n",
        "fprintf('C2 %d %d %d %d %d\\n', ok(T', D'), ok(T + T, 2*D), ok(T - 0.5*T, 0.5*D), ok(2*T, 2*D), ok(T/4, D/4));\n",
        "fprintf('C3 %d %d %d %d\\n', ok(T*T, D*D), ok(T*x, D*x), ok(x'*T, x'*D), ok(T\\b, D\\b));\n",
        "fprintf('C4 %d %d %d %d %d\\n', ok(T.*T, D.*D), ok(-T, -D), ok(sum(T), sum(D)), ok(sum(T,2), sum(D,2)), ok(diag(T), diag(D)));\n",
        "[i, j, v] = find(T); fprintf('C5 %d %d %d %d %d\\n', numel(i), trace(T) == trace(D), norm(T,1) == norm(D,1), norm(T,inf) == norm(D,inf), abs(norm(T,'fro') - norm(D,'fro')) < 1e-9);\n",
        "fprintf('C6 %d %d %d %d %d %d\\n', full(T(3,4)), full(T(5,5)), ok(T(:,7), D(:,7)), ok(T(2:5,3:9), D(2:5,3:9)), ok(T([1 3 3],:), D([1 3 3],:)), ok(T(:,[4 2]), D(:,[4 2])));\n",
        "K = kron(T, speye(3)); fprintf('C7 %d %d %d\\n', issparse(K), isequal(size(K), [3*n 3*n]), ok(K, kron(D, eye(3))));\n",
        "S = sparse([1 2 2 5 5], [1 2 2 7 7], [3 4 5 1 -1], n, n); fprintf('C8 %d %d %d %d\\n', nnz(S), full(S(2,2)), full(S(5,7)), issparse(S));\n",
        "I = speye(n); fprintf('C9 %d %d %d\\n', nnz(I), ok(I, eye(n)), ok(I*T, D));\n",
        "A = T; A(2,5) = 3; A(1,1) = 0; B = D; B(2,5) = 3; B(1,1) = 0; fprintf('C10 %d %d %d\\n', ok(A, B), nnz(A), ok(T, D));\n",
        "P = T + speye(n); [u, fl] = pcg(P, b, 1e-10, 300); fprintf('C11 %d %d\\n', fl, norm(P*u - b) < 1e-7);\n",
        "fprintf('C12 %d %d %d %d\\n', nnz(T > 0), nnz(abs(T)), class(T(1,1)) == \"double\", numel(T(:)));\n",
        "H = [T T]; V = [T; T]; fprintf('C13 %d %d %d %d\\n', isequal(size(H), [n 2*n]), isequal(size(V), [2*n n]), ok(H, [D D]), ok(V, [D; D]));\n",
        "L = T > 0; fprintf('C14 %d %d %s\\n', issparse(L), nnz(L), class(L));\n",
        "Z = sparse(200, 300); fprintf('C15 %d %d %d\\n', nnz(Z), size(Z, 1), size(Z, 2));\n",
        "fprintf('C16 %d %d\\n', ok(T^2, D^2), ok(T.^2, D.^2));\n",
    );
    let out: Vec<String> = shown(src).into_iter().map(|s| s.trim_end().to_string()).collect();
    assert_eq!(
        out,
        [
            "C1 1 298 1 1",
            "C2 1 1 1 1 1",
            "C3 1 1 1 1",
            "C4 1 1 1 1 1",
            "C5 298 1 1 1 1",
            "C6 -1 2 1 1 1 1",
            "C7 1 1 1",
            "C8 2 9 0 1",
            "C9 100 1 1",
            "C10 1 298 1",
            "C11 0 1",
            "C12 100 298 1 10000",
            "C13 1 1 1 1",
            "C14 1 100 logical",
            "C15 0 200 300",
            "C16 1 1",
        ]
    );
}

/// A tridiagonal system of 200,000 unknowns (the dense form would need 320 GB).
#[test]
fn huge_sparse_systems_are_held_compressed() {
    let src = concat!(
        "n = 200000; e = ones(n,1); A = spdiags([-e 2*e -e], -1:1, n, n); b = A*ones(n,1); x = A\\b;\n",
        "fprintf('H1 %d %d %d %d\\n', nnz(A), issparse(A), norm(x - 1, inf) < 1e-6, size(A, 1));\n",
        "B = A'; C = A*A; fprintf('H2 %d %d %d\\n', nnz(B), nnz(C), full(C(1,1)));\n",
        "[u, fl] = pcg(A + speye(n), b, 1e-8, 2000); fprintf('H3 %d\\n', fl);\n",
        "w = whos('A'); fprintf('H4 %d\\n', w.bytes < 1e8);\n",
        "s = sparse(1:n, 1:n, 1:n, n, n); fprintf('H5 %d %d\\n', nnz(s), full(s(n, n)));\n",
        "try, F = full(A); catch err, disp(err.message(1:11)); end\n",
    );
    let out: Vec<String> = shown(src).into_iter().map(|s| s.trim_end().to_string()).collect();
    assert_eq!(out, ["H1 599998 1 1 200000", "H2 599998 999994 5", "H3 0", "H4 1", "H5 200000 200000", "Out of memo"]);
}

/// The real vector 2-norm avoids overflow and underflow like MATLAB's.
#[test]
fn vector_two_norm_is_scaled() {
    let src = concat!(
        "fprintf('N1 %g %g %g %g\\n', norm([3 4]), norm([1e200 1e200]), norm([1e-200 1e-200]), norm([0 0]));\n",
        "fprintf('N2 %g %g %g %g\\n', norm([NaN 1]), norm([Inf 1]), norm([3; 4]), norm([1 -2 2]));\n",
    );
    let out: Vec<String> = shown(src).into_iter().map(|s| s.trim_end().to_string()).collect();
    assert_eq!(out, ["N1 5 1.41421e+200 1.41421e-200 0", "N2 NaN Inf 5 3"]);
}

/// Banded systems (few diagonals) are solved by a banded LU with partial pivoting; results agree
/// with the dense solver and MATLAB R2023b, including with tiny pivots and several right sides.
#[test]
fn banded_sparse_systems_match_dense_solves() {
    let src = concat!(
        "n = 150; k = (1:n)';\n",
        "B = [sin(k)+0.3, 1e-9*cos(2*k), 0.5*cos(k), sin(3*k), 1+0.2*sin(5*k), cos(7*k)];\n",
        "A = spdiags(B, [-2 -1 0 1 2 3], n, n);\n",
        "D = full(A);\n",
        "b = cos(k) + 2; X = [b, k/n, sin(2*k)];\n",
        "x1 = A\\X; x2 = D\\X;\n",
        "fprintf('B1 %d %d %d\\n', issparse(A), nnz(A) > 600, norm(x1 - x2, 1) < 1e-8*norm(x2, 1));\n",
        "fprintf('B2 %d\\n', norm(D*x1 - X, 1) < 1e-8*norm(X, 1));\n",
        "m = 500; e = ones(m,1); T = spdiags([-e 2*e -e], -1:1, m, m); bt = T*(1:m)';\n",
        "xt = T\\bt; fprintf('B3 %d\\n', norm(xt - (1:m)', inf) < 1e-6);\n",
        "P = spdiags([e 1e-12*e e], -1:1, m, m); xp = P\\ones(m,1); fprintf('B4 %d\\n', norm(full(P)*xp - ones(m,1), inf) < 1e-6);\n",
        "S = spdiags([e 0*e e], -1:1, m, m); lastwarn(''); xs = S\\ones(m,1); [w, id] = lastwarn; fprintf('B5 %d\\n', ~isempty(w));\n",
    );
    let out: Vec<String> = shown(src).into_iter().map(|s| s.trim_end().to_string()).collect();
    assert_eq!(out, ["B1 1 1 1", "B2 1", "B3 1", "B4 1", "B5 0"]);
}

/// Sparse systems with a wide band or general pattern (2-D Poisson, convection-diffusion) are
/// solved by faer's sparse Cholesky or LU; a singular one still warns like MATLAB.
#[test]
fn wide_sparse_systems_match_dense_solves() {
    let src = concat!(
        "m = 40; e = ones(m,1); I = speye(m,m);\n",
        "T = spdiags([-e 2*e -e], -1:1, m, m); P = kron(I,T) + kron(T,I); f = (1:m*m)'/(m*m);\n",
        "u = P\\f; fprintf('W1 %d %d\\n', issparse(P), norm(P*u - f, inf) < 1e-9);\n",
        "D = full(P); ud = D\\f; fprintf('W2 %d\\n', norm(u - ud, inf) < 1e-9);\n",
        "Tn = spdiags([-1.5*e 2*e -0.5*e], -1:1, m, m); Q = kron(I,Tn) + kron(Tn,I);\n",
        "v = Q\\f; fprintf('W3 %d %d\\n', norm(Q*v - f, inf) < 1e-9, norm(v - full(Q)\\f, inf) < 1e-9);\n",
        "F = [f, 2*f, sin(f)]; V = P\\F; fprintf('W4 %d\\n', norm(P*V - F, inf) < 1e-9);\n",
        "N = P - 4*speye(m*m); N(1,:) = 0; lastwarn(''); w = N\\f; [msg, id] = lastwarn; fprintf('W5 %d\\n', ~isempty(msg));\n",
    );
    let out: Vec<String> = shown(src).into_iter().map(|s| s.trim_end().to_string()).collect();
    assert_eq!(out, ["W1 1 1", "W2 1", "W3 1 1", "W4 1", "Warning: Matrix is singular to working precision.", "W5 1"]);
}
