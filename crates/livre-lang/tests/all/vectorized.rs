//! Reductions, cumulative sums, max/min, `find` and implicit expansion on N-D and large arrays,
//! against MATLAB R2023b (`data/vectorized.m` printed with `fprintf`; the reference output is in
//! `data/vectorized_matlab.txt`). The large cases cross the thresholds of the parallel paths.

use livre_lang::{Interpreter, OutputKind};

#[test]
fn reductions_scans_and_expansion_match_matlab() {
    let mut it = Interpreter::new();
    let _ = it.run_source(include_str!("data/vectorized.m"));
    let got: Vec<String> = it
        .take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Text(t) => Some(t.trim_end().to_string()),
            OutputKind::Error(e) => Some(format!("Error: {e}")),
            _ => None,
        })
        .collect();
    let want: Vec<&str> = include_str!("data/vectorized_matlab.txt").lines().collect();
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(&want) {
        assert_eq!(g, w);
    }
}

/// Eigenvalues, QR, Cholesky, LU, norm, convolution, `ismember`, `unique` and `containers.Map` on
/// matrices large enough to take the fast kernels (`data/linalg_fast.m`), against MATLAB R2023b.
#[test]
fn linear_algebra_and_lookups_match_matlab() {
    let mut it = Interpreter::new();
    let _ = it.run_source(include_str!("data/linalg_fast.m"));
    let got: String = it
        .take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Text(t) => Some(t),
            OutputKind::Error(e) => Some(format!("Error: {e}
")),
            _ => None,
        })
        .collect();
    let want = include_str!("data/linalg_fast_matlab.txt");
    for (g, w) in got.trim_end().lines().zip(want.trim_end().lines()) {
        assert_eq!(g, w);
    }
    assert_eq!(got.trim_end().lines().count(), want.trim_end().lines().count());
}

/// Chains of elementwise operators on large arrays are evaluated fused; the result must equal
/// the same computation one operator at a time, as in MATLAB (`data/fusion.m`), including the
/// cases that turn complex or hold NaN, where fusion gives way to the ordinary evaluator.
#[test]
fn fused_expressions_equal_stepwise_evaluation() {
    let mut it = Interpreter::new();
    let _ = it.run_source(include_str!("data/fusion.m"));
    let got: String = it
        .take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Text(t) => Some(t),
            OutputKind::Error(e) => Some(format!("Error: {e}\n")),
            _ => None,
        })
        .collect();
    assert_eq!(got.trim_end(), include_str!("data/fusion_matlab.txt").trim_end());
}

/// Comparisons, logical operators, `find(expression)` and rows or columns expanded over a matrix
/// are fused as well; each result equals the operator-by-operator computation, with the output of
/// MATLAB R2023b (`data/fusion2.m`), including NaN in logical operators and complex outcomes.
#[test]
fn fused_comparisons_find_and_broadcast_equal_stepwise_evaluation() {
    let mut it = Interpreter::new();
    let _ = it.run_source(include_str!("data/fusion2.m"));
    let got: String = it
        .take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Text(t) => Some(t),
            OutputKind::Error(e) => Some(format!("Error: {e}\n")),
            _ => None,
        })
        .collect();
    assert_eq!(got.trim_end(), include_str!("data/fusion2_matlab.txt").trim_end());
}

/// `f(A(:))` on a large variable is lent with its shape flattened instead of copied; the variable
/// keeps its shape and values afterwards, also when the call fails (output of MATLAB R2023b).
#[test]
fn flattened_large_arguments_are_lent_and_restored() {
    let mut it = Interpreter::new();
    let src = concat!(
        "M = reshape(1:6000, 60, 100);\n",
        "s = sum(M(:)); c = size(M(:)); d = size(M);\n",
        "fprintf('F1 %d %s %s\\n', s, mat2str(c), mat2str(d));\n",
        "m = max(M(:)); n = numel(M(:)); q = norm(M(:)) > 0; fprintf('F2 %d %d %d %s\\n', m, n, q, mat2str(size(M)));\n",
        "try, r = norm(M(:), 'bogus'); catch, end; fprintf('F3 %s\\n', mat2str(size(M)));\n",
        "t = sum(M(:)) + sum(M(:)); u = sum(M(:) .* 2); fprintf('F4 %d %d\\n', t, u);\n",
        "T = zeros(4,5,300); T(:) = 1:6000; w = sum(T(:)); fprintf('F5 %d %s %d\\n', w, mat2str(size(T)), numel(find(T(:) > 3000)));\n",
        "k = sum(M(:), 1); l = any(M(:) > 5999); fprintf('F6 %d %d %s\\n', k, l, mat2str(size(M)));\n",
    );
    let _ = it.run_source(src);
    let got: String = it
        .take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Text(t) => Some(t),
            OutputKind::Error(e) => Some(format!("Error: {e}\n")),
            _ => None,
        })
        .collect();
    let want = "F1 18003000 [6000 1] [60 100]\nF2 6000 6000 1 [60 100]\nF3 [60 100]\nF4 36006000 36006000\nF5 18003000 [4 5 300] 3000\nF6 18003000 1 [60 100]";
    assert_eq!(got.trim_end(), want);
}
