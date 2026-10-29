//! Elementary matrices and array manipulation against values recorded from MATLAB R2023b
//! (printed with %.17g).

use livre_lang::{Interpreter, OutputKind, Value};

fn run(src: &str) -> (Interpreter, Vec<String>, Vec<String>) {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    for o in it.take_outputs() {
        match o.kind {
            OutputKind::Error(e) => errors.push(e),
            OutputKind::Warning(w) => warnings.push(w),
            _ => {}
        }
    }
    (it, errors, warnings)
}

fn ok(src: &str) -> Interpreter {
    let (it, errors, _) = run(src);
    assert!(errors.is_empty(), "{src}: {errors:?}");
    it
}

fn error_of(src: &str) -> String {
    let (_, errors, _) = run(src);
    assert_eq!(errors.len(), 1, "{src}: {errors:?}");
    errors[0].clone()
}

/// (real, imaginary) parts of a variable.
fn parts(it: &Interpreter, name: &str) -> Vec<(f64, f64)> {
    match it.get_var(name) {
        Some(Value::Num(m)) => (0..m.numel()).map(|i| (m.data[i], m.im_at(i))).collect(),
        other => panic!("{name} = {other:?}"),
    }
}

fn same(a: f64, b: f64) -> bool {
    (a == b && a.is_sign_negative() == b.is_sign_negative())
        || (a.is_nan() && b.is_nan())
        || (a != 0.0 && b != 0.0 && (a - b).abs() <= 4.0 * f64::EPSILON * a.abs().max(b.abs()))
}

/// Class, size and column-major values of a variable.
fn check(it: &Interpreter, name: &str, class: &str, dims: &[usize], want: &[(f64, f64)]) {
    let v = it.get_var(name).unwrap_or_else(|| panic!("{name} undefined"));
    assert_eq!(v.class_name(), class, "{name} class");
    assert_eq!(v.dims(), dims, "{name} size");
    let got = parts(it, name);
    assert_eq!(got.len(), want.len(), "{name}: {got:?}");
    for (k, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(same(g.0, w.0) && same(g.1, w.1), "{name}({}) = {g:?}, MATLAB {w:?}", k + 1);
    }
}

fn real(v: &[f64]) -> Vec<(f64, f64)> {
    v.iter().map(|&x| (x, 0.0)).collect()
}

fn re(it: &Interpreter, name: &str, dims: &[usize], v: &[f64]) {
    check(it, name, "double", dims, &real(v));
}

#[test]
fn repelem_vectors_and_matrices() {
    let it = ok(concat!(
        "a1 = repelem(5,3); a2 = repelem([1 2 3],2); a3 = repelem([1;2],[2 3]);\n",
        "a4 = repelem([1 2;3 4],2,3); a5 = repelem([1 2;3 4],[1 2],2); a6 = repelem(int8([1 2]),2);\n",
        "a7 = repelem('ab',[2 1]); a8 = repelem({1,'a'},2); a9 = repelem([1 2;3 4],2,1,2);\n",
        "a10 = repelem(zeros(1,0),3); a11 = repelem([1 2 3],[0 1 2]); a12 = repelem(true,2,2);\n",
        "a13 = repelem(1+2i,2); a14 = repelem([1 2;3 4],0,2);\n",
    ));
    re(&it, "a1", &[1, 3], &[5.0, 5.0, 5.0]);
    re(&it, "a2", &[1, 6], &[1.0, 1.0, 2.0, 2.0, 3.0, 3.0]);
    re(&it, "a3", &[5, 1], &[1.0, 1.0, 2.0, 2.0, 2.0]);
    re(&it, "a4", &[4, 6], &[1., 1., 3., 3., 1., 1., 3., 3., 1., 1., 3., 3., 2., 2., 4., 4., 2., 2., 4., 4., 2., 2., 4., 4.]);
    re(&it, "a5", &[3, 4], &[1., 3., 3., 1., 3., 3., 2., 4., 4., 2., 4., 4.]);
    check(&it, "a6", "int8", &[1, 4], &real(&[1., 1., 2., 2.]));
    check(&it, "a7", "char", &[1, 3], &real(&[97., 97., 98.]));
    assert_eq!(it.get_var("a8").unwrap().dims(), vec![1, 4]);
    re(&it, "a9", &[4, 2, 2], &[1., 1., 3., 3., 2., 2., 4., 4., 1., 1., 3., 3., 2., 2., 4., 4.]);
    re(&it, "a10", &[1, 0], &[]);
    re(&it, "a11", &[1, 3], &[2., 3., 3.]);
    check(&it, "a12", "logical", &[2, 2], &real(&[1., 1., 1., 1.]));
    check(&it, "a13", "double", &[1, 2], &[(1., 2.), (1., 2.)]);
    re(&it, "a14", &[0, 4], &[]);
    assert_eq!(error_of("repelem([],3)"), "With two inputs, the first argument must be a vector. Use three-input syntax for matrices.");
    assert_eq!(error_of("repelem([1 2 3],[1 2])"), "In repelem(v, N), N must be a scalar or a vector of length equal to v.");
    assert_eq!(error_of("repelem(7,[2 3])"), "In repelem(v, N), N must be a scalar or a vector of length equal to v.");
    assert_eq!(error_of("repelem([1 2],1.5)"), "Replication factors must be nonnegative integer-valued scalars or vectors.");
    assert_eq!(
        error_of("repelem([1 2;3 4],[1 2 3],1)"),
        "In repelem(A, N1, N2, ..., Nn), Nj must be a scalar or a vector of length size(A, j)."
    );
}

#[test]
fn resize_paddata_trimdata() {
    let it = ok(concat!(
        "b1 = resize([1 2 3],5); b2 = resize([1 2 3]',5); b3 = resize([1 2;3 4],3); b4 = resize([1 2 3],[2 4]);\n",
        "b5 = resize([1 2 3],8,'Pattern','flip'); b6 = resize([1 2 3],8,'Pattern','reflect');\n",
        "b7 = resize([1 2 3],6,'Pattern','edge','Side','both'); b8 = resize([1 2 3 4 5],2,'Side','both');\n",
        "b9 = resize([1 2;3 4],4,'Dimension',2,'FillValue',9); b10 = resize(reshape(1:4,1,1,4),6);\n",
        "b11 = resize([1 2 3],11,'Pattern','flip','Side','both'); b12 = resize(single([1 2]),3,'FillValue',NaN);\n",
        "b13 = resize(5,3); b14 = resize([1 2 3],6,\"Pattern\",\"circular\",\"Side\",\"leading\"); b15 = resize({1,2},3);\n",
        "c1 = paddata([1 2;3 4;5 6],[8 6],'Side','both'); c2 = paddata([1 2 3],2);\n",
        "d1 = trimdata([1 2 3;4 5 6;7 8 9],[2 1],'Side','both'); d2 = trimdata([1 2 3],5); d3 = trimdata(1:5,2,'Side','leading');\n",
    ));
    re(&it, "b1", &[1, 5], &[1., 2., 3., 0., 0.]);
    re(&it, "b2", &[5, 1], &[1., 2., 3., 0., 0.]);
    re(&it, "b3", &[3, 2], &[1., 3., 0., 2., 4., 0.]);
    re(&it, "b4", &[2, 4], &[1., 0., 2., 0., 3., 0., 0., 0.]);
    re(&it, "b5", &[1, 8], &[1., 2., 3., 3., 2., 1., 1., 2.]);
    re(&it, "b6", &[1, 8], &[1., 2., 3., 2., 1., 2., 3., 2.]);
    re(&it, "b7", &[1, 6], &[1., 1., 2., 3., 3., 3.]);
    re(&it, "b8", &[1, 2], &[2., 3.]);
    re(&it, "b9", &[2, 4], &[1., 3., 2., 4., 9., 9., 9., 9.]);
    re(&it, "b10", &[1, 1, 6], &[1., 2., 3., 4., 0., 0.]);
    re(&it, "b11", &[1, 11], &[3., 3., 2., 1., 1., 2., 3., 3., 2., 1., 1.]);
    check(&it, "b12", "single", &[1, 3], &real(&[1., 2., f64::NAN]));
    re(&it, "b13", &[1, 3], &[5., 0., 0.]);
    re(&it, "b14", &[1, 6], &[1., 2., 3., 1., 2., 3.]);
    assert_eq!(it.get_var("b15").unwrap().dims(), vec![1, 3]);
    let mut pad = vec![0.0; 48];
    for (k, v) in [(18, 1.), (19, 3.), (20, 5.), (26, 2.), (27, 4.), (28, 6.)] {
        pad[k] = v;
    }
    re(&it, "c1", &[8, 6], &pad);
    re(&it, "c2", &[1, 3], &[1., 2., 3.]);
    re(&it, "d1", &[2, 1], &[2., 5.]);
    re(&it, "d2", &[1, 3], &[1., 2., 3.]);
    re(&it, "d3", &[1, 2], &[4., 5.]);
    assert_eq!(error_of("resize(1:3, 1.5)"), "Second argument must be a finite nonnegative integer scalar or vector of nonnegative integers.");
    assert_eq!(error_of("resize(1:3, 4, 'Pattern', 'foo')"), "Pattern must be 'constant', 'edge', 'circular', 'flip', or 'reflect'.");
    assert_eq!(error_of("resize(1:3, 4, 'FillValue', [1 2])"), "Fill value must be scalar when the first argument is an array.");
    assert_eq!(error_of("resize(1:3, 4, 'Side', 'x')"), "Side must be 'trailing', 'leading', or 'both'.");
    assert_eq!(error_of("resize(1:3, [4 4], 'Dimension', 1)"), "Second argument must be a scalar or have the same number of elements as 'Dimension'.");
    assert_eq!(error_of("resize(zeros(1,0), 3, 'Pattern', 'edge')"), "When the first argument is empty, the pattern must be constant.");
    assert_eq!(error_of("trimdata([1 2 3],2,'Pattern','edge')"), "Parameter name must be 'Dimension' or 'Side'.");
    assert_eq!(error_of("paddata([1 2 3],5,'Foo',1)"), "Parameter name must be 'Dimension', 'FillValue', 'Pattern', or 'Side'.");
}

#[test]
fn rearrangement() {
    let it = ok(concat!(
        "r1 = rot90([1 2;3 4]); r2 = rot90([1 2 3;4 5 6],2); r3 = rot90([1 2 3;4 5 6],-1); r4 = rot90(reshape(1:8,2,2,2));\n",
        "r5 = rot90('ab'); r6 = rot90([1 2;3 4],5);\n",
        "p1 = ipermute(reshape(1:24,2,3,4),[3 1 2]); p2 = ipermute([1 2 3],[2 1]);\n",
        "[s1,n1] = shiftdim(reshape(1:6,1,1,3,1,2)); s2 = shiftdim(reshape(1:6,1,1,3,1,2),3); s3 = shiftdim([1 2 3],-1);\n",
        "s4 = shiftdim(reshape(1:24,2,3,4),5); [s5,n5] = shiftdim(zeros(1,0)); [s6,n6] = shiftdim(5);\n",
        "f1 = flipdim([1 2;3 4],1); f2 = flipdim([1 2;3 4],2);\n",
        "k1 = blkdiag([1 2;3 4], 5, [6 7]); k2 = blkdiag(int8(100), 1000); k3 = blkdiag(single(1), 2); k4 = blkdiag();\n",
        "k5 = blkdiag([], 3, zeros(0,2)); k6 = blkdiag(1i, 2); k7 = blkdiag(1, 'a');\n",
    ));
    re(&it, "r1", &[2, 2], &[2., 1., 4., 3.]);
    re(&it, "r2", &[2, 3], &[6., 3., 5., 2., 4., 1.]);
    re(&it, "r3", &[3, 2], &[4., 5., 6., 1., 2., 3.]);
    re(&it, "r4", &[2, 2, 2], &[3., 1., 4., 2., 7., 5., 8., 6.]);
    check(&it, "r5", "char", &[2, 1], &real(&[98., 97.]));
    re(&it, "r6", &[2, 2], &[2., 1., 4., 3.]);
    let odd_even: Vec<f64> = (1..=23).step_by(2).chain((2..=24).step_by(2)).map(|x| x as f64).collect();
    re(&it, "p1", &[3, 4, 2], &odd_even);
    re(&it, "p2", &[3, 1], &[1., 2., 3.]);
    re(&it, "s1", &[3, 1, 2], &[1., 2., 3., 4., 5., 6.]);
    re(&it, "n1", &[1, 1], &[2.]);
    re(&it, "s2", &[1, 2, 1, 1, 3], &[1., 4., 2., 5., 3., 6.]);
    re(&it, "s3", &[1, 1, 3], &[1., 2., 3.]);
    re(&it, "s4", &[4, 2, 3], &[1., 7., 13., 19., 2., 8., 14., 20., 3., 9., 15., 21., 4., 10., 16., 22., 5., 11., 17., 23., 6., 12., 18., 24.]);
    re(&it, "s5", &[0, 1], &[]);
    re(&it, "n5", &[1, 1], &[1.]);
    re(&it, "n6", &[1, 1], &[0.]);
    re(&it, "f1", &[2, 2], &[3., 1., 4., 2.]);
    re(&it, "f2", &[2, 2], &[2., 4., 1., 3.]);
    re(&it, "k1", &[4, 5], &[1., 3., 0., 0., 2., 4., 0., 0., 0., 0., 5., 0., 0., 0., 0., 6., 0., 0., 0., 7.]);
    check(&it, "k2", "int8", &[2, 2], &real(&[100., 0., 0., 127.]));
    check(&it, "k3", "single", &[2, 2], &real(&[1., 0., 0., 2.]));
    re(&it, "k4", &[0, 0], &[]);
    re(&it, "k5", &[1, 3], &[3., 0., 0.]);
    check(&it, "k6", "double", &[2, 2], &[(0., 1.), (0., 0.), (0., 0.), (2., 0.)]);
    check(&it, "k7", "char", &[2, 2], &real(&[1., 0., 0., 97.]));
    assert_eq!(error_of("rot90([1 2],1.5)"), "k must be an integer.");
    assert_eq!(error_of("rot90([1 2],[1 2])"), "k must be a scalar.");
    assert_eq!(error_of("ipermute([1 2],[1 1])"), "ORDER must have at least N elements for an N-D array.");
    assert_eq!(error_of("ipermute(1:3,[2 4 1])"), "ORDER contains an invalid permutation index.");
    assert_eq!(error_of("flipdim([1 2;3 4])"), "Requires two arguments.");
    assert_eq!(error_of("flipdim([1 2;3 4],0)"), "DIM must be a positive integer.");
    assert_eq!(error_of("blkdiag(ones(2,2,2))"), "Input arrays must be 2-dimensional.");
}

#[test]
fn index_conversion() {
    let it = ok(concat!(
        "i1 = sub2ind([3 4],[1 2 3],[2 3 4]); i2 = sub2ind([3 4 2],2,3,2); i3 = sub2ind([3 4],[1;2],[2;3]);\n",
        "i4 = sub2ind([3 4 2],2,5); i5 = sub2ind([3 4],2); i6 = sub2ind([3 4],int8(2),3); i7 = sub2ind([3 4],2,3,1,1);\n",
        "i8 = sub2ind([3 4],1.5,1); i9 = sub2ind([3 4],NaN,1); i10 = sub2ind([3 4],[],[]);\n",
        "[r1,c1] = ind2sub([3 4],[1 5 12]); [r2,c2,p2] = ind2sub([3 4 2],[1 13 24]); [r3,c3] = ind2sub([3 4 2],[1 13 24]);\n",
        "q4 = ind2sub([3 4],[1 5 12]); [r5,c5,p5,l5] = ind2sub([3 4],[5;7]); [r6,c6] = ind2sub([3 4],int16([5 7]));\n",
        "[r7,c7] = ind2sub(12,5); [r8,c8] = ind2sub([3 4],[1 2;3 4]);\n",
    ));
    re(&it, "i1", &[1, 3], &[4., 8., 12.]);
    re(&it, "i2", &[1, 1], &[20.]);
    re(&it, "i3", &[2, 1], &[4., 8.]);
    re(&it, "i4", &[1, 1], &[14.]);
    re(&it, "i5", &[1, 1], &[2.]);
    re(&it, "i6", &[1, 1], &[8.]);
    re(&it, "i7", &[1, 1], &[8.]);
    re(&it, "i8", &[1, 1], &[1.5]);
    re(&it, "i9", &[1, 1], &[f64::NAN]);
    re(&it, "i10", &[0, 0], &[]);
    re(&it, "r1", &[1, 3], &[1., 2., 3.]);
    re(&it, "c1", &[1, 3], &[1., 2., 4.]);
    re(&it, "r2", &[1, 3], &[1., 1., 3.]);
    re(&it, "c2", &[1, 3], &[1., 1., 4.]);
    re(&it, "p2", &[1, 3], &[1., 2., 2.]);
    re(&it, "c3", &[1, 3], &[1., 5., 8.]);
    re(&it, "q4", &[1, 3], &[1., 5., 12.]);
    re(&it, "r5", &[2, 1], &[2., 1.]);
    re(&it, "c5", &[2, 1], &[2., 3.]);
    re(&it, "l5", &[2, 1], &[1., 1.]);
    re(&it, "r6", &[1, 2], &[2., 1.]);
    re(&it, "c6", &[1, 2], &[2., 3.]);
    re(&it, "r7", &[1, 1], &[5.]);
    re(&it, "c7", &[1, 1], &[1.]);
    re(&it, "r8", &[2, 2], &[1., 3., 2., 1.]);
    re(&it, "c8", &[2, 2], &[1., 1., 1., 2.]);
    assert_eq!(error_of("sub2ind([3 4],4,1)"), "Out of range subscript.");
    assert_eq!(error_of("sub2ind([3 4],1,1,2)"), "Out of range subscript.");
    assert_eq!(error_of("sub2ind([3 4],13)"), "Out of range subscript.");
    assert_eq!(error_of("sub2ind([3 4],[1 2],[1 2 3])"), "The subscript vectors must all be of the same size.");
    assert_eq!(error_of("sub2ind([3],1,1)"), "Size vector must have at least 2 elements.");
}

#[test]
fn bsxfun_isequaln_freqspace() {
    let (it, errors, _) = run(concat!(
        "x1 = bsxfun(@plus,[1 2 3],[10;20]); x2 = bsxfun(@(a,b) a.*b+1,[1 2],[3;4]); x3 = bsxfun(@eq,[1 2],[1;2]);\n",
        "x4 = bsxfun(@times,int8([1 2]),int8([3;4])); x5 = size(bsxfun(@plus, ones(2,3), ones(2,1,4)));\n",
        "e1 = isequaln([1 NaN],[1 NaN],[1 NaN]); e2 = isequaln(struct('a',NaN),struct('a',NaN)); e3 = isequaln({NaN,'x'},{NaN,'x'});\n",
        "e4 = isequaln(NaN+1i, NaN+1i); e5 = isequaln(int8(1), 1); e6 = isequalwithequalnans([NaN 2],[NaN 2]); e7 = isequaln([1 2],[1 2]');\n",
        "f1 = freqspace(5); f2 = freqspace(5,'whole'); [f3,f4] = freqspace([3 4]); [f5,f6] = freqspace(3,'meshgrid'); f7 = freqspace(0);\n",
        "t1 = maxNumCompThreads; t2 = maxNumCompThreads(2); t3 = maxNumCompThreads('automatic'); t4 = maxNumCompThreads;\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    re(&it, "x1", &[2, 3], &[11., 21., 12., 22., 13., 23.]);
    re(&it, "x2", &[2, 2], &[4., 5., 7., 9.]);
    check(&it, "x3", "logical", &[2, 2], &real(&[1., 0., 0., 1.]));
    check(&it, "x4", "int8", &[2, 2], &real(&[3., 4., 6., 8.]));
    re(&it, "x5", &[1, 3], &[2., 3., 4.]);
    for e in ["e1", "e2", "e3", "e4", "e5", "e6"] {
        check(&it, e, "logical", &[1, 1], &real(&[1.]));
    }
    check(&it, "e7", "logical", &[1, 1], &real(&[0.]));
    re(&it, "f1", &[1, 3], &[0., 0.40000000000000002, 0.80000000000000004]);
    re(&it, "f2", &[1, 5], &[0., 0.40000000000000002, 0.80000000000000004, 1.2000000000000002, 1.6000000000000001]);
    re(&it, "f3", &[1, 4], &[-1., -0.5, 0., 0.5]);
    re(&it, "f4", &[1, 3], &[-0.66666666666666663, 0., 0.66666666666666663]);
    let (a, b) = (-0.66666666666666663, 0.66666666666666663);
    re(&it, "f5", &[3, 3], &[a, a, a, 0., 0., 0., b, b, b]);
    re(&it, "f6", &[3, 3], &[a, 0., b, a, 0., b, a, 0., b]);
    re(&it, "f7", &[1, 0], &[]);
    // Setting returns the previous value; 'automatic' restores the default.
    let t1 = parts(&it, "t1")[0].0;
    assert_eq!(parts(&it, "t2")[0].0, t1);
    assert_eq!(parts(&it, "t3")[0].0, 2.0);
    assert_eq!(parts(&it, "t4")[0].0, t1);
    assert_eq!(error_of("bsxfun(@plus,[1 2 3],[1 2])"), "Non-singleton dimensions of the two input arrays must match each other.");
    assert_eq!(error_of("bsxfun('plus',[1 2 3],[1 2]')"), "First Argument must be a function handle.");
    assert_eq!(
        error_of("bsxfun(@(a,b) sum(a),[1 2 3],[1;2])"),
        "Specified function handle produces invalid output dimensions. The function handle must be a binary elementwise function."
    );
    assert_eq!(error_of("isequaln(1)"), "Not enough input arguments.");
    assert_eq!(error_of("maxNumCompThreads(0)"), "Input to maxNumCompThreads must be a positive numeric integer-valued scalar or 'automatic'.");
}

#[test]
fn specialized_matrices() {
    let (it, errors, warnings) = run(concat!(
        "m1 = compan([2 4 6 8]'); m2 = compan(single([1 2 3])); m3 = compan(5); m4 = compan([0 1 2]); m5 = compan([1i 2 3]); m6 = compan(int8([1 2 3]));\n",
        "h1 = hankel([1 2 3]); h2 = hankel([1 2 3],[4 5 6 7]); h3 = hankel([1+1i 2]); h4 = hankel(int8([1 2]),[2 3]); h5 = hankel([1 2],[]);\n",
        "t1 = toeplitz([1 2 3],[1 5 6 7]); t2 = toeplitz([1 2+1i 3]); t3 = toeplitz([1i 2 3]); t4 = toeplitz([1 2],[1 2i]); t5 = toeplitz(single([1 2]),[1 3]);\n",
        "t6 = toeplitz([1 2 3],[9 5]);\n",
        "v1 = vander([1 2 3]); v2 = vander([1i 2]); v3 = vander(int8([2 3])); v4 = vander([1 2;3 4]);\n",
        "w1 = wilkinson(4); w2 = wilkinson(3,'single'); w3 = wilkinson(1);\n",
        "b1 = hilb(3); b2 = hilb(2,'single'); b3 = invhilb(4); b4 = invhilb(3,'single');\n",
        "g1 = pascal(4); g2 = pascal(4,1); g3 = pascal(4,2); g4 = pascal(3,1,'single');\n",
        "d1 = hadamard(4); d2 = hadamard(4,'int8'); d3 = sum(sum(hadamard(12)*hadamard(12)')); d4 = sum(sum(hadamard(40)*hadamard(40)'));\n",
        "o1 = rosser; o2 = rosser('single');\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    re(&it, "m1", &[3, 3], &[-2., 1., 0., -3., 0., 1., -4., 0., 0.]);
    check(&it, "m2", "single", &[2, 2], &real(&[-2., 1., -3., 0.]));
    re(&it, "m3", &[0, 0], &[]);
    re(&it, "m4", &[2, 2], &[f64::NEG_INFINITY, 1., f64::NEG_INFINITY, 0.]);
    check(&it, "m5", "double", &[2, 2], &[(0., 2.), (1., 0.), (0., 3.), (0., 0.)]);
    check(&it, "m6", "int8", &[2, 2], &real(&[-2., 1., -3., 0.]));
    re(&it, "h1", &[3, 3], &[1., 2., 3., 2., 3., 0., 3., 0., 0.]);
    re(&it, "h2", &[3, 4], &[1., 2., 3., 2., 3., 5., 3., 5., 6., 5., 6., 7.]);
    check(&it, "h3", "double", &[2, 2], &[(1., 1.), (2., 0.), (2., 0.), (0., 0.)]);
    check(&it, "h4", "int8", &[2, 2], &real(&[1., 2., 2., 3.]));
    re(&it, "h5", &[2, 0], &[]);
    re(&it, "t1", &[3, 4], &[1., 2., 3., 5., 1., 2., 6., 5., 1., 7., 6., 5.]);
    check(&it, "t2", "double", &[3, 3], &[(1., 0.), (2., -1.), (3., 0.), (2., 1.), (1., 0.), (2., -1.), (3., 0.), (2., 1.), (1., 0.)]);
    check(&it, "t3", "double", &[3, 3], &[(0., 1.), (2., 0.), (3., 0.), (2., 0.), (0., 1.), (2., 0.), (3., 0.), (2., 0.), (0., 1.)]);
    check(&it, "t4", "double", &[2, 2], &[(1., 0.), (2., 0.), (0., 2.), (1., 0.)]);
    check(&it, "t5", "single", &[2, 2], &real(&[1., 2., 3., 1.]));
    re(&it, "v1", &[3, 3], &[1., 4., 9., 1., 2., 3., 1., 1., 1.]);
    check(&it, "v2", "double", &[2, 2], &[(0., 1.), (2., 0.), (1., 0.), (1., 0.)]);
    check(&it, "v3", "int8", &[2, 2], &real(&[2., 3., 1., 1.]));
    re(&it, "v4", &[4, 4], &[1., 27., 8., 64., 1., 9., 4., 16., 1., 3., 2., 4., 1., 1., 1., 1.]);
    re(&it, "w1", &[4, 4], &[1.5, 1., 0., 0., 1., 0.5, 1., 0., 0., 1., 0.5, 1., 0., 0., 1., 1.5]);
    check(&it, "w2", "single", &[3, 3], &real(&[1., 1., 0., 1., 0., 1., 0., 1., 1.]));
    re(&it, "w3", &[1, 1], &[0.]);
    let (h, t) = (0.33333333333333331, 0.20000000000000001);
    re(&it, "b1", &[3, 3], &[1., 0.5, h, 0.5, h, 0.25, h, 0.25, t]);
    check(&it, "b2", "single", &[2, 2], &real(&[1., 0.5, 0.5, 0.3333333432674408]));
    re(&it, "b3", &[4, 4], &[16., -120., 240., -140., -120., 1200., -2700., 1680., 240., -2700., 6480., -4200., -140., 1680., -4200., 2800.]);
    check(&it, "b4", "single", &[3, 3], &real(&[9., -36., 30., -36., 192., -180., 30., -180., 180.]));
    re(&it, "g1", &[4, 4], &[1., 1., 1., 1., 1., 2., 3., 4., 1., 3., 6., 10., 1., 4., 10., 20.]);
    re(&it, "g2", &[4, 4], &[1., 1., 1., 1., 0., -1., -2., -3., 0., 0., 1., 3., 0., 0., 0., -1.]);
    re(&it, "g3", &[4, 4], &[-1., 3., -3., 1., -1., 2., -1., -0., -1., 1., -0., -0., -1., -0., -0., -0.]);
    check(&it, "g4", "single", &[3, 3], &real(&[1., 1., 1., 0., -1., -2., 0., 0., 1.]));
    re(&it, "d1", &[4, 4], &[1., 1., 1., 1., 1., -1., 1., -1., 1., 1., -1., -1., 1., -1., -1., 1.]);
    check(&it, "d2", "int8", &[4, 4], &real(&[1., 1., 1., 1., 1., -1., 1., -1., 1., 1., -1., -1., 1., -1., -1., 1.]));
    // Hadamard matrices satisfy H*H' = n*I.
    re(&it, "d3", &[1, 1], &[144.]);
    re(&it, "d4", &[1, 1], &[1600.]);
    let rosser = [611., 196., -192., 407., -8., -52., -49., 29.];
    let o1 = parts(&it, "o1");
    assert_eq!(o1.len(), 64);
    assert!(o1.iter().take(8).zip(rosser).all(|(g, w)| g.0 == w));
    assert_eq!(it.get_var("o2").unwrap().class_name(), "single");
    let anti = "Last element of input column does not match first element of input row. \n         Column wins anti-diagonal conflict.";
    let diag = "First element of input column does not match first element of input row. \n         Column wins diagonal conflict.";
    assert_eq!(warnings, vec![anti.to_string(), diag.to_string()]);
    assert_eq!(error_of("compan([1 2;3 4])"), "Input argument must be a vector.");
    assert_eq!(error_of("hadamard(6)"), "n must be an integer and n, n/12 or n/20 must be a power of 2.");
    assert_eq!(error_of("toeplitz([])"), "Index exceeds the number of array elements. Index must not exceed 0.");
    assert_eq!(error_of("toeplitz('ab')"), "Inputs must be numeric.");
    assert_eq!(error_of("hilb(2,'int8')"), "CLASSNAME must be 'double' or 'single'.");
    assert_eq!(error_of("hilb(3,'like',single(1))"), "Too many input arguments.");
    assert_eq!(error_of("pascal(3,3)"), "Second argument must be 0, 1, or 2.");
    assert_eq!(error_of("wilkinson(2.5)"), "Size inputs must be integers.");
    assert_eq!(error_of("magic([3 4])"), "Invalid argument at position 1. Value must be a scalar.");
}

#[test]
fn magic_squares() {
    let it = ok("a = magic(3); b = magic(4); c = magic(6); d = magic(2); e = magic(0); f = magic(int8(3));\n\
                 s = [sum(magic(10)) sum(magic(10),2)' sum(diag(magic(10))) sum(diag(flipud(magic(10))))];\n\
                 t = [sum(magic(12)) sum(magic(14)) sum(magic(97))];");
    re(&it, "a", &[3, 3], &[8., 3., 4., 1., 5., 9., 6., 7., 2.]);
    re(&it, "b", &[4, 4], &[16., 5., 9., 4., 2., 11., 7., 14., 3., 10., 6., 15., 13., 8., 12., 1.]);
    re(&it, "c", &[6, 6], &[
        35., 3., 31., 8., 30., 4., 1., 32., 9., 28., 5., 36., 6., 7., 2., 33., 34., 29., 26., 21., 22., 17., 12., 13., 19., 23., 27., 10., 14., 18.,
        24., 25., 20., 15., 16., 11.,
    ]);
    re(&it, "d", &[2, 2], &[1., 4., 3., 2.]);
    re(&it, "e", &[0, 0], &[]);
    re(&it, "f", &[3, 3], &[8., 3., 4., 1., 5., 9., 6., 7., 2.]);
    re(&it, "s", &[1, 22], &[505.0; 22]);
    let t: Vec<f64> = [870.0; 12].into_iter().chain([1379.0; 14]).chain([456385.0; 97]).collect();
    re(&it, "t", &[1, 123], &t);
}

#[test]
fn gallery_matrices() {
    let (it, errors, _) = run(concat!(
        "a1 = gallery(3); a2 = gallery('cauchy',[1 2],[3 4]); a3 = gallery('chebspec',4); a4 = gallery('chebspec',4,1);\n",
        "a5 = gallery('chebvand',4); a6 = gallery('chow',4,2,3); a7 = gallery('circul',3); a8 = gallery('clement',4,1);\n",
        "a9 = gallery('compar',[1 -2 5;3 4 -1;2 7 1],1); a10 = gallery('compar',[1 -2 5;0 4 -1;0 0 1],1); a11 = gallery('dramadah',5,3);\n",
        "a12 = gallery('fiedler',[1 4 2]); a13 = gallery('forsythe',3); a14 = gallery('frank',4); a15 = gallery('frank',4,1);\n",
        "a16 = gallery('gcdmat',4); a17 = gallery('gearmat',4,2,-3); a18 = gallery('grcar',5,1); a19 = gallery('hanowa',4);\n",
        "a20 = gallery('invhess',[1 2 3],[4 5]); a21 = gallery('invol',4); [a22,d22] = gallery('ipjfact',4); [a23,d23] = gallery('ipjfact',3,1);\n",
        "a24 = gallery('kahan',3); a25 = gallery('kahan',[2 4]); a26 = gallery('kms',3,1i); a27 = gallery('lauchli',3,2); a28 = gallery('lehmer',3);\n",
        "a29 = gallery('leslie',[1 2 3],[0.5 0.25]); a30 = gallery('lesp',4); a31 = gallery('lotkin',3); a32 = gallery('minij',3); a33 = gallery('moler',3,2);\n",
        "a34 = gallery('orthog',3); a35 = gallery('orthog',3,3); a36 = gallery('orthog',3,4); a37 = gallery('orthog',3,6); a38 = gallery('orthog',3,7);\n",
        "a39 = gallery('parter',3); a40 = gallery('pei',3,4); a41 = gallery('prolate',3); a42 = gallery('redheff',6); a43 = gallery('riemann',5);\n",
        "a44 = gallery('ris',3); a45 = gallery('smoke',3); a46 = gallery('toeppen',5); a47 = gallery('tridiag',[1 2],[3 4 5],[6 7]); a48 = gallery('triw',[2 4],3);\n",
        "[a49,b49] = gallery('wilk',3); a50 = gallery('binomial',4); a51 = gallery('sampling',[1 3 4]); a52 = gallery('krylov',[1 2;3 4],[1;-1],3);\n",
        "[v53,b53,s53] = gallery('house',[3;4]); a54 = gallery('neumann',4); a55 = gallery('poisson',2); a56 = gallery('minij',3,'single');\n",
        "a57 = gallery('cauchy',single([1 2]));\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    re(&it, "a1", &[3, 3], &[-149., 537., -27., -50., 180., -9., -154., 546., -25.]);
    re(&it, "a2", &[2, 2], &[0.25, 0.20000000000000001, 0.20000000000000001, 0.16666666666666666]);
    re(&it, "a3", &[4, 4], &[
        3.1666666666666665, 1., -0.33333333333333331, 0.5, -4., -0.33333333333333331, 1., -1.3333333333333333, 1.3333333333333333, -1.,
        0.33333333333333331, 4., -0.5, 0.33333333333333331, -1., -3.1666666666666665,
    ]);
    re(&it, "a4", &[4, 4], &[
        -0.70710678118654768, 1.4142135623730949, -0.70710678118654746, 1.1715728752538099, -1.4142135623730949, -0., 1.4142135623730949, -2.,
        0.70710678118654746, -1.4142135623730949, 0.70710678118654768, 6.8284271247461916, -0.29289321881345248, 0.5, -1.7071067811865479, -5.5,
    ]);
    re(&it, "a5", &[4, 4], &[
        1., 0., -1., -0., 1., 0.33333333333333331, -0.77777777777777779, -0.85185185185185186, 1., 0.66666666666666663, -0.11111111111111116,
        -0.81481481481481488, 1., 1., 1., 1.,
    ]);
    re(&it, "a6", &[4, 4], &[5., 4., 8., 16., 1., 5., 4., 8., 0., 1., 5., 4., 0., 0., 1., 5.]);
    re(&it, "a7", &[3, 3], &[1., 3., 2., 2., 1., 3., 3., 2., 1.]);
    let r3 = 1.7320508075688772;
    re(&it, "a8", &[4, 4], &[0., r3, 0., 0., r3, 0., 2., 0., 0., 2., 0., r3, 0., 0., r3, 0.]);
    re(&it, "a9", &[3, 3], &[1., -3., -7., -5., 4., -7., -5., -3., 1.]);
    re(&it, "a10", &[3, 3], &[1., 0., 0., -5., 4., 0., -5., -1., 1.]);
    re(&it, "a11", &[5, 5], &[1., 0., 1., 0., 1., 1., 1., 0., 1., 0., 0., 1., 1., 0., 1., 0., 0., 1., 1., 0., 0., 0., 0., 1., 1.]);
    re(&it, "a12", &[3, 3], &[0., 3., 1., 3., 0., 2., 1., 2., 0.]);
    re(&it, "a13", &[3, 3], &[0., 0., 1.4901161193847656e-08, 1., 0., 0., 0., 1., 0.]);
    re(&it, "a14", &[4, 4], &[4., 3., 0., 0., 3., 3., 2., 0., 2., 2., 2., 1., 1., 1., 1., 1.]);
    re(&it, "a15", &[4, 4], &[1., 1., 0., 0., 1., 2., 2., 0., 1., 2., 3., 3., 1., 2., 3., 4.]);
    re(&it, "a16", &[4, 4], &[1., 1., 1., 1., 1., 2., 1., 2., 1., 1., 3., 1., 1., 2., 1., 4.]);
    re(&it, "a17", &[4, 4], &[0., 1., 0., 0., 1., 0., 1., -1., 0., 1., 0., 1., 0., 0., 1., 0.]);
    re(&it, "a18", &[5, 5], &[1., -1., 0., 0., 0., 1., 1., -1., 0., 0., 0., 1., 1., -1., 0., 0., 0., 1., 1., -1., 0., 0., 0., 1., 1.]);
    re(&it, "a19", &[4, 4], &[-1., -0., 1., 0., -0., -1., 0., 2., -1., -0., -1., -0., -0., -2., -0., -1.]);
    re(&it, "a20", &[3, 3], &[1., 1., 1., 4., 2., 2., 4., 5., 3.]);
    re(&it, "a21", &[4, 4], &[-4., -120., 240., -140., 0.5, 20., -45., 28., 0.33333333333333331, 15., -36., 23.333333333333332, 0.25, 12., -30., 20.]);
    re(&it, "a22", &[4, 4], &[2., 6., 24., 120., 6., 24., 120., 720., 24., 120., 720., 5040., 120., 720., 5040., 40320.]);
    re(&it, "d22", &[1, 1], &[414720.]);
    re(&it, "d23", &[1, 1], &[-9.6450617283950598e-07]);
    re(&it, "a24", &[3, 3], &[
        1.0000000000006661, 0., 0., -0.36235775447667362, 0.93203908596767038, 0., -0.36235775447667362, -0.33773159027557548, 0.86869685777084471,
    ]);
    re(&it, "a25", &[2, 4], &[
        1.0000000000008882, 0., -0.36235775447667362, 0.93203908596789242, -0.36235775447667362, -0.33773159027557548, -0.36235775447667362,
        -0.33773159027557548,
    ]);
    check(&it, "a26", "double", &[3, 3], &[(1., 0.), (0., -1.), (-1., 0.), (0., 1.), (1., 0.), (0., -1.), (-1., 0.), (0., 1.), (1., 0.)]);
    re(&it, "a27", &[4, 3], &[1., 2., 0., 0., 1., 0., 2., 0., 1., 0., 0., 2.]);
    re(&it, "a28", &[3, 3], &[1., 0.5, 0.33333333333333331, 0.5, 1., 0.66666666666666663, 0.33333333333333331, 0.66666666666666663, 1.]);
    re(&it, "a29", &[3, 3], &[1., 0.5, 0., 2., 0., 0.25, 3., 0., 0.]);
    re(&it, "a30", &[4, 4], &[-5., 0.5, 0., 0., 2., -7., 0.33333333333333331, 0., 0., 3., -9., 0.25, 0., 0., 4., -11.]);
    re(&it, "a31", &[3, 3], &[1., 0.5, 0.33333333333333331, 1., 0.33333333333333331, 0.25, 1., 0.25, 0.20000000000000001]);
    re(&it, "a32", &[3, 3], &[1., 1., 1., 1., 2., 2., 1., 2., 3.]);
    re(&it, "a33", &[3, 3], &[1., 2., 2., 2., 5., 6., 2., 6., 9.]);
    let (h, s) = (0.50000000000000011, 0.70710678118654757);
    re(&it, "a34", &[3, 3], &[h, s, h, s, 0., -s, h, -s, h]);
    let (q, a, b) = (0.57735026918962584, -0.28867513459481292, 0.5);
    check(&it, "a35", "double", &[3, 3], &[(q, 0.), (q, 0.), (q, 0.), (q, 0.), (a, b), (a, -b), (q, 0.), (a, -b), (-0.28867513459481264, 0.50000000000000022)]);
    re(&it, "a36", &[3, 3], &[q, 0.70710678118654746, 0.40824829046386307, q, -0.70710678118654746, 0.40824829046386307, q, 0., -0.81649658092772615]);
    re(&it, "a37", &[3, 3], &[0.78867513459481287, q, 0.21132486540518711, q, -q, -q, 0.21132486540518711, -q, 0.78867513459481287]);
    re(&it, "a38", &[3, 3], &[q, q, q, q, 0.21132486540518713, -0.78867513459481287, q, -0.78867513459481287, 0.21132486540518713]);
    re(&it, "a39", &[3, 3], &[2., 0.66666666666666663, 0.40000000000000002, -2., 2., 0.66666666666666663, -0.66666666666666663, -2., 2.]);
    re(&it, "a40", &[3, 3], &[5., 1., 1., 1., 5., 1., 1., 1., 5.]);
    re(&it, "a41", &[3, 3], &[0.5, 0.31830988618379069, 0., 0.31830988618379069, 0.5, 0.31830988618379069, 0., 0.31830988618379069, 0.5]);
    re(&it, "a42", &[6, 6], &[
        1., 1., 1., 1., 1., 1., 1., 1., 0., 0., 0., 0., 1., 0., 1., 0., 0., 0., 1., 1., 0., 1., 0., 0., 1., 0., 0., 0., 1., 0., 1., 1., 1., 0., 0., 1.,
    ]);
    re(&it, "a43", &[5, 5], &[1., -1., -1., -1., -1., -1., 2., -1., -1., -1., 1., -1., 3., -1., -1., -1., -1., -1., 4., -1., 1., 2., -1., -1., 5.]);
    re(&it, "a44", &[3, 3], &[0.20000000000000001, 0.33333333333333331, 1., 0.33333333333333331, 1., -1., 1., -1., -0.33333333333333331]);
    let w = 0.8660254037844386;
    check(&it, "a45", "double", &[3, 3], &[(-0.5, w), (0., 0.), (1., 0.), (1., 0.), (-0.5, -w), (0., 0.), (0., 0.), (1., 0.), (1., 0.)]);
    re(&it, "a46", &[5, 5], &[0., -10., 1., 0., 0., 10., 0., -10., 1., 0., 1., 10., 0., -10., 1., 0., 1., 10., 0., -10., 0., 0., 1., 10., 0.]);
    re(&it, "a47", &[3, 3], &[3., 1., 0., 6., 4., 2., 0., 7., 5.]);
    re(&it, "a48", &[2, 4], &[1., 0., 3., 1., 3., 3., 3., 3.]);
    re(&it, "a49", &[3, 3], &[1e-10, 0., 0., 0.90000000000000002, 0.90000000000000002, 0., -0.40000000000000002, -0.40000000000000002, 1e-10]);
    re(&it, "b49", &[3, 1], &[0., 0., 1.]);
    re(&it, "a50", &[4, 4], &[1., 1., 1., 1., 3., 1., -1., -3., 3., -1., -1., 3., 1., -1., 1., -1.]);
    re(&it, "a51", &[3, 3], &[2.833333333333333, 1.5, 1.3333333333333333, -0.5, 3.5, 4., -0.33333333333333331, -3., -3.3333333333333335]);
    re(&it, "a52", &[2, 3], &[1., -1., -1., -1., -3., -7.]);
    re(&it, "v53", &[2, 1], &[8., 4.]);
    re(&it, "b53", &[1, 1], &[0.025000000000000001]);
    re(&it, "s53", &[1, 1], &[-5.]);
    re(&it, "a54", &[4, 4], &[4., -2., -2., 0., -2., 4., 0., -2., -2., 0., 4., -2., 0., -2., -2., 4.]);
    re(&it, "a55", &[4, 4], &[4., -1., -1., 0., -1., 4., 0., -1., -1., 0., 4., -1., 0., -1., -1., 4.]);
    check(&it, "a56", "single", &[3, 3], &real(&[1., 1., 1., 1., 2., 2., 1., 2., 3.]));
    check(&it, "a57", "single", &[2, 2], &real(&[0.5, 0.3333333432674408, 0.3333333432674408, 0.25]));
    assert_eq!(error_of("gallery('foo',3)"), "Invalid matrix name.");
    assert_eq!(error_of("gallery(4)"), "Invalid N in GALLERY(N).");
    assert_eq!(error_of("gallery('hanowa',3)"), "N must be even.");
    assert_eq!(error_of("gallery('wilk',6)"), "Sorry, that value of N is not available.");
    assert_eq!(error_of("[a,b] = gallery('wilk',5)"), "GALLERY('WILK',5) supports only one output.");
    assert_eq!(error_of("gallery('minij',3,'int8')"), "Invalid class name.");
}

#[test]
fn end_in_indexing() {
    let it = ok("x = 1:5; a = x(end); b = x(end-1:end); y = magic(4); c = y(end,end); d = y(2:end,end-1); z = {1,2,3}; e = z{end}; x(end+1) = 6;");
    re(&it, "a", &[1, 1], &[5.]);
    re(&it, "b", &[1, 2], &[4., 5.]);
    re(&it, "c", &[1, 1], &[1.]);
    re(&it, "d", &[3, 1], &[10., 6., 15.]);
    re(&it, "e", &[1, 1], &[3.]);
    re(&it, "x", &[1, 6], &[1., 2., 3., 4., 5., 6.]);
}
