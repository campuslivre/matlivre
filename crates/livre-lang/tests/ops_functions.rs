//! `toolbox/matlab/ops` functions against results recorded from MATLAB R2023b (values printed
//! with %.17g, with class and size).

use livre_lang::{Interpreter, OutputKind, Value};

fn run(src: &str) -> (Interpreter, Vec<String>) {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    let errors = it
        .take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Error(e) => Some(e),
            _ => None,
        })
        .collect();
    (it, errors)
}

fn ok(src: &str) -> Interpreter {
    let (it, errors) = run(src);
    assert!(errors.is_empty(), "{src}\n{errors:?}");
    it
}

/// The error message of a statement that must fail.
fn err(src: &str) -> String {
    let (_, errors) = run(src);
    assert_eq!(errors.len(), 1, "{src}: {errors:?}");
    errors[0].clone()
}

fn same(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan()) || (a - b).abs() <= 8.0 * f64::EPSILON * a.abs().max(b.abs())
}

/// Class, size and real values of a numeric variable.
fn check(it: &Interpreter, name: &str, class: &str, dims: &[usize], want: &[f64]) {
    let Some(Value::Num(m)) = it.get_var(name) else { panic!("{name} = {:?}", it.get_var(name)) };
    assert_eq!(m.kind.name(), class, "{name} class");
    assert_eq!(m.dims, dims, "{name} size");
    assert!(m.im.is_none(), "{name} should be real: {m:?}");
    assert_eq!(m.data.len(), want.len(), "{name}: {:?}", m.data);
    for (k, (g, w)) in m.data.iter().zip(want).enumerate() {
        assert!(same(*g, *w), "{name}({}) = {g:?}, MATLAB {w:?}", k + 1);
    }
}

fn check_c(it: &Interpreter, name: &str, dims: &[usize], want: &[(f64, f64)]) {
    let Some(Value::Num(m)) = it.get_var(name) else { panic!("{name}") };
    assert_eq!(m.dims, dims, "{name} size");
    assert!(m.im.is_some(), "{name} should be complex");
    for (k, w) in want.iter().enumerate() {
        let g = (m.data[k], m.im_at(k));
        assert!(same(g.0, w.0) && same(g.1, w.1), "{name}({}) = {g:?}, MATLAB {w:?}", k + 1);
    }
}

/// Elements of a cell array of char vectors or of a string array, with its size.
fn texts(it: &Interpreter, name: &str) -> (String, Vec<usize>, Vec<String>) {
    match it.get_var(name) {
        Some(Value::Cell(c)) => ("cell".into(), c.dims.clone(), c.data.iter().map(|v| v.text().unwrap()).collect()),
        Some(Value::Str(s)) => ("string".into(), s.dims.clone(), s.data.iter().map(|v| v.clone().unwrap_or_else(|| "<missing>".into())).collect()),
        other => panic!("{name} = {other:?}"),
    }
}

fn check_t(it: &Interpreter, name: &str, class: &str, dims: &[usize], want: &[&str]) {
    let (c, d, v) = texts(it, name);
    assert_eq!((c.as_str(), d.as_slice()), (class, dims), "{name}");
    assert_eq!(v, want, "{name}");
}

const NAN: f64 = f64::NAN;

#[test]
fn bitand_bitor_bitxor() {
    let it = ok(concat!(
        "b1 = bitand(12, 10); b2 = bitor([12 5], 10); b3 = bitxor(uint8([255 7]), uint8(15));\n",
        "b4 = bitand(int8(-1), int8(5)); b5 = bitor(int8(-128), int8(1)); b6 = bitxor(int16(-5), int16(3));\n",
        "b7 = bitand(uint8(200), 100); b8 = bitand(100, uint8(200)); b12 = bitand(2^53, 1); b13 = bitand(2^53-1, 2^52+1);\n",
        "b14 = bitand(-1, 3, 'int8'); b15 = bitor(-8, 3, 'int8'); b16 = bitxor(255, 1, 'uint8'); b20 = bitand(true, 1);\n",
        "b25 = bitand([1;2;3], [1 2]); b26 = bitand(intmax('uint64'), uint64(5)); b27 = bitor(int64(-2), int64(1));\n",
        "b33 = bitand([], 1); b34 = bitand(uint8([]), uint8(1)); e19 = bitor(uint8([1 2 3]), uint8([1;2])); e39 = bitxor(int8(-1), -1);\n",
    ));
    check(&it, "b1", "double", &[1, 1], &[8.0]);
    check(&it, "b2", "double", &[1, 2], &[14.0, 15.0]);
    check(&it, "b3", "uint8", &[1, 2], &[240.0, 8.0]);
    check(&it, "b4", "int8", &[1, 1], &[5.0]);
    check(&it, "b5", "int8", &[1, 1], &[-127.0]);
    check(&it, "b6", "int16", &[1, 1], &[-8.0]);
    check(&it, "b7", "uint8", &[1, 1], &[64.0]);
    check(&it, "b8", "uint8", &[1, 1], &[64.0]);
    check(&it, "b12", "double", &[1, 1], &[0.0]);
    check(&it, "b13", "double", &[1, 1], &[4503599627370497.0]);
    check(&it, "b14", "double", &[1, 1], &[3.0]);
    check(&it, "b15", "double", &[1, 1], &[-5.0]);
    check(&it, "b16", "double", &[1, 1], &[254.0]);
    check(&it, "b20", "double", &[1, 1], &[1.0]);
    check(&it, "b25", "double", &[3, 2], &[1.0, 0.0, 1.0, 0.0, 2.0, 2.0]);
    check(&it, "b26", "uint64", &[1, 1], &[5.0]);
    check(&it, "b27", "int64", &[1, 1], &[-1.0]);
    check(&it, "b33", "double", &[0, 0], &[]);
    check(&it, "b34", "uint8", &[0, 0], &[]);
    check(&it, "e19", "uint8", &[2, 3], &[1.0, 3.0, 3.0, 2.0, 3.0, 3.0]);
    check(&it, "e39", "int8", &[1, 1], &[0.0]);

    let mix = "Inputs must be signed or unsigned integers of the same class or scalar doubles.";
    let range = "Double inputs must have integer values in the range of ASSUMEDTYPE.";
    let mixed = "Double inputs must have integer values in the integer range when mixed with integer inputs.";
    assert_eq!(err("bitand(uint8(1), int8(1))"), mix);
    assert_eq!(err("bitand(uint8([1 2]), [3 4])"), mix);
    assert_eq!(err("bitand(int8(5), true)"), mix);
    assert_eq!(err("bitand(-1, 3)"), range);
    assert_eq!(err("bitand(1.5, 3)"), range);
    assert_eq!(err("bitand(NaN, 1)"), range);
    assert_eq!(err("bitand(2^64, 1)"), range);
    assert_eq!(err("bitand(300, 1, 'uint8')"), range);
    assert_eq!(err("bitand(uint8(3), 300)"), mixed);
    assert_eq!(err("bitand(uint8(3), -1)"), mixed);
    assert_eq!(err("bitand('a', 1)"), "Operands to BIT Ops must be numeric.");
    assert_eq!(err("bitand(single(7), 3)"), "Invalid data type. First two arguments must be integer or double.");
    assert_eq!(err("bitand([1 2 3], [1 2])"), "Inputs must have the same size.");
    assert_eq!(err("bitand(5, 3, 'single')"), "ASSUMEDTYPE must be an integer type name.");
    assert_eq!(err("bitand(1+2i, 1)"), "Inputs must be real.");
    assert_eq!(err("bitand(uint8(1), 1, 'int8')"), "Integer inputs cannot be assumed as any other types than their own types.");
    assert_eq!(err("bitand(1, 2, 'uint8', 4)"), "Too many input arguments.");
}

#[test]
fn bitcmp_bitget_bitset_bitshift() {
    let it = ok(concat!(
        "c1 = bitcmp(uint8(10)); c2 = bitcmp(int8(10)); c3 = bitcmp(int8(-128)); c4 = bitcmp(10); c5 = bitcmp(10, 'uint8');\n",
        "c6 = bitcmp(10, 'int8'); c7 = bitcmp(-3, 'int8'); c12 = bitcmp(int16([1 -1 0])); c14 = bitcmp(10, 'int64');\n",
        "g1 = bitget(uint8(10), 1:8); g2 = bitget(10, [1 2 4]); g4 = bitget(int8(-128), 1:8); g5 = bitget(10, 54);\n",
        "g10 = bitget(-1, 8, 'int8'); g14 = bitget(intmax('uint64'), 64); g15 = bitget(int64(-1), 64); e24 = bitget(3, [1 2; 1 2]);\n",
        "s1 = bitset(uint8(10), 1); s2 = bitset(uint8(10), 2, 0); s3 = bitset(10, [1 3 5]); s4 = bitset(int8(0), 8);\n",
        "s5 = bitset(int8(-1), 8, 0); s6 = bitset(10, 54); s9 = bitset(10, 8, 1, 'int8'); s10 = bitset(0, 8, 'int8');\n",
        "s12 = bitset(uint8(10), 1, 5); s13 = bitset(uint8(10), [1 2], [1 0]); s15 = bitset(10, 1, true); s17 = bitset(10, 1, -1);\n",
        "h3 = bitshift(uint8(255), 1); h4 = bitshift(uint8(255), -1); h5 = bitshift(int8(-8), -1); h7 = bitshift(int8(64), 1);\n",
        "h8 = bitshift(int8(127), 1); h10 = bitshift(1, 53); h12 = bitshift(3, 52); h15 = bitshift(100, 1, 'int8');\n",
        "h16 = bitshift(1, [1 2 3]); h19 = bitshift(uint8(1), 100); h20 = bitshift(int8(-1), -100); h21 = bitshift(int8(-128), -7);\n",
        "h23 = bitshift(int64(1), 63); h28 = bitshift(intmin('int64'), -1); h31 = bitshift(uint16(1), int8(3));\n",
        "e25 = bitshift(uint8(3), [1 2; 3 4]); e44 = bitshift(int8(1), uint8(2));\n",
    ));
    check(&it, "c1", "uint8", &[1, 1], &[245.0]);
    check(&it, "c2", "int8", &[1, 1], &[-11.0]);
    check(&it, "c3", "int8", &[1, 1], &[127.0]);
    check(&it, "c4", "double", &[1, 1], &[1.8446744073709552e19]);
    check(&it, "c5", "double", &[1, 1], &[245.0]);
    check(&it, "c6", "double", &[1, 1], &[-11.0]);
    check(&it, "c7", "double", &[1, 1], &[2.0]);
    check(&it, "c12", "int16", &[1, 3], &[-2.0, 0.0, -1.0]);
    check(&it, "c14", "double", &[1, 1], &[-11.0]);
    check(&it, "g1", "uint8", &[1, 8], &[0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
    check(&it, "g2", "double", &[1, 3], &[0.0, 1.0, 1.0]);
    check(&it, "g4", "int8", &[1, 8], &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    check(&it, "g5", "double", &[1, 1], &[0.0]);
    check(&it, "g10", "double", &[1, 1], &[1.0]);
    check(&it, "g14", "uint64", &[1, 1], &[1.0]);
    check(&it, "g15", "int64", &[1, 1], &[1.0]);
    check(&it, "e24", "double", &[2, 2], &[1.0, 1.0, 1.0, 1.0]);
    check(&it, "s1", "uint8", &[1, 1], &[11.0]);
    check(&it, "s2", "uint8", &[1, 1], &[8.0]);
    check(&it, "s3", "double", &[1, 3], &[11.0, 14.0, 26.0]);
    check(&it, "s4", "int8", &[1, 1], &[-128.0]);
    check(&it, "s5", "int8", &[1, 1], &[127.0]);
    check(&it, "s6", "double", &[1, 1], &[9007199254741002.0]);
    check(&it, "s9", "double", &[1, 1], &[-118.0]);
    check(&it, "s10", "double", &[1, 1], &[-128.0]);
    check(&it, "s12", "uint8", &[1, 1], &[11.0]);
    check(&it, "s13", "uint8", &[1, 2], &[11.0, 8.0]);
    check(&it, "s15", "double", &[1, 1], &[11.0]);
    check(&it, "s17", "double", &[1, 1], &[11.0]);
    check(&it, "h3", "uint8", &[1, 1], &[254.0]);
    check(&it, "h4", "uint8", &[1, 1], &[127.0]);
    check(&it, "h5", "int8", &[1, 1], &[-4.0]);
    check(&it, "h7", "int8", &[1, 1], &[-128.0]);
    check(&it, "h8", "int8", &[1, 1], &[-2.0]);
    check(&it, "h10", "double", &[1, 1], &[9007199254740992.0]);
    check(&it, "h12", "double", &[1, 1], &[13510798882111488.0]);
    check(&it, "h15", "double", &[1, 1], &[-56.0]);
    check(&it, "h16", "double", &[1, 3], &[2.0, 4.0, 8.0]);
    check(&it, "h19", "uint8", &[1, 1], &[0.0]);
    check(&it, "h20", "int8", &[1, 1], &[-1.0]);
    check(&it, "h21", "int8", &[1, 1], &[-1.0]);
    check(&it, "h23", "int64", &[1, 1], &[-9.2233720368547758e18]);
    check(&it, "h28", "int64", &[1, 1], &[-4.6116860184273879e18]);
    check(&it, "h31", "uint16", &[1, 1], &[8.0]);
    check(&it, "e25", "uint8", &[2, 2], &[6.0, 24.0, 12.0, 48.0]);
    check(&it, "e44", "int8", &[1, 1], &[4.0]);

    let range = "Double inputs must have integer values in the range of ASSUMEDTYPE.";
    let bit = "The BIT input is out of range for the datatype specified.";
    assert_eq!(err("bitcmp(10, 8)"), "ASSUMEDTYPE must be an integer type name.");
    assert_eq!(err("bitcmp(single(10))"), "Invalid data type. First argument must be integer or double.");
    assert_eq!(err("bitcmp(true)"), "Invalid data type. First argument must be integer or double.");
    assert_eq!(err("bitcmp(300, 'uint8')"), range);
    assert_eq!(err("bitcmp(-1)"), range);
    assert_eq!(err("bitcmp(uint8(1), 'int8')"), "Integer inputs cannot be assumed as any other types than their own types.");
    assert_eq!(err("bitget(uint8(10), 9)"), bit);
    assert_eq!(err("bitget(uint8(10), 0)"), bit);
    assert_eq!(err("bitget(10, 1.5)"), bit);
    assert_eq!(err("bitget(-1, 9, 'int8')"), bit);
    assert_eq!(err("bitset(uint8(10), 9)"), bit);
    assert_eq!(err("bitget(uint8([1 2 3]), [1;2])"), "Inputs must have the same size.");
    assert_eq!(err("bitset(uint8([1 2]), 1, [1 0 1])"), "Inputs must have the same size.");
    assert_eq!(err("bitshift([1;2], [1 2])"), "Inputs must have same size.");
    assert_eq!(err("bitshift(10, 1.5)"), "Inputs must be integers.");
    assert_eq!(err("bitshift(uint8(5), Inf)"), "Inputs must be integers.");
    assert_eq!(err("bitshift(-1, 1)"), range);
    assert_eq!(err("bitset(-1, 1)"), range);
    assert_eq!(err("bitshift(1, 3, 8)"), "ASSUMEDTYPE must be an integer type name.");
    assert_eq!(err("bitshift(single(1), 3)"), "Invalid data type. First and second arguments must be integer or double.");
    assert_eq!(err("bitget(single(1), 1)"), "Invalid data type. First and second arguments must be integer or double.");
    assert_eq!(err("bitshift(true, 3)"), "Inputs must be numeric.");
    assert_eq!(err("bitget('a', 1)"), "Inputs must be numeric.");
    assert_eq!(err("bitshift(uint8(1), 1, 'int8')"), "Integer inputs cannot be assumed as any other types than their own types.");
}

#[test]
fn union_orientation_indices_and_classes() {
    let it = ok(concat!(
        "[u1, u1a, u1b] = union([5 1 3 3], [2 1 7]);\n",
        "[u3, u3a, u3b] = union([5 1 3], [2;1]);\n",
        "[u4, u4a, u4b] = union([5 1 3 1], []);\n",
        "[u7, u7a, u7b] = union(4, [3 1]);\n",
        "[u10, u10a, u10b] = union([4 2; 1 3], [9 1]);\n",
        "[u11, u11a, u11b] = union([5 1 3 3], [2 1 7], 'stable');\n",
        "[u12, u12a, u12b] = union([5 1 NaN 3], [NaN 1]);\n",
        "[u13, u13a] = union([5 1 NaN 3], [NaN 1], 'stable');\n",
        "u14 = union(int8([5 1]), [2 1]); u19 = union([99 97], 'ab'); u20 = union(true, [false true]); u21 = union(true, [2 0]);\n",
        "u22 = union(single([1 2]), [3 1]); u35 = union(zeros(1,0), [2 1]); u37 = union(zeros(1,0), zeros(1,0));\n",
        "u38 = union(zeros(0,3), zeros(0,3)); u48 = union('', 'ab'); u51 = union(int8([1 2]), 2.5); f31 = union(int8([1 2]), [NaN 2]);\n",
        "f32 = union(uint8(250), 300); f33 = union(zeros(1,0), zeros(0,1)); u50 = union([0 -0 1], -0); u45 = union(ones(2,2,2), [1 2]);\n",
        "[u17, u17a, u17b] = union('hello', 'world');\n",
        "[u23, u23a, u23b] = union([1+2i 3 -1], [2i 1]);\n",
        "f29 = union([1 2i], [1 2]); f30 = intersect([1 2+1i 3], [3 1]);\n",
    ));
    check(&it, "u1", "double", &[1, 5], &[1.0, 2.0, 3.0, 5.0, 7.0]);
    check(&it, "u1a", "double", &[3, 1], &[2.0, 3.0, 1.0]);
    check(&it, "u1b", "double", &[2, 1], &[1.0, 3.0]);
    check(&it, "u3", "double", &[4, 1], &[1.0, 2.0, 3.0, 5.0]);
    check(&it, "u3b", "double", &[1, 1], &[1.0]);
    check(&it, "u4", "double", &[3, 1], &[1.0, 3.0, 5.0]);
    check(&it, "u4a", "double", &[3, 1], &[2.0, 3.0, 1.0]);
    check(&it, "u4b", "double", &[0, 1], &[]);
    check(&it, "u7", "double", &[1, 3], &[1.0, 3.0, 4.0]);
    check(&it, "u7b", "double", &[2, 1], &[2.0, 1.0]);
    check(&it, "u10", "double", &[5, 1], &[1.0, 2.0, 3.0, 4.0, 9.0]);
    check(&it, "u10a", "double", &[4, 1], &[2.0, 3.0, 4.0, 1.0]);
    check(&it, "u11", "double", &[1, 5], &[5.0, 1.0, 3.0, 2.0, 7.0]);
    check(&it, "u11a", "double", &[3, 1], &[1.0, 2.0, 3.0]);
    check(&it, "u11b", "double", &[2, 1], &[1.0, 3.0]);
    check(&it, "u12", "double", &[1, 5], &[1.0, 3.0, 5.0, NAN, NAN]);
    check(&it, "u12a", "double", &[4, 1], &[2.0, 4.0, 1.0, 3.0]);
    check(&it, "u12b", "double", &[1, 1], &[1.0]);
    check(&it, "u13", "double", &[1, 5], &[5.0, 1.0, NAN, 3.0, NAN]);
    check(&it, "u13a", "double", &[4, 1], &[1.0, 2.0, 3.0, 4.0]);
    check(&it, "u14", "int8", &[1, 3], &[1.0, 2.0, 5.0]);
    check(&it, "u19", "char", &[1, 3], &[97.0, 98.0, 99.0]);
    check(&it, "u20", "logical", &[1, 2], &[0.0, 1.0]);
    check(&it, "u21", "double", &[1, 3], &[0.0, 1.0, 2.0]);
    check(&it, "u22", "single", &[1, 3], &[1.0, 2.0, 3.0]);
    check(&it, "u35", "double", &[1, 2], &[1.0, 2.0]);
    check(&it, "u37", "double", &[1, 0], &[]);
    check(&it, "u38", "double", &[0, 1], &[]);
    check(&it, "u48", "char", &[2, 1], &[97.0, 98.0]);
    check(&it, "u51", "int8", &[1, 3], &[1.0, 2.0, 3.0]);
    check(&it, "f31", "int8", &[1, 3], &[0.0, 1.0, 2.0]);
    check(&it, "f32", "uint8", &[1, 2], &[250.0, 255.0]);
    check(&it, "f33", "double", &[0, 1], &[]);
    check(&it, "u45", "double", &[2, 1], &[1.0, 2.0]);
    check(&it, "u17", "char", &[1, 7], &"dehlorw".chars().map(|c| c as u32 as f64).collect::<Vec<_>>());
    check(&it, "u17a", "double", &[4, 1], &[2.0, 1.0, 3.0, 5.0]);
    check(&it, "u17b", "double", &[3, 1], &[5.0, 3.0, 1.0]);
    check_c(&it, "u23", &[1, 5], &[(1.0, 0.0), (-1.0, 0.0), (0.0, 2.0), (1.0, 2.0), (3.0, 0.0)]);
    check(&it, "u23a", "double", &[3, 1], &[3.0, 1.0, 2.0]);
    check(&it, "u23b", "double", &[2, 1], &[2.0, 1.0]);
    check_c(&it, "f29", &[1, 3], &[(1.0, 0.0), (2.0, 0.0), (0.0, 2.0)]);
    check(&it, "f30", "double", &[1, 2], &[1.0, 3.0]);
    // union([0 -0 1], -0) keeps the first zero, which is +0 here.
    let Some(Value::Num(m)) = it.get_var("u50") else { panic!() };
    assert!(m.data == [0.0, 1.0] && m.data[0].is_sign_positive());

    assert_eq!(err("union(int8([5 1]), uint8([2 1]))"), "Input A of class int8 and input B of class uint8 must be the same class, unless one is double.");
    assert_eq!(err("union('a', true)"), "Input A of class char and input B of class logical must be the same class, unless one is double.");
    assert_eq!(err("union(int8(1), single(2))"), "Input A of class int8 and input B of class single must be the same class, unless one is double.");
    assert_eq!(err("union([1 2 3], [1 2], 'rows')"), "Inputs A and B must have the same number of columns in the 'rows' case.");
    assert_eq!(err("union([1 2], [3 4], 'foo')"), "Unrecognized flag 'foo'. Valid flags are 'rows', 'stable', 'sorted', 'legacy'.");
    assert_eq!(err("union([1 2], [3 4], 3)"), "Invalid input. Valid flags are 'rows', 'stable', 'sorted', 'legacy'.");
    assert_eq!(err("union([1 2], [3 4], 'stable', 'sorted')"), "You can specify either 'sorted' or 'stable' but not both.");
    assert_eq!(
        err("union([2 1], {'a'})"),
        "Input A of class double and input B of class cell must be cell arrays of character vectors, unless one is a character vector."
    );
    assert_eq!(err("union({'a', 1}, {'a'})"), "Input A of class cell and input B of class cell must be cell arrays of character vectors, unless one is a character vector.");
    assert_eq!(err("union(\"a\", 1)"), "Second argument must be text.");
}

#[test]
fn set_operations_on_text_and_rows() {
    let it = ok(concat!(
        "[u24, u24a, u24b] = union({'b','a','c'}, {'d','a'}); u25 = union({'b';'a'}, {'d','a'});\n",
        "u26 = union({'b','a'}, 'z'); [u27, u27a, u27b] = union('z', {'b','a'}); [u28, u28a, u28b] = union(\"b\", {'c','a'});\n",
        "u29 = union([\"b\" \"a\"], [\"c\";\"a\"]); u30 = union([\"b\" \"a\"], 'ab'); [u34, u34a, u34b] = union({'b','a'}, {'a'}, 'stable');\n",
        "u46 = union({}, {'a'}); u47 = union({}, {}); f9 = union({'a'}, \"b\"); f14 = union(['ab';'cd'], {'x'});\n",
        "f15 = union(\"b\", [\"a\" \"b\"; \"c\" \"d\"]); f17 = union({'b','a'}, {}); f18 = union({'b','a'}, []); f20 = union('', {'b','a'});\n",
        "f21 = union({'a', ''}, {''}); f22 = union([\"b\" missing], [\"a\" missing]); [f44, f44a, f44b] = union({'b';'a'}, {'c';'a'}, 'stable');\n",
        "[u31, u31a, u31b] = union([1 2; 3 4; 1 2], [3 4; 0 5], 'rows'); u32 = union([1 2; 3 4; 1 2], [3 4; 0 5], 'rows', 'stable');\n",
        "u44 = union([3 1 2]', [2 5]', 'rows'); f13 = union(['ab';'cd'], ['cd';'ef'], 'rows'); f16 = union([1 2], [3 4], 'ROWS');\n",
        "f27 = union(1, 2, 'rows'); f28 = union([1 NaN; 1 NaN], [1 NaN], 'rows');\n",
        "[f12, f12a] = union({'b','a'}, {'a'}, 'rows');\n",
    ));
    check_t(&it, "u24", "cell", &[1, 4], &["a", "b", "c", "d"]);
    check(&it, "u24a", "double", &[3, 1], &[2.0, 1.0, 3.0]);
    check(&it, "u24b", "double", &[1, 1], &[1.0]);
    check_t(&it, "u25", "cell", &[3, 1], &["a", "b", "d"]);
    check_t(&it, "u26", "cell", &[1, 3], &["a", "b", "z"]);
    check_t(&it, "u27", "cell", &[1, 3], &["a", "b", "z"]);
    check(&it, "u27b", "double", &[2, 1], &[2.0, 1.0]);
    check_t(&it, "u28", "string", &[1, 3], &["a", "b", "c"]);
    check(&it, "u28a", "double", &[1, 1], &[1.0]);
    check_t(&it, "u29", "string", &[3, 1], &["a", "b", "c"]);
    check_t(&it, "u30", "string", &[1, 3], &["a", "ab", "b"]);
    check_t(&it, "u34", "cell", &[1, 2], &["b", "a"]);
    check(&it, "u34b", "double", &[0, 1], &[]);
    check_t(&it, "u46", "cell", &[1, 1], &["a"]);
    check_t(&it, "u47", "cell", &[0, 1], &[]);
    check_t(&it, "f9", "string", &[1, 2], &["a", "b"]);
    check_t(&it, "f14", "cell", &[3, 1], &["ab", "cd", "x"]);
    check_t(&it, "f15", "string", &[4, 1], &["a", "b", "c", "d"]);
    check_t(&it, "f17", "cell", &[2, 1], &["a", "b"]);
    check_t(&it, "f18", "cell", &[2, 1], &["a", "b"]);
    check_t(&it, "f20", "cell", &[1, 3], &["", "a", "b"]);
    check_t(&it, "f21", "cell", &[1, 2], &["", "a"]);
    check_t(&it, "f22", "string", &[1, 4], &["a", "b", "<missing>", "<missing>"]);
    check_t(&it, "f44", "cell", &[3, 1], &["b", "a", "c"]);
    check(&it, "u31", "double", &[3, 2], &[0.0, 1.0, 3.0, 5.0, 2.0, 4.0]);
    check(&it, "u31a", "double", &[2, 1], &[1.0, 2.0]);
    check(&it, "u31b", "double", &[1, 1], &[2.0]);
    check(&it, "u32", "double", &[3, 2], &[1.0, 3.0, 0.0, 2.0, 4.0, 5.0]);
    check(&it, "u44", "double", &[4, 1], &[1.0, 2.0, 3.0, 5.0]);
    check(&it, "f13", "char", &[3, 2], &"acebdf".chars().map(|c| c as u32 as f64).collect::<Vec<_>>());
    check(&it, "f16", "double", &[2, 2], &[1.0, 3.0, 2.0, 4.0]);
    check(&it, "f27", "double", &[2, 1], &[1.0, 2.0]);
    check(&it, "f28", "double", &[3, 2], &[1.0, 1.0, 1.0, NAN, NAN, NAN]);
    // 'rows' is ignored, with a warning, for cell arrays.
    check_t(&it, "f12", "cell", &[1, 2], &["a", "b"]);
    check(&it, "f12a", "double", &[2, 1], &[2.0, 1.0]);
}

#[test]
fn intersect_setdiff_setxor() {
    let it = ok(concat!(
        "[i1, i1a, i1b] = intersect([5 1 3 3 7], [7 1 9 1]); [i2, i2a, i2b] = intersect([5;1;3], [3 1 7]);\n",
        "i4 = intersect([1 2], [3 4]); i6 = intersect([1 2], [3;4]); i7 = intersect([], [3 4]); [i8, i8a, i8b] = intersect([NaN 1], [NaN 1]);\n",
        "[i9, i9a, i9b] = intersect({'b','a','c'}, {'c','b','q'}); [i11, i11a, i11b] = intersect([1 2; 3 4; 5 6], [5 6; 1 2; 7 8], 'rows');\n",
        "i14 = intersect([4 4], 4); [i15, i15a] = intersect(int16([1 2 3]), [3 2]); [i16, i16a, i16b] = intersect(\"a\", [\"b\" \"a\"]);\n",
        "i17 = intersect('abcab', 'bcd'); [i18, i18a] = intersect([1 2; 3 4], [4 1 9]); i20 = intersect(zeros(0,2), [1 2], 'rows');\n",
        "f2 = intersect(zeros(1,0), 1); f24 = intersect(1:3, 2:4, 'rows'); f34 = intersect(zeros(1,0), zeros(1,0));\n",
        "[d1, d1a] = setdiff([5 1 3 3 7], [1 9]); d2 = setdiff([5;1;3;3;7], [1 9]); [d3, d3a] = setdiff([5 1 3 3 7], [1 9], 'stable');\n",
        "d4 = setdiff([1 2], [1 2]); d5 = setdiff([1;2], [1 2]); d6 = setdiff([], [1 2]); d7 = setdiff([3 1], []); d8 = setdiff([NaN 1 NaN], 1);\n",
        "[d9, d9a] = setdiff({'b','a','c','a'}, {'a'}); d10 = setdiff([1 2; 3 4; 5 6; 1 2], [3 4], 'rows'); [d11, d11a] = setdiff('hello', 'l');\n",
        "[d13, d13a] = setdiff([4 2; 1 3], 2); d14 = setdiff([\"b\" \"a\"; \"c\" \"d\"], \"a\"); f1 = setdiff([3 1], [1;5]);\n",
        "f25 = setdiff('abc', {'a'}); f41 = setdiff(zeros(1,0), 1); f42 = setdiff({}, {'a'});\n",
        "[x1, x1a, x1b] = setxor([5 1 3 3], [2 1 7]); x2 = setxor([5;1;3;3], [2 1 7]); [x3, x3a, x3b] = setxor([5 1 3 3], [2 1 7], 'stable');\n",
        "x4 = setxor([1 2], [1 2]); x5 = setxor({'b','a'}, {'a','z'}); x6 = setxor([1 2; 3 4], [3 4; 5 6], 'rows');\n",
        "[x7, x7a, x7b] = setxor([NaN 1], [NaN 2]); x8 = setxor([], [2 1]); x9 = setxor(int8([1 2]), int8([2 3])); f26 = setxor({'a'}, 'a');\n",
        "f39 = setxor(1:3, [], 'stable');\n",
    ));
    check(&it, "i1", "double", &[1, 2], &[1.0, 7.0]);
    check(&it, "i1a", "double", &[2, 1], &[2.0, 5.0]);
    check(&it, "i1b", "double", &[2, 1], &[2.0, 1.0]);
    check(&it, "i2", "double", &[2, 1], &[1.0, 3.0]);
    check(&it, "i2b", "double", &[2, 1], &[2.0, 1.0]);
    check(&it, "i4", "double", &[1, 0], &[]);
    check(&it, "i6", "double", &[0, 1], &[]);
    check(&it, "i7", "double", &[0, 1], &[]);
    check(&it, "i8", "double", &[1, 1], &[1.0]);
    check(&it, "i8a", "double", &[1, 1], &[2.0]);
    check_t(&it, "i9", "cell", &[1, 2], &["b", "c"]);
    check(&it, "i9a", "double", &[2, 1], &[1.0, 3.0]);
    check(&it, "i9b", "double", &[2, 1], &[2.0, 1.0]);
    check(&it, "i11", "double", &[2, 2], &[1.0, 5.0, 2.0, 6.0]);
    check(&it, "i11a", "double", &[2, 1], &[1.0, 3.0]);
    check(&it, "i11b", "double", &[2, 1], &[2.0, 1.0]);
    check(&it, "i14", "double", &[1, 1], &[4.0]);
    check(&it, "i15", "int16", &[1, 2], &[2.0, 3.0]);
    check(&it, "i15a", "double", &[2, 1], &[2.0, 3.0]);
    check_t(&it, "i16", "string", &[1, 1], &["a"]);
    check(&it, "i16b", "double", &[1, 1], &[2.0]);
    check(&it, "i17", "char", &[1, 2], &[98.0, 99.0]);
    check(&it, "i18", "double", &[2, 1], &[1.0, 4.0]);
    check(&it, "i18a", "double", &[2, 1], &[1.0, 4.0]);
    check(&it, "i20", "double", &[0, 2], &[]);
    check(&it, "f2", "double", &[1, 0], &[]);
    check(&it, "f24", "double", &[0, 3], &[]);
    check(&it, "f34", "double", &[1, 0], &[]);
    check(&it, "d1", "double", &[1, 3], &[3.0, 5.0, 7.0]);
    check(&it, "d1a", "double", &[3, 1], &[3.0, 1.0, 5.0]);
    check(&it, "d2", "double", &[3, 1], &[3.0, 5.0, 7.0]);
    check(&it, "d3", "double", &[1, 3], &[5.0, 3.0, 7.0]);
    check(&it, "d3a", "double", &[3, 1], &[1.0, 3.0, 5.0]);
    check(&it, "d4", "double", &[1, 0], &[]);
    check(&it, "d5", "double", &[0, 1], &[]);
    check(&it, "d6", "double", &[0, 1], &[]);
    check(&it, "d7", "double", &[1, 2], &[1.0, 3.0]);
    check(&it, "d8", "double", &[1, 2], &[NAN, NAN]);
    check_t(&it, "d9", "cell", &[1, 2], &["b", "c"]);
    check(&it, "d9a", "double", &[2, 1], &[1.0, 3.0]);
    check(&it, "d10", "double", &[2, 2], &[1.0, 5.0, 2.0, 6.0]);
    check(&it, "d11", "char", &[1, 3], &[101.0, 104.0, 111.0]);
    check(&it, "d11a", "double", &[3, 1], &[2.0, 1.0, 5.0]);
    check(&it, "d13", "double", &[3, 1], &[1.0, 3.0, 4.0]);
    check(&it, "d13a", "double", &[3, 1], &[2.0, 4.0, 1.0]);
    check_t(&it, "d14", "string", &[3, 1], &["b", "c", "d"]);
    check(&it, "f1", "double", &[1, 1], &[3.0]);
    check_t(&it, "f25", "cell", &[1, 1], &["abc"]);
    check(&it, "f41", "double", &[1, 0], &[]);
    check_t(&it, "f42", "cell", &[0, 1], &[]);
    check(&it, "x1", "double", &[1, 4], &[2.0, 3.0, 5.0, 7.0]);
    check(&it, "x1a", "double", &[2, 1], &[3.0, 1.0]);
    check(&it, "x1b", "double", &[2, 1], &[1.0, 3.0]);
    check(&it, "x2", "double", &[4, 1], &[2.0, 3.0, 5.0, 7.0]);
    check(&it, "x3", "double", &[1, 4], &[5.0, 3.0, 2.0, 7.0]);
    check(&it, "x3a", "double", &[2, 1], &[1.0, 3.0]);
    check(&it, "x4", "double", &[1, 0], &[]);
    check_t(&it, "x5", "cell", &[1, 2], &["b", "z"]);
    check(&it, "x6", "double", &[2, 2], &[1.0, 5.0, 2.0, 6.0]);
    check(&it, "x7", "double", &[1, 4], &[1.0, 2.0, NAN, NAN]);
    check(&it, "x7a", "double", &[2, 1], &[2.0, 1.0]);
    check(&it, "x7b", "double", &[2, 1], &[2.0, 1.0]);
    check(&it, "x8", "double", &[2, 1], &[1.0, 2.0]);
    check(&it, "x9", "int8", &[1, 2], &[1.0, 3.0]);
    check_t(&it, "f26", "cell", &[1, 0], &[]);
    check(&it, "f39", "double", &[3, 1], &[1.0, 2.0, 3.0]);
    assert_eq!(err("intersect([1 2 3], [1 2], 'rows')"), "Inputs A and B must have the same number of columns in the 'rows' case.");
}

#[test]
fn pagemtimes_and_page_transposes() {
    let it = ok(concat!(
        "X = reshape(1:12, 2, 3, 2); Y = reshape(1:18, 3, 3, 2); Z = reshape([1 2 3 4 5 6]+1i*[1 0 -1 2 0 1], 3, 2);\n",
        "p1 = pagemtimes(X, Y); p2 = pagemtimes(X, Y(:,:,1)); p3 = pagemtimes([1 2; 3 4], X); p4 = pagemtimes(X, 'transpose', X, 'none');\n",
        "p5 = pagemtimes(X, 'none', X, 'transpose'); p6 = pagemtimes(Z, 'ctranspose', Z, 'none'); p7 = pagemtimes(Z, 'transpose', Z, 'none');\n",
        "p8 = pagemtimes(reshape(1:8,2,2,1,2), reshape(1:12,2,2,3)); p11 = pagemtimes(single(X), Y); p13 = pagemtimes(X, 2);\n",
        "p15 = pagemtimes(zeros(2,0,3), zeros(0,4));\n",
        "t1 = pagetranspose(X); t2 = pagectranspose(Z); t4 = pagetranspose(reshape(1:24, 2,3,2,2)); t5 = pagetranspose('abc');\n",
        "t6 = pagetranspose({1, 2}); t7 = pagectranspose(int8([1 2; 3 4])); t8 = pagetranspose(true(2,3));\n",
    ));
    check(&it, "p1", "double", &[2, 3, 2], &[22.0, 28.0, 49.0, 64.0, 76.0, 100.0, 301.0, 334.0, 382.0, 424.0, 463.0, 514.0]);
    check(&it, "p2", "double", &[2, 3, 2], &[22.0, 28.0, 49.0, 64.0, 76.0, 100.0, 58.0, 64.0, 139.0, 154.0, 220.0, 244.0]);
    check(&it, "p3", "double", &[2, 3, 2], &[5.0, 11.0, 11.0, 25.0, 17.0, 39.0, 23.0, 53.0, 29.0, 67.0, 35.0, 81.0]);
    check(
        &it,
        "p4",
        "double",
        &[3, 3, 2],
        &[5.0, 11.0, 17.0, 11.0, 25.0, 39.0, 17.0, 39.0, 61.0, 113.0, 143.0, 173.0, 143.0, 181.0, 219.0, 173.0, 219.0, 265.0],
    );
    check(&it, "p5", "double", &[2, 2, 2], &[35.0, 44.0, 44.0, 56.0, 251.0, 278.0, 278.0, 308.0]);
    check_c(&it, "p6", &[2, 2], &[(16.0, 0.0), (33.0, -7.0), (33.0, 7.0), (82.0, 0.0)]);
    check_c(&it, "p7", &[2, 2], &[(12.0, -4.0), (31.0, 3.0), (31.0, 3.0), (72.0, 28.0)]);
    check(
        &it,
        "p8",
        "double",
        &[2, 2, 3, 2],
        &[7.0, 10.0, 15.0, 22.0, 23.0, 34.0, 31.0, 46.0, 39.0, 58.0, 47.0, 70.0, 19.0, 22.0, 43.0, 50.0, 67.0, 78.0, 91.0, 106.0, 115.0, 134.0, 139.0, 162.0],
    );
    check(&it, "p11", "single", &[2, 3, 2], &[22.0, 28.0, 49.0, 64.0, 76.0, 100.0, 301.0, 334.0, 382.0, 424.0, 463.0, 514.0]);
    check(&it, "p13", "double", &[2, 3, 2], &[2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0]);
    check(&it, "p15", "double", &[2, 4, 3], &[0.0; 24]);
    check(&it, "t1", "double", &[3, 2, 2], &[1.0, 3.0, 5.0, 2.0, 4.0, 6.0, 7.0, 9.0, 11.0, 8.0, 10.0, 12.0]);
    check_c(&it, "t2", &[2, 3], &[(1.0, -1.0), (4.0, -2.0), (2.0, 0.0), (5.0, 0.0), (3.0, 1.0), (6.0, -1.0)]);
    let t4: Vec<f64> = [1, 3, 5, 2, 4, 6, 7, 9, 11, 8, 10, 12, 13, 15, 17, 14, 16, 18, 19, 21, 23, 20, 22, 24].iter().map(|&x| x as f64).collect();
    check(&it, "t4", "double", &[3, 2, 2, 2], &t4);
    check(&it, "t5", "char", &[3, 1], &[97.0, 98.0, 99.0]);
    assert_eq!(it.get_var("t6").unwrap().dims(), vec![2, 1]);
    check(&it, "t7", "int8", &[2, 2], &[1.0, 2.0, 3.0, 4.0]);
    check(&it, "t8", "logical", &[3, 2], &[1.0; 6]);

    assert_eq!(
        err("pagemtimes(reshape(1:12, 2, 3, 2), reshape(1:12, 2, 3, 2))"),
        "Incorrect dimensions for matrix multiplication. Check that the number of columns in the first array matches the number of rows in the second array."
    );
    assert_eq!(
        err("pagemtimes(reshape(1:12,2,2,3), reshape(1:8,2,2,2))"),
        "Arrays have incompatible sizes. For every dimension beyond the first two, the dimension sizes for both arrays must either be the same, or one of them must be 1."
    );
    assert_eq!(err("pagemtimes(int8([1 2]), [1;2])"), "Invalid data types. Input arrays must be double or single.");
    assert_eq!(err("pagemtimes(true(2,2), [1 2; 3 4])"), "Invalid data types. Input arrays must be double or single.");
    assert_eq!(err("pagemtimes([1 2], 'foo', [1 2], 'none')"), "Option must be 'transpose', 'ctranspose' or 'none'.");
    assert_eq!(err("pagemtimes([1 2], 'transpose', [1 2])"), "Incorrect number of input arguments. Specify either two or four input arguments.");
}

#[test]
fn pagemldivide_and_pagemrdivide() {
    let it = ok(concat!(
        "A = cat(3, [4 1; 2 3], [2 0; 1 5]); B = cat(3, [1;2], [3;4]);\n",
        "[l2, l2r] = pagemldivide(A, B); l3 = pagemldivide(A, 'transpose', B); l4 = pagemldivide([4 1; 2 3], B); l5 = pagemldivide(A, [1;2]);\n",
        "[l8, l8r] = pagemldivide([1 2; 2 4], [1;2]); l9 = pagemldivide(A + 1i, B); l10 = pagemldivide(A, 'ctranspose', B + 1i);\n",
        "l11 = pagemldivide(single(A), B); [l13, l13r] = pagemldivide(2, B); [z, zr] = pagemldivide(zeros(2), [1;1]);\n",
        "l7 = pagemldivide([1 2; 3 4; 5 6], [1;2;3]); u = pagemldivide([1 2 3; 4 5 6], [1;1]); [k, kr] = pagemldivide([1 2; 2 4; 3 6], [1;1;1]);\n",
        "[r2, r2r] = pagemrdivide(permute(B, [2 1 3]), A); r3 = pagemrdivide(permute(B, [2 1 3]), A, 'transpose'); r4 = pagemrdivide([1 2], A);\n",
        "[~, g1] = pagemldivide([3 -1 2; 0.5 4 1; 2 2 -5], [1;1;1]); [~, g2] = pagemldivide([3 0 0; 1 -4 0; 2 7 0.5], [1;1;1]);\n",
        "[~, g3] = pagemldivide([4 -2 7 1 0 3; 1 8 -3 2 2 1; 0 5 6 -4 1 2; 9 1 1 7 -2 0; 2 -3 4 1 6 5; 1 1 0 3 2 -8], ones(6,1));\n",
        "[~, g4] = pagemldivide([1+2i 3; 4i 5-1i], [1;1]);\n",
    ));
    check(&it, "l2", "double", &[2, 1, 2], &[0.10000000000000001, 0.59999999999999998, 1.5, 0.5]);
    check(&it, "l2r", "double", &[1, 1, 2], &[0.33333333333333331, 0.40000000000000002]);
    check(&it, "l3", "double", &[2, 1, 2], &[-0.099999999999999978, 0.69999999999999996, 1.1000000000000001, 0.80000000000000004]);
    check(&it, "l4", "double", &[2, 1, 2], &[0.10000000000000001, 0.59999999999999998, 0.5, 1.0]);
    check(&it, "l5", "double", &[2, 1, 2], &[0.10000000000000001, 0.59999999999999998, 0.5, 0.29999999999999999]);
    // Singular pages give NaN/Inf without a warning and rcond 0.
    check(&it, "l8", "double", &[2, 1], &[NAN, NAN]);
    check(&it, "l8r", "double", &[1, 1], &[0.0]);
    check(&it, "z", "double", &[2, 1], &[f64::INFINITY, NAN]);
    check(&it, "zr", "double", &[1, 1], &[0.0]);
    check_c(
        &it,
        "l9",
        &[2, 1, 2],
        &[
            (0.051724137931034475, -0.12068965517241381),
            (0.55172413793103448, -0.12068965517241377),
            (1.0588235294117647, -0.73529411764705888),
            (0.41176470588235298, -0.1470588235294118),
        ],
    );
    check_c(
        &it,
        "l10",
        &[2, 1, 2],
        &[
            (-0.099999999999999978, 0.10000000000000001),
            (0.69999999999999996, 0.29999999999999999),
            (1.1000000000000001, 0.40000000000000002),
            (0.80000000000000004, 0.20000000000000001),
        ],
    );
    check(&it, "l11", "single", &[2, 1, 2], &[0.099999994039535522, 0.60000002384185791, 1.5, 0.5]);
    check(&it, "l13", "double", &[2, 1, 2], &[0.5, 1.0, 1.5, 2.0]);
    check(&it, "l13r", "double", &[1, 1, 2], &[1.0, 1.0]);
    check(&it, "r2", "double", &[1, 2, 2], &[-0.099999999999999978, 0.69999999999999996, 1.1000000000000001, 0.80000000000000004]);
    check(&it, "r2r", "double", &[1, 1, 2], &[0.33333333333333331, 0.40000000000000002]);
    check(&it, "r3", "double", &[1, 2, 2], &[0.10000000000000001, 0.59999999999999998, 1.5, 0.5]);
    check(&it, "r4", "double", &[1, 2, 2], &[-0.099999999999999978, 0.69999999999999996, 0.29999999999999999, 0.40000000000000002]);
    check(&it, "g1", "double", &[1, 1], &[0.31529850746268656]);
    check(&it, "g2", "double", &[1, 1], &[0.125]);
    check(&it, "g3", "double", &[1, 1], &[0.02248544041091877]);
    check(&it, "g4", "double", &[1, 1], &[0.10334440163271841]);
    // Rectangular pages: least squares and basic solutions through pivoted QR (MATLAB:
    // [-1.2087427198889535e-15 0.500000000000001], [-0.4999999999999995 0 0.49999999999999967],
    // [0 0.2142857142857143]); compared to rounding.
    let near = |name: &str, want: &[f64]| {
        let Some(Value::Num(m)) = it.get_var(name) else { panic!() };
        assert!(m.data.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-13), "{name} = {:?}", m.data);
    };
    near("l7", &[0.0, 0.5]);
    near("u", &[-0.5, 0.0, 0.5]);
    near("k", &[0.0, 0.21428571428571430]);

    assert_eq!(
        err("pagemldivide(cat(3, [4 1; 2 3], [2 0; 1 5]), ones(3,1))"),
        "Incorrect dimensions for matrix division. Check that the number of rows in the first array matches the number of rows in the second array, unless first input is scalar."
    );
    assert_eq!(
        err("pagemrdivide([1 2 3], [4 1; 2 3])"),
        "Incorrect dimensions for matrix division. Check that the number of columns in the first array matches the number of columns in the second array, unless second input is scalar."
    );
    assert_eq!(err("pagemldivide(int8([4 1; 2 3]), [1;2])"), "Invalid data types. Input arrays must be double or single.");
}

#[test]
fn tensorprod_contractions() {
    let it = ok(concat!(
        "k1 = tensorprod([1 2; 3 4], [5 6; 7 8], 2, 1); k2 = tensorprod([1 2; 3 4], [5 6; 7 8]); k3 = tensorprod([1 2; 3 4], [5 6; 7 8], 'all');\n",
        "k4 = tensorprod(reshape(1:24,2,3,4), reshape(1:24,3,2,4), [2 1], [1 2]); k5 = tensorprod(reshape(1:12,3,4), reshape(1:60,3,4,5), 1);\n",
        "k6 = tensorprod(reshape(1:12,3,4), reshape(1:60,3,4,5), 1, 'NumDimensionsA', 3); k7 = tensorprod([1 2 3], [4 5 6]);\n",
        "k8 = tensorprod([1 2 3], [4 5 6], 2); k10 = tensorprod([1+1i 2], [3 4i], 'all'); k11 = tensorprod(single([1 2]), [3 4], 2);\n",
        "k12 = tensorprod(2, 3); k13 = tensorprod(2, [3 4]); k14 = tensorprod([1 2; 3 4], [5 6; 7 8], [1 2], [2 1]);\n",
        "k17 = tensorprod([1 2; 3 4], [1;2], 2, 1); k19 = tensorprod([1 2], [1 2], 3); k21 = tensorprod([1 2; 3 4], 5, 3);\n",
    ));
    check(&it, "k1", "double", &[2, 2], &[19.0, 43.0, 22.0, 50.0]);
    check(&it, "k2", "double", &[2, 2, 2, 2], &[5.0, 15.0, 10.0, 20.0, 7.0, 21.0, 14.0, 28.0, 6.0, 18.0, 12.0, 24.0, 8.0, 24.0, 16.0, 32.0]);
    check(&it, "k3", "double", &[1, 1], &[70.0]);
    check(&it, "k4", "double", &[4, 4], &[86.0, 212.0, 338.0, 464.0, 212.0, 554.0, 896.0, 1238.0, 338.0, 896.0, 1454.0, 2012.0, 464.0, 1238.0, 2012.0, 2786.0]);
    let k5 = it.get_var("k5").unwrap().clone();
    assert_eq!(k5.dims(), vec![4, 4, 5]);
    let Value::Num(m) = &k5 else { panic!() };
    assert_eq!(&m.data[..6], &[14.0, 32.0, 50.0, 68.0, 32.0, 77.0]);
    assert_eq!(m.data[79], 1949.0);
    assert_eq!(it.get_var("k6").unwrap().dims(), vec![4, 1, 4, 5]);
    check(&it, "k7", "double", &[1, 3, 1, 3], &[4.0, 8.0, 12.0, 5.0, 10.0, 15.0, 6.0, 12.0, 18.0]);
    check(&it, "k8", "double", &[1, 1], &[32.0]);
    check_c(&it, "k10", &[1, 1], &[(3.0, 11.0)]);
    check(&it, "k11", "single", &[1, 1], &[11.0]);
    check(&it, "k12", "double", &[1, 1], &[6.0]);
    check(&it, "k13", "double", &[1, 1, 1, 2], &[6.0, 8.0]);
    check(&it, "k14", "double", &[1, 1], &[69.0]);
    check(&it, "k17", "double", &[2, 1], &[5.0, 11.0]);
    check(&it, "k19", "double", &[1, 2, 1, 2], &[1.0, 2.0, 2.0, 4.0]);
    check(&it, "k21", "double", &[2, 2], &[5.0, 15.0, 10.0, 20.0]);

    let sizes = "Specified dimensions of the input arrays must have the same sizes.";
    let dims = "Dimension argument must be a positive integer scalar or a vector of unique positive integers.";
    assert_eq!(err("tensorprod([1 2 3], [4 5], 2)"), sizes);
    assert_eq!(err("tensorprod([1 2], [1 2 3], 'all')"), sizes);
    assert_eq!(err("tensorprod(reshape(1:8,2,2,2), [1 2], 3, 1)"), sizes);
    assert_eq!(err("tensorprod([1 2; 3 4], [5 6; 7 8], [1 2], [1])"), "Dimension arguments must have the same length.");
    assert_eq!(err("tensorprod(ones(2,3), ones(3,2), [2 2], [1 1])"), dims);
    assert_eq!(err("tensorprod(ones(2,3), ones(3,2), 0, 1)"), dims);
    assert_eq!(err("tensorprod(int8([1 2]), int8([3 4]))"), "Invalid data types. Input arrays must be double or single.");
}

#[test]
fn subsref_subsasgn_and_substruct() {
    let it = ok(concat!(
        "s = struct('a', {10, 20}); s(2).a = [7 8 9]; c = {1, 'two', [3 4 5]}; M = [16 2 3 13; 5 11 10 8; 9 7 6 12; 4 14 15 1];\n",
        "q1 = subsref(M, substruct('()', {2, ':'})); q2 = subsref(M, struct('type', '()', 'subs', {{[1 3], [2 4]}}));\n",
        "q3 = subsref(c, substruct('{}', {3})); q4 = subsref(c, substruct('()', {[1 2]})); q5 = subsref(s, substruct('()', {2}, '.', 'a'));\n",
        "q6 = subsref(s, substruct('()', {2}, '.', 'a', '()', {3})); q7 = subsref(c, substruct('{}', {3}, '()', {2}));\n",
        "q16 = subsref(c, substruct('{}', {[1 2]})); q18 = subsref(s(1), substruct('.', 'a')); q19 = subsref(M, substruct('()', {}));\n",
        "q23 = subsref(1:5, substruct('()', {':'})); [qa, qb] = subsref({1, 2}, substruct('{}', {':'}));\n",
        "S8 = substruct('()', {1, 2}, '.', 'f'); S9 = substruct('x', {1});\n",
        "a1 = subsasgn(M, substruct('()', {1, 1}), 100); a2 = subsasgn([], substruct('()', {3}), 5); a3 = subsasgn(c, substruct('{}', {2}), 'X');\n",
        "a4 = subsasgn(struct('a', 1), substruct('.', 'b'), 2); a5 = subsasgn(s, substruct('()', {2}, '.', 'a', '()', {1}), 99);\n",
        "a6 = subsasgn(c, substruct('{}', {3}, '()', {2}), -1); a7 = subsasgn(M, substruct('()', {':', 1}), []); a8 = subsasgn(1:3, substruct('()', {5}), 9);\n",
        "a10 = subsasgn(int8([1 2]), substruct('()', {1}), 300); a11 = subsasgn({}, substruct('{}', {2}, '.', 'f'), 3);\n",
        "a12 = subsasgn(1:5, substruct('()', {logical([1 0 1 0 0])}), 0);\n",
    ));
    check(&it, "q1", "double", &[1, 4], &[5.0, 11.0, 10.0, 8.0]);
    check(&it, "q2", "double", &[2, 2], &[2.0, 7.0, 13.0, 12.0]);
    check(&it, "q3", "double", &[1, 3], &[3.0, 4.0, 5.0]);
    assert_eq!(it.get_var("q4").unwrap().dims(), vec![1, 2]);
    check(&it, "q5", "double", &[1, 3], &[7.0, 8.0, 9.0]);
    check(&it, "q6", "double", &[1, 1], &[9.0]);
    check(&it, "q7", "double", &[1, 1], &[4.0]);
    check(&it, "q16", "double", &[1, 1], &[1.0]);
    check(&it, "q18", "double", &[1, 1], &[10.0]);
    check(&it, "q19", "double", &[4, 4], &[16.0, 5.0, 9.0, 4.0, 2.0, 11.0, 7.0, 14.0, 3.0, 10.0, 6.0, 15.0, 13.0, 8.0, 12.0, 1.0]);
    check(&it, "q23", "double", &[5, 1], &[1.0, 2.0, 3.0, 4.0, 5.0]);
    check(&it, "qa", "double", &[1, 1], &[1.0]);
    check(&it, "qb", "double", &[1, 1], &[2.0]);
    let Some(Value::Struct(s8)) = it.get_var("S8") else { panic!() };
    assert_eq!((s8.dims.clone(), s8.fields.clone()), (vec![1, 2], vec!["type".to_string(), "subs".to_string()]));
    assert_eq!(s8.get_at(0, "type").unwrap().text().unwrap(), "()");
    assert_eq!(s8.get_at(1, "subs").unwrap().text().unwrap(), "f");
    assert_eq!(it.get_var("S9").unwrap().dims(), vec![1, 1]);
    check(&it, "a1", "double", &[4, 4], &[100.0, 5.0, 9.0, 4.0, 2.0, 11.0, 7.0, 14.0, 3.0, 10.0, 6.0, 15.0, 13.0, 8.0, 12.0, 1.0]);
    check(&it, "a2", "double", &[1, 3], &[0.0, 0.0, 5.0]);
    let Some(Value::Cell(a3)) = it.get_var("a3") else { panic!() };
    assert_eq!((a3.dims.clone(), a3.data[1].text().unwrap()), (vec![1, 3], "X".to_string()));
    let Some(Value::Struct(a4)) = it.get_var("a4") else { panic!() };
    assert_eq!(a4.fields, vec!["a".to_string(), "b".to_string()]);
    let Some(Value::Struct(a5)) = it.get_var("a5") else { panic!() };
    assert_eq!(a5.get_at(1, "a"), Some(&Value::Num(livre_lang::value::Matrix::row(vec![99.0, 8.0, 9.0]))));
    let Some(Value::Cell(a6)) = it.get_var("a6") else { panic!() };
    assert_eq!(a6.data[2], Value::Num(livre_lang::value::Matrix::row(vec![3.0, -1.0, 5.0])));
    check(&it, "a7", "double", &[4, 3], &[2.0, 11.0, 7.0, 14.0, 3.0, 10.0, 6.0, 15.0, 13.0, 8.0, 12.0, 1.0]);
    check(&it, "a8", "double", &[1, 5], &[1.0, 2.0, 3.0, 0.0, 9.0]);
    check(&it, "a10", "int8", &[1, 2], &[127.0, 2.0]);
    let Some(Value::Cell(a11)) = it.get_var("a11") else { panic!() };
    assert_eq!(a11.dims, vec![1, 2]);
    check(&it, "a12", "double", &[1, 5], &[0.0, 2.0, 0.0, 4.0, 5.0]);
    // The temporaries used to evaluate the subscripts do not leak into the workspace.
    assert!(it.workspace().iter().all(|v| !v.name.starts_with('#')));

    assert_eq!(err("substruct('()')"), "SUBSTRUCT takes at least two arguments.");
    assert_eq!(err("substruct('()', {1}, '.')"), "SUBSTRUCT takes an even number of arguments.");
    assert_eq!(err("subsref(1, struct('type', '()', 'subs', 1))"), "SUBS field must be a cell array for () TYPE.");
    assert_eq!(err("subsref({1}, struct('type', '{}', 'subs', 1))"), "SUBS field must be a cell array for {} TYPE.");
    assert_eq!(
        err("subsref(1, struct('type', '[]', 'subs', {{1}}))"),
        "The \"type\" field in the indexing structure argument to SUBSREF and SUBSASGN must be a string scalar or character vector of \".\" or \"{}\" or \"()\"."
    );
    assert_eq!(err("subsref(1, struct('type', {}, 'subs', {}))"), "Subscript argument to SUBSREF and SUBSASGN must not be empty.");
    assert_eq!(
        err("subsref(1, struct('typ', '()', 'subs', {{1}}))"),
        "Subscript argument to SUBSREF and SUBSASGN must have two fields whose names are \"type\" and \"subs\"."
    );
    assert_eq!(err("subsref(1, struct('type', '.', 'subs', 'a', 'extra', 1))"), "Subscript argument to SUBSREF and SUBSASGN must have two fields.");
    assert_eq!(err("subsref(1, 2)"), "Subscript argument to SUBSREF and SUBSASGN must be a structure.");
    assert_eq!(err("subsref(1:5, struct('type', {'()', '()'}, 'subs', {{2}, {1}}))"), "Only a dot field name can follow ()'s.");
    assert_eq!(err("subsref(struct('a', 1), substruct('.', 1))"), "Argument to dynamic structure reference must evaluate to a valid field name.");
    assert_eq!(err("subsref(struct('a', 1), substruct('.', 'b'))"), "Unrecognized field name \"b\".");
    assert_eq!(err("x = subsref(1:4, substruct('()', {20}));"), "Index exceeds the number of array elements. Index must not exceed 4.");
    assert_eq!(err("subsref(1)"), "Not enough input arguments.");
    assert_eq!(err("subsasgn(1, substruct('()', {1}), 2)"), "SUBSASGN must be called with an output.");
    assert_eq!(err("x = subsasgn(1, substruct('()', {1}));"), "Not enough input arguments.");
}

#[test]
fn subsindex_numargumentsfromsubscript_and_metaclass() {
    assert_eq!(err("subsindex(3)"), "Function 'subsindex' is not defined for values of class 'double'.");
    assert_eq!(err("subsindex(int8(3))"), "Function 'subsindex' is not defined for values of class 'int8'.");
    assert_eq!(err("subsindex({})"), "Unable to use a value of type cell as an index.");
    assert_eq!(err("subsindex()"), "Not enough input arguments.");
    assert_eq!(err("subsindex(1, 2)"), "Too many input arguments.");
    assert_eq!(
        err("numArgumentsFromSubscript({1}, substruct('{}', {':'}), 0)"),
        "The third argument must be a scalar of class matlab.indexing.IndexingContext."
    );
    assert_eq!(err("numArgumentsFromSubscript({1}, substruct('{}', {1}))"), "Not enough input arguments.");
    assert_eq!(err("metaclass()"), "Not enough input arguments.");
    assert_eq!(err("metaclass(1, 2)"), "Too many input arguments.");

    let it = ok(concat!(
        "m1 = metaclass(1); c1 = class(m1); n1 = m1.Name; n2 = metaclass(int8(1)).Name; n3 = metaclass({1}).Name;\n",
        "n4 = metaclass(struct()).Name; n5 = metaclass(\"s\").Name; n6 = metaclass(@sin).Name; p = properties(m1);\n",
        "h = m1.HandleCompatible; e = size(m1.PropertyList);\n",
    ));
    let text = |n: &str| it.get_var(n).unwrap().text().unwrap();
    assert_eq!(text("c1"), "meta.class");
    assert_eq!(
        [text("n1"), text("n2"), text("n3"), text("n4"), text("n5"), text("n6")],
        ["double", "int8", "cell", "struct", "string", "function_handle"].map(String::from)
    );
    let (_, d, names) = texts(&it, "p");
    assert_eq!(d.iter().product::<usize>(), 18);
    assert_eq!(names[0], "Name");
    assert_eq!(names[17], "SuperclassList");
    check(&it, "h", "logical", &[1, 1], &[0.0]);
    check(&it, "e", "double", &[1, 2], &[0.0, 1.0]);
}
