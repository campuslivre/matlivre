//! Table reshaping, joins and display against MATLAB R2023b (Live Editor output).

use livre_lang::{Interpreter, OutputKind};

fn shown(src: &str) -> Vec<String> {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    it.take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Value { text, .. } => Some(text),
            OutputKind::Text(t) => Some(t),
            OutputKind::Error(e) => Some(format!("Error: {e}")),
            _ => None,
        })
        .collect()
}

fn check(src: &str, expected: &[&str]) {
    assert_eq!(shown(src), expected, "{src}");
}

const T: &str = "T = table([1;2;3], [4;5;6], {'a';'b';'c'}, 'VariableNames', {'X','Y','Name'});\n";

#[test]
fn display_centers_columns_like_matlab() {
    check(
        "t00 = table([1;22;333],[0.5;1;2], {'x';'yy';'zzz'}, 'VariableNames', {'LongerName','B','C'})\nt1 = table([pi; 12345.678; 1e6; 0.001; -2.5])\nt3 = table(int8([1;-2]), [true;false])\n",
        &[
            "t00 = 3×3 table\n\n    LongerName     B        C   \n    __________    ___    _______\n\n         1        0.5    {'x'  }\n        22          1    {'yy' }\n       333          2    {'zzz'}",
            "t1 = 5×1 table\n\n     Var1 \n    ______\n\n    3.1416\n     12346\n     1e+06\n     0.001\n      -2.5",
            "t3 = 2×2 table\n\n    Var1    Var2 \n    ____    _____\n\n      1     true \n     -2     false",
        ],
    );
}

#[test]
fn adding_moving_merging_splitting() {
    check(
        &format!("{T}A2 = addvars(T, [7;8;9], 'Before', 'Y')\nM2 = movevars(T, 'X', 'After', 'Name')\nG2 = mergevars(T, [1 2], 'NewVariableName', 'XY')\nS = splitvars(G2)\n"),
        &[
            "A2 = 3×4 table\n\n    X    Var2    Y    Name \n    _    ____    _    _____\n\n    1     7      4    {'a'}\n    2     8      5    {'b'}\n    3     9      6    {'c'}",
            "M2 = 3×3 table\n\n    Y    Name     X\n    _    _____    _\n\n    4    {'a'}    1\n    5    {'b'}    2\n    6    {'c'}    3",
            "G2 = 3×2 table\n\n      XY      Name \n    ______    _____\n\n    1    4    {'a'}\n    2    5    {'b'}\n    3    6    {'c'}",
            "S = 3×3 table\n\n    XY_1    XY_2    Name \n    ____    ____    _____\n\n     1       4      {'a'}\n     2       5      {'b'}\n     3       6      {'c'}",
        ],
    );
}

#[test]
fn varfun_rowfun_rows2vars() {
    check(
        &format!("{T}V = varfun(@(x) x*2, T, 'InputVariables', {{'X','Y'}})\nV2 = varfun(@mean, T(:,1:2))\nRF = rowfun(@(a,b) a+b, T, 'InputVariables', {{'X','Y'}})\nR = rows2vars(table([1;2],[3;4],'VariableNames',{{'a','b'}}))\n"),
        &[
            "V = 3×2 table\n\n    Fun_X    Fun_Y\n    _____    _____\n\n      2        8  \n      4       10  \n      6       12  ",
            "V2 = 1×2 table\n\n    mean_X    mean_Y\n    ______    ______\n\n      2         5   ",
            "RF = 3×1 table\n\n    Var1\n    ____\n\n     5  \n     7  \n     9  ",
            "R = 2×3 table\n\n    OriginalVariableNames    Var1    Var2\n    _____________________    ____    ____\n\n            {'a'}             1       2  \n            {'b'}             3       4  ",
        ],
    );
}

#[test]
fn joins() {
    check(
        "L = table([1;2;3], {'a';'b';'c'}, 'VariableNames', {'K','A'});\nRt = table([2;3;4], [20;30;40], 'VariableNames', {'K','B'});\nIJ = innerjoin(L, Rt)\nOJ = outerjoin(L, Rt)\nOJ3 = outerjoin(L, Rt, 'Type', 'left', 'MergeKeys', true)\n",
        &[
            "IJ = 2×3 table\n\n    K      A      B \n    _    _____    __\n\n    2    {'b'}    20\n    3    {'c'}    30",
            "OJ = 4×4 table\n\n    K_L        A         K_Rt     B \n    ___    __________    ____    ___\n\n      1    {'a'     }    NaN     NaN\n      2    {'b'     }      2      20\n      3    {'c'     }      3      30\n    NaN    {0×0 char}      4      40",
            "OJ3 = 3×3 table\n\n    K      A       B \n    _    _____    ___\n\n    1    {'a'}    NaN\n    2    {'b'}     20\n    3    {'c'}     30",
        ],
    );
}

#[test]
fn stack_unstack_and_helpers() {
    check(
        "U = table([1;2], [3;4], [5;6], 'VariableNames', {'id','m1','m2'});\nSt = stack(U, {'m1','m2'})\nUn = unstack(St, 'm1_m2', 'm1_m2_Indicator')\nam = anymissing(table([1;NaN], {'a';''}))\ncelldisp({1, 'two'}, 'q')\n",
        &[
            "St = 4×3 table\n\n    id    m1_m2_Indicator    m1_m2\n    __    _______________    _____\n\n    1           m1             3  \n    1           m2             5  \n    2           m1             4  \n    2           m2             6  ",
            "Un = 2×3 table\n\n    id    m1    m2\n    __    __    __\n\n    1     3     5 \n    2     4     6 ",
            "am = logical\n   1",
            " \nq{1} =\n \n     1\n\n \n \nq{2} =\n \ntwo\n \n",
        ],
    );
}
