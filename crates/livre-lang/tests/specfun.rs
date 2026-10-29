//! Special functions (specfun) against values recorded from MATLAB R2023b (printed with %.17g).
//!
//! Each tolerance is relative to the element's magnitude, plus an absolute 1e-15. Most cases
//! hold to 4 ulps; the looser ones are where MATLAB's algorithm differs from ours: Amos'
//! Bessel and Airy routines (the `complex-bessel` port differs from MATLAB's build by up to
//! ~1e-13), MATLAB's own psi/expint approximations, betainc's normal approximation for large
//! parameters, the summed series of gammainc for huge a, legendre's long recursions, and
//! single-precision results (MATLAB computes them in single; we round the double result).
#![allow(clippy::approx_constant)]

use livre_lang::{Interpreter, OutputKind, Value};

const INF: f64 = f64::INFINITY;
const NAN: f64 = f64::NAN;

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

/// The error message a statement raises.
fn error_of(src: &str) -> String {
    let (_, errors) = run(src);
    errors.into_iter().next().unwrap_or_else(|| panic!("no error from {src}"))
}

fn value<'a>(it: &'a Interpreter, name: &str) -> &'a Value {
    it.get_var(name).unwrap_or_else(|| panic!("{name} undefined"))
}

fn check_class(it: &Interpreter, name: &str, class: &str) {
    assert_eq!(value(it, name).class_name(), class, "class of {name}");
}

fn check_dims(it: &Interpreter, name: &str, dims: &[usize]) {
    assert_eq!(value(it, name).dims(), dims, "size of {name}");
}

fn check(it: &Interpreter, name: &str, want: &[(f64, f64)], tol: f64) {
    let Value::Num(m) = value(it, name) else { panic!("{name} is not numeric") };
    assert_eq!(m.numel(), want.len(), "{name}");
    for (k, &(wr, wi)) in want.iter().enumerate() {
        let (gr, gi) = (m.data[k], m.im_at(k));
        let modulus = wr.hypot(wi).max(gr.hypot(gi));
        let close = |g: f64, w: f64| g == w || (g.is_nan() && w.is_nan()) || (g - w).abs() <= tol * modulus + 1e-15;
        assert!(close(gr, wr) && close(gi, wi), "{name}({}) = ({gr:e}, {gi:e}), MATLAB ({wr:e}, {wi:e})", k + 1);
    }
}

fn real(v: &[f64]) -> Vec<(f64, f64)> {
    v.iter().map(|&x| (x, 0.0)).collect()
}

/// A char matrix, row by row.
fn check_text(it: &Interpreter, name: &str, rows: &[&str]) {
    let Value::Num(m) = value(it, name) else { panic!("{name} is not char") };
    assert_eq!(m.rows(), rows.len(), "{name}");
    for (r, want) in rows.iter().enumerate() {
        let got: String = (0..m.cols()).map(|c| char::from_u32(m.data[c * m.rows() + r] as u32).unwrap()).collect();
        assert_eq!(got, *want, "{name} row {}", r + 1);
    }
}

#[test]
fn airy_matches_matlab() {
    let (it, errors) = run(concat!(
        "v1=airy([0 1 -1 2.5 -5 10 -10 Inf -Inf NaN]);\n",
        "v2=airy(1,[0 1 -1 2.5 -5 10]);\n",
        "v3=airy(2,[0 1 -1 2.5 -5 10 100]);\n",
        "v4=airy(3,[0 1 -1 2.5 -5]);\n",
        "v5=airy(0,[1+2i -3-1i 0.5i]);\n",
        "v6=airy(0,[1 10 1+2i],1);\n",
        "v7=airy(2,[1 -10 1+2i],1);\n",
        "v9=airy(single([0 1 -1]));\n",
        "v11=airy(0,[-100 100 1e4 -1e3]);\n",
        "v12=airy(1,[-100 30]);\n",
        "v13=airy(3,[100 -30]);\n",
        "v14=airy(1,1+1i);\n",
        "v15=airy(2,1+1i,1);\n",
        "v16=airy(3,-2+3i,1);\n",
        "v17=airy(1,-2+3i,1);\n",
        "v22=airy(0,[1;2]);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[1, 10]);
    check(&it, "v1", &real(&[0.3550280538878172, 0.13529241631288147, 0.5355608832923522, 0.01572592338047049, 0.35076100902411383, 1.1047532552898694e-10, 0.040241238486438895, 0.0, NAN, NAN]), 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[1, 6]);
    check(&it, "v2", &real(&[-0.2588194037928068, -0.15914744129679323, -0.010160567116645147, -0.026250881035903235, 0.327192818554432, -3.520633676738926e-10]), 1e-13);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[1, 7]);
    check(&it, "v3", &real(&[0.6149266274460007, 1.2074235949528713, 0.10399738949694459, 6.4816607384605645, -0.13836913490160407, 455641153.54822814, 6.041223996669973e+288]), 1e-13);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[1, 5]);
    check(&it, "v4", &real(&[0.4482883573538264, 0.9324359333927754, 0.5923756264227923, 9.42142331733428, 0.778411773001887]), 1e-13);
    check_class(&it, "v5", "double");
    check_dims(&it, "v5", &[1, 3]);
    check(&it, "v5", &[(-0.21938625498142753, -0.17538591140810939), (-1.0661276538021986, -0.6039936031973206), (0.3536492233751019, -0.1368020542285243)], 8.881784197001252e-16);
    check_class(&it, "v6", "double");
    check_dims(&it, "v6", &[1, 3]);
    check(&it, "v6", &[(0.26351364474914013, 0.0), (0.15812366685434615, 0.0), (0.22293560034630835, -0.05622877592061669)], 8.881784197001252e-16);
    check_class(&it, "v7", "double");
    check_dims(&it, "v7", &[1, 3]);
    check(&it, "v7", &[(0.6199119435726785, 0.0), (-0.3146798296438335, 0.0), (0.03996455588151768, 0.10909497762692791)], 8.881784197001252e-16);
    assert_eq!(error_of("[w,ierr]=airy(0,[1 200]); r=[w ierr];"), "Too many output arguments.");
    check_class(&it, "v9", "single");
    check_dims(&it, "v9", &[1, 3]);
    check(&it, "v9", &real(&[0.35502806305885315, 0.1352924108505249, 0.5355609059333801]), 1.1920928955078125e-07);
    assert_eq!(error_of("r=airy(4,1);"), "K must be 0, 1, 2, or 3.");
    check_class(&it, "v11", "double");
    check_dims(&it, "v11", &[1, 4]);
    check(&it, "v11", &real(&[0.17675339323955194, 2.6344821520883423e-291, 0.0, 0.05597189577318026]), 8.881784197001252e-16);
    check_class(&it, "v12", "double");
    check_dims(&it, "v12", &[1, 2]);
    check(&it, "v12", &real(&[-0.24229703166065097, -1.759876581432743e-48]), 8.881784197001252e-16);
    check_class(&it, "v13", "double");
    check_dims(&it, "v13", &[1, 2]);
    check(&it, "v13", &real(&[6.039712745310376e+289, -0.48369472582766976]), 8.881784197001252e-16);
    check_class(&it, "v14", "double");
    check_dims(&it, "v14", &[1, 1]);
    check(&it, "v14", &[(-0.13062795349964748, 0.16306759644932392)], 8.881784197001252e-16);
    check_class(&it, "v15", "double");
    check_dims(&it, "v15", &[1, 1]);
    check(&it, "v15", &[(0.4666296985864471, 0.4036217039617708)], 8.881784197001252e-16);
    check_class(&it, "v16", "double");
    check_dims(&it, "v16", &[1, 1]);
    check(&it, "v16", &[(0.3157390678678005, -0.21047242544460892)], 1e-13);
    check_class(&it, "v17", "double");
    check_dims(&it, "v17", &[1, 1]);
    check(&it, "v17", &[(-0.3249286827063226, -0.19585309475614537)], 1e-13);
    assert_eq!(error_of("r=airy(0,1,2);"), "SCALE must be 0 or 1.");
    assert_eq!(error_of("r=airy([0 1],1);"), "K must be 0, 1, 2, or 3.");
    assert_eq!(error_of("r=airy(0,int8(1));"), "Undefined function 'airy' for input arguments of type 'int8'.");
    assert_eq!(error_of("r=airy(1.5,1);"), "K must be 0, 1, 2, or 3.");
    check_class(&it, "v22", "double");
    check_dims(&it, "v22", &[2, 1]);
    check(&it, "v22", &real(&[0.13529241631288147, 0.034924130423274115]), 8.881784197001252e-16);
}

#[test]
fn besselj_matches_matlab() {
    let (it, errors) = run(concat!(
        "v1=besselj(0,[0 1 2.5 10 100 -1 -2.5 Inf NaN]);\n",
        "v2=besselj(1,[0 1 -1 1e-5 50]);\n",
        "v3=besselj(2.5,[0 1 3 -1]);\n",
        "v4=besselj(-0.5,[0 1 3 -1]);\n",
        "v5=besselj(-3,[0 1 3 -1]);\n",
        "v7=besselj([0 1 2],[1 2 3]);\n",
        "v8=besselj(0,[1+1i -2+0.5i 3i]);\n",
        "v9=besselj(1.5,1+1i,1);\n",
        "v10=besselj(0,10i,1);\n",
        "v12=besselj(0,single(1));\n",
        "v13=besselj(single(0),[1 2]);\n",
        "v15=besselj(1e5,1);\n",
        "v16=besselj(0,1e20);\n",
        "v17=besselj(0,[1;2]);\n",
        "v19=besselj([0 1;2 3],1);\n",
        "v20=besselj(0,[]);\n",
        "v21=besselj(0.5,-2+0i);\n",
        "v24=besselj(-1.5,[0 2 -2 1+1i]);\n",
        "v25=besselj([0.3 7.5 30],[0.7 12 25]);\n",
        "v26=besselj(0,1+1i,1);\n",
        "v27=besselj(0,1+1i,0);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[1, 9]);
    check(&it, "v1", &real(&[1.0, 0.7651976865579666, -0.04838377646819794, -0.2459357644513483, 0.01998585030422312, 0.7651976865579666, -0.04838377646819794, NAN, NAN]), 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[1, 5]);
    check(&it, "v2", &real(&[0.0, 0.44005058574493355, -0.44005058574493355, 4.999999999937499e-06, -0.09751182812517514]), 8.881784197001252e-16);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[1, 4]);
    check(&it, "v3", &[(0.0, 0.0), (0.04949681022847795, 0.0), (0.41271003220971614, 0.0), (6.938893903907228e-18, 0.04949681022847795)], 8.881784197001252e-16);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[1, 4]);
    check(&it, "v4", &[(INF, 0.0), (0.4310988680183762, 0.0), (-0.4560488207946334, 0.0), (0.0, -0.43109886801837627)], 8.881784197001252e-16);
    check_class(&it, "v5", "double");
    check_dims(&it, "v5", &[1, 4]);
    check(&it, "v5", &real(&[-0.0, -0.019563353982668414, -0.3090627222552516, 0.019563353982668414]), 8.881784197001252e-16);
    assert_eq!(error_of("r=besselj([0 1 2],[1;2]);"), "NU and Z must be the same size or one must be a scalar.");
    check_class(&it, "v7", "double");
    check_dims(&it, "v7", &[1, 3]);
    check(&it, "v7", &real(&[0.7651976865579666, 0.5767248077568736, 0.4860912605858912]), 8.881784197001252e-16);
    check_class(&it, "v8", "double");
    check_dims(&it, "v8", &[1, 3]);
    check(&it, "v8", &[(0.9376084768060292, -0.4965299476091221), (0.21560017471888612, 0.29678606450319617), (4.8807925858650245, 0.0)], 8.881784197001252e-16);
    check_class(&it, "v9", "double");
    check_dims(&it, "v9", &[1, 1]);
    check(&it, "v9", &[(0.09239597725217388, 0.1372932857054782)], 8.881784197001252e-16);
    check_class(&it, "v10", "double");
    check_dims(&it, "v10", &[1, 1]);
    check(&it, "v10", &real(&[0.12783333716342862]), 8.881784197001252e-16);
    assert_eq!(error_of("r=besselj(1i,1);"), "NU must be real.");
    check_class(&it, "v12", "single");
    check_dims(&it, "v12", &[1, 1]);
    check(&it, "v12", &real(&[0.7651976943016052]), 1.1920928955078125e-07);
    check_class(&it, "v13", "single");
    check_dims(&it, "v13", &[1, 2]);
    check(&it, "v13", &real(&[0.7651976943016052, 0.2238907814025879]), 1.1920928955078125e-07);
    assert_eq!(error_of("[j,ierr]=besselj(0,[1e6 1]); r=[j ierr];"), "Too many output arguments.");
    check_class(&it, "v15", "double");
    check_dims(&it, "v15", &[1, 1]);
    check(&it, "v15", &real(&[0.0]), 8.881784197001252e-16);
    check_class(&it, "v16", "double");
    check_dims(&it, "v16", &[1, 1]);
    check(&it, "v16", &real(&[6.698009040703424e-12]), 8.881784197001252e-16);
    check_class(&it, "v17", "double");
    check_dims(&it, "v17", &[2, 1]);
    check(&it, "v17", &real(&[0.7651976865579666, 0.22389077914123562]), 8.881784197001252e-16);
    assert_eq!(error_of("r=besselj([0;1],[1 2]);"), "NU and Z must be the same size or one must be a scalar.");
    check_class(&it, "v19", "double");
    check_dims(&it, "v19", &[2, 2]);
    check(&it, "v19", &real(&[0.7651976865579666, 0.1149034849319005, 0.44005058574493355, 0.019563353982668414]), 8.881784197001252e-16);
    check_class(&it, "v20", "double");
    check_dims(&it, "v20", &[0, 0]);
    check(&it, "v20", &real(&[]), 8.881784197001252e-16);
    check_class(&it, "v21", "double");
    check_dims(&it, "v21", &[1, 1]);
    check(&it, "v21", &[(5.551115123125783e-17, 0.5130161365618278)], 8.881784197001252e-16);
    assert_eq!(error_of("r=besselj(0,int8(1));"), "Undefined function 'besselj' for input arguments of type 'int8'.");
    assert_eq!(error_of("r=besselj(0,1,2);"), "SCALE must be 0 or 1.");
    check_class(&it, "v24", "double");
    check_dims(&it, "v24", &[1, 4]);
    check(&it, "v24", &[(INF, 0.0), (-0.3956232813587036, 0.0), (-1.1102230246251565e-16, -0.39562328135870367), (-0.6858235111523032, 0.48476780408845255)], 8.881784197001252e-16);
    check_class(&it, "v25", "double");
    check_dims(&it, "v25", &[1, 3]);
    check(&it, "v25", &real(&[0.7385918206202189, -0.06865311679777128, 0.011809026124268956]), 8.881784197001252e-16);
    check_class(&it, "v26", "double");
    check_dims(&it, "v26", &[1, 1]);
    check(&it, "v26", &[(0.3449268824850093, -0.18266315965132937)], 8.881784197001252e-16);
    check_class(&it, "v27", "double");
    check_dims(&it, "v27", &[1, 1]);
    check(&it, "v27", &[(0.9376084768060292, -0.4965299476091221)], 8.881784197001252e-16);
    assert_eq!(error_of("r=besselj(0,1+1i,true);"), "Inputs must be single or double.");
}

#[test]
fn bessely_matches_matlab() {
    let (it, errors) = run(concat!(
        "v1=bessely(0,[0 1 2.5 10 -1 Inf NaN]);\n",
        "v2=bessely(1,[0 1e-5 1 50]);\n",
        "v3=bessely(-0.5,[1 3]);\n",
        "v4=bessely(-2,[1 3]);\n",
        "v5=bessely(2.3,[0.5 -1 1+1i]);\n",
        "v6=bessely(0,[1 10i],1);\n",
        "v7=bessely(0,-0);\n",
        "v8=bessely([0.5 -0.5 -1 1.5 -1.5],0);\n",
        "v9=bessely([0.3 7.5 30],[0.7 12 25]);\n",
        "v10=bessely(-2.5,[0.5 1+1i -1]);\n",
        "v11=bessely(0,1,1);\n",
        "v12=bessely(0,1i,1);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[1, 7]);
    check(&it, "v1", &[(-INF, 0.0), (0.088256964215677, 0.0), (0.4980703596152319, 0.0), (0.05567116728359935, 0.0), (0.088256964215677, 1.5303953731159332), (NAN, 0.0), (NAN, 0.0)], 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[1, 4]);
    check(&it, "v2", &real(&[-INF, -63661.97727536548, -0.7812128213002889, -0.05679566856201477]), 8.881784197001252e-16);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[1, 2]);
    check(&it, "v3", &real(&[0.6713967071418032, 0.06500818287737578]), 8.881784197001252e-16);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[1, 2]);
    check(&it, "v4", &real(&[-1.6506826068162548, -0.1604003934849238]), 8.881784197001252e-16);
    check_class(&it, "v5", "double");
    check_dims(&it, "v5", &[1, 3]);
    check(&it, "v5", &[(-9.472230711912161, 0.0), (-1.328024205539276, 1.9102807323780393), (-0.24768799812528633, 0.7595467103431256)], 1e-15);
    check_class(&it, "v6", "double");
    check_dims(&it, "v6", &[1, 2]);
    check(&it, "v6", &[(0.08825696421567697, 0.0), (-5.138881257595495e-10, 0.12783333716342862)], 8.881784197001252e-16);
    check_class(&it, "v7", "double");
    check_dims(&it, "v7", &[1, 1]);
    check(&it, "v7", &real(&[-INF]), 8.881784197001252e-16);
    check_class(&it, "v8", "double");
    check_dims(&it, "v8", &[1, 5]);
    check(&it, "v8", &real(&[-INF, -INF, -INF, -INF, -INF]), 8.881784197001252e-16);
    check_class(&it, "v9", "double");
    check_dims(&it, "v9", &[1, 3]);
    check(&it, "v9", &real(&[-0.5479072045668654, 0.25035734829167944, -1.6575809094094076]), 8.881784197001252e-16);
    check_class(&it, "v10", "double");
    check_dims(&it, "v10", &[1, 3]);
    check(&it, "v10", &[(0.009236407819379726, 0.0), (-0.0313610583552256, 0.12286070035872487), (6.938893903907228e-18, 0.04949681022847795)], 8.881784197001252e-16);
    check_class(&it, "v11", "double");
    check_dims(&it, "v11", &[1, 1]);
    check(&it, "v11", &real(&[0.08825696421567697]), 8.881784197001252e-16);
    check_class(&it, "v12", "double");
    check_dims(&it, "v12", &[1, 1]);
    check(&it, "v12", &[(-0.09860363970645836, 0.46575960759364043)], 8.881784197001252e-16);
}

#[test]
fn besseli_besselk_match_matlab() {
    let (it, errors) = run(concat!(
        "v1=besseli(0,[0 1 2.5 10 -1 -2.5 Inf NaN 700 800]);\n",
        "v2=besseli(1,[0 1 -1 50]);\n",
        "v3=besseli(-1.5,[0 1 3]);\n",
        "v4=besseli(-2,[1 3]);\n",
        "v5=besseli(0,[1 10 800 -800],1);\n",
        "v6=besseli(2,1+2i);\n",
        "v7=besselk(0,[0 1 2.5 10 -1 Inf NaN 800]);\n",
        "v8=besselk(1,[0 1e-5 1 50]);\n",
        "v9=besselk(-1.5,[1 3]);\n",
        "v10=besselk(0,[1 800],1);\n",
        "v11=besselk(2,1+2i);\n",
        "v12=besselk([-0.5 0.5 2],0);\n",
        "v13=besseli([0.5 -0.5 -1.5],-1);\n",
        "v14=besselk(0.5,-1);\n",
        "v15=besselk(1,1e-310);\n",
        "v16=besseli(1e3,1);\n",
        "v17=besseli(0,700,1);\n",
        "v18=besseli([0.3 7.5 30],[0.7 12 25]);\n",
        "v19=besselk([0.3 7.5 30],[0.7 12 25]);\n",
        "v20=besselk(-2.5,[0.5 1+1i -1]);\n",
        "v21=besseli(-2.5,[0.5 1+1i -1]);\n",
        "v22=besselk(0,1i);\n",
        "v23=besseli(1,1i,1);\n",
        "v24=besselk(1,-1,1);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[1, 10]);
    check(&it, "v1", &real(&[1.0, 1.2660658777520082, 3.289839144050123, 2815.716628466255, 1.2660658777520082, 3.289839144050123, INF, NAN, 1.5295933476718737e+302, INF]), 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[1, 4]);
    check(&it, "v2", &real(&[0.0, 0.565159103992485, -0.565159103992485, 2.903078590103557e+20]), 8.881784197001252e-16);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[1, 3]);
    check(&it, "v3", &real(&[INF, -0.29352532634747985, 3.068903650787101]), 1e-14);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[1, 2]);
    check(&it, "v4", &real(&[0.13574766976703834, 2.245212440929951]), 8.881784197001252e-16);
    check_class(&it, "v5", "double");
    check_dims(&it, "v5", &[1, 4]);
    check(&it, "v5", &real(&[0.4657596075936404, 0.12783333716342862, 0.014106945005869183, 0.014106945005869183]), 8.881784197001252e-16);
    check_class(&it, "v6", "double");
    check_dims(&it, "v6", &[1, 1]);
    check(&it, "v6", &[(-0.4126719082931706, 0.2659739227983887)], 8.881784197001252e-16);
    check_class(&it, "v7", "double");
    check_dims(&it, "v7", &[1, 8]);
    check(&it, "v7", &[(INF, 0.0), (0.42102443824070834, 0.0), (0.062347553200366196, 0.0), (1.778006231616765e-05, 0.0), (0.42102443824070834, -3.977463260506422), (0.0, 0.0), (NAN, 0.0), (0.0, 0.0)], 8.881784197001252e-16);
    check_class(&it, "v8", "double");
    check_dims(&it, "v8", &[1, 4]);
    check(&it, "v8", &real(&[INF, 99999.9999393557, 0.6019072301972346, 3.4441022267175555e-23]), 8.881784197001252e-16);
    check_class(&it, "v9", "double");
    check_dims(&it, "v9", &[1, 2]);
    check(&it, "v9", &real(&[0.9221370088957892, 0.04803464684235279]), 8.881784197001252e-16);
    check_class(&it, "v10", "double");
    check_dims(&it, "v10", &[1, 2]);
    check(&it, "v10", &real(&[1.1444630798068949, 0.04430442748664601]), 8.881784197001252e-16);
    check_class(&it, "v11", "double");
    check_dims(&it, "v11", &[1, 1]);
    check(&it, "v11", &[(-0.4834389764814574, 0.003548130510448805)], 8.881784197001252e-16);
    check_class(&it, "v12", "double");
    check_dims(&it, "v12", &[1, 3]);
    check(&it, "v12", &real(&[INF, INF, INF]), 8.881784197001252e-16);
    check_class(&it, "v13", "double");
    check_dims(&it, "v13", &[1, 3]);
    check(&it, "v13", &[(0.0, 0.9376748882454878), (0.0, -1.2312002145929681), (0.0, -0.29352532634747985)], 8.881784197001252e-16);
    check_class(&it, "v14", "double");
    check_dims(&it, "v14", &[1, 1]);
    check(&it, "v14", &[(0.0, -3.4068610448155496)], 1e-14);
    check_class(&it, "v15", "double");
    check_dims(&it, "v15", &[1, 1]);
    check(&it, "v15", &real(&[INF]), 8.881784197001252e-16);
    check_class(&it, "v16", "double");
    check_dims(&it, "v16", &[1, 1]);
    check(&it, "v16", &real(&[0.0]), 8.881784197001252e-16);
    check_class(&it, "v17", "double");
    check_dims(&it, "v17", &[1, 1]);
    check(&it, "v17", &real(&[0.015081295651531358]), 8.881784197001252e-16);
    check_class(&it, "v18", "double");
    check_dims(&it, "v18", &[1, 3]);
    check(&it, "v18", &real(&[0.8919002227528229, 1782.2829436017576, 337.20726860494017]), 1e-14);
    check_class(&it, "v19", "double");
    check_dims(&it, "v19", &[1, 3]);
    check(&it, "v19", &real(&[0.6895624897569758, 1.98210496845945e-05, 3.796729955708769e-05]), 1e-14);
    check_class(&it, "v20", "double");
    check_dims(&it, "v20", &[1, 3]);
    check(&it, "v20", &[(20.425904466498487, 0.0), (-0.973020320888058, -1.1600029997916972), (0.0, -3.406861044815549)], 8.881784197001252e-16);
    check_class(&it, "v21", "double");
    check_dims(&it, "v21", &[1, 3]);
    check(&it, "v21", &[(13.013106895650546, 0.0), (-0.6841439925296022, -0.6294295942799519), (0.0, -2.111776193635407)], 8.881784197001252e-16);
    check_class(&it, "v22", "double");
    check_dims(&it, "v22", &[1, 1]);
    check(&it, "v22", &[(-0.13863371520405404, -1.2019697153172062)], 8.881784197001252e-16);
    check_class(&it, "v23", "double");
    check_dims(&it, "v23", &[1, 1]);
    check(&it, "v23", &[(2.694532706477254e-17, 0.44005058574493355)], 8.881784197001252e-16);
    check_class(&it, "v24", "double");
    check_dims(&it, "v24", &[1, 1]);
    check(&it, "v24", &[(-0.22142929548200932, -0.6531698334674465)], 8.881784197001252e-16);
}

#[test]
fn besselh_matches_matlab() {
    let (it, errors) = run(concat!(
        "v1=besselh(0,[1 2.5 -1 1+1i]);\n",
        "v2=besselh(1,2,[1 2.5 -1 1+1i]);\n",
        "v3=besselh(0.5,1,[1 3],1);\n",
        "v4=besselh(0,2,[1 3],1);\n",
        "v5=besselh(0,0);\n",
        "v7=besselh(0,1,0);\n",
        "v8=besselh(0,1,-0.5);\n",
        "v9=besselh(-1.5,1,2);\n",
        "v10=besselh(-1.5,2,2+1i);\n",
        "v11=besselh(1,1,[1 2],1);\n",
        "v12=besselh(-2,[1 -1]);\n",
        "v13=besselh(0,1,[1;2]);\n",
        "v14=besselh(2.5,2,-3,1);\n",
        "v15=besselh(0,[1 1e-3]);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[1, 4]);
    check(&it, "v1", &[(0.7651976865579664, 0.088256964215677), (-0.04838377646819803, 0.4980703596152319), (-0.7651976865579664, 0.088256964215677), (0.2274498948022947, -0.05105545867308963)], 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[1, 4]);
    check(&it, "v2", &[(0.4400505857449335, 0.7812128213002889), (0.497094102464274, -0.14591813796678582), (-1.3201517572348005, -0.7812128213002889), (1.2439613389157882, 1.0227225644184332)], 8.881784197001252e-16);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[1, 2]);
    check(&it, "v3", &[(0.0, -0.7978845608028655), (0.0, -0.4606588659617808)], 8.881784197001252e-16);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[1, 2]);
    check(&it, "v4", &[(0.4877037490869563, 0.596206209606004), (0.31063056052955773, 0.3363801482847203)], 8.881784197001252e-16);
    check_class(&it, "v5", "double");
    check_dims(&it, "v5", &[1, 1]);
    check(&it, "v5", &real(&[NAN]), 8.881784197001252e-16);
    assert_eq!(error_of("r=besselh(0,3,1);"), "K must be 1 or 2.");
    check_class(&it, "v7", "double");
    check_dims(&it, "v7", &[1, 1]);
    check(&it, "v7", &real(&[NAN]), 8.881784197001252e-16);
    check_class(&it, "v8", "double");
    check_dims(&it, "v8", &[1, 1]);
    check(&it, "v8", &[(-0.9384698072408126, -0.44451873350670656)], 8.881784197001252e-16);
    check_class(&it, "v9", "double");
    check_dims(&it, "v9", &[1, 1]);
    check(&it, "v9", &[(-0.3956232813587036, -0.4912937786871625)], 8.881784197001252e-16);
    check_class(&it, "v10", "double");
    check_dims(&it, "v10", &[1, 1]);
    check(&it, "v10", &[(-0.5597412212539254, 1.1703208283226576)], 8.881784197001252e-16);
    check_class(&it, "v11", "double");
    check_dims(&it, "v11", &[1, 2]);
    check(&it, "v11", &[(-0.4196075759074962, -0.792380888474382), (-0.33732651889335213, -0.47987317588615114)], 8.881784197001252e-16);
    check_class(&it, "v12", "double");
    check_dims(&it, "v12", &[1, 2]);
    check(&it, "v12", &[(0.11490348493190057, -1.6506826068162548), (-0.11490348493190057, -1.6506826068162548)], 8.881784197001252e-16);
    check_class(&it, "v13", "double");
    check_dims(&it, "v13", &[2, 1]);
    check(&it, "v13", &[(0.7651976865579664, 0.088256964215677), (0.22389077914123567, 0.5103756726497453)], 8.881784197001252e-16);
    check_class(&it, "v14", "double");
    check_dims(&it, "v14", &[1, 1]);
    check(&it, "v14", &[(-0.3071059106411875, -0.4606588659617811)], 8.881784197001252e-16);
    check_class(&it, "v15", "double");
    check_dims(&it, "v15", &[1, 2]);
    check(&it, "v15", &[(0.7651976865579664, 0.088256964215677), (0.9999997500000157, -4.471416611375924)], 8.881784197001252e-16);
}

#[test]
fn error_functions_match_matlab() {
    let (it, errors) = run(concat!(
        "v1=erf([0 -0 0.1 0.46875 0.5 1 2 3.9 4 5 6 27 -1 -6 Inf -Inf NaN 1e-300]);\n",
        "v2=erfc([0 0.1 0.5 1 2 4 5 10 26 27 30 -1 -6 Inf -Inf NaN]);\n",
        "v3=erfcx([0 0.1 0.5 1 2 4 5 10 26 27 30 1e7 1e300 -1 -6 -26 -27 -30 Inf -Inf NaN]);\n",
        "v4=erfinv([0 0.1 0.5 0.9 0.99 0.999999 1-eps -0.5 1 -1 1.5 NaN 1e-10]);\n",
        "v5=erfcinv([0 1e-300 1e-10 0.1 0.5 1 1.5 1.9 2 2.5 -1 NaN 2-eps]);\n",
        "v6=erf(single([0.5 1 2]));\n",
        "v7=erfc(single([0.5 1 2]));\n",
        "v8=erfinv(single([0.5 0.9]));\n",
        "v13=erfcinv([1e-310 1e-20 0.001 0.3 0.7 0.99 1.01 1.3 1.999]);\n",
        "v14=erfinv([-1+eps 0.999 -0.3 0.75 0.8 0.95 1e-300 0.9999999]);\n",
        "v15=erfcinv(single([0.1 1.5]));\n",
        "v16=erfcx(single([1 -1 10]));\n",
        "v17=erf([0.2 0.3 0.47 0.6 0.8 1.5 2.5 3 3.5 4.1 4.5 5.5]);\n",
        "v18=erfc([0.2 0.3 0.47 0.6 0.8 1.5 2.5 3 3.5 4.1 4.5 5.5 -0.2 -0.6 -3]);\n",
        "v19=erfcx([0.2 0.3 0.47 0.6 0.8 1.5 2.5 3 3.5 4.1 4.5 5.5 -0.2 -0.6 -3 6.71e7 1e8]);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[1, 18]);
    check(&it, "v1", &real(&[0.0, -0.0, 0.1124629160182849, 0.49261347321793797, 0.5204998778130465, 0.8427007929497149, 0.9953222650189527, 0.9999999652077514, 0.9999999845827421, 0.9999999999984626, 1.0, 1.0, -0.8427007929497149, -1.0, 1.0, -1.0, NAN, 1.1283791670955126e-300]), 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[1, 16]);
    check(&it, "v2", &real(&[1.0, 0.8875370839817152, 0.4795001221869535, 0.15729920705028513, 0.004677734981047266, 1.541725790028002e-08, 1.5374597944280351e-12, 2.088487583762545e-45, 5.663192408856143e-296, 5.23705e-319, 0.0, 1.842700792949715, 2.0, 0.0, 2.0, NAN]), 8.881784197001252e-16);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[1, 21]);
    check(&it, "v3", &real(&[1.0, 0.8964569799691265, 0.6156903441929259, 0.427583576155807, 0.25539567631050575, 0.13699945762506138, 0.11070463773306863, 0.056140992743822594, 0.021683584850562907, 0.02088160799042094, 0.01879588886141675, 5.641895835477535e-08, 5.641895835477562e-301, 5.008980080762283, 8622463094230390.0, 7.657724931490568e+293, INF, INF, 0.0, INF, NAN]), 8.881784197001252e-16);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[1, 13]);
    check(&it, "v4", &real(&[0.0, 0.08885599049425769, 0.4769362762044699, 1.1630871536766743, 1.8213863677184494, 3.458910737275499, 5.805018683193453, -0.4769362762044699, INF, -INF, NAN, NAN, 8.862269254527581e-11]), 8.881784197001252e-16);
    check_class(&it, "v5", "double");
    check_dims(&it, "v5", &[1, 13]);
    check(&it, "v5", &real(&[INF, 26.209469960516124, 4.572824967389486, 1.163087153676674, 0.4769362762044699, 0.0, -0.4769362762044699, -1.1630871536766738, -INF, NAN, NAN, NAN, -5.805018683193453]), 8.881784197001252e-16);
    check_class(&it, "v6", "single");
    check_dims(&it, "v6", &[1, 3]);
    check(&it, "v6", &real(&[0.5204998850822449, 0.8427007794380188, 0.9953222870826721]), 1.1920928955078125e-07);
    check_class(&it, "v7", "single");
    check_dims(&it, "v7", &[1, 3]);
    check(&it, "v7", &real(&[0.4795001149177551, 0.15729920566082, 0.004677734803408384]), 1.1920928955078125e-07);
    check_class(&it, "v8", "single");
    check_dims(&it, "v8", &[1, 2]);
    check(&it, "v8", &real(&[0.4769362807273865, 1.1630871295928955]), 1.1920928955078125e-07);
    assert_eq!(error_of("r=erf(1+1i);"), "Input must be real and full.");
    assert_eq!(error_of("r=erf(int8(1));"), "Undefined function 'erf' for input arguments of type 'int8'.");
    assert_eq!(error_of("r=erf('a');"), "Undefined function 'erf' for input arguments of type 'char'.");
    assert_eq!(error_of("r=erf(true);"), "Undefined function 'erf' for input arguments of type 'logical'.");
    check_class(&it, "v13", "double");
    check_dims(&it, "v13", &[1, 9]);
    check(&it, "v13", &real(&[26.644806559364763, 6.601580622355143, 2.3267537655135246, 0.7328690779592167, 0.27246271472675443, 0.008862501280950607, -0.008862501280950607, -0.27246271472675443, -2.3267537655135464]), 8.881784197001252e-16);
    check_class(&it, "v14", "double");
    check_dims(&it, "v14", &[1, 8]);
    check(&it, "v14", &real(&[-5.805018683193453, 2.3267537655135246, -0.2724627147267544, 0.8134198475976185, 0.9061938024368232, 1.3859038243496777, 8.86226925452758e-301, 3.7665625816384707]), 8.881784197001252e-16);
    check_class(&it, "v15", "single");
    check_dims(&it, "v15", &[1, 2]);
    check(&it, "v15", &real(&[1.1630871295928955, -0.4769362807273865]), 1.1920928955078125e-07);
    check_class(&it, "v16", "single");
    check_dims(&it, "v16", &[1, 3]);
    check(&it, "v16", &real(&[0.42758357524871826, 5.0089802742004395, 0.05614099279046059]), 1.1920928955078125e-07);
    check_class(&it, "v17", "double");
    check_dims(&it, "v17", &[1, 12]);
    check(&it, "v17", &real(&[0.22270258921047847, 0.3286267594591274, 0.4937450508860821, 0.6038560908479259, 0.7421009647076605, 0.9661051464753108, 0.999593047982555, 0.9999779095030014, 0.9999992569016276, 0.9999999932999724, 0.9999999998033839, 0.9999999999999927]), 8.881784197001252e-16);
    check_class(&it, "v18", "double");
    check_dims(&it, "v18", &[1, 15]);
    check(&it, "v18", &real(&[0.7772974107895215, 0.6713732405408726, 0.5062549491139179, 0.3961439091520741, 0.2578990352923395, 0.033894853524689274, 0.0004069520174449589, 2.2090496998585438e-05, 7.430983723414128e-07, 6.700027654084918e-09, 1.9661604415428873e-10, 7.357847917974398e-15, 1.2227025892104786, 1.603856090847926, 1.9999779095030015]), 8.881784197001252e-16);
    check_class(&it, "v19", "double");
    check_dims(&it, "v19", &[1, 17]);
    check(&it, "v19", &real(&[0.8090195199015807, 0.7345993345676551, 0.6314005166600344, 0.567804717386587, 0.4891005892231148, 0.32158541645431754, 0.2108063640611436, 0.17900115118138996, 0.15529365560889433, 0.133834116418652, 0.12248480427384141, 0.10096221839949909, 1.2726020284831958, 2.298854111734094, 16205.988853999586, 8.40819051486969e-09, 5.6418958354775614e-09]), 8.881784197001252e-16);
}

#[test]
fn gammaln_and_psi_match_matlab() {
    let (it, errors) = run(concat!(
        "v2=gammaln(single([0.5 10]));\n",
        "v6=psi(2,[0.5 1 2 10]);\n",
        "v7=psi(3,[0.5 1 5]);\n",
        "v11=psi(0,[1 2]);\n",
        "v12=psi(1,0);\n",
        "v13=psi(single([1 2]));\n",
        "v14=gammaln([0 0.5 1 1.5 2 3 10 100 1e5 1e300 Inf NaN 1e-300 0.6796875 4 12 2.55e305 3e305 0.4 0.7 1.4 2.5 6.3 13 40]);\n",
        "v16=gammaln(-0);\n",
        "v18=psi([0.5 1 1.5 2 3 10 100 1e10 0 Inf NaN 1e-10 1e-300 2.05e-9 2.9 3 3.1 0.4999 1e16 4e16]);\n",
        "v20=psi(1,[0.5 1 2 10 100 1e-5 1e10 0 Inf NaN]);\n",
        "v21=psi(4,[0.1 30]);\n",
        "v22=psi(10,2);\n",
        "v23=psi(0,0);\n",
        "v24=psi(2,0);\n",
        "v25=psi(1,single(2));\n",
        "v26=psi(single(1),2);\n",
        "v27=psi(1,[1 2;3 4]);\n",
        "v28=psi(1,Inf); v28=[v28 psi(2,Inf) psi(3,Inf) psi(0,Inf)];\n",
        "v29=psi(2,[1e-5 1e10 Inf NaN 3.5 25 1e3]);\n",
        "v30=psi(1,[0.25 0.75 3.5 7 12 25 40 1e3 1e6]);\n",
        "v31=psi(5,[0.25 0.75 3.5 7 12 25 40]);\n",
        "v32=psi([0.25 0.75 3.5 7 12 25 40 2.5 0.05]);\n",
        "v33=gammaln([0.05 0.25 0.55 0.75 0.95 1.25 1.75 2.25 3.5 4.5 7.5 11.5 15 22.5 1e3 1e6]);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(error_of("r=gammaln([0 0.5 1 1.5 2 3 10 100 1e5 1e300 Inf -1 -0.5 NaN 1e-300 0.6796875 4 12]);"), "Input must be nonnegative.");
    check_class(&it, "v2", "single");
    check_dims(&it, "v2", &[1, 2]);
    check(&it, "v2", &real(&[0.5723649263381958, 12.801827430725098]), 1.1920928955078125e-07);
    assert_eq!(error_of("r=gammaln(1+1i);"), "Input must be real and full.");
    assert_eq!(error_of("r=psi([0.5 1 1.5 2 3 10 100 1e10 -0.5 -1.5 -1 0 Inf NaN 1e-10 -3.7]);"), "X must be nonnegative.");
    assert_eq!(error_of("r=psi(1,[0.5 1 2 10 100 -0.5]);"), "X must be nonnegative.");
    check_class(&it, "v6", "double");
    check_dims(&it, "v6", &[1, 4]);
    check(&it, "v6", &real(&[-16.82879664423432, -2.4041138063191885, -0.4041138063191886, -0.011049834970802063]), 8.881784197001252e-16);
    check_class(&it, "v7", "double");
    check_dims(&it, "v7", &[1, 3]);
    check(&it, "v7", &real(&[97.40909103400244, 6.493939402266829, 0.021427828192755075]), 8.881784197001252e-16);
    assert_eq!(error_of("r=psi([0 1 2],1);"), "K must be a scalar.");
    assert_eq!(error_of("r=psi(-1,1);"), "Order must be nonnegative integer value smaller than 2^31-1.");
    assert_eq!(error_of("r=psi(1.5,1);"), "Order must be nonnegative integer value smaller than 2^31-1.");
    check_class(&it, "v11", "double");
    check_dims(&it, "v11", &[1, 2]);
    check(&it, "v11", &real(&[-0.5772156649015323, 0.42278433509846747]), 8.881784197001252e-16);
    check_class(&it, "v12", "double");
    check_dims(&it, "v12", &[1, 1]);
    check(&it, "v12", &real(&[INF]), 8.881784197001252e-16);
    check_class(&it, "v13", "single");
    check_dims(&it, "v13", &[1, 2]);
    check(&it, "v13", &real(&[-0.5772156715393066, 0.42278432846069336]), 1.1920928955078125e-07);
    check_class(&it, "v14", "double");
    check_dims(&it, "v14", &[1, 25]);
    check(&it, "v14", &real(&[INF, 0.5723649429247, 0.0, -0.12078223763524526, 0.0, 0.6931471805599453, 12.801827480081469, 359.1342053695754, 1051287.708973657, 6.897755278982137e+302, INF, NAN, 690.7755278982137, 0.28624281231900117, 1.791759469228055, 17.502307845873887, 1.7906725941420033e+308, INF, 0.7966778177017837, 0.26086724653166654, -0.1196129141723713, 0.2846828704729192, 5.307342889624762, 19.987214495661885, 106.63176026064346]), 8.881784197001252e-16);
    assert_eq!(error_of("r=gammaln(-1);"), "Input must be nonnegative.");
    check_class(&it, "v16", "double");
    check_dims(&it, "v16", &[1, 1]);
    check(&it, "v16", &real(&[INF]), 8.881784197001252e-16);
    assert_eq!(error_of("r=gammaln(int8(3));"), "Undefined function 'gammaln' for input arguments of type 'int8'.");
    check_class(&it, "v18", "double");
    check_dims(&it, "v18", &[1, 20]);
    check(&it, "v18", &real(&[-1.963510026021423, -0.5772156649015323, 0.036489973978576895, 0.42278433509846747, 0.9227843350984675, 2.2517525890667214, 4.600161852738088, 23.025850929890456, -INF, INF, NAN, -10000000000.577217, -9.999999999999999e+299, -487804878.6259961, 0.882499950637744, 0.9227843350984675, 0.9615264448703085, -1.9640035904016986, 36.841361487904734, 38.22765584902462]), 8.881784197001252e-16);
    assert_eq!(error_of("r=psi(-0.5);"), "X must be nonnegative.");
    check_class(&it, "v20", "double");
    check_dims(&it, "v20", &[1, 10]);
    check(&it, "v20", &real(&[4.934802200544679, 1.6449340668482264, 0.6449340668482264, 0.10516633568168572, 0.010050166663333566, 10000000001.644909, 1.0000000000499996e-10, INF, INF, NAN]), 8.881784197001252e-16);
    check_class(&it, "v21", "double");
    check_dims(&it, "v21", &[1, 2]);
    check(&it, "v21", &real(&[-2400015.607203195, -7.914941340181022e-06]), 8.881784197001252e-16);
    check_class(&it, "v22", "double");
    check_dims(&it, "v22", &[1, 1]);
    check(&it, "v22", &real(&[-1793.311606628713]), 8.881784197001252e-16);
    check_class(&it, "v23", "double");
    check_dims(&it, "v23", &[1, 1]);
    check(&it, "v23", &real(&[-INF]), 8.881784197001252e-16);
    check_class(&it, "v24", "double");
    check_dims(&it, "v24", &[1, 1]);
    check(&it, "v24", &real(&[-INF]), 8.881784197001252e-16);
    check_class(&it, "v25", "single");
    check_dims(&it, "v25", &[1, 1]);
    check(&it, "v25", &real(&[0.6449340581893921]), 1.1920928955078125e-07);
    check_class(&it, "v26", "double");
    check_dims(&it, "v26", &[1, 1]);
    check(&it, "v26", &real(&[0.6449340668482264]), 8.881784197001252e-16);
    check_class(&it, "v27", "double");
    check_dims(&it, "v27", &[2, 2]);
    check(&it, "v27", &real(&[1.6449340668482264, 0.39493406684822635, 0.6449340668482264, 0.28382295573711525]), 8.881784197001252e-16);
    check_class(&it, "v28", "double");
    check_dims(&it, "v28", &[1, 4]);
    check(&it, "v28", &real(&[INF, -INF, INF, INF]), 8.881784197001252e-16);
    check_class(&it, "v29", "double");
    check_dims(&it, "v29", &[1, 7]);
    check(&it, "v29", &real(&[-1999999999999999.8, -1.0000000000999992e-20, -INF, NAN, -0.10820405164172742, -0.001665279318422469, -1.0010004999998335e-06]), 1e-14);
    check_class(&it, "v30", "double");
    check_dims(&it, "v30", &[1, 9]);
    check(&it, "v30", &real(&[17.19732915450711, 2.5418796476716063, 0.3303577561002349, 0.1535451779593375, 0.08690187287176838, 0.04081066325722559, 0.02531510384129103, 0.0010005001666666335, 1.0000005000001672e-06]), 8.881784197001252e-16);
    check_class(&it, "v31", "double");
    check_dims(&it, "v31", &[1, 7]);
    check(&it, "v31", &real(&[491552.51376768126, 678.7533428745797, 0.08704917856718347, 0.0020094931750490293, 0.00011820829051081495, 2.7131757700038334e-06, 2.493894350999672e-07]), 8.881784197001252e-16);
    check_class(&it, "v32", "double");
    check_dims(&it, "v32", &[1, 9]);
    check(&it, "v32", &real(&[-4.2274535333762655, -1.085860879786472, 1.1031566406452433, 1.8727843350984674, 2.442661679975812, 3.198742512851974, 3.676327374034843, 0.7031566406452434, -20.49784499129987]), 8.881784197001252e-16);
    check_class(&it, "v33", "double");
    check_dims(&it, "v33", &[1, 16]);
    check(&it, "v33", &real(&[2.9688792010517306, 1.2880225246980774, 0.48003085611112595, 0.20328095143129535, 0.03096879523797293, -0.09827183642181317, -0.08440112102048555, 0.1248717148923966, 1.2009736023470743, 2.4537365708424423, 7.534364236758734, 16.29200047656724, 25.19122118273868, 46.919978795808774, 5905.220423209181, 12815504.569147611]), 8.881784197001252e-16);
}

#[test]
fn beta_and_betaln_match_matlab() {
    let (it, errors) = run(concat!(
        "v2=beta(100,200);\n",
        "v3=beta(single(2),3);\n",
        "v5=betaln([1 2 0.5 0 100 1e5],[1 3 0.5 1 200 1e5]);\n",
        "v7=beta([1 2 0.5 0 3],[1 3 0.5 1 Inf]);\n",
        "v8=beta(0,0);\n",
        "v9=beta([1e-300 200],[1 300]);\n",
        "v10=beta(NaN,1);\n",
        "v11=beta([1 2],[1;2]);\n",
        "v12=betaln(0,0);\n",
        "v13=betaln([1e-300 1 NaN],[1 Inf 1]);\n",
        "v14=betaln(single(3),4);\n",
        "v15=betaln([0.3 2.5 7 40 1e3],[0.6 3.5 0.2 50 1e4]);\n",
        "v16=beta([0.3 2.5 7 40 1e3],[0.6 3.5 0.2 50 1e-3]);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(error_of("r=beta([1 2 0.5 0 -0.5 3],[1 3 0.5 1 2 Inf]);"), "Input must be nonnegative.");
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[1, 1]);
    check(&it, "v2", &real(&[3.607285449794902e-84]), 8.881784197001252e-16);
    check_class(&it, "v3", "single");
    check_dims(&it, "v3", &[1, 1]);
    check(&it, "v3", &real(&[0.0833333283662796]), 1e-06);
    assert_eq!(error_of("r=beta(1+1i,2);"), "Input must be real and full.");
    check_class(&it, "v5", "double");
    check_dims(&it, "v5", &[1, 6]);
    check(&it, "v5", &real(&[0.0, -2.4849066497880004, 1.1447298858494, INF, -192.1341922749789, -138633.92706134776]), 8.881784197001252e-16);
    assert_eq!(error_of("r=betaln(-1,2);"), "Input must be nonnegative.");
    check_class(&it, "v7", "double");
    check_dims(&it, "v7", &[1, 5]);
    check(&it, "v7", &real(&[1.0, 0.08333333333333333, 3.1415926535897927, INF, NAN]), 8.881784197001252e-16);
    check_class(&it, "v8", "double");
    check_dims(&it, "v8", &[1, 1]);
    check(&it, "v8", &real(&[NAN]), 8.881784197001252e-16);
    check_class(&it, "v9", "double");
    check_dims(&it, "v9", &[1, 2]);
    check(&it, "v9", &real(&[9.999999999999763e+299, 1.6485491608665986e-147]), 8.881784197001252e-16);
    check_class(&it, "v10", "double");
    check_dims(&it, "v10", &[1, 1]);
    check(&it, "v10", &real(&[NAN]), 8.881784197001252e-16);
    check_class(&it, "v11", "double");
    check_dims(&it, "v11", &[2, 2]);
    check(&it, "v11", &real(&[1.0, 0.5, 0.5, 0.16666666666666669]), 8.881784197001252e-16);
    check_class(&it, "v12", "double");
    check_dims(&it, "v12", &[1, 1]);
    check(&it, "v12", &real(&[NAN]), 8.881784197001252e-16);
    check_class(&it, "v13", "double");
    check_dims(&it, "v13", &[1, 3]);
    check(&it, "v13", &real(&[690.7755278982137, NAN, NAN]), 8.881784197001252e-16);
    check_class(&it, "v14", "single");
    check_dims(&it, "v14", &[1, 1]);
    check(&it, "v14", &real(&[-4.094344615936279]), 1.1920928955078125e-07);
    check_class(&it, "v15", "double");
    check_dims(&it, "v15", &[1, 5]);
    check(&it, "v15", &real(&[1.4276556131525677, -3.3018352699620523, 1.1464669545525439, -62.45532574289069, -3353.484270767105]), 8.881784197001252e-16);
    check_class(&it, "v16", "double");
    check_dims(&it, "v16", &[1, 5]);
    check(&it, "v16", &real(&[4.16891417890789, 0.0368155389092554, 3.147054558344876, 7.516171211855558e-28, 992.5442834853714]), 8.881784197001252e-16);
}

#[test]
fn gammainc_matches_matlab() {
    let (it, errors) = run(concat!(
        "v1=gammainc([0 0.5 1 2 10 Inf],2);\n",
        "v2=gammainc(1,[0 0.5 1 2 10 100 Inf]);\n",
        "v3=gammainc([0 0.5 1 2 10],2,'upper');\n",
        "v4=gammainc(100,[90 100 110 1e3]);\n",
        "v5=gammainc([0.5 1 10],[0.5 1 10],'scaledlower');\n",
        "v6=gammainc([0.5 1 10],[0.5 1 10],'scaledupper');\n",
        "v7=gammainc(1e-3,[1e-3 1e-5]);\n",
        "v8=gammainc(-1,2);\n",
        "v10=gammainc(NaN,1);\n",
        "v12=gammainc(single(1),2);\n",
        "v13=gammainc(1e5,1e5+[0 10 -10]);\n",
        "v14=gammainc(0,0);\n",
        "v15=gammainc(0.5,0);\n",
        "v16=gammainc([1e-300 1e5],1e5);\n",
        "v17=gammainc(10,[1e6 1e7]);\n",
        "v18=gammainc(1e10,1e10);\n",
        "v19=gammainc(50,30,'upper');\n",
        "v20=gammainc(1e3,500,'upper');\n",
        "v21=gammainc(0,2,'scaledlower');\n",
        "v22=gammainc(0,2,'scaledupper');\n",
        "v23=gammainc(Inf,2,'scaledupper');\n",
        "v24=gammainc(-1,2,'upper');\n",
        "v25=gammainc(-1,0.5);\n",
        "v26=gammainc(-1,3);\n",
        "v27=gammainc(-5,[1 2.5]);\n",
        "v28=gammainc([0.1 0.5 2 5 20 80],[0.3 7 2.5 0.5 25 60]);\n",
        "v29=gammainc([0.1 0.5 2 5 20 80],[0.3 7 2.5 0.5 25 60],'upper');\n",
        "v30=gammainc([0.1 0.5 2 5 20 80],[0.3 7 2.5 0.5 25 60],'scaledlower');\n",
        "v31=gammainc([0.1 0.5 2 5 20 80],[0.3 7 2.5 0.5 25 60],'scaledupper');\n",
        "v34=gammainc(1e-20,[0.1 1 10]);\n",
        "v35=gammainc(2,1e-10);\n",
        "v36=gammainc(1e3,[900 1100]);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[1, 6]);
    check(&it, "v1", &real(&[0.0, 0.09020401043104988, 0.26424111765711533, 0.5939941502901619, 0.9995006007726127, 1.0]), 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[1, 7]);
    check(&it, "v2", &real(&[1.0, 0.842700792949715, 0.6321205588285577, 0.26424111765711533, 1.1142547833872003e-07, 3.981280818956709e-159, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[1, 5]);
    check(&it, "v3", &real(&[1.0, 0.9097959895689501, 0.7357588823428847, 0.4060058497098381, 0.0004993992273873337]), 8.881784197001252e-16);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[1, 4]);
    check(&it, "v4", &real(&[0.8536538253012673, 0.5132987982791489, 0.1705598979081047, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v5", "double");
    check_dims(&it, "v5", &[1, 3]);
    check(&it, "v5", &real(&[1.410686134642448, 1.718281828459045, 4.332748227619463]), 8.881784197001252e-16);
    check_class(&it, "v6", "double");
    check_dims(&it, "v6", &[1, 3]);
    check(&it, "v6", &real(&[0.6556795424187983, 1.0, 3.66021568]), 8.881784197001252e-16);
    check_class(&it, "v7", "double");
    check_dims(&it, "v7", &[1, 2]);
    check(&it, "v7", &real(&[0.9936876467088606, 0.9999366865285958]), 8.881784197001252e-16);
    check_class(&it, "v8", "double");
    check_dims(&it, "v8", &[1, 1]);
    check(&it, "v8", &real(&[1.0000000000000004]), 8.881784197001252e-16);
    assert_eq!(error_of("r=gammainc(1,-1);"), "A must be real and non-negative.");
    check_class(&it, "v10", "double");
    check_dims(&it, "v10", &[1, 1]);
    check(&it, "v10", &real(&[NAN]), 8.881784197001252e-16);
    assert_eq!(error_of("r=gammainc(1+1i,1);"), "Inputs must be real, full, and double or single.");
    check_class(&it, "v12", "single");
    check_dims(&it, "v12", &[1, 1]);
    check(&it, "v12", &real(&[0.26424112915992737]), 1.1920928955078125e-07);
    check_class(&it, "v13", "double");
    check_dims(&it, "v13", &[1, 3]);
    check(&it, "v13", &real(&[0.5004205221103651, 0.48780695123376855, 0.5130340928829172]), 1e-14);
    check_class(&it, "v14", "double");
    check_dims(&it, "v14", &[1, 1]);
    check(&it, "v14", &real(&[1.0]), 8.881784197001252e-16);
    check_class(&it, "v15", "double");
    check_dims(&it, "v15", &[1, 1]);
    check(&it, "v15", &real(&[1.0]), 8.881784197001252e-16);
    check_class(&it, "v16", "double");
    check_dims(&it, "v16", &[1, 2]);
    check(&it, "v16", &real(&[0.0, 0.5004205221103651]), 1e-14);
    check_class(&it, "v17", "double");
    check_dims(&it, "v17", &[1, 2]);
    check(&it, "v17", &real(&[0.0, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v18", "double");
    check_dims(&it, "v18", &[1, 1]);
    check(&it, "v18", &real(&[0.5000013298076009]), 1e-11);
    check_class(&it, "v19", "double");
    check_dims(&it, "v19", &[1, 1]);
    check(&it, "v19", &real(&[0.0009168288614560779]), 8.881784197001252e-16);
    check_class(&it, "v20", "double");
    check_dims(&it, "v20", &[1, 1]);
    check(&it, "v20", &real(&[4.143678591455105e-69]), 8.881784197001252e-16);
    check_class(&it, "v21", "double");
    check_dims(&it, "v21", &[1, 1]);
    check(&it, "v21", &real(&[1.0]), 8.881784197001252e-16);
    check_class(&it, "v22", "double");
    check_dims(&it, "v22", &[1, 1]);
    check(&it, "v22", &real(&[INF]), 8.881784197001252e-16);
    check_class(&it, "v23", "double");
    check_dims(&it, "v23", &[1, 1]);
    check(&it, "v23", &real(&[0.0]), 8.881784197001252e-16);
    check_class(&it, "v24", "double");
    check_dims(&it, "v24", &[1, 1]);
    check(&it, "v24", &real(&[-4.440892098500626e-16]), 8.881784197001252e-16);
    check_class(&it, "v25", "double");
    check_dims(&it, "v25", &[1, 1]);
    check(&it, "v25", &[(0.0, 1.650425758797543)], 8.881784197001252e-16);
    check_class(&it, "v26", "double");
    check_dims(&it, "v26", &[1, 1]);
    check(&it, "v26", &real(&[-0.3591409142295226]), 8.881784197001252e-16);
    check_class(&it, "v27", "double");
    check_dims(&it, "v27", &[1, 2]);
    check(&it, "v27", &[(-147.41315910257663, 0.0), (0.0, 917.0817894165214)], 8.881784197001252e-16);
    check_class(&it, "v28", "double");
    check_dims(&it, "v28", &[1, 6]);
    check(&it, "v28", &real(&[0.5459128495917965, 1.0023796028843027e-06, 0.45058404864721985, 0.9984345977419975, 0.15677262182623752, 0.9914024627419687]), 8.881784197001252e-16);
    check_class(&it, "v29", "double");
    check_dims(&it, "v29", &[1, 6]);
    check(&it, "v29", &real(&[0.45408715040820347, 0.9999989976203971, 0.5494159513527801, 0.001565402258002547, 0.8432273781737625, 0.0085975372580313]), 8.881784197001252e-16);
    check_class(&it, "v30", "double");
    check_dims(&it, "v30", &[1, 6]);
    check(&it, "v30", &real(&[1.0803713085704238, 1.066154066670095, 1.9559871441808043, 58.72891396507323, 3.516054893263422, 298.25255689842703]), 1e-13);
    check_class(&it, "v31", "double");
    check_dims(&it, "v31", &[1, 6]);
    check(&it, "v31", &real(&[0.8986466049633315, 1063621.9999999972, 2.3850168262287745, 0.0920785144453893, 18.911680589533574, 2.5864747835559068]), 1e-14);
    assert_eq!(error_of("r=gammainc(3,2,'foo');"), "TAIL must be 'lower', 'upper', 'scaledlower', or 'scaledupper'.");
    assert_eq!(error_of("r=gammainc([1 2],[1;2]);"), "Input sizes must match.");
    check_class(&it, "v34", "double");
    check_dims(&it, "v34", &[1, 3]);
    check(&it, "v34", &real(&[0.010511370061117766, 9.999999999999992e-21, 2.7557319223986293e-207]), 8.881784197001252e-16);
    check_class(&it, "v35", "double");
    check_dims(&it, "v35", &[1, 1]);
    check(&it, "v35", &real(&[0.9999999999951099]), 8.881784197001252e-16);
    check_class(&it, "v36", "double");
    check_dims(&it, "v36", &[1, 2]);
    check(&it, "v36", &real(&[0.9993774022157249, 0.0009626304058665574]), 8.881784197001252e-16);
}

#[test]
fn gammaincinv_matches_matlab() {
    let (it, errors) = run(concat!(
        "v1=gammaincinv([0 0.1 0.5 0.9 1],2);\n",
        "v2=gammaincinv([0 0.1 0.5 0.9 1],2,'upper');\n",
        "v3=gammaincinv(0.5,[0.01 0.1 1 10 100 1e4]);\n",
        "v4=gammaincinv(1e-20,[0.5 5]);\n",
        "v6=gammaincinv([1e-300 0.999999],3);\n",
        "v7=gammaincinv(0.3,[1e-5 1e6]);\n",
        "v8=gammaincinv([0.01 0.2 0.7 0.95 0.999],[0.5 3 20 1.5 60]);\n",
        "v9=gammaincinv([0.01 0.2 0.7 0.95 0.999],[0.5 3 20 1.5 60],'upper');\n",
        "v10=gammaincinv(0.5,0);\n",
        "v11=gammaincinv(single(0.5),2);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[1, 5]);
    check(&it, "v1", &real(&[0.0, 0.5318116083896122, 1.6783469900166603, 3.889720169867429, INF]), 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[1, 5]);
    check(&it, "v2", &real(&[INF, 3.889720169867429, 1.6783469900166603, 0.531811608389612, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[1, 6]);
    check(&it, "v3", &real(&[4.46553501891037e-31, 0.0005933911044602258, 0.6931471805599452, 9.668714614714132, 99.6668649193155, 9999.666668642047]), 8.881784197001252e-16);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[1, 2]);
    check(&it, "v4", &real(&[7.853981633974477e-41, 0.0002605284206987163]), 8.881784197001252e-16);
    assert_eq!(error_of("r=gammaincinv(1.5,2);"), "Y must be in the interval [0,1].");
    check_class(&it, "v6", "double");
    check_dims(&it, "v6", &[1, 2]);
    check(&it, "v6", &real(&[1.817120592832162e-100, 19.129168188572923]), 8.881784197001252e-16);
    check_class(&it, "v7", "double");
    check_dims(&it, "v7", &[1, 2]);
    check(&it, "v7", &real(&[0.0, 999475.3579172359]), 8.881784197001252e-16);
    check_class(&it, "v8", "double");
    check_dims(&it, "v8", &[1, 5]);
    check(&it, "v8", &real(&[7.854392895485168e-05, 1.5350442026446434, 22.082433326215003, 3.907363951625589, 86.80871822808007]), 8.881784197001252e-16);
    check_class(&it, "v9", "double");
    check_dims(&it, "v9", &[1, 5]);
    check(&it, "v9", &real(&[3.3174483005106077, 4.279029860125333, 17.435969663475472, 0.17592315887463533, 38.87757021265457]), 8.881784197001252e-16);
    check_class(&it, "v10", "double");
    check_dims(&it, "v10", &[1, 1]);
    check(&it, "v10", &real(&[0.0]), 8.881784197001252e-16);
    check_class(&it, "v11", "single");
    check_dims(&it, "v11", &[1, 1]);
    check(&it, "v11", &real(&[1.6783469915390015]), 1.1920928955078125e-07);
    assert_eq!(error_of("r=gammaincinv(0.5,-1);"), "A must be real and non-negative.");
}

#[test]
fn betainc_matches_matlab() {
    let (it, errors) = run(concat!(
        "v1=betainc([0 0.2 0.5 0.9 1],2,3);\n",
        "v2=betainc([0 0.2 0.5 0.9 1],2,3,'upper');\n",
        "v3=betainc(0.5,[0.1 1 10 100 1000],[0.1 1 10 100 1000]);\n",
        "v4=betainc(0.3,[0 2 Inf],[2 0 1]);\n",
        "v5=betainc(0.001,0.5,200);\n",
        "v6=betainc(0.999,200,0.5,'upper');\n",
        "v11=betainc(single(0.5),2,3);\n",
        "v12=betainc(0.5,2,3,'lower');\n",
        "v14=betainc(0.5,0,0);\n",
        "v15=betainc(0,0,1);\n",
        "v16=betainc(1,1,0);\n",
        "v17=betainc([0.1 0.5],1e-5,1e-5);\n",
        "v18=betainc(0.3,1e5,2e5);\n",
        "v19=betainc(0.7,5,0.5);\n",
        "v20=betainc([0.01 0.1 0.3 0.6 0.95 0.999],[0.5 3 7 20 1.5 40],[2 0.7 4 30 8 2]);\n",
        "v21=betainc([0.01 0.1 0.3 0.6 0.95 0.999],[0.5 3 7 20 1.5 40],[2 0.7 4 30 8 2],'upper');\n",
        "v23=betainc(0.2,Inf,Inf);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[1, 5]);
    check(&it, "v1", &real(&[0.0, 0.18079999999999988, 0.6875, 0.9963, 1.0]), 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[1, 5]);
    check(&it, "v2", &real(&[1.0, 0.8192000000000002, 0.31249999999999994, 0.0036999999999999976, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[1, 5]);
    check(&it, "v3", &real(&[0.5, 0.5, 0.4999999999999991, 0.5000000000000482, 0.49999999999917466]), 1e-11);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[1, 3]);
    check(&it, "v4", &real(&[1.0, 0.0, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v5", "double");
    check_dims(&it, "v5", &[1, 1]);
    check(&it, "v5", &real(&[0.4727558929748405]), 1e-12);
    check_class(&it, "v6", "double");
    check_dims(&it, "v6", &[1, 1]);
    check(&it, "v6", &real(&[0.4727558929748407]), 1e-12);
    assert_eq!(error_of("r=betainc(1.5,1,1);"), "X must be in the interval [0,1].");
    assert_eq!(error_of("r=betainc(-0.1,1,1);"), "X must be in the interval [0,1].");
    assert_eq!(error_of("r=betainc(0.5,-1,1);"), "Z must be real and non-negative.");
    assert_eq!(error_of("r=betainc(NaN,1,1);"), "X must be in the interval [0,1].");
    check_class(&it, "v11", "single");
    check_dims(&it, "v11", &[1, 1]);
    check(&it, "v11", &real(&[0.6875]), 1.1920928955078125e-07);
    check_class(&it, "v12", "double");
    check_dims(&it, "v12", &[1, 1]);
    check(&it, "v12", &real(&[0.6875]), 8.881784197001252e-16);
    assert_eq!(error_of("r=betainc(0.5,2,3,'foo');"), "TAIL must be 'lower' or 'upper'.");
    check_class(&it, "v14", "double");
    check_dims(&it, "v14", &[1, 1]);
    check(&it, "v14", &real(&[NAN]), 8.881784197001252e-16);
    check_class(&it, "v15", "double");
    check_dims(&it, "v15", &[1, 1]);
    check(&it, "v15", &real(&[0.0]), 8.881784197001252e-16);
    check_class(&it, "v16", "double");
    check_dims(&it, "v16", &[1, 1]);
    check(&it, "v16", &real(&[1.0]), 8.881784197001252e-16);
    check_class(&it, "v17", "double");
    check_dims(&it, "v17", &[1, 2]);
    check(&it, "v17", &real(&[0.4999890140692348, 0.5]), 8.881784197001252e-16);
    check_class(&it, "v18", "double");
    check_dims(&it, "v18", &[1, 1]);
    check(&it, "v18", &real(&[0.0]), 8.881784197001252e-16);
    check_class(&it, "v19", "double");
    check_dims(&it, "v19", &[1, 1]);
    check(&it, "v19", &real(&[0.06526224616890819]), 8.881784197001252e-16);
    check_class(&it, "v20", "double");
    check_dims(&it, "v20", &[1, 6]);
    check(&it, "v20", &real(&[0.1495000000000001, 0.0005482183754250396, 0.010592078399999993, 0.9978315900795819, 0.9999999998725223, 0.9992010191652443]), 8.881784197001252e-16);
    check_class(&it, "v21", "double");
    check_dims(&it, "v21", &[1, 6]);
    check(&it, "v21", &real(&[0.8504999999999999, 0.9994517816245749, 0.9894079216, 0.0021684099204181168, 1.2747765932832534e-10, 0.000798980834755718]), 8.881784197001252e-16);
    assert_eq!(error_of("r=betainc(0.5,[1 2],[1;2]);"), "Input sizes must match.");
    check_class(&it, "v23", "double");
    check_dims(&it, "v23", &[1, 1]);
    check(&it, "v23", &real(&[NAN]), 8.881784197001252e-16);
    assert_eq!(error_of("r=betainc(0.2,1,NaN);"), "W must be real and non-negative.");
}

#[test]
fn betaincinv_matches_matlab() {
    let (it, errors) = run(concat!(
        "v1=betaincinv([0 0.1 0.5 0.9 1],2,3);\n",
        "v2=betaincinv([0 0.1 0.5 0.9 1],2,3,'upper');\n",
        "v3=betaincinv(0.5,[0.1 1 10 100],[0.5 1 10 1000]);\n",
        "v4=betaincinv(1e-10,0.5,0.5);\n",
        "v6=betaincinv(0.5,1e-3,1e-3);\n",
        "v7=betaincinv(0.9,5,0.5);\n",
        "v8=betaincinv(0.999,0.5,10);\n",
        "v9=betaincinv([0.01 0.2 0.7 0.95],[0.5 3 20 1.5],[2 0.7 30 8]);\n",
        "v10=betaincinv([0.01 0.2 0.7 0.95],[0.5 3 20 1.5],[2 0.7 30 8],'upper');\n",
        "v11=betaincinv(0.5,0,1);\n",
        "v12=betaincinv(single(0.5),2,3);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[1, 5]);
    check(&it, "v1", &real(&[0.0, 0.14255931671003075, 0.38572756813238945, 0.6795394162781818, 1.0]), 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[1, 5]);
    check(&it, "v2", &real(&[1.0, 0.6795394162781817, 0.3857275681323894, 0.14255931671003072, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[1, 4]);
    check(&it, "v3", &real(&[0.003378164303725277, 0.5, 0.5000000000000001, 0.0906611757229166]), 1e-13);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[1, 1]);
    check(&it, "v4", &real(&[2.4674011002723452e-20]), 8.881784197001252e-16);
    assert_eq!(error_of("r=betaincinv(1.5,1,1);"), "Y must be in the interval [0,1].");
    check_class(&it, "v6", "double");
    check_dims(&it, "v6", &[1, 1]);
    check(&it, "v6", &real(&[0.500000000000177]), 1e-12);
    check_class(&it, "v7", "double");
    check_dims(&it, "v7", &[1, 1]);
    check(&it, "v7", &real(&[0.9983414871448768]), 8.881784197001252e-16);
    check_class(&it, "v8", "double");
    check_dims(&it, "v8", &[1, 1]);
    check(&it, "v8", &real(&[0.4255972622951665]), 8.881784197001252e-16);
    check_class(&it, "v9", "double");
    check_dims(&it, "v9", &[1, 4]);
    check(&it, "v9", &real(&[4.4445761385158994e-05, 0.6689791278625884, 0.43547176824527656, 0.3778341248058554]), 8.881784197001252e-16);
    check_class(&it, "v10", "double");
    check_dims(&it, "v10", &[1, 4]);
    check(&it, "v10", &real(&[0.8412552870530718, 0.9682047389059713, 0.3625922620506534, 0.02111537308981053]), 8.881784197001252e-16);
    check_class(&it, "v11", "double");
    check_dims(&it, "v11", &[1, 1]);
    check(&it, "v11", &real(&[0.0]), 8.881784197001252e-16);
    check_class(&it, "v12", "single");
    check_dims(&it, "v12", &[1, 1]);
    check(&it, "v12", &real(&[0.38572755455970764]), 1.1920928955078125e-07);
}

#[test]
fn elliptic_functions_match_matlab() {
    let (it, errors) = run(concat!(
        "[sn,cn,dn]=ellipj([0 0.5 1 2 -1],0.3); v1=[sn;cn;dn];\n",
        "[sn,cn,dn]=ellipj([0 0.5 1 2],[0 1 0.999 0.5]); v2=[sn;cn;dn];\n",
        "[sn,cn,dn]=ellipj(1,0.5,1e-3); v3=[sn cn dn];\n",
        "[k,e]=ellipke(0.5,1e-2); v9=[k e];\n",
        "v12=ellipke(single(0.5));\n",
        "v13=ellipke([0 0.1 0.5 0.9 0.99 1 1-1e-10 1e-10]);\n",
        "[k,e]=ellipke([0 0.1 0.5 0.9 0.99 1 1-1e-10 1e-10]); v14=[k;e];\n",
        "v15=ellipke(NaN);\n",
        "v18=ellipke(single([0 1]));\n",
        "v19=ellipke([0.5 0.5],[1e-2]);\n",
        "v20=ellipke([]);\n",
        "[sn,cn,dn]=ellipj([0.1 0.7 1.5 3 5],0.75); v22=[sn;cn;dn];\n",
        "[sn,cn,dn]=ellipj([0.1 0.7 1.5 3 5],1); v23=[sn;cn;dn];\n",
        "[sn,cn,dn]=ellipj(0.8,[1e-10 1e-3 0.2 0.6 0.95 0.99999]); v24=[sn;cn;dn];\n",
        "[sn,cn,dn]=ellipj(single(1),0.5); v25=[sn cn dn];\n",
        "[sn,cn,dn]=ellipj([1 2;3 4],0.5); v26=[sn;cn;dn];\n",
        "v27=ellipj(1,0.5);\n",
        "[sn,cn,dn]=ellipj([0.3 1 2.5 -4 10 100],0); v28=[sn;cn;dn];\n",
        "[sn,cn,dn]=ellipj([0.3 1 2.5 -4 10 100],[1e-20 1e-12 1e-6 0.01 0.5 0.9999999]); v29=[sn;cn;dn];\n",
        "[sn,cn,dn]=ellipj([0.3 1 2.5 -4 10 100],1-1e-12); v30=[sn;cn;dn];\n",
        "[sn,cn,dn]=ellipj([0.3 1 2.5 -4 10 100],1-1e-6); v31=[sn;cn;dn];\n",
        "[sn,cn,dn]=ellipj(0.7,[0.2 0.4 0.6 0.8]); v32=[sn;cn;dn];\n",
        "[sn,cn,dn]=ellipj([0.7 0.7 0.7 0.7],[0.2 0.4 0.6 0.8]); v33=[sn;cn;dn];\n",
        "[sn,cn,dn]=ellipj([0.7 0.7],[0.2 1]); v34=[sn;cn;dn];\n",
        "[sn,cn,dn]=ellipj([0.7 0.7],[0.2 0]); v35=[sn;cn;dn];\n",
        "[k,e]=ellipke([0.2 0.4 0.6 0.8 0.95 0.999 1e-5 0.3]); v36=[k;e];\n",
        "[k,e]=ellipke(0.2); v37=[k;e];\n",
        "[k,e]=ellipke([0.2 1]); v38=[k;e];\n",
        "[k,e]=ellipke([0.2 0]); v39=[k;e];\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[3, 5]);
    check(&it, "v1", &real(&[0.0, 1.0, 1.0, 0.47421562271182066, 0.8804087364264624, 0.9656789647459512, 0.8187707145344889, 0.5741206467465549, 0.8938033089590823, 0.9712554361312351, -0.23803965590952328, 0.8467578540147456, -0.8187707145344889, 0.5741206467465549, 0.8938033089590823]), 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[3, 4]);
    check(&it, "v2", &real(&[0.0, 1.0, 1.0, 0.46211715726000974, 0.886818883970074, 0.886818883970074, 0.7616795560884094, 0.6479538979255881, 0.6484014262654938, 0.9946623253580177, -0.10318361552776205, 0.7108610477840873]), 8.881784197001252e-16);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[1, 3]);
    check(&it, "v3", &real(&[0.8030018249180381, 0.5959765676419674, 0.8231610016206734]), 8.881784197001252e-16);
    assert_eq!(error_of("[sn,cn,dn]=ellipj(1,1.5); r=[sn cn dn];"), "M must be in the range 0 <= M <= 1.");
    assert_eq!(error_of("[sn,cn,dn]=ellipj(1+1i,0.5); r=[sn cn dn];"), "Input arguments must be real.");
    assert_eq!(error_of("[sn,cn,dn]=ellipj([1 2],[0.1;0.2]); r=[sn;cn;dn];"), "U and M must be the same size.");
    assert_eq!(error_of("r=ellipke([0 0.1 0.5 0.9 0.99 1 -1 -10 NaN]);"), "M must be in the range 0 <= M <= 1.");
    assert_eq!(error_of("[k,e]=ellipke([0 0.1 0.5 0.9 0.99 1 -1 -10 NaN]); r=[k;e];"), "M must be in the range 0 <= M <= 1.");
    check_class(&it, "v9", "double");
    check_dims(&it, "v9", &[1, 2]);
    check(&it, "v9", &real(&[1.8540488143993357, 1.350625041650126]), 8.881784197001252e-16);
    assert_eq!(error_of("r=ellipke(1.5);"), "M must be in the range 0 <= M <= 1.");
    assert_eq!(error_of("r=ellipke(1i);"), "Input arguments must be real.");
    check_class(&it, "v12", "single");
    check_dims(&it, "v12", &[1, 1]);
    check(&it, "v12", &real(&[1.854074478149414]), 1e-06);
    check_class(&it, "v13", "double");
    check_dims(&it, "v13", &[1, 8]);
    check(&it, "v13", &real(&[1.5707963267948966, 1.6124413487202194, 1.8540746773013717, 2.578092113348173, 3.6956373629898738, INF, 12.899219785017413, 1.5707963268341665]), 8.881784197001252e-16);
    check_class(&it, "v14", "double");
    check_dims(&it, "v14", &[2, 8]);
    check(&it, "v14", &real(&[1.5707963267948966, 1.5707963267948966, 1.6124413487202194, 1.5307576368977631, 1.8540746773013717, 1.3506438810476753, 2.578092113348173, 1.104774732704073, 3.6956373629898738, 1.0159935450252238, INF, 1.0, 12.899219785017413, 1.0000000006199605, 1.5707963268341665, 1.5707963267556266]), 8.881784197001252e-16);
    check_class(&it, "v15", "double");
    check_dims(&it, "v15", &[1, 1]);
    check(&it, "v15", &real(&[NAN]), 8.881784197001252e-16);
    assert_eq!(error_of("r=ellipke(-1);"), "M must be in the range 0 <= M <= 1.");
    assert_eq!(error_of("r=ellipke(-0.5);"), "M must be in the range 0 <= M <= 1.");
    check_class(&it, "v18", "single");
    check_dims(&it, "v18", &[1, 2]);
    check(&it, "v18", &real(&[1.5707963705062866, INF]), 1.1920928955078125e-07);
    check_class(&it, "v19", "double");
    check_dims(&it, "v19", &[1, 2]);
    check(&it, "v19", &real(&[1.8540488143993357, 1.8540488143993357]), 8.881784197001252e-16);
    check_class(&it, "v20", "double");
    check_dims(&it, "v20", &[0, 0]);
    check(&it, "v20", &real(&[]), 8.881784197001252e-16);
    assert_eq!(error_of("[sn,cn,dn]=ellipj([NaN 1 Inf 1e3 -3 10],[0.5 NaN 0.5 0.3 1 0.9]); r=[sn;cn;dn];"), "M must be in the range 0 <= M <= 1.");
    check_class(&it, "v22", "double");
    check_dims(&it, "v22", &[3, 5]);
    check(&it, "v22", &real(&[0.09970933501024688, 0.9950166071537773, 0.9962647923036629, 0.6143632474844943, 0.7890233204033363, 0.8467103106170548, 0.9407973092127777, 0.3389696490513526, 0.5798062756072047, 0.8966015302833202, -0.44283822767869574, 0.6301422632391882, -0.6055956378870258, -0.7957725324313515, 0.8514343442269373]), 8.881784197001252e-16);
    check_class(&it, "v23", "double");
    check_dims(&it, "v23", &[3, 5]);
    check(&it, "v23", &real(&[0.09966799462495582, 0.9950207489532266, 0.9950207489532266, 0.6043677771171636, 0.796705459992875, 0.796705459992875, 0.9051482536448664, 0.4250960349422805, 0.4250960349422805, 0.9950547536867305, 0.0993279274194332, 0.0993279274194332, 0.9999092042625951, 0.013475282221304556, 0.013475282221304556]), 8.881784197001252e-16);
    check_class(&it, "v24", "double");
    check_dims(&it, "v24", &[3, 6]);
    check(&it, "v24", &real(&[0.7173560908942938, 0.6967067093525494, 0.99999999997427, 0.717303799581581, 0.696760546461858, 0.9997427045290732, 0.7068520536814624, 0.7073614169618662, 0.9487212629857411, 0.6855897751159167, 0.7279880907381018, 0.8473370027054785, 0.6667448349093528, 0.7452860693194929, 0.7600518132769732, 0.6640373122493661, 0.7476994369000407, 0.7477023855760783]), 8.881784197001252e-16);
    check_class(&it, "v25", "single");
    check_dims(&it, "v25", &[1, 3]);
    check(&it, "v25", &real(&[0.8030018210411072, 0.5959765315055847, 0.8231610059738159]), 1e-06);
    check_class(&it, "v26", "double");
    check_dims(&it, "v26", &[6, 2]);
    check(&it, "v26", &real(&[0.8030018248956439, 0.630028998242033, 0.5959765676721407, -0.7765716073705892, 0.8231610016315963, 0.8952830450126207, 0.9946623253580177, -0.28577791571123856, -0.10318361552776205, -0.9582958743998329, 0.7108610477840873, 0.979369946162261]), 8.881784197001252e-16);
    check_class(&it, "v27", "double");
    check_dims(&it, "v27", &[1, 1]);
    check(&it, "v27", &real(&[0.8030018248956439]), 8.881784197001252e-16);
    check_class(&it, "v28", "double");
    check_dims(&it, "v28", &[3, 6]);
    check(&it, "v28", &real(&[0.29552020666133955, 0.955336489125606, 1.0, 0.8414709848078965, 0.5403023058681398, 1.0, 0.5984721441039564, -0.8011436155469337, 1.0, 0.7568024953079282, -0.6536436208636119, 1.0, -0.54402111088937, -0.8390715290764523, 1.0, -0.5063656411097557, 0.8623188722876858, 1.0]), 8.881784197001252e-16);
    check_class(&it, "v29", "double");
    check_dims(&it, "v29", &[3, 6]);
    check(&it, "v29", &real(&[0.29552020666133955, 0.955336489125606, 1.0, 0.8414709848078229, 0.5403023058682545, 0.999999999999646, 0.5984727408482995, -0.8011431697652575, 0.9999998209151733, 0.7510309857821887, -0.6602669599450164, 0.9971757741661951, 0.8588125059527788, -0.5122900346669924, 0.794493889095161, -0.9999695931136021, -0.0077982593068567346, 0.0078046679708824625]), 8.881784197001252e-16);
    check_class(&it, "v30", "double");
    check_dims(&it, "v30", &[3, 6]);
    check(&it, "v30", &real(&[0.29131261245159507, 0.956627911900247, 0.9566279119002914, 0.7615941559558502, 0.6480542736637851, 0.6480542736642326, 0.9866142981516604, 0.1630712319285859, 0.1630712319315703, -0.9993292997393155, 0.03661899346690698, 0.036618993480541054, 0.9999999958779427, 9.079710609203771e-05, 9.080261273803955e-05, -0.9999999535582252, -0.00030476802218102163, 0.00030476966283879593]), 8.881784197001252e-16);
    check_class(&it, "v31", "double");
    check_dims(&it, "v31", &[3, 6]);
    check(&it, "v31", &real(&[0.29131261664447183, 0.956627910623431, 0.9566279549787324, 0.7615942413607136, 0.6480541732958742, 0.6480546208090701, 0.9866145281838538, 0.1630698401806775, 0.16307282478935267, -0.9993295482073223, 0.036612212166831894, 0.03662584796690691, 0.9999964555461508, -0.0026624979127338358, 0.002844097052910794, 0.43931563576591615, 0.8983327736265604, 0.8983328810467848]), 8.881784197001252e-16);
    check_class(&it, "v32", "double");
    check_dims(&it, "v32", &[3, 4]);
    check(&it, "v32", &real(&[0.6362806256364313, 0.771457688690515, 0.9586602073143238, 0.6283244887511652, 0.777951371767919, 0.9176509874316241, 0.6203519088371462, 0.784323599799285, 0.87698238609522, 0.6123654841172452, 0.7905747996627844, 0.8366640969286787]), 8.881784197001252e-16);
    check_class(&it, "v33", "double");
    check_dims(&it, "v33", &[3, 4]);
    check(&it, "v33", &real(&[0.6362806256364313, 0.771457688690515, 0.9586602073143238, 0.6283244887511652, 0.777951371767919, 0.9176509874316241, 0.6203519088371462, 0.784323599799285, 0.87698238609522, 0.6123654841172452, 0.7905747996627844, 0.8366640969286787]), 8.881784197001252e-16);
    check_class(&it, "v34", "double");
    check_dims(&it, "v34", &[3, 2]);
    check(&it, "v34", &real(&[0.6362806256364313, 0.771457688690515, 0.9586602073143238, 0.6043677771171636, 0.796705459992875, 0.796705459992875]), 8.881784197001252e-16);
    check_class(&it, "v35", "double");
    check_dims(&it, "v35", &[3, 2]);
    check(&it, "v35", &real(&[0.6362806256364313, 0.771457688690515, 0.9586602073143238, 0.644217687237691, 0.7648421872844885, 1.0]), 8.881784197001252e-16);
    check_class(&it, "v36", "double");
    check_dims(&it, "v36", &[2, 8]);
    check(&it, "v36", &real(&[1.6596235986105279, 1.4890350580958527, 1.7775193714912534, 1.3993921388974324, 1.9495677498060258, 1.2984280350469133, 2.257205326820854, 1.1784899243278384, 2.9083372484445515, 1.0604737277662786, 4.841132560550295, 1.0021707908344448, 1.570800253807803, 1.5707923997967164, 1.713889448178791, 1.4453630644126652]), 8.881784197001252e-16);
    check_class(&it, "v37", "double");
    check_dims(&it, "v37", &[2, 1]);
    check(&it, "v37", &real(&[1.6596235986105279, 1.4890350580958527]), 8.881784197001252e-16);
    check_class(&it, "v38", "double");
    check_dims(&it, "v38", &[2, 2]);
    check(&it, "v38", &real(&[1.6596235986105279, 1.4890350580958527, INF, 1.0]), 8.881784197001252e-16);
    check_class(&it, "v39", "double");
    check_dims(&it, "v39", &[2, 2]);
    check(&it, "v39", &real(&[1.6596235986105279, 1.4890350580958527, 1.5707963267948966, 1.5707963267948966]), 8.881784197001252e-16);
}

#[test]
fn expint_matches_matlab() {
    let (it, errors) = run(concat!(
        "v1=expint([0 0.1 0.5 1 2 5 10 50 700 800 -1 -0.5 -10 Inf -Inf NaN]);\n",
        "v2=expint([1+1i -1+1i 10i -2-3i 1e-5i]);\n",
        "v3=expint(single([0.5 1]));\n",
        "v4=expint([1e-300 -700 -1e-5 0.037 0.3725 6 12 24 3.9 4 1 0.9 -0.3 -3 -6 -12 -30 -700 -750 1e-5]);\n",
        "v5=expint([0.5+0.1i 3-4i -10+0.1i -10-0.1i -0.1+1e-3i 20+20i 0.2i -5i 100+1i -100+1i]);\n",
        "v6=expint(-1+0i);\n",
        "v7=expint(complex(-1,-0));\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[1, 16]);
    check(&it, "v1", &[(INF, 0.0), (1.8229239584193904, 0.0), (0.5597735947761607, 0.0), (0.21938393439552029, 0.0), (0.04890051070806113, 0.0), (0.0011482955912753329, 0.0), (4.156968929685323e-06, 0.0), (3.7832640295504606e-24, 0.0), (1.4065187662340308e-307, 0.0), (0.0, 0.0), (-1.8951178163559368, -3.141592653589793), (-0.4542199048631736, -3.141592653589793), (-2492.228976241876, -3.141592653589793), (NAN, 0.0), (NAN, -3.141592653589793), (0.0, 0.0)], 1e-14);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[1, 5]);
    check(&it, "v2", &[(0.00028162445198137373, -0.1793245350393589), (-1.764625985563854, -0.7538228020792708), (0.04545643300445558, 0.08755126742397705), (0.36155194459964046, -2.128955782223901), (10.935709800093695, -1.5707863267948965)], 1e-14);
    check_class(&it, "v3", "single");
    check_dims(&it, "v3", &[1, 2]);
    check(&it, "v3", &real(&[0.5597735643386841, 0.21938392519950867]), 1e-06);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[1, 20]);
    check(&it, "v4", &[(690.1983122333121, 0.0), (-1.4509787360525612e+301, -3.141592653589793), (10.935699800043695, -3.141592653589793), (2.756282246084407, 0.0), (0.7507958038725263, 0.0), (0.0003600824521626586, 0.0), (4.7510818246724947e-07, 0.0), (1.5123058939997056e-12, 0.0), (0.00426714528121861, 0.0), (0.003779352409848927, 0.0), (0.21938393439552029, 0.0), (0.2601839393259996, 0.0), (0.302668539265826, -3.141592653589793), (-9.933832570625414, -3.141592653589793), (-85.9897621424392, -3.141592653589793), (-14959.53266639753, -3.141592653589793), (-368973209407.27423, -3.141592653589793), (-1.4509787360525612e+301, -3.141592653589793), (-INF, -3.141592653589793), (10.935719800043694, 0.0)], 8.881784197001252e-16);
    check_class(&it, "v5", "double");
    check_dims(&it, "v5", &[1, 10]);
    check(&it, "v5", &[(0.5419666179101915, -0.11874004792008513), (0.0008639539589796099, -0.008786208377197385), (-2482.323984624916, 216.8221650960111), (-2482.323984624916, -216.8221650960111), (1.622763083777785, -3.1305412777822648), (-2.3824537449367377e-11, -6.696987315652555e-11), (1.0422055956727816, -1.3712402382686628), (0.190029749656644, 0.02086508185022172), (1.9593688389908637e-46, -3.1190439956276912e-46), (-1.4901535515855502e+41, 2.2700035911161186e+41)], 1e-15);
    check_class(&it, "v6", "double");
    check_dims(&it, "v6", &[1, 1]);
    check(&it, "v6", &[(-1.8951178163559366, -3.141592653589793)], 8.881784197001252e-16);
    check_class(&it, "v7", "double");
    check_dims(&it, "v7", &[1, 1]);
    check(&it, "v7", &[(-1.8951178163559366, -3.141592653589793)], 8.881784197001252e-16);
}

#[test]
fn legendre_matches_matlab() {
    let (it, errors) = run(concat!(
        "v1=legendre(0,[0 0.5 1]);\n",
        "v2=legendre(1,[0 0.5 1 -1]);\n",
        "v3=legendre(2,[0 0.5 1 -0.3]);\n",
        "v4=legendre(3,[0.1 0.7]);\n",
        "v5=legendre(3,[0.1 0.7],'sch');\n",
        "v6=legendre(3,[0.1 0.7],'norm');\n",
        "v7=legendre(2,[0.2 0.4; 0.6 0.8]);\n",
        "v8=size(legendre(2,[0.2 0.4; 0.6 0.8]));\n",
        "v9=legendre(2,0.5);\n",
        "v10=legendre(20,0.3);\n",
        "v14=legendre(30,[0.1 0.9],'norm');\n",
        "v15=legendre(1,[0.3 -0.7 0.9]);\n",
        "v16=legendre(2,[0.1 0.3 0.9 -1 1]);\n",
        "v17=legendre(4,[0 0.25 0.999]);\n",
        "v18=legendre(5,0.6,'sch');\n",
        "v19=legendre(2,single(0.5));\n",
        "v20=legendre(0,[1 2;3 4]/5);\n",
        "v21=legendre(1,[0.5;0.6]);\n",
        "v23=legendre(1,0,'sch');\n",
        "v24=legendre(0,0.5,'norm');\n",
        "v25=legendre(2,1,'norm');\n",
        "v26=legendre(2,-1,'sch');\n",
        "v27=legendre(3,[0 1e-10 -0.5],'norm');\n",
        "v28=legendre(6,[0.15 0.85]);\n",
        "v31=legendre(2,NaN);\n",
        "v32=legendre(1,[-0.95 -0.7 -0.5 -0.3 -0.1 0 0.05 0.2 0.4 0.5 0.6 0.8 0.9 0.99 1 -1 0.123 0.777]);\n",
        "v33=legendre(2,[-0.95 -0.7 -0.5 -0.3 -0.1 0 0.05 0.2 0.4 0.5 0.6 0.8 0.9 0.99 1 -1 0.123 0.777]);\n",
        "v34=legendre(3,[-0.95 -0.7 -0.5 -0.3 -0.1 0 0.05 0.2 0.4 0.5 0.6 0.8 0.9 0.99 1 -1 0.123 0.777]);\n",
        "v35=legendre(4,[-0.95 -0.7 -0.5 -0.3 -0.1 0 0.05 0.2 0.4 0.5 0.6 0.8 0.9 0.99 1 -1 0.123 0.777]);\n",
        "v36=legendre(5,[-0.95 -0.7 -0.5 -0.3 -0.1 0 0.05 0.2 0.4 0.5 0.6 0.8 0.9 0.99 1 -1 0.123 0.777]);\n",
        "v37=legendre(6,[-0.95 -0.7 -0.5 -0.3 -0.1 0 0.05 0.2 0.4 0.5 0.6 0.8 0.9 0.99 1 -1 0.123 0.777]);\n",
        "v38=legendre(2,[-0.95 -0.7 -0.3 0 0.2 0.5 0.8 0.99 1 -1],'sch');\n",
        "v39=legendre(2,[-0.95 -0.7 -0.3 0 0.2 0.5 0.8 0.99 1 -1],'norm');\n",
        "v40=legendre(3,[-0.95 -0.7 -0.3 0 0.2 0.5 0.8 0.99 1 -1],'sch');\n",
        "v41=legendre(3,[-0.95 -0.7 -0.3 0 0.2 0.5 0.8 0.99 1 -1],'norm');\n",
        "v42=legendre(5,[-0.95 -0.7 -0.3 0 0.2 0.5 0.8 0.99 1 -1],'sch');\n",
        "v43=legendre(5,[-0.95 -0.7 -0.3 0 0.2 0.5 0.8 0.99 1 -1],'norm');\n",
        "v44=legendre(12,[0.3 0.95 -0.6]);\n",
        "v45=legendre(40,[0.3 0.95]);\n",
        "v46=legendre(200,0.5);\n",
        "v47=legendre(200,0.5,'norm');\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[1, 3]);
    check(&it, "v1", &real(&[1.0, 1.0, 1.0]), 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[2, 4]);
    check(&it, "v2", &real(&[0.0, -1.0000000000000002, 0.5000000000000001, -0.8660254037844388, 1.0, 0.0, -1.0, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[3, 4]);
    check(&it, "v3", &real(&[-0.4999999999999999, -0.0, 3.0, -0.12499999999999989, -1.299038105676658, 2.2499999999999996, 1.0, 0.0, 0.0, -0.36499999999999994, 0.8585452812752511, 2.73]), 8.881784197001252e-16);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[4, 2]);
    check(&it, "v4", &real(&[-0.14750000000000002, 1.417857097876933, 1.4850000000000003, -14.775563441033308, -0.19250000000000006, -1.5532606832080695, 5.355000000000001, -5.46319274783528]), 8.881784197001252e-16);
    check_class(&it, "v5", "double");
    check_dims(&it, "v5", &[4, 2]);
    check(&it, "v5", &real(&[-0.14750000000000002, -0.5788377363303121, 0.19171267563726715, 0.7787405697663375, -0.19250000000000006, 0.6341160185644263, 0.6913275272980239, 0.2879355396612235]), 8.881784197001252e-16);
    check_class(&it, "v6", "double");
    check_dims(&it, "v6", &[4, 2]);
    check(&it, "v6", &real(&[-0.27594723227457824, -0.7657303498947914, 0.25361203145750005, 1.0301769417192368, -0.360134523476992, 0.8388566437419447, 0.9145403558618942, 0.3809029157803862]), 8.881784197001252e-16);
    check_class(&it, "v7", "double");
    check_dims(&it, "v7", &[3, 2, 2]);
    check(&it, "v7", &real(&[-0.43999999999999984, -0.5878775382679627, 2.88, 0.03999999999999987, -1.44, 1.9200000000000002, -0.2599999999999999, -1.0998181667894016, 2.52, 0.46, -1.4399999999999995, 1.0799999999999994]), 8.881784197001252e-16);
    check_class(&it, "v8", "double");
    check_dims(&it, "v8", &[1, 3]);
    check(&it, "v8", &real(&[3.0, 2.0, 2.0]), 8.881784197001252e-16);
    check_class(&it, "v9", "double");
    check_dims(&it, "v9", &[3, 1]);
    check(&it, "v9", &real(&[-0.12499999999999989, -1.299038105676658, 2.2499999999999996]), 8.881784197001252e-16);
    check_class(&it, "v10", "double");
    check_dims(&it, "v10", &[21, 1]);
    check(&it, "v10", &real(&[0.1802871594799804, -0.15804468835344446, -75.62120147139655, 161.18975033632967, 31003.026421155544, -143765.42411270976, -11949089.226035468, 101162291.97780162, 4071358761.182275, -57309203820.09089, -1092420454297.7133, 25783044046020.965, 160265711653642.22, -8635146315939355.0, 2.829617710125849e+16, 1.8060002146970107e+18, -2.2981020952914924e+19, -9.380974299109958e+19, 4.4042522783977775e+21, -3.916832680887205e+22, 1.2454734132297784e+23]), 1e-14);
    assert_eq!(error_of("r=legendre(2,1.5);"), "X must be real and in the range (-1,1).");
    assert_eq!(error_of("r=legendre(-1,0.5);"), "N must be a positive scalar integer.");
    assert_eq!(error_of("r=legendre(2,0.5,'foo');"), "Normalization option foo not recognized");
    check_class(&it, "v14", "double");
    check_dims(&it, "v14", &[31, 2]);
    check(&it, "v14", &real(&[0.7968764798367288, 0.0700920939517696, -0.7972722253457314, -0.08078783023597114, 0.7982655495775528, 0.10241861028247545, -0.7992569546963124, -0.13545601932458196, 0.799185963554757, 0.18058711385415355, -0.7964312566201971, -0.23868152964214037, 0.7886530316830629, 0.31073062692424314, -0.7725539350955758, -0.3977369916015917, 0.7435162920739692, 0.5005137744437679, -0.6950360988771387, -0.6193139550069706, 0.6177908937054124, 0.7531185966703884, -0.4979658986598524, -0.8981739877276828, 0.3138185963603413, 1.0446173215337415, -0.02697621130207052, -1.1669642728572507, -0.44925053414861144, 1.1843044097609567, 1.521266558731822, 1.1145453966010628, 0.5041902350557834, -1.0473992898266915, -0.7898588143956842, 0.727859301880658, 1.1918727779943017, 0.08841130988727654, -1.1263886307221231, -1.190468956003581, -0.2057990945419549, 0.939254768001905, 1.5627724417338296, 1.5608294286122524, 1.1932930791768195, 0.7545370730462941, 0.40941657709273194, 0.19468014175336923, 0.08219244389362028, 0.031071653457970756, 0.01057421521881953, 0.003249489564698385, 0.0009027481415739766, 0.00022658313969468752, 5.125182563367305e-05, 1.0398467967862822e-05, 1.87812287660906e-06, 2.9847074128920033e-07, 4.097911502334619e-08, 4.715366780574542e-09, 4.294600278903347e-10, 2.6852295253896313e-11]), 8.881784197001252e-16);
    check_class(&it, "v15", "double");
    check_dims(&it, "v15", &[2, 3]);
    check(&it, "v15", &real(&[0.3, -0.9539392014169458, -0.7, -0.7141428428542851, 0.9, -0.43588989435406733]), 8.881784197001252e-16);
    check_class(&it, "v16", "double");
    check_dims(&it, "v16", &[3, 5]);
    check(&it, "v16", &real(&[-0.4849999999999999, -0.298496231131986, 2.9699999999999998, -0.36499999999999994, -0.8585452812752511, 2.73, 0.715, -1.1769027147559816, 0.5699999999999997, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v17", "double");
    check_dims(&it, "v17", &[5, 3]);
    check(&it, "v17", &real(&[0.3749999999999999, 0.0, -7.499999999999999, -0.0, 105.00000000000001, 0.15771484375000003, 1.5507062226025794, -3.9550781250000018, -23.82792488389329, 92.28515625000003, 0.990022482504375, -0.44509216662251744, 0.08974520994749878, -0.009375058329123059, 0.0004195801049999887]), 8.881784197001252e-16);
    check_class(&it, "v18", "double");
    check_dims(&it, "v18", &[6, 1]);
    check(&it, "v18", &real(&[-0.15263999999999972, -0.5106141243639858, 0.07869658188256935, 0.5997179070196251, 0.5452259128104607, 0.22988742984339103]), 8.881784197001252e-16);
    check_class(&it, "v19", "single");
    check_dims(&it, "v19", &[3, 1]);
    check(&it, "v19", &real(&[-0.1250000149011612, -1.299038052558899, 2.25]), 1e-06);
    check_class(&it, "v20", "double");
    check_dims(&it, "v20", &[2, 2]);
    check(&it, "v20", &real(&[1.0, 1.0, 1.0, 1.0]), 8.881784197001252e-16);
    check_class(&it, "v21", "double");
    check_dims(&it, "v21", &[2, 2]);
    check(&it, "v21", &real(&[0.5000000000000001, -0.8660254037844388, 0.6, -0.8000000000000002]), 8.881784197001252e-16);
    assert_eq!(error_of("r=legendre(2,[]);"), "Operands to the logical AND (&&) and OR (||) operators must be convertible to logical scalar values. Use the ANY or ALL functions to reduce operands to logical scalar values.");
    check_class(&it, "v23", "double");
    check_dims(&it, "v23", &[2, 1]);
    check(&it, "v23", &real(&[0.0, 1.0000000000000002]), 8.881784197001252e-16);
    check_class(&it, "v24", "double");
    check_dims(&it, "v24", &[1, 1]);
    check(&it, "v24", &real(&[0.7071067811865475]), 8.881784197001252e-16);
    check_class(&it, "v25", "double");
    check_dims(&it, "v25", &[3, 1]);
    check(&it, "v25", &real(&[1.5811388300841898, -0.0, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v26", "double");
    check_dims(&it, "v26", &[3, 1]);
    check(&it, "v26", &real(&[1.0, -0.0, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v27", "double");
    check_dims(&it, "v27", &[4, 3]);
    check(&it, "v27", &real(&[-0.0, -0.8100925873009824, 0.0, 1.0458250331675945, -2.806243040080457e-10, -0.8100925873009824, 2.5617376914898997e-10, 1.0458250331675945, 0.8184875533567998, 0.1753901900050288, -0.9606516343087125, 0.6792832849776299]), 8.881784197001252e-16);
    check_class(&it, "v28", "double");
    check_dims(&it, "v28", &[7, 2]);
    check(&it, "v28", &real(&[-0.17464609472656256, -1.6902050173330987, 7.848000029296878, 62.84551564117877, -339.73621910156265, -1473.0166908594547, 9709.006500703128, -0.4029956650390628, -0.6478789362868101, 19.016612138671885, -96.82327490062984, 252.7879355859377, -358.42714094088655, 222.1331913281252]), 8.881784197001252e-16);
    assert_eq!(error_of("r=legendre(1.5,0.5);"), "N must be a positive scalar integer.");
    assert_eq!(error_of("r=legendre(2,0.5i);"), "X must be real and in the range (-1,1).");
    check_class(&it, "v31", "double");
    check_dims(&it, "v31", &[3, 1]);
    check(&it, "v31", &real(&[0.0, 0.0, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v32", "double");
    check_dims(&it, "v32", &[2, 18]);
    check(&it, "v32", &real(&[-0.95, -0.31224989991992, -0.7, -0.7141428428542851, -0.5000000000000001, -0.8660254037844388, -0.3, -0.9539392014169458, -0.09999999999999999, -0.9949874371066201, 0.0, -1.0000000000000002, 0.049999999999999996, -0.9987492177719091, 0.2, -0.9797958971132713, 0.4, -0.9165151389911681, 0.5000000000000001, -0.8660254037844388, 0.6, -0.8000000000000002, 0.8, -0.6, 0.9, -0.43588989435406733, 0.9900000000000001, -0.14106735979665896, 1.0, 0.0, -1.0, 0.0, 0.12300000000000001, -0.9924066706748804, 0.777, -0.6295005957105998]), 8.881784197001252e-16);
    check_class(&it, "v33", "double");
    check_dims(&it, "v33", &[3, 18]);
    check(&it, "v33", &real(&[0.8537499999999996, 0.8899122147717718, 0.2925000000000001, 0.23499999999999993, 1.4996999699939986, 1.53, -0.12499999999999989, 1.299038105676658, 2.2499999999999996, -0.36499999999999994, 0.8585452812752511, 2.73, -0.4849999999999999, 0.298496231131986, 2.9699999999999998, -0.4999999999999999, -0.0, 3.0, -0.49625, -0.14981238266578636, 2.9925000000000006, -0.43999999999999984, -0.5878775382679627, 2.88, -0.2599999999999999, -1.0998181667894016, 2.52, -0.12499999999999989, -1.299038105676658, 2.2499999999999996, 0.03999999999999987, -1.44, 1.9200000000000002, 0.46, -1.4399999999999995, 1.0799999999999994, 0.715, -1.1769027147559816, 0.5699999999999997, 0.9701499999999998, -0.41897005859607706, 0.05970000000000007, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, -0.47730649999999997, -0.36619806147903083, 2.9546130000000006, 0.4055935000000001, -1.467365888601408, 1.1888129999999997]), 8.881784197001252e-16);
    check_class(&it, "v34", "double");
    check_dims(&it, "v34", &[4, 18]);
    check(&it, "v34", &real(&[-0.7184374999999998, -1.6451666602030783, -1.389375000000001, -0.45666547863288315, 0.19250000000000006, -1.5532606832080695, -5.355000000000001, -5.46319274783528, 0.43750000000000006, -0.32475952641916506, -5.625000000000002, -9.742785792574935, 0.38250000000000006, 0.7869998411689797, -4.0950000000000015, -13.021270099341308, 0.14750000000000002, 1.417857097876933, -1.4850000000000003, -14.775563441033308, -0.0, 1.4999999999999998, 0.0, -15.000000000000002, -0.07468749999999999, 1.4793972788246395, 0.7481250000000002, -14.943785170912188, -0.28000000000000014, 1.175755076535925, 2.8800000000000012, -14.109060918431107, -0.44, 0.27495454169735023, 5.040000000000001, -11.548090751288717, -0.43750000000000006, -0.32475952641916506, 5.625000000000002, -9.742785792574935, -0.36000000000000026, -0.9599999999999997, 5.7600000000000025, -7.680000000000002, 0.08000000000000027, -1.9800000000000006, 4.32, -3.2399999999999984, 0.4725000000000003, -1.9941962666698585, 2.5650000000000004, -1.2422861989090914, 0.9407475000000001, -0.8253498553303023, 0.29551500000000047, -0.04210860689930275, 1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, -0.17984783250000005, 1.3760041021075182, 1.817086995000001, -14.660888252313603, 0.007243582500000157, -1.9061073450423358, 4.618538505, -3.7417924584425246]), 8.881784197001252e-16);
    check_class(&it, "v35", "double");
    check_dims(&it, "v35", &[5, 18]);
    check(&it, "v35", &real(&[0.5540898437499996, 2.4602364770877934, 3.888421875, 3.0368254329086724, 0.9981562500000009, -0.41206249999999983, 0.5373924892478488, 9.294749999999997, 26.76964446439287, 27.310499999999998, -0.28906250000000006, -1.3531646934131858, 4.218750000000001, 34.09975027401227, 59.06249999999999, 0.07293750000000003, -1.6956269305186216, -2.5252500000000015, 27.34466720861675, 86.95050000000002, 0.3379375, -0.7288282976805994, -6.905250000000001, 10.342894408723316, 102.91050000000003, 0.3749999999999999, 0.0, -7.499999999999999, -0.0, 105.00000000000001, 0.3656523437499999, 0.3723461927505899, -7.350328125, -5.230324809819265, 104.47565625000001, 0.23199999999999987, 1.332522420074049, -5.183999999999999, -19.752685285803548, 96.768, -0.11299999999999999, 1.7230484613033963, 0.7559999999999997, -32.33465410360841, 74.08800000000001, -0.28906250000000006, 1.3531646934131858, 4.218750000000001, -34.09975027401227, 59.06249999999999, -0.40799999999999986, 0.5760000000000011, 7.295999999999997, -32.256, 43.00800000000002, -0.2329999999999995, -1.7760000000000005, 9.395999999999995, -18.14399999999999, 13.60799999999999, 0.20793750000000036, -2.61860854033206, 6.654749999999998, -7.826403053127275, 3.790499999999997, 0.9022325437499998, -1.347931421018229, 0.8747094750000011, -0.29181264581216804, 0.04158105000000011, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.31926762905437506, 0.8831772592768992, -6.604276548652502, -12.623024785242011, 101.84694309730504, -0.29434566369562465, -1.4992847650932106, 9.588082964347496, -20.35160918146889, 16.48822407130499]), 8.881784197001252e-16);
    check_class(&it, "v36", "double");
    check_dims(&it, "v36", &[6, 18]);
    check(&it, "v36", &real(&[-0.3727436328124995, -3.2022971445213106, -8.30325234375, -11.384099550469482, -8.534235937500007, -2.805067702502486, 0.3651987499999999, 1.0951826834447247, -8.808974999999997, -65.20320544541407, -172.05615, -175.53238298794759, -0.0898437499999999, 1.9282596881137883, 4.921874999999997, -42.62468784251533, -265.78124999999994, -460.3466286991656, -0.3453862500000001, 0.1607983766388439, 10.462725000000004, 8.659144616061974, -234.7663500000001, -746.5094147952375, -0.17882874999999998, -1.608335005368032, 5.041575, 47.06016955969109, -92.61945000000001, -921.5518918172476, 0.0, -1.8749999999999998, -0.0, 52.50000000000001, 0.0, -945.0000000000002, 0.09265871093750001, -1.8073576518463585, -2.5987992187500004, 51.12642501598332, 47.01404531250001, -939.1048196030494, 0.30751999999999996, -0.8700587566365844, -8.8704, 31.604296457285667, 174.18240000000003, -853.3160043467133, 0.27064000000000005, 1.2070504380513682, -9.172800000000002, -17.78405975698462, 266.71680000000003, -611.1249625581991, 0.0898437499999999, 1.9282596881137883, -4.921874999999997, -42.62468784251533, 265.78124999999994, -460.3466286991656, -0.15263999999999972, 1.9775999999999998, 1.6127999999999927, -60.21119999999998, 232.24320000000006, -309.6576000000001, -0.39951999999999965, -0.7218000000000007, 13.910399999999994, -53.978399999999965, 97.97759999999992, -73.48319999999993, -0.041141249999999734, -2.8099369608350986, 12.837824999999999, -27.348930668983648, 30.703049999999987, -14.87016580094182, 0.8551803929624997, -1.9708299211552267, 2.006857140750003, -1.1526452129456488, 0.37048715550000105, -0.05279156046965594, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.21456411907263864, -1.4755858211295165, -6.071152036452773, 44.32626466446616, 112.74456600871666, -909.6622714584942, -0.41746671124470086, -0.23849040927128665, 13.11274437989402, -58.06312789845715, 115.30215093063582, -93.41412187586708]), 8.881784197001252e-16);
    check_class(&it, "v37", "double");
    check_dims(&it, "v37", &[7, 18]);
    check(&it, "v37", &real(&[0.18745362402343704, 3.740517259544188, 14.887508466796877, 31.556412279712244, 40.09967964843753, 29.312957491150982, 9.63470320312501, -0.12528631249999983, -2.331452319602294, 0.6914643749999952, 95.96917540484843, 539.5189274999999, 1351.5993490071962, 1378.9071449999997, 0.3232421875, -0.4972880248293461, -14.150390625000002, -12.787406352754587, 465.1171875000001, 2531.9064578454113, 4385.390624999998, 0.12918118750000007, 1.9286253880407078, -4.212560625000004, -82.44417163397951, -3.912772499999931, 2463.4810688242833, 7833.370545, -0.24882931250000004, 1.2284276583976859, 10.697754375, -44.83644726181558, -412.1565525, 1013.7070809989724, 10086.258105, -0.31250000000000006, -0.0, 13.125000000000002, 0.0, -472.50000000000006, -0.0, 10395.0, -0.2962165712890625, -0.6456247730038073, 12.505739326171875, 23.320710745781653, -457.2115906640625, -516.5076507816773, 10317.232243828126, -0.08057599999999993, -1.9818527570089557, 4.193279999999996, 75.85031149748563, -243.8553599999999, -1877.2952095627693, 9196.830719999998, 0.292636, -1.005453768078871, -11.413080000000003, 60.14245663271164, 253.38096000000002, -2688.949835256075, 6161.158079999999, 0.3232421875, 0.4972880248293461, -14.150390625000002, 12.787406352754587, 465.1171875000001, -2531.9064578454113, 4385.390624999998, 0.1720960000000002, 1.919232, -10.10688000000001, -46.44863999999998, 572.8665600000002, -2043.7401600000012, 2724.9868800000013, -0.39179600000000003, 0.8608319999999982, 14.159880000000008, -109.95263999999999, 369.8654399999998, -646.6521599999994, 484.98911999999933, -0.2411643124999999, -2.4213449340550244, 20.127804375000004, -69.38106306597331, 134.92284749999993, -147.21464142932402, 71.29930499999993, 0.8002919601019374, -2.67494986305421, 3.932926984441882, -3.4059350674935924, 1.8301878366975053, -0.5749000935145532, 0.08191882660500033, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.21767214869443244, -1.459106234329926, 9.503916783811725, 53.65254479098627, -382.0395450329757, -1230.7730532833427, 9930.303968823404, -0.3493932770883893, 1.391466212503515, 11.239511366130436, -111.15089356545406, 418.5467336811997, -798.410499673036, 646.8466990550497]), 1e-13);
    check_class(&it, "v38", "double");
    check_dims(&it, "v38", &[3, 10]);
    check(&it, "v38", &real(&[0.8537499999999996, -0.5137910567536186, 0.0844374768689828, 0.23499999999999993, -0.8658521813797088, 0.4416729559300637, -0.36499999999999994, -0.49568134925574914, 0.7880831174438391, -0.4999999999999999, 0.0, 0.8660254037844386, -0.43999999999999984, 0.33941125496954283, 0.831384387633061, -0.12499999999999989, 0.75, 0.6495190528383289, 0.46, 0.8313843876330609, 0.31176914536239775, 0.9701499999999998, 0.24189247611283834, 0.01723390553531035, 1.0, -0.0, 0.0, 1.0, -0.0, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v39", "double");
    check_dims(&it, "v39", &[3, 10]);
    check(&it, "v39", &real(&[1.3498972761843764, -0.5744358645662717, 0.09440396906380581, 0.37156762506978447, -0.9680521680157531, 0.49380537664144564, -0.5771156729807292, -0.554188596057335, 0.8811037112621872, -0.7905694150420947, 0.0, 0.9682458365518541, -0.6957010852370432, 0.37947331922020555, 0.9295160030897799, -0.19764235376052355, 0.8385254915624211, 0.7261843774138905, 0.7273238618387273, 0.9295160030897798, 0.34856850115866733, 1.5339418360061765, 0.27044400991702533, 0.019268092147381923, 1.5811388300841898, -0.0, 0.0, 1.5811388300841898, -0.0, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v40", "double");
    check_dims(&it, "v40", &[4, 10]);
    check(&it, "v40", &real(&[-0.7184374999999998, 0.6716364765560497, -0.17936754122123108, 0.02406838402084778, 0.19250000000000006, 0.6341160185644263, -0.6913275272980239, 0.2879355396612235, 0.38250000000000006, -0.32129133975256774, -0.5286622267573126, 0.6862811923694252, -0.0, -0.6123724356957946, 0.0, 0.7905694150420949, -0.28000000000000014, -0.4799999999999999, 0.37180640123591213, 0.7436128024718242, -0.43750000000000006, 0.1325825214724779, 0.7261843774138909, 0.5134898976610932, 0.08000000000000027, 0.8083316151184491, 0.557709601853868, 0.1707629936490924, 0.9407475000000001, 0.3369476674731926, 0.03815082245181622, 0.0022193184483079533, 1.0, -0.0, 0.0, -0.0, -1.0, -0.0, 0.0, -0.0]), 8.881784197001252e-16);
    check_class(&it, "v41", "double");
    check_dims(&it, "v41", &[4, 10]);
    check(&it, "v41", &real(&[-1.3440734894052013, 0.8884915442034854, -0.23728095367425206, 0.03183947928918203, 0.360134523476992, 0.8388566437419447, -0.9145403558618942, 0.3809029157803862, 0.7155919752205164, -0.4250284916920274, -0.6993543897767427, 0.9078646822351886, -0.0, -0.8100925873009824, 0.0, 1.0458250331675945, -0.523832034148352, -0.6349803146555015, 0.4918536367660608, 0.9837072735321215, -0.8184875533567998, 0.1753901900050288, 0.9606516343087125, 0.6792832849776299, 0.14966629547095814, 1.0693222152372972, 0.737780455149091, 0.22589820716420025, 1.7599774162320594, 0.44573986648867747, 0.05046879426004258, 0.0029358823471402995, 1.8708286933869707, -0.0, 0.0, -0.0, -1.8708286933869707, -0.0, 0.0, -0.0]), 8.881784197001252e-16);
    check_class(&it, "v42", "double");
    check_dims(&it, "v42", &[6, 10]);
    check(&it, "v42", &real(&[-0.3727436328124995, 0.8268295673559068, -0.40515722839878515, 0.11338834561859946, -0.020035405037319837, 0.002082460771720126, 0.3651987499999999, -0.28277495293574456, -0.4298339672550901, 0.6494394714055585, -0.40392774229085515, 0.1303138963144616, -0.3453862500000001, -0.04151796232129535, 0.5105287045370223, -0.08624714481234297, -0.5511493876932894, 0.5542028702708119, 0.0, 0.48412291827592707, -0.0, -0.5229125165837972, 0.0, 0.7015607600201141, 0.30751999999999996, 0.22464820497836158, -0.4328312003541334, -0.31478632753027874, 0.4089194346078455, 0.6334952640706953, 0.0898437499999999, -0.4978745106151874, -0.24016290857717792, 0.4245520531110186, 0.6239615396237875, 0.34175781026760194, -0.39951999999999965, 0.1863679586195011, 0.6787580187371635, 0.5376377330507965, 0.23001718196691287, 0.054553364699164004, 0.8551803929624997, 0.5088660975227647, 0.09792460150276061, 0.011480621124374783, 0.0008697744327585871, 3.9192050037819706e-05, 1.0, -0.0, 0.0, -0.0, 0.0, -0.0, -1.0, -0.0, 0.0, -0.0, 0.0, -0.0]), 8.881784197001252e-16);
    check_class(&it, "v43", "double");
    check_dims(&it, "v43", &[6, 10]);
    check(&it, "v43", &real(&[-0.8741613048587927, 1.3711417202457152, -0.6718772538495479, 0.18803329900801655, -0.03322496051579321, 0.0034533705102148034, 0.8564669862339082, -0.46892920949913586, -0.7127989957675215, 1.076973525349491, -0.6698383817970687, 0.21610114952217327, -0.8100025551131578, -0.06884975153992479, 0.8466160788277577, -0.1430247092909944, -0.9139778612063812, 0.9190414892131463, 0.0, 0.8028270361665705, -0.0, -0.8671523078444755, 0.0, 1.1634069043116428, 0.7211983272304504, 0.37253690287003766, -0.7177693445669019, -0.5220140687759285, 0.678116167039247, 1.0505330486948041, 0.2107022704608179, -0.8256314721961966, -0.3982651281554629, 0.7040399320721433, 1.0347231552722882, 0.5667412129155528, -0.9369574521823275, 0.3090562958426833, 1.1255928357980958, 0.8915713168333753, 0.3814403439595761, 0.09046652087927325, 2.005575796321651, 0.8438589570077049, 0.16238958046486474, 0.019038456314889644, 0.001442357722852218, 6.499266237014107e-05, 2.345207879911715, -0.0, 0.0, -0.0, 0.0, -0.0, -2.345207879911715, -0.0, 0.0, -0.0, 0.0, -0.0]), 8.881784197001252e-16);
    check_class(&it, "v44", "double");
    check_dims(&it, "v44", &[13, 3]);
    check(&it, "v44", &real(&[-0.18100217969140722, -1.766384015470839, 29.34734417940715, 235.10588730091112, -4845.725912937949, -21663.96537746722, 727148.7433849638, -14472.768673807854, -82831236.09331208, 418234775.12443197, 4590306297.893399, -56475186086.15719, 179579646383.00726, -0.40230527546437184, 0.6692312544968022, 58.68743789289504, -817.272455374347, 6115.8738716380285, -31169.952405559485, 116566.67070721593, -328344.1137253632, 696917.9131776337, -1090826.9798755215, 1196757.052698973, -826528.3409390486, 271666.72814652143, -0.04941413990399974, -3.113163915264, 3.0388599521279582, 488.5438228070398, 1742.618209812485, -59894.60122533885, -686205.5857245395, 1370869.482871842, 92621566.34275182, 974371847.8258374, 5373808372.857649, 16298583636.35947, 21731444848.479294]), 1e-12);
    check_class(&it, "v45", "double");
    check_dims(&it, "v45", &[41, 2]);
    check(&it, "v45", &real(&[0.12511584585570795, -1.1813780250506072, -204.44693480130286, 2192.2795510326714, 329929.65132516314, -4399095.710687072, -520651519.1603593, 9047392028.113516, 792167303343.4634, -18317070439292.664, -1138430192791290.8, 3.555185383352075e+16, 1.4958262976498522e+18, -6.490217016956211e+19, -1.68912573101142e+21, 1.0950111719276477e+23, 1.382354548530765e+24, -1.67212936666975e+26, -1.0313580507041542e+26, 2.2422970707999176e+29, -2.545774986388501e+30, -2.505050631295454e+32, 6.414613789323665e+33, 2.0633364509877025e+35, -1.0259062791276949e+37, -6.962735839920124e+37, 1.1764264863391302e+40, -1.2345280253977827e+41, -8.938378486417994e+42, 2.6654771686601304e+44, 2.5391054073034003e+45, -2.5315244453849618e+47, 3.133226324006118e+48, 1.0098012206785305e+50, -3.9257516025459366e+51, 3.1644536673639865e+52, 1.0699659739842572e+54, -3.625211263284281e+55, 5.141068673944502e+56, -3.80461952872273e+57, 1.2097919049750259e+58, 0.19683281675736458, 4.0586720246879535, -347.5023104941961, -2419.0921163335993, 611978.3573574314, -10956952.64352083, -658046785.2730548, 41665467996.30602, -723143885534.789, -30796209927007.33, 2820407974532057.5, -1.2388402894994478e+17, 3.976775910792629e+18, -1.0356085016292934e+20, 2.2904703734628483e+21, -4.412927793830747e+22, 7.524410548053859e+23, -1.1475105033877935e+25, 1.5767748695737298e+26, -1.9622556248539566e+27, 2.2195953431421275e+28, -2.287476668210687e+29, 2.150832190738617e+30, -1.8460995125312487e+31, 1.4460960410396297e+32, -1.0327721656787674e+33, 6.713116913729961e+33, -3.9615065439223604e+34, 2.1151429615479567e+35, -1.0173170981618173e+36, 4.38318464065553e+36, -1.6799075711682532e+37, 5.6764770801337695e+37, -1.6719868552765024e+38, 4.229877595724749e+38, -9.011466337348006e+38, 1.5731073933244755e+39, -2.161813818863545e+39, 2.1940843490934207e+39, -1.462838693052876e+39, 4.8081182684710225e+38]), 1e-14);
    check_class(&it, "v46", "double");
    check_dims(&it, "v46", &[201, 1]);
    check(&it, "v46", &real(&[-0.015650531003771034, -11.723333877625691, 642.6882862916896, 469770.35019241535, -27459545.208053544, -18752300626.99123, 1211590984608.7734, 744885767612898.4, -5.4675910738174696e+16, -2.9397619826526233e+19, 2.4995439730682993e+21, 1.1502762835279212e+24, -1.148171889629285e+26, -4.449831648946851e+28, 5.265708504927181e+30, 1.6956091990373246e+33, -2.399444959371761e+35, -6.332351897529471e+37, 1.082353875847788e+40, 2.3012654374319125e+42, -4.818927841869789e+44, -8.050755257170315e+46, 2.112189935521899e+49, 2.6626408158389376e+51, -9.091273870279379e+53, -8.03739058682266e+55, 3.832163933574846e+58, 2.0282875202016917e+60, -1.5768638571556424e+63, -2.902121769220022e+64, 6.308132726076098e+67, -1.0437967846561195e+69, -2.4398402675448574e+72, 1.3107830018858663e+74, 9.051035336745507e+76, -8.675697842687676e+78, -3.180184730922572e+81, 4.70029470748162e+83, 1.0352591865319886e+86, -2.2776897780396295e+88, -2.982597169358874e+90, 1.0178595925235976e+93, 6.682070305173745e+94, -4.240583327624925e+97, -4.5997441684465696e+98, 1.6478525002444905e+102, -6.804470540228306e+103, -5.921833796807623e+106, 5.802118500574163e+108, 1.9253945667557296e+111, -3.285380340081719e+113, -5.371549265766395e+115, 1.5532747703254349e+118, 1.0842543672662091e+120, -6.4796362738664e+122, -8.087629773528185e+121, 2.4128822182354136e+127, -1.5572454538356724e+129, -7.90464777980399e+131, 1.1039251984663394e+134, 2.1550965780666031e+136, -5.5400844849492726e+138, -3.972468388418515e+140, 2.3019826737105116e+143, -2.32835721691525e+144, -8.15374339303387e+147, 6.958975666708864e+149, 2.3976640778099496e+152, -4.3447335950399275e+154, -5.1347648084570124e+156, 1.9518369874523305e+159, 2.3851525858342543e+160, -7.071865432018103e+163, 5.042536172304264e+165, 2.046141452558576e+168, -3.503085408521865e+170, -4.05611917770894e+172, 1.5645182938055926e+175, 2.1522257151083323e+174, -5.351652291268559e+179, 4.8745233218485445e+181, 1.3628506199553325e+184, -2.9183735832475915e+186, -1.8101791212765218e+188, 1.1480495032794742e+191, -5.120625203814242e+192, -3.292864620068918e+195, 4.9541280221935704e+197, 5.796729627790275e+199, -2.201298263882933e+202, 3.8595241256749375e+203, 6.684885598716753e+206, -8.259760982484705e+208, -1.2502107466505366e+211, 3.956284455985224e+213, -3.61311397015885e+214, -1.1974955352441823e+218, 1.439734371875616e+220, 2.08623601656418e+222, -6.7799189755176025e+224, 1.387885726968471e+226, 1.8940562099678976e+229, -2.6266920064417128e+231, -2.5691457290874072e+233, 1.0855291019176563e+236, -5.4600978735379466e+237, -2.516427525226239e+240, 4.66731388309885e+242, 1.495302208946733e+244, -1.5233810253715676e+247, 1.492278778307701e+249, 2.4020092529769673e+251, -7.255585829624201e+253, 2.7134930337043576e+255, 1.6444188399512258e+258, -2.9059169315337945e+260, -5.959472153012981e+261, 8.603535939710449e+264, -1.00364856255079e+267, -9.032993040124409e+268, 3.8665617391866665e+271, -3.0163133102983117e+273, -5.714974836211792e+275, 1.572378091337797e+278, -7.93389008947097e+279, -2.786771734708047e+282, 5.98202938010865e+284, -1.8897333163619044e+286, -1.1704078075185618e+289, 2.1823600178648432e+291, -4.7830223651003105e+292, -4.39528491684936e+295, 7.75678787835292e+297, -1.7542139362621323e+299, -1.4870433408818763e+302, 2.6934582547115835e+304, -9.108330640137994e+305, -INF, INF, -INF, -INF, INF, -INF, -INF, INF, -INF, INF, INF, -INF, INF, INF, -INF, INF, -INF, -INF, INF, -INF, INF, INF, -INF, INF, -INF, -INF, INF, -INF, INF, -INF, -INF, INF, -INF, INF, -INF, INF, -INF, INF, -INF, INF, -INF, INF, -INF, INF, -INF, INF, -INF, INF, -INF, INF, -INF, INF, -INF, INF, -INF, INF, -INF, INF, -INF, INF, -INF, INF, -INF, INF]), 1e-11);
    check_class(&it, "v47", "double");
    check_dims(&it, "v47", &[201, 1]);
    check(&it, "v47", &real(&[-0.22160842425401991, 0.8279331954426818, 0.22638222469624564, -0.8253666705791511, -0.2406614162900961, 0.8199034591532407, 0.26430980796721665, -0.8108880209402027, -0.2970687112755902, 0.797350550163449, 0.3385100597290673, -0.77802747792804, -0.3879724141775501, 0.7513958489980845, 0.44448216055838213, -0.7157273002467579, -0.5066637944968447, 0.6691682732866338, 0.5726454122733702, -0.6098534324753616, -0.6399684337646147, 0.5360587196207118, 0.7055141143957343, -0.44639866802968375, -0.7654633532035622, 0.3400690777468656, 0.815310255114195, -0.2171304795602495, -0.8499531679548857, 0.0788196431461766, 0.8638884799448654, 0.07213439011188544, -0.8515309929258493, -0.23122631833639445, 0.8076785692635569, 0.3919735316986307, -0.7281262670333082, -0.5458626035742248, 0.6104148150226811, 0.6825179656314886, -0.4546692229625832, -0.7901696001728539, 0.264446144121219, 0.8564857062368967, -0.04746608974879068, -0.8698067477538378, -0.1839354218641582, 0.8207642124797807, 0.4128356907471384, -0.7041884801406999, -0.6184382608346689, 0.521108034488837, 0.7776480538218464, -0.28052712650726824, -0.867598485768093, 0.0005612326154437805, 0.8690680387242227, 0.2915593138278684, -0.770503293076351, -0.5610965115320264, 0.5720953645054371, 0.7693674486246413, -0.2890811835623512, -0.8793140393921663, -0.04676590255339606, 0.8626693303017237, 0.3885303362572857, -0.707717480669954, -0.6792685710419861, 0.4260257200546953, 0.8610755878610833, -0.05606057902145491, -0.8873523268583525, -0.3384736138521251, 0.7362652579620393, 0.6771700794088623, -0.42213192110177217, -0.8785518664965871, 0.0006535813069851736, 0.8808818830349738, 0.4359033846274389, -0.6636858549357485, -0.7758131850753421, 0.2633319776461076, 0.9162005822719465, 0.22475093818978198, -0.7969305267200949, -0.6628579025274908, 0.42993301054035, 0.9074834339752971, 0.0886815114405049, -0.8585201023797155, -0.5945923190265294, 0.5059287441652522, 0.9026680739156799, 0.04661849972367329, -0.8764154597519469, -0.5995475127831409, 0.49588152369459565, 0.9227909188490904, 0.10852035441482362, -0.8536249682680167, -0.6846400956535643, 0.3886042666865174, 0.9561755564976097, 0.2810677763256737, -0.7597532056838505, -0.8295107306367221, 0.15702439980484895, 0.9487975524532625, 0.553367225789945, -0.5323983459607443, -0.9650775324308024, -0.2174727905643701, 0.7973779277717428, 0.8561117522687071, -0.10712776739756046, -0.9477678277935014, -0.6805406097943412, 0.37870565204866175, 1.0068796462061356, 0.49015287405048674, -0.5822747316941346, -1.0093041127029658, -0.3224284615251648, 0.7206097129819907, 0.9892540218721582, 0.20089543419713937, -0.8040969318276133, -0.9741669560248751, -0.13948363762303412, 0.8420635508939862, 0.98185230569726, 0.14756053902438562, -0.8361817175873953, -1.0185747803085874, -0.23307470354640278, 0.7757484756330965, 1.0755428173845885, 0.4013461725570917, -0.6354271170674247, -1.1222251112972679, -0.6464099426674373, 0.378749027672224, 1.0977292826687859, 0.9316162913805192, 0.02465808149072347, -0.908478802933232, -1.1603193356910004, -0.556096330624391, 0.4538435825111108, 1.1542028130878275, 1.0745543025155526, 0.2843722964730598, -0.6948995988730099, -1.254025027879741, -1.0716838682756575, -0.27386408383086736, 0.6824034575848088, 1.290620572890678, 1.260676579397775, 0.6334057051980055, -0.28393340469032263, -1.0930276394543832, -1.485356144675063, -1.347253772175328, -0.7635555074605123, 0.05483509934032122, 0.8699385450361846, 1.4974173616264475, 1.8457944844979672, 1.9143082028313314, 1.7643841331551202, 1.483342253811585, 1.1544010038844843, 0.8393637261397987, 0.5737222900425591, 0.37023060536200275, 0.22624494859413646, 0.13120566762501287, 0.07231608361615172, 0.03791732675826347, 0.018922620197635992, 0.008989122313490094, 0.00406388594237888, 0.0017474226744082337, 0.0007139633066383972, 0.0002768212109785069, 0.00010167375074365768, 3.5296591684052484e-05, 1.1549040914096454e-05, 3.5490252001587397e-06, 1.019719780780834e-06, 2.723864706524751e-07, 6.71441882228221e-08, 1.512384343694663e-08, 3.070480334450099e-09, 5.507333614507428e-10, 8.45292926112186e-11, 1.0470615504238385e-11, 9.06781901992965e-13]), 1e-13);
}

#[test]
fn rat_matches_matlab() {
    let (it, errors) = run(concat!(
        "v1=rat(pi);\n",
        "v2=rat([0.5 0.75 1/3 -2.25 0 1e-10 1e10 Inf -Inf NaN]);\n",
        "v3=rat([pi;exp(1)]);\n",
        "v4=rat(pi,1e-2);\n",
        "[n,d]=rat([pi 0.5 -0.75 0 Inf -Inf NaN 1e-20]); v5=[n;d];\n",
        "[n,d]=rat([pi exp(1)],1e-3); v6=[n;d];\n",
        "[n,d]=rat([1.5 2;0.1 3]); v7=[n d];\n",
        "v8=rat(single(0.1));\n",
        "v9=rat(1+1i);\n",
        "v10=rat(0.1);\n",
        "v11=rat(-pi);\n",
        "v12=rat([1 2;3 4.5]);\n",
        "v13=rat(0);\n",
        "v14=rat(sqrt(2));\n",
        "[n,d]=rat(sqrt(2)); v15=[n d];\n",
        "v16=rat(1.5);\n",
        "v17=rat(-0.5);\n",
        "v18=rat(123.456);\n",
        "[n,d]=rat(123.456); v19=[n d];\n",
        "v20=rat(1e-7);\n",
        "v21=rat(0.333);\n",
        "v22=rat(1e6+0.5);\n",
        "v25=rat([]);\n",
        "v27=rat(-0.0625);\n",
        "v28=rat([0.3 -0.7]);\n",
        "v29=rat(2.675);\n",
        "v30=rat(pi,0);\n",
        "[n,d]=rat(0.3+0.4i); v31=[n d];\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_text(&it, "v1", &["3 + 1/(7 + 1/(16))"]);
    check_text(&it, "v2", &["1          ", "1          ", "0          ", "-2         ", "0          ", "0          ", "10000000000", "Inf        ", "-Inf       ", "NaN        "]);
    check_text(&it, "v3", &["3 + 1/(7 + 1/(16))                          ", "3 + 1/(-4 + 1/(2 + 1/(5 + 1/(-2 + 1/(-7)))))"]);
    check_text(&it, "v4", &["3 + 1/(7)"]);
    check_class(&it, "v5", "double");
    check_dims(&it, "v5", &[2, 8]);
    check(&it, "v5", &real(&[355.0, 113.0, 1.0, 2.0, -3.0, 4.0, 0.0, 1.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0]), 8.881784197001252e-16);
    check_class(&it, "v6", "double");
    check_dims(&it, "v6", &[2, 2]);
    check(&it, "v6", &real(&[355.0, 113.0, 106.0, 39.0]), 8.881784197001252e-16);
    check_class(&it, "v7", "double");
    check_dims(&it, "v7", &[2, 4]);
    check(&it, "v7", &real(&[3.0, 1.0, 2.0, 3.0, 2.0, 10.0, 1.0, 1.0]), 8.881784197001252e-16);
    check_text(&it, "v8", &["0 + 1/(10)"]);
    check_text(&it, "v9", &["1       ", " +i* ...", "1       "]);
    check_text(&it, "v10", &["0 + 1/(10)"]);
    check_text(&it, "v11", &["-3 + 1/(-7 + 1/(-16))"]);
    check_text(&it, "v12", &["1         ", "3         ", "2         ", "5 + 1/(-2)"]);
    check_text(&it, "v13", &["0"]);
    check_text(&it, "v14", &["1 + 1/(2 + 1/(2 + 1/(2 + 1/(2 + 1/(2 + 1/(2 + 1/(2 + 1/(2))))))))"]);
    check_class(&it, "v15", "double");
    check_dims(&it, "v15", &[1, 2]);
    check(&it, "v15", &real(&[1393.0, 985.0]), 8.881784197001252e-16);
    check_text(&it, "v16", &["2 + 1/(-2)"]);
    check_text(&it, "v17", &["-1 + 1/(2)"]);
    check_text(&it, "v18", &["123 + 1/(2 + 1/(5 + 1/(5 + 1/(2))))"]);
    check_class(&it, "v19", "double");
    check_dims(&it, "v19", &[1, 2]);
    check(&it, "v19", &real(&[15432.0, 125.0]), 8.881784197001252e-16);
    check_text(&it, "v20", &["0 + 1/(10000000)"]);
    check_text(&it, "v21", &["0 + 1/(3 + 1/(333))"]);
    check_text(&it, "v22", &["1000001"]);
    assert_eq!(error_of("r=rat(int8(3));"), "First argument must be single or double.");
    assert_eq!(error_of("r=rat('a');"), "First argument must be single or double.");
    check_class(&it, "v25", "double");
    check_dims(&it, "v25", &[0, 0]);
    check(&it, "v25", &real(&[]), 8.881784197001252e-16);
    check_text(&it, "v27", &["-0 + 1/(-16)"]);
    check_text(&it, "v28", &["0 + 1/(3 + 1/(3)) ", "-1 + 1/(3 + 1/(3))"]);
    check_text(&it, "v29", &["3 + 1/(-3 + 1/(-13))"]);
    check_text(&it, "v30", &["3 + 1/(7 + 1/(16 + 1/(-294 + 1/(3 + 1/(-4 + 1/(5 + 1/(-15)))))))"]);
    check_class(&it, "v31", "double");
    check_dims(&it, "v31", &[1, 2]);
    check(&it, "v31", &[(3.0, 4.0), (10.0, 0.0)], 8.881784197001252e-16);
}

#[test]
fn rats_matches_matlab() {
    let (it, errors) = run(concat!(
        "v1=rats(pi);\n",
        "v2=rats([0.5 0.75 1/3 -2.25 0 pi]);\n",
        "v3=rats([0.5;1/3]);\n",
        "v4=rats(pi,8);\n",
        "v5=rats(1e-10);\n",
        "v6=rats(1e10);\n",
        "v7=rats([Inf -Inf NaN]);\n",
        "v8=rats(1/7, 20);\n",
        "v9=size(rats([0.5 0.75;1 2]));\n",
        "v10=rats(0.1);\n",
        "v11=rats(-pi);\n",
        "v12=rats(sqrt(2));\n",
        "v13=rats(2/3,5);\n",
        "v14=rats(123456.789);\n",
        "v15=rats(1e5/3);\n",
        "v16=rats(-1e-10);\n",
        "v17=rats([1 2;3 4]);\n",
        "v18=rats(pi,4);\n",
        "v19=rats(pi,3);\n",
        "v20=rats(0.5,2);\n",
        "v21=rats([]);\n",
        "v22=rats(1e15);\n",
        "v23=rats(1/3,9);\n",
        "v24=rats(single(0.25));\n",
        "v25=rats(1.5+2i);\n",
        "v26=rats(-0.5);\n",
        "v27=rats([1/3 -1/7 22/7]);\n",
        "v28=rats(0.123456);\n",
        "v29=rats(pi*1e4);\n",
        "v30=rats(exp(1),6);\n",
        "v31=rats(1e-5);\n",
        "v32=rats(0.001);\n",
        "v33=rats([pi exp(1) sqrt(2) 0.123456 1/7 2/3 1234.5678 0.0123 1e-4 1e-6 5e-7 99999 123456 1234567 0.999 7.1234 -2.71828 0.3 -0.45 17.25 1/3+1e-9 1e-7 3.5e-6 0.0001234 98765.4321 1/97 355/113 0.5].',3);\n",
        "v34=rats([pi exp(1) sqrt(2) 0.123456 1/7 2/3 1234.5678 0.0123 1e-4 1e-6 5e-7 99999 123456 1234567 0.999 7.1234 -2.71828 0.3 -0.45 17.25 1/3+1e-9 1e-7 3.5e-6 0.0001234 98765.4321 1/97 355/113 0.5].',4);\n",
        "v35=rats([pi exp(1) sqrt(2) 0.123456 1/7 2/3 1234.5678 0.0123 1e-4 1e-6 5e-7 99999 123456 1234567 0.999 7.1234 -2.71828 0.3 -0.45 17.25 1/3+1e-9 1e-7 3.5e-6 0.0001234 98765.4321 1/97 355/113 0.5].',5);\n",
        "v36=rats([pi exp(1) sqrt(2) 0.123456 1/7 2/3 1234.5678 0.0123 1e-4 1e-6 5e-7 99999 123456 1234567 0.999 7.1234 -2.71828 0.3 -0.45 17.25 1/3+1e-9 1e-7 3.5e-6 0.0001234 98765.4321 1/97 355/113 0.5].',6);\n",
        "v37=rats([pi exp(1) sqrt(2) 0.123456 1/7 2/3 1234.5678 0.0123 1e-4 1e-6 5e-7 99999 123456 1234567 0.999 7.1234 -2.71828 0.3 -0.45 17.25 1/3+1e-9 1e-7 3.5e-6 0.0001234 98765.4321 1/97 355/113 0.5].',7);\n",
        "v38=rats([pi exp(1) sqrt(2) 0.123456 1/7 2/3 1234.5678 0.0123 1e-4 1e-6 5e-7 99999 123456 1234567 0.999 7.1234 -2.71828 0.3 -0.45 17.25 1/3+1e-9 1e-7 3.5e-6 0.0001234 98765.4321 1/97 355/113 0.5].',8);\n",
        "v39=rats([pi exp(1) sqrt(2) 0.123456 1/7 2/3 1234.5678 0.0123 1e-4 1e-6 5e-7 99999 123456 1234567 0.999 7.1234 -2.71828 0.3 -0.45 17.25 1/3+1e-9 1e-7 3.5e-6 0.0001234 98765.4321 1/97 355/113 0.5].',9);\n",
        "v40=rats([pi exp(1) sqrt(2) 0.123456 1/7 2/3 1234.5678 0.0123 1e-4 1e-6 5e-7 99999 123456 1234567 0.999 7.1234 -2.71828 0.3 -0.45 17.25 1/3+1e-9 1e-7 3.5e-6 0.0001234 98765.4321 1/97 355/113 0.5].',10);\n",
        "v41=rats([pi exp(1) sqrt(2) 0.123456 1/7 2/3 1234.5678 0.0123 1e-4 1e-6 5e-7 99999 123456 1234567 0.999 7.1234 -2.71828 0.3 -0.45 17.25 1/3+1e-9 1e-7 3.5e-6 0.0001234 98765.4321 1/97 355/113 0.5].',11);\n",
        "v42=rats([pi exp(1) sqrt(2) 0.123456 1/7 2/3 1234.5678 0.0123 1e-4 1e-6 5e-7 99999 123456 1234567 0.999 7.1234 -2.71828 0.3 -0.45 17.25 1/3+1e-9 1e-7 3.5e-6 0.0001234 98765.4321 1/97 355/113 0.5].',12);\n",
        "v43=rats([pi exp(1) sqrt(2) 0.123456 1/7 2/3 1234.5678 0.0123 1e-4 1e-6 5e-7 99999 123456 1234567 0.999 7.1234 -2.71828 0.3 -0.45 17.25 1/3+1e-9 1e-7 3.5e-6 0.0001234 98765.4321 1/97 355/113 0.5].',13);\n",
        "v44=rats([pi exp(1) sqrt(2) 0.123456 1/7 2/3 1234.5678 0.0123 1e-4 1e-6 5e-7 99999 123456 1234567 0.999 7.1234 -2.71828 0.3 -0.45 17.25 1/3+1e-9 1e-7 3.5e-6 0.0001234 98765.4321 1/97 355/113 0.5].',14);\n",
        "v45=rats([pi exp(1) sqrt(2) 0.123456 1/7 2/3 1234.5678 0.0123 1e-4 1e-6 5e-7 99999 123456 1234567 0.999 7.1234 -2.71828 0.3 -0.45 17.25 1/3+1e-9 1e-7 3.5e-6 0.0001234 98765.4321 1/97 355/113 0.5].',16);\n",
        "v46=rats([1 1e-10]);\n",
        "v47=rats([1e6 0.001]);\n",
        "v48=rats([1 1e-7]);\n",
        "v49=rats([pi 1e-5]);\n",
        "v50=rats([1 1e-6]);\n",
        "v51=rats([100 1e-5 0.5]);\n",
        "v52=rats([1e5 1/3]);\n",
        "v53=rats([0 0]);\n",
        "v54=rats([1e-10 1e-12]);\n",
        "v55=rats([1e10 1]);\n",
        "v56=rats([-1e-10 5]);\n",
        "v57=rats([2;1e-9]);\n",
        "v58=rats(1e-3*[1 2 3]);\n",
        "v59=rats([1+2i 0.5-0.25i]);\n",
        "v60=rats(1/3+1i/7,8);\n",
        "v61=rats([sqrt(2) 1]);\n",
        "v62=rats([sqrt(2) 3]);\n",
        "v63=rats([sqrt(2) 10]);\n",
        "v64=rats([sqrt(2) 30]);\n",
        "v65=rats([sqrt(2) 100]);\n",
        "v66=rats([sqrt(2) 1000]);\n",
        "v67=rats([sqrt(2) 1e4]);\n",
        "v68=rats([sqrt(2) 1e5]);\n",
        "v69=rats([sqrt(2) 1e6]);\n",
        "v70=rats([sqrt(2) 1e7]);\n",
        "v71=rats([sqrt(2) 1e8]);\n",
        "v72=rats([sqrt(2) 1e9]);\n",
        "v73=rats([sqrt(2) 1e10]);\n",
        "v74=rats([sqrt(2) 1e12]);\n",
        "v75=rats([sqrt(2) 1e6],5);\n",
        "v76=rats([sqrt(2) pi exp(1)],5);\n",
        "v77=rats(sqrt(2),5);\n",
        "v78=rats([sqrt(2) 1e6],8);\n",
        "v79=rats([sqrt(2) pi exp(1)],8);\n",
        "v80=rats(sqrt(2),8);\n",
        "v81=rats([sqrt(2) 1e6],10);\n",
        "v82=rats([sqrt(2) pi exp(1)],10);\n",
        "v83=rats(sqrt(2),10);\n",
        "v84=rats([sqrt(2) 1e6],13);\n",
        "v85=rats([sqrt(2) pi exp(1)],13);\n",
        "v86=rats(sqrt(2),13);\n",
        "v87=rats([sqrt(2) 1e6],16);\n",
        "v88=rats([sqrt(2) pi exp(1)],16);\n",
        "v89=rats(sqrt(2),16);\n",
        "v90=rats([sqrt(2) 1e6],20);\n",
        "v91=rats([sqrt(2) pi exp(1)],20);\n",
        "v92=rats(sqrt(2),20);\n",
        "v93=rats([sqrt(2) 1e6],24);\n",
        "v94=rats([sqrt(2) pi exp(1)],24);\n",
        "v95=rats(sqrt(2),24);\n",
        "v96=rats([pi -1]);\n",
        "v97=rats([pi -10]);\n",
        "v98=rats([pi -100]);\n",
        "v99=rats([pi -1e3]);\n",
        "v100=rats([pi -1e4]);\n",
        "v101=rats([0.001 1000]);\n",
        "v102=rats([1e-4 1]);\n",
        "v103=rats([1e-4 10]);\n",
        "v104=rats([1e-4 100]);\n",
        "v105=rats([0.5 1e9]);\n",
        "v106=rats([0.5 1e10]);\n",
        "v107=rats([0.5 1e11]);\n",
        "v108=rats([1/3 1e9]);\n",
        "v109=rats(1234567.5);\n",
        "v110=rats(-1234567.5);\n",
        "v111=rats(1/3*1e6);\n",
        "v112=rats(1e6/7);\n",
        "v113=rats(1e7/7);\n",
        "v114=rats(12345678);\n",
        "v115=rats(123456789012);\n",
        "v116=rats(1234567890123);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_text(&it, "v1", &["    355/113   "]);
    check_text(&it, "v2", &["      1/2           3/4           1/3          -9/4            0          355/113   "]);
    check_text(&it, "v3", &["      1/2     ", "      1/3     "]);
    check_text(&it, "v4", &[" 355/113 "]);
    check_text(&it, "v5", &["       *      "]);
    check_text(&it, "v6", &["  10000000000 "]);
    check_text(&it, "v7", &["      1/0          -1/0           0/0     "]);
    check_text(&it, "v8", &["         1/7         "]);
    check_class(&it, "v9", "double");
    check_dims(&it, "v9", &[1, 2]);
    check(&it, "v9", &real(&[2.0, 28.0]), 8.881784197001252e-16);
    check_text(&it, "v10", &["      1/10    "]);
    check_text(&it, "v11", &["   -355/113   "]);
    check_text(&it, "v12", &["   1393/985   "]);
    check_text(&it, "v13", &["  2/3 "]);
    check_text(&it, "v14", &[" 617284/5     "]);
    check_text(&it, "v15", &[" 100000/3     "]);
    check_text(&it, "v16", &["       *      "]);
    check_text(&it, "v17", &["       1             2      ", "       3             4      "]);
    check_text(&it, "v18", &["  *  "]);
    check_text(&it, "v19", &["  * "]);
    check_text(&it, "v20", &[" * "]);
    check_text(&it, "v21", &[]);
    check_text(&it, "v22", &["       *      "]);
    check_text(&it, "v23", &["    1/3   "]);
    check_text(&it, "v24", &["      1/4     "]);
    check_text(&it, "v25", &["      3/2     +      2i      "]);
    check_text(&it, "v26", &["     -1/2     "]);
    check_text(&it, "v27", &["      1/3          -1/7          22/7     "]);
    check_text(&it, "v28", &["   1929/15625 "]);
    check_text(&it, "v29", &[" 439823/14    "]);
    check_text(&it, "v30", &[" 19/7  "]);
    check_text(&it, "v31", &["      1/100000"]);
    check_text(&it, "v32", &["      1/1000  "]);
    check_text(&it, "v33", &["  * ", "  * ", " 3/2", " 1/8", " 1/7", " 2/3", "  * ", "  0 ", "  0 ", "  0 ", "  0 ", "  * ", "  * ", "  * ", "  1 ", "  * ", "  * ", " 1/3", "  * ", "  * ", " 1/3", "  0 ", "  0 ", "  0 ", "  * ", "  0 ", "  * ", " 1/2"]);
    check_text(&it, "v34", &["  *  ", "  *  ", " 3/2 ", " 1/8 ", " 1/7 ", " 2/3 ", "  *  ", "  0  ", "  0  ", "  0  ", "  0  ", "  *  ", "  *  ", "  *  ", "  1  ", "  *  ", "  *  ", " 1/3 ", "  *  ", "  *  ", " 1/3 ", "  0  ", "  0  ", "  0  ", "  *  ", "  0  ", "  *  ", " 1/2 "]);
    check_text(&it, "v35", &[" 22/7 ", " 11/4 ", "  3/2 ", "  1/8 ", "  1/7 ", "  2/3 ", "   *  ", "   0  ", "   0  ", "   0  ", "   0  ", " 99999", "   *  ", "   *  ", "   1  ", " 57/8 ", "   *  ", "  1/3 ", " -1/2 ", " 69/4 ", "  1/3 ", "   0  ", "   0  ", "   0  ", "   *  ", "   0  ", " 22/7 ", "  1/2 "]);
    check_text(&it, "v36", &[" 22/7  ", " 11/4  ", "  3/2  ", "  1/8  ", "  1/7  ", "  2/3  ", "   *   ", "   0   ", "   0   ", "   0   ", "   0   ", " 99999 ", " 123456", "   *   ", "   1   ", " 57/8  ", "   *   ", "  1/3  ", " -1/2  ", " 69/4  ", "  1/3  ", "   0   ", "   0   ", "   0   ", "   *   ", "   0   ", " 22/7  ", "  1/2  "]);
    check_text(&it, "v37", &["  22/7  ", "  11/4  ", "   3/2  ", "   1/8  ", "   1/7  ", "   2/3  ", "    *   ", "    0   ", "    0   ", "    0   ", "    0   ", "  99999 ", " 123456 ", " 1234567", "    1   ", "  57/8  ", " -11/4  ", "   1/3  ", "  -1/2  ", "  69/4  ", "   1/3  ", "    0   ", "    0   ", "    0   ", "    *   ", "    0   ", "  22/7  ", "   1/2  "]);
    check_text(&it, "v38", &["  22/7   ", "  11/4   ", "   3/2   ", "   1/8   ", "   1/7   ", "   2/3   ", "    *    ", "    0    ", "    0    ", "    0    ", "    0    ", "  99999  ", "  123456 ", " 1234567 ", "    1    ", "  57/8   ", " -11/4   ", "   1/3   ", "  -1/2   ", "  69/4   ", "   1/3   ", "    0    ", "    0    ", "    0    ", "    *    ", "    0    ", "  22/7   ", "   1/2   "]);
    check_text(&it, "v39", &["   22/7   ", "   11/4   ", "    3/2   ", "    1/8   ", "    1/7   ", "    2/3   ", " 2469/2   ", "     0    ", "     0    ", "     0    ", "     0    ", "   99999  ", "  123456  ", "  1234567 ", "     1    ", "   57/8   ", "  -11/4   ", "    1/3   ", "   -1/2   ", "   69/4   ", "    1/3   ", "     0    ", "     0    ", "     0    ", "     *    ", "     0    ", "   22/7   ", "    1/2   "]);
    check_text(&it, "v40", &["   22/7    ", "   11/4    ", "    3/2    ", "    1/8    ", "    1/7    ", "    2/3    ", " 2469/2    ", "     0     ", "     0     ", "     0     ", "     0     ", "   99999   ", "   123456  ", "  1234567  ", "     1     ", "   57/8    ", "  -11/4    ", "    1/3    ", "   -1/2    ", "   69/4    ", "    1/3    ", "     0     ", "     0     ", "     0     ", "     *     ", "     0     ", "   22/7    ", "    1/2    "]);
    check_text(&it, "v41", &["    22/7    ", "    11/4    ", "     3/2    ", "     1/8    ", "     1/7    ", "     2/3    ", "  2469/2    ", "      0     ", "      0     ", "      0     ", "      0     ", "    99999   ", "   123456   ", "   1234567  ", "      1     ", "    57/8    ", "   -11/4    ", "     1/3    ", "    -1/2    ", "    69/4    ", "     1/3    ", "      0     ", "      0     ", "      0     ", "      *     ", "      0     ", "    22/7    ", "     1/2    "]);
    check_text(&it, "v42", &["    22/7     ", "    11/4     ", "     3/2     ", "     1/8     ", "     1/7     ", "     2/3     ", "  2469/2     ", "      0      ", "      0      ", "      0      ", "      0      ", "    99999    ", "    123456   ", "   1234567   ", "      1      ", "    57/8     ", "   -11/4     ", "     1/3     ", "    -1/2     ", "    69/4     ", "     1/3     ", "      0      ", "      0      ", "      0      ", "      *      ", "      0      ", "    22/7     ", "     1/2     "]);
    check_text(&it, "v43", &["     22/7     ", "     11/4     ", "      3/2     ", "      1/8     ", "      1/7     ", "      2/3     ", "   2469/2     ", "       0      ", "       0      ", "       0      ", "       0      ", "     99999    ", "    123456    ", "    1234567   ", "       1      ", "     57/8     ", "    -11/4     ", "      1/3     ", "     -1/2     ", "     69/4     ", "      1/3     ", "       0      ", "       0      ", "       0      ", " 197531/2     ", "       0      ", "     22/7     ", "      1/2     "]);
    check_text(&it, "v44", &["     22/7      ", "     11/4      ", "      3/2      ", "      1/8      ", "      1/7      ", "      2/3      ", "   2469/2      ", "       0       ", "       0       ", "       0       ", "       0       ", "     99999     ", "     123456    ", "    1234567    ", "       1       ", "     57/8      ", "    -11/4      ", "      1/3      ", "     -1/2      ", "     69/4      ", "      1/3      ", "       0       ", "       0       ", "       0       ", " 197531/2      ", "       0       ", "     22/7      ", "      1/2      "]);
    check_text(&it, "v45", &["      22/7       ", "      11/4       ", "       7/5       ", "       1/8       ", "       1/7       ", "       2/3       ", "    8642/7       ", "        0        ", "        0        ", "        0        ", "        0        ", "      99999      ", "      123456     ", "     1234567     ", "        1        ", "      57/8       ", "     -11/4       ", "       1/3       ", "      -4/9       ", "      69/4       ", "       1/3       ", "        0        ", "        0        ", "        0        ", "  691358/7       ", "        0        ", "      22/7       ", "       1/2       "]);
    check_text(&it, "v46", &["       1             0      "]);
    check_text(&it, "v47", &["    1000000          0      "]);
    check_text(&it, "v48", &["       1             0      "]);
    check_text(&it, "v49", &["    355/113         1/100000"]);
    check_text(&it, "v50", &["       1             0      "]);
    check_text(&it, "v51", &["      100            0            1/2     "]);
    check_text(&it, "v52", &["    100000          1/3     "]);
    check_text(&it, "v53", &["       0             0      "]);
    check_text(&it, "v54", &["       *             *      "]);
    check_text(&it, "v55", &["  10000000000        1      "]);
    check_text(&it, "v56", &["       0             5      "]);
    check_text(&it, "v57", &["       2      ", "       0      "]);
    check_text(&it, "v58", &["      1/1000        1/500         3/1000  "]);
    check_text(&it, "v59", &["       1      +      2i            1/2     -     1/4i     "]);
    check_text(&it, "v60", &["   1/3   +  1/7i   "]);
    check_text(&it, "v61", &["    577/408          1      "]);
    check_text(&it, "v62", &["    577/408          3      "]);
    check_text(&it, "v63", &["    577/408         10      "]);
    check_text(&it, "v64", &["    239/169         30      "]);
    check_text(&it, "v65", &["     99/70          100     "]);
    check_text(&it, "v66", &["     41/29         1000     "]);
    check_text(&it, "v67", &["     17/12         10000    "]);
    check_text(&it, "v68", &["      3/2         100000    "]);
    check_text(&it, "v69", &["      3/2         1000000   "]);
    check_text(&it, "v70", &["      3/2        10000000   "]);
    check_text(&it, "v71", &["      3/2        100000000  "]);
    check_text(&it, "v72", &["      3/2       1000000000  "]);
    check_text(&it, "v73", &["      3/2       10000000000 "]);
    check_text(&it, "v74", &["      3/2      1000000000000"]);
    check_text(&it, "v75", &["  3/2    *  "]);
    check_text(&it, "v76", &["  7/5  22/7  11/4 "]);
    check_text(&it, "v77", &[" 17/12"]);
    check_text(&it, "v78", &["   3/2    1000000 "]);
    check_text(&it, "v79", &["  41/29    22/7    106/39  "]);
    check_text(&it, "v80", &["  41/29  "]);
    check_text(&it, "v81", &["    3/2      1000000  "]);
    check_text(&it, "v82", &["   99/70     355/113    193/71   "]);
    check_text(&it, "v83", &["  239/169  "]);
    check_text(&it, "v84", &["      3/2         1000000   "]);
    check_text(&it, "v85", &["    577/408       355/113      1457/536   "]);
    check_text(&it, "v86", &["   1393/985   "]);
    check_text(&it, "v87", &["       7/5            1000000     "]);
    check_text(&it, "v88", &["    3363/2378      104348/33215       2721/1001    "]);
    check_text(&it, "v89", &["    8119/5741    "]);
    check_text(&it, "v90", &["        99/70               1000000       "]);
    check_text(&it, "v91", &["     19601/13860         104348/33215          49171/18089     "]);
    check_text(&it, "v92", &["     47321/33461     "]);
    check_text(&it, "v93", &["         577/408                  1000000         "]);
    check_text(&it, "v94", &["      275807/195025           1146408/364913            566827/208524      "]);
    check_text(&it, "v95", &["      665857/470832      "]);
    check_text(&it, "v96", &["    355/113         -1      "]);
    check_text(&it, "v97", &["    355/113         -10     "]);
    check_text(&it, "v98", &["    355/113        -100     "]);
    check_text(&it, "v99", &["    355/113        -1000    "]);
    check_text(&it, "v100", &["     22/7         -10000    "]);
    check_text(&it, "v101", &["       0           1000     "]);
    check_text(&it, "v102", &["      1/10000        1      "]);
    check_text(&it, "v103", &["      1/10000       10      "]);
    check_text(&it, "v104", &["       0            100     "]);
    check_text(&it, "v105", &["      1/2       1000000000  "]);
    check_text(&it, "v106", &["      1/2       10000000000 "]);
    check_text(&it, "v107", &["      1/2      100000000000 "]);
    check_text(&it, "v108", &["      1/3       1000000000  "]);
    check_text(&it, "v109", &["       *      "]);
    check_text(&it, "v110", &["       *      "]);
    check_text(&it, "v111", &["       *      "]);
    check_text(&it, "v112", &["       *      "]);
    check_text(&it, "v113", &["       *      "]);
    check_text(&it, "v114", &["   12345678   "]);
    check_text(&it, "v115", &[" 123456789012 "]);
    check_text(&it, "v116", &[" 1234567890123"]);
}

#[test]
fn perms_matches_matlab() {
    let (it, errors) = run(concat!(
        "v1=perms([1 2 3]);\n",
        "v2=perms([3 1 2]);\n",
        "v3=perms(1:4);\n",
        "v4=perms([]);\n",
        "v5=size(perms([]));\n",
        "v6=perms(5);\n",
        "v7=perms('abc');\n",
        "v8=perms(int8([1 2]));\n",
        "v10=perms([1 1 2]);\n",
        "v11=perms([1;2;3]);\n",
        "v12=perms(['a';'b']);\n",
        "v13=perms(true(1,2));\n",
        "v14=size(perms(1:8));\n",
        "v15=perms(single([1.5 2]));\n",
        "v16=perms(1+[1i 2]);\n",
        "v17=perms(\"ab\");\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[6, 3]);
    check(&it, "v1", &real(&[3.0, 3.0, 2.0, 2.0, 1.0, 1.0, 2.0, 1.0, 3.0, 1.0, 3.0, 2.0, 1.0, 2.0, 1.0, 3.0, 2.0, 3.0]), 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[6, 3]);
    check(&it, "v2", &real(&[2.0, 2.0, 1.0, 1.0, 3.0, 3.0, 1.0, 3.0, 2.0, 3.0, 2.0, 1.0, 3.0, 1.0, 3.0, 2.0, 1.0, 2.0]), 8.881784197001252e-16);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[24, 4]);
    check(&it, "v3", &real(&[4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 3.0, 3.0, 2.0, 2.0, 1.0, 1.0, 4.0, 4.0, 2.0, 2.0, 1.0, 1.0, 4.0, 4.0, 3.0, 3.0, 1.0, 1.0, 4.0, 4.0, 3.0, 3.0, 2.0, 2.0, 2.0, 1.0, 3.0, 1.0, 3.0, 2.0, 2.0, 1.0, 4.0, 1.0, 4.0, 2.0, 3.0, 1.0, 4.0, 1.0, 4.0, 3.0, 3.0, 2.0, 4.0, 2.0, 4.0, 3.0, 1.0, 2.0, 1.0, 3.0, 2.0, 3.0, 1.0, 2.0, 1.0, 4.0, 2.0, 4.0, 1.0, 3.0, 1.0, 4.0, 3.0, 4.0, 2.0, 3.0, 2.0, 4.0, 3.0, 4.0]), 8.881784197001252e-16);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[1, 0]);
    check(&it, "v4", &real(&[]), 8.881784197001252e-16);
    check_class(&it, "v5", "double");
    check_dims(&it, "v5", &[1, 2]);
    check(&it, "v5", &real(&[1.0, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v6", "double");
    check_dims(&it, "v6", &[1, 1]);
    check(&it, "v6", &real(&[5.0]), 8.881784197001252e-16);
    check_text(&it, "v7", &["cba", "cab", "bca", "bac", "acb", "abc"]);
    check_class(&it, "v8", "int8");
    check_dims(&it, "v8", &[2, 2]);
    check(&it, "v8", &real(&[2.0, 1.0, 1.0, 2.0]), 8.881784197001252e-16);
    check_class(&it, "v10", "double");
    check_dims(&it, "v10", &[6, 3]);
    check(&it, "v10", &real(&[2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 1.0, 2.0, 1.0, 1.0, 1.0, 1.0, 2.0, 1.0, 2.0]), 8.881784197001252e-16);
    check_class(&it, "v11", "double");
    check_dims(&it, "v11", &[6, 3]);
    check(&it, "v11", &real(&[3.0, 3.0, 2.0, 2.0, 1.0, 1.0, 2.0, 1.0, 3.0, 1.0, 3.0, 2.0, 1.0, 2.0, 1.0, 3.0, 2.0, 3.0]), 8.881784197001252e-16);
    check_text(&it, "v12", &["ba", "ab"]);
    check_class(&it, "v13", "logical");
    check_dims(&it, "v13", &[2, 2]);
    check(&it, "v13", &real(&[1.0, 1.0, 1.0, 1.0]), 8.881784197001252e-16);
    check_class(&it, "v14", "double");
    check_dims(&it, "v14", &[1, 2]);
    check(&it, "v14", &real(&[40320.0, 8.0]), 8.881784197001252e-16);
    check_class(&it, "v15", "single");
    check_dims(&it, "v15", &[2, 2]);
    check(&it, "v15", &real(&[2.0, 1.5, 1.5, 2.0]), 1.1920928955078125e-07);
    check_class(&it, "v16", "double");
    check_dims(&it, "v16", &[2, 2]);
    check(&it, "v16", &[(3.0, 0.0), (1.0, 1.0), (1.0, 1.0), (3.0, 0.0)], 8.881784197001252e-16);
    check_class(&it, "v17", "string");
    check_dims(&it, "v17", &[1, 1]);
    assert_eq!(value(&it, "v17"), &Value::string("ab"));
}

#[test]
fn coordinate_transforms_match_matlab() {
    let (it, errors) = run(concat!(
        "[t,p]=cart2pol([1 -1 0 0 3],[0 1 -2 0 4]); v1=[t;p];\n",
        "[t,p,z]=cart2pol([1 -1],[1 1],[5 6]); v2=[t;p;z];\n",
        "v3=cart2pol([1 2],[3 4]);\n",
        "[x,y]=pol2cart([0 pi/2 pi 1],[1 2 3 0]); v4=[x;y];\n",
        "[x,y,z]=pol2cart(1,2,3); v5=[x y z];\n",
        "[a,e,rr]=cart2sph([1 0 0 1 0],[0 1 0 1 0],[0 0 1 1 0]); v6=[a;e;rr];\n",
        "[x,y,z]=sph2cart([0 pi/2 1],[0 0 pi/4],[1 2 3]); v7=[x;y;z];\n",
        "[t,p]=cart2pol(single(1),2); v8=[t p];\n",
        "[t,p]=cart2pol([1 2],[1;2]); v9=[t(:) p(:)];\n",
        "v11=cart2sph(1,2,3);\n",
        "v12=pol2cart(1,2);\n",
        "v13=sph2cart(1,2,3);\n",
        "[a,e,rr]=cart2sph(single(1),2,3); v17=[a e rr];\n",
        "[a,e,rr]=cart2sph([-1 2.5],[3 -4],[0.5 -2]); v18=[a;e;rr];\n",
        "[x,y,z]=sph2cart([-1 2.5],[0.3 -1.2],[2 0.5]); v19=[x;y;z];\n",
        "[x,y]=pol2cart([-1 2.5 4],[2 0.5 -1]); v20=[x;y];\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[2, 5]);
    check(&it, "v1", &real(&[0.0, 1.0, 2.356194490192345, 1.4142135623730951, -1.5707963267948966, 2.0, 0.0, 0.0, 0.9272952180016122, 5.0]), 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[3, 2]);
    check(&it, "v2", &real(&[0.7853981633974483, 1.4142135623730951, 5.0, 2.356194490192345, 1.4142135623730951, 6.0]), 8.881784197001252e-16);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[1, 2]);
    check(&it, "v3", &real(&[1.2490457723982544, 1.1071487177940904]), 8.881784197001252e-16);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[2, 4]);
    check(&it, "v4", &real(&[1.0, 0.0, 1.2246467991473532e-16, 2.0, -3.0, 3.6739403974420594e-16, 0.0, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v5", "double");
    check_dims(&it, "v5", &[1, 3]);
    check(&it, "v5", &real(&[1.0806046117362795, 1.682941969615793, 3.0]), 8.881784197001252e-16);
    check_class(&it, "v6", "double");
    check_dims(&it, "v6", &[3, 5]);
    check(&it, "v6", &real(&[0.0, 0.0, 1.0, 1.5707963267948966, 0.0, 1.0, 0.0, 1.5707963267948966, 1.0, 0.7853981633974483, 0.6154797086703873, 1.7320508075688774, 0.0, 0.0, 0.0]), 8.881784197001252e-16);
    check_class(&it, "v7", "double");
    check_dims(&it, "v7", &[3, 3]);
    check(&it, "v7", &real(&[1.0, 0.0, 0.0, 1.2246467991473532e-16, 2.0, 0.0, 1.1461542731102694, 1.785029518588158, 2.1213203435596424]), 8.881784197001252e-16);
    check_class(&it, "v8", "single");
    check_dims(&it, "v8", &[1, 2]);
    check(&it, "v8", &real(&[1.1071487665176392, 2.2360680103302]), 1.1920928955078125e-07);
    check_class(&it, "v9", "double");
    check_dims(&it, "v9", &[4, 2]);
    check(&it, "v9", &real(&[0.7853981633974483, 1.1071487177940904, 0.4636476090008061, 0.7853981633974483, 1.4142135623730951, 2.23606797749979, 2.23606797749979, 2.8284271247461903]), 8.881784197001252e-16);
    assert_eq!(error_of("r=cart2pol(1+1i,2);"), "Inputs must be real.");
    check_class(&it, "v11", "double");
    check_dims(&it, "v11", &[1, 1]);
    check(&it, "v11", &real(&[1.1071487177940904]), 8.881784197001252e-16);
    check_class(&it, "v12", "double");
    check_dims(&it, "v12", &[1, 1]);
    check(&it, "v12", &real(&[1.0806046117362795]), 8.881784197001252e-16);
    check_class(&it, "v13", "double");
    check_dims(&it, "v13", &[1, 1]);
    check(&it, "v13", &real(&[-0.6745352860984587]), 8.881784197001252e-16);
    assert_eq!(error_of("[t,r2,z]=cart2pol(1,2); r=[t r2 z];"), "Output argument \"z\" (and possibly others) not assigned a value in the execution with \"cart2pol\" function.");
    assert_eq!(error_of("r=cart2pol(1);"), "Not enough input arguments.");
    assert_eq!(error_of("[x,y,z]=pol2cart([1 2],3); r=[x y z];"), "Output argument \"z\" (and possibly others) not assigned a value in the execution with \"pol2cart\" function.");
    check_class(&it, "v17", "single");
    check_dims(&it, "v17", &[1, 3]);
    check(&it, "v17", &real(&[1.1071487665176392, 0.9302740097045898, 3.7416574954986572]), 1.1920928955078125e-07);
    check_class(&it, "v18", "double");
    check_dims(&it, "v18", &[3, 2]);
    check(&it, "v18", &real(&[1.892546881191539, 0.1568156853444008, 3.2015621187164247, -1.0121970114513341, -0.4010226148500795, 5.123475382979799]), 8.881784197001252e-16);
    check_class(&it, "v19", "double");
    check_dims(&it, "v19", &[3, 2]);
    check(&it, "v19", &real(&[1.0323410159090758, -1.6077758726548839, 0.5910404133226791, -0.14515030077145522, 0.10843051112717494, -0.46601954298361314]), 8.881784197001252e-16);
    check_class(&it, "v20", "double");
    check_dims(&it, "v20", &[2, 3]);
    check(&it, "v20", &real(&[1.0806046117362795, -1.682941969615793, -0.40057180777346685, 0.2992360720519782, 0.6536436208636119, 0.7568024953079282]), 8.881784197001252e-16);
}

#[test]
fn colour_conversions_match_matlab() {
    let (it, errors) = run(concat!(
        "v1=hsv2rgb([0 1 1; 0.5 0.5 0.5; 1/6 1 1; 0.9 0.2 0.7; 0 0 0; 2/3 1 0.5]);\n",
        "v2=rgb2hsv([1 0 0; 0.5 0.5 0.5; 1 1 0; 0.2 0.4 0.6; 0 0 0; 0.3 0.1 0.9; 1 1 1]);\n",
        "v3=rgb2hsv(cat(3,[1 0;0.2 0.5],[0 1;0.4 0.5],[0 0;0.6 0.1]));\n",
        "v4=size(rgb2hsv(cat(3,[1 0;0.2 0.5],[0 1;0.4 0.5],[0 0;0.6 0.1])));\n",
        "v6=hsv2rgb(single([0.5 0.5 0.5]));\n",
        "v9=rgb2hsv([0.5 0.5 0.6]);\n",
        "[h,s,v]=rgb2hsv([0.2 0.4 0.6; 1 0 0]); v10=[h s v];\n",
        "v11=hsv2rgb(0.3,0.5,0.7);\n",
        "v12=rgb2hsv(0.2,0.4,0.6);\n",
        "v13=hsv2rgb([0.25 0.8 0.9; 0.75 0.3 0.4; 0.999 1 1; 1 1 1; 0.5 0 1; 0.1 0.6 0.3; 0.45 0.9 0.8; 0.6 0.2 0.1]);\n",
        "v14=hsv2rgb(cat(3,[0.1;0.6],[0.5;1],[0.9;0.4]));\n",
        "[a,b,c]=hsv2rgb([0.1 0.5 0.9;0.6 1 0.4]); v15=[a b c];\n",
        "v16=rgb2hsv([0.9 0.1 0.2; 0.1 0.9 0.5; 0.5 0.1 0.9; 0.3 0.3 0.1; 0.7 0.8 0.75; 0.25 0.5 0.25; 0.6 0.2 0.4]);\n",
        "v23=rgb2hsv(cat(3,uint8(200),uint8(100),uint8(50)));\n",
        "v24=hsv2rgb(cat(3,0.5,0.5,0.5));\n",
        "v25=rgb2hsv(cat(3,1.5,0,0));\n",
        "v26=rgb2hsv([0.7 0.25 0.8;0.3 0.9 0.8;0.95 0.8 0.75;1 0.6 0;0.95 0.5 0.9;0.2 0.75 0;1 0.1 0.05;0.3 0.5 0.95;0.2 0.4 0.6;0.05 0.7 0.2;0 0.8 0.2;0.4 0.25 0.1;1 0.95 0.9;0.4 0.1 0.9;0.9 0.05 0.1;0.7 0.7 0.5;0.1 0.1 0;0.95 0 0.2;0.9 0.2 0.1;0.95 0.1 0.25;0.3 0.2 0.6;1 0.75 0.75;0.2 0.1 0.8;0.2 1 0.4;0.25 0 0.3;0.4 0.1 1;0.1 0.25 0.05;0.3 0.25 0.95;0.7 0.7 0;0.7 0.75 0.8;0.3 0.05 0.25;0.3 0.95 0.25;0.5 0.8 0.3;0.1 0.5 0.5;0.8 0.1 0;0.25 0 0.8;0.3 0.95 0.4;0 0.6 0.9;0.4 0.3 0.4;0.7 0.95 0;0.5 0 0.8;0.1 0.7 0.2;0.05 0.9 0.2;1 0.95 0.5;0.3 0.6 0.3;1 0.6 0.25;0.9 0.5 0.05;0.7 0.8 0.9;0.9 0.3 0.95;0.25 0 0.4;0.05 0.2 0.3;0.6 0.7 0.3;1 0.1 0.3;0.25 1 0.8;0.6 0.05 0.25;0.75 0.3 0.25;0.1 0.9 0.05;0.75 0.1 0.8;0.8 0.25 0.5;0.1 0.8 0;0.05 0.7 0.6;1 0.4 0;0.2 0.8 0.7;0.3 0.95 0.25;0.5 0.75 0.4;0.1 0 1;0.75 0 0.9;0.5 0.3 0.95;0.2 0.1 0.8;0.7 0.1 0.75;1 0.9 0.4;0.05 0.1 0.4;0.3 0.1 0;0.95 0.4 0.25;0.1 0.5 1;0.7 0.95 0.1;0.6 0.5 0.5;0.8 0.8 0.3;0.5 0.25 0.25;0.5 0.4 1;0.1 0.1 0.05;0.1 0.05 0.1;0.05 0.1 0.1;0.4 0.4 0.95;0.4 0.95 0.4;0.95 0.4 0.4;0.6 0.6 0.1;0.6 0.1 0.6;0.1 0.6 0.6;0.75 0.75 1;0.75 1 0.75;1 0.75 0.75;0.5 0.5 0.95;0.5 0.95 0.5;0.95 0.5 0.5;0.3 0.3 0.1;0.3 0.1 0.3;0.1 0.3 0.3;0.05 0.05 0.5;0.05 0.5 0.05;0.5 0.05 0.05;0.25 0.25 0.6;0.25 0.6 0.25;0.6 0.25 0.25;0.9 0.9 0.6;0.9 0.6 0.9;0.6 0.9 0.9;0.95 0.95 0.6;0.95 0.6 0.95;0.6 0.95 0.95;0.3 0.3 0.05;0.3 0.05 0.3;0.05 0.3 0.3;0.9 0.9 0.9;0.9 0.9 0.9;0.9 0.9 0.9;0.9 0.9 0.3;0.9 0.3 0.9;0.3 0.9 0.9;0.8 0.8 0.7;0.8 0.7 0.8;0.7 0.8 0.8;0.75 0.75 0;0.75 0 0.75;0 0.75 0.75]);\n",
        "v27=hsv2rgb([0.7 0.8 0.6666666666666666;0.6 0.16666666666666666 0.6;0.7 0.75 0.999;0.3 0.8333333333333334 0;0.16666666666666666 0.6666666666666666 0.6;0.75 0.6 1;0.7 0.3333333333333333 0.999;0.8 0.8 0.6;0.999 0.8 0.95;0.8 0.3 1;0.8 0.75 0.3333333333333333;0.25 0.3333333333333333 0.3;0.4 0.8 0.16666666666666666;0.7 0.1 0.95;0.3 0.123 0.8333333333333334;0.3333333333333333 0.95 0.7;0.123 0.6666666666666666 0.999;0.6 0 0.4;0.3 0.8333333333333334 1;0.123 0.999 0.3;0.5 0.3 0.999;0.05 0.16666666666666666 0.5;0.3 0.05 0.25;0.2 0.75 0.3;0.16666666666666666 0.4 0.6666666666666666;0.05 0.95 1;0.8 0.9 0.123;0.1 0.8333333333333334 0.4;0.5 0.8 0;0.8 0.9 0.6;0.95 0.2 0.6666666666666666;0.8 0.05 0.6666666666666666;0.123 0.7 0.2;0.7 0.6666666666666666 0.3333333333333333;0.75 0.8333333333333334 0.7;0.8 0.25 0.95;0.95 0.8333333333333334 0.999;0.6666666666666666 0.8 1;0.25 0.3 0.123;0.9 0.8333333333333334 0.16666666666666666;0.4 0.25 0.123;0.1 0.8 0;0.9 0.2 0.75;0.8333333333333334 0.123 0.6666666666666666;0.25 0.75 0.999;0.8333333333333334 0.9 0.95;0.95 0.5 0.16666666666666666;0.7 0.16666666666666666 0.9;0.9 0.3 0.123;0.123 0.6 0.7;0.16666666666666666 0.6 0.95;0 0.75 0.7;0.16666666666666666 0.7 0.25;0.16666666666666666 0 0.2;0.123 1 0.5;0.7 0.05 0.25;0.9 0 0.16666666666666666;0.6666666666666666 0.6666666666666666 0.6;0.5 0.16666666666666666 0.05;0.5 0.16666666666666666 0.6;0.25 0.7 0.7;0.16666666666666666 0.123 0.16666666666666666;0.3333333333333333 0.999 0.123;0.2 0 0.25;0.7 0.7 0.6666666666666666;0.75 0.123 0.7;0.3333333333333333 0 1;0.8 0.8 0.8333333333333334;0.25 0.05 0;0.6 0.6666666666666666 1;0.2 0.6666666666666666 0.4;0 0.95 0.95;0.123 0.8333333333333334 0.999;0.999 0.16666666666666666 0.9;0.16666666666666666 0.9 0.4;0.7 1 0.1;0.7 0 0.95;0.8333333333333334 0.7 0.999;0.16666666666666666 0.7 0.25;0.3 0.16666666666666666 0.6666666666666666]);\n",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    check_class(&it, "v1", "double");
    check_dims(&it, "v1", &[6, 3]);
    check(&it, "v1", &real(&[1.0, 0.25, 1.0, 0.7, 0.0, 0.0, 0.0, 0.5, 1.0, 0.5599999999999999, 0.0, 0.0, 0.0, 0.5, 0.0, 0.6439999999999999, 0.0, 0.5]), 8.881784197001252e-16);
    check_class(&it, "v2", "double");
    check_dims(&it, "v2", &[7, 3]);
    check(&it, "v2", &real(&[0.0, 0.0, 0.16666666666666666, 0.5833333333333334, 0.0, 0.7083333333333334, 0.0, 1.0, 0.0, 1.0, 0.6666666666666666, 0.0, 0.888888888888889, 0.0, 1.0, 0.5, 1.0, 0.6, 0.0, 0.9, 1.0]), 8.881784197001252e-16);
    check_class(&it, "v3", "double");
    check_dims(&it, "v3", &[2, 2, 3]);
    check(&it, "v3", &real(&[0.0, 0.5833333333333334, 0.3333333333333333, 0.16666666666666666, 1.0, 0.6666666666666666, 1.0, 0.8, 1.0, 0.6, 1.0, 0.5]), 8.881784197001252e-16);
    check_class(&it, "v4", "double");
    check_dims(&it, "v4", &[1, 3]);
    check(&it, "v4", &real(&[2.0, 2.0, 3.0]), 8.881784197001252e-16);
    assert!(error_of("r=rgb2hsv(uint8([255 0 0; 10 20 30]));").starts_with("Expected M to be one of these types:"));
    check_class(&it, "v6", "single");
    check_dims(&it, "v6", &[1, 3]);
    check(&it, "v6", &real(&[0.25, 0.5, 0.5]), 1.1920928955078125e-07);
    assert_eq!(error_of("r=rgb2hsv([1 0]);"), "MAP must be a Mx3 array.");
    assert_eq!(error_of("r=hsv2rgb([1.2 0.5 0.5]);"), "Valid colormaps cannot have values outside the range [0,1].");
    check_class(&it, "v9", "double");
    check_dims(&it, "v9", &[1, 3]);
    check(&it, "v9", &real(&[0.6666666666666667, 0.16666666666666663, 0.6]), 8.881784197001252e-16);
    check_class(&it, "v10", "double");
    check_dims(&it, "v10", &[2, 3]);
    check(&it, "v10", &real(&[0.5833333333333334, 0.0, 0.6666666666666666, 1.0, 0.6, 1.0]), 8.881784197001252e-16);
    check_class(&it, "v11", "double");
    check_dims(&it, "v11", &[1, 1, 3]);
    check(&it, "v11", &real(&[0.42000000000000004, 0.7, 0.35]), 8.881784197001252e-16);
    check_class(&it, "v12", "double");
    check_dims(&it, "v12", &[1, 1, 3]);
    check(&it, "v12", &real(&[0.5833333333333334, 0.6666666666666666, 0.6]), 8.881784197001252e-16);
    check_class(&it, "v13", "double");
    check_dims(&it, "v13", &[8, 3]);
    check(&it, "v13", &real(&[0.54, 0.34, 1.0, 1.0, 1.0, 0.3, 0.07999999999999999, 0.08000000000000002, 0.9, 0.27999999999999997, 0.0, 0.0, 1.0, 0.22799999999999998, 0.8, 0.08800000000000002, 0.17999999999999997, 0.4, 0.006000000000000227, 0.0, 1.0, 0.12, 0.5840000000000002, 0.1]), 8.881784197001252e-16);
    check_class(&it, "v14", "double");
    check_dims(&it, "v14", &[2, 1, 3]);
    check(&it, "v14", &real(&[0.9, 0.0, 0.7200000000000001, 0.16000000000000014, 0.45, 0.4]), 8.881784197001252e-16);
    check_class(&it, "v15", "double");
    check_dims(&it, "v15", &[2, 3]);
    check(&it, "v15", &real(&[0.9, 0.0, 0.7200000000000001, 0.16000000000000014, 0.45, 0.4]), 8.881784197001252e-16);
    check_class(&it, "v16", "double");
    check_dims(&it, "v16", &[7, 3]);
    check(&it, "v16", &real(&[0.9791666666666666, 0.41666666666666663, 0.7500000000000001, 0.16666666666666666, 0.41666666666666663, 0.3333333333333333, 0.9166666666666666, 0.888888888888889, 0.888888888888889, 0.888888888888889, 0.6666666666666666, 0.1250000000000001, 0.5, 0.6666666666666666, 0.9, 0.9, 0.9, 0.3, 0.8, 0.5, 0.6]), 8.881784197001252e-16);
    assert!(error_of("r=rgb2hsv(single([0.2 0.4 0.6]));").starts_with("Expected M to be one of these types:"));
    assert!(error_of("r=hsv2rgb(uint8([1 1 1]));").starts_with("Expected H to be one of these types:"));
    assert_eq!(error_of("r=hsv2rgb([0.5 0.5]);"), "MAP must be a Mx3 array.");
    assert_eq!(error_of("r=rgb2hsv([1.5 0 0]);"), "Valid colormaps cannot have values outside the range [0,1].");
    assert_eq!(error_of("r=hsv2rgb([]);"), "MAP must be a Mx3 array.");
    assert_eq!(error_of("r=rgb2hsv(zeros(0,3));"), "Expected M to be nonempty.");
    check_class(&it, "v23", "double");
    check_dims(&it, "v23", &[1, 1, 3]);
    check(&it, "v23", &real(&[0.05555555555555555, 0.75, 0.7843137254901961]), 8.881784197001252e-16);
    check_class(&it, "v24", "double");
    check_dims(&it, "v24", &[1, 1, 3]);
    check(&it, "v24", &real(&[0.25, 0.5, 0.5]), 8.881784197001252e-16);
    check_class(&it, "v25", "double");
    check_dims(&it, "v25", &[1, 1, 3]);
    check(&it, "v25", &real(&[0.0, 1.0, 1.5]), 8.881784197001252e-16);
    check_class(&it, "v26", "double");
    check_dims(&it, "v26", &[125, 3]);
    check(&it, "v26", &real(&[0.8030303030303031, 0.4722222222222222, 0.04166666666666671, 0.09999999999999999, 0.8518518518518519, 0.28888888888888886, 0.008771929824561405, 0.6153846153846154, 0.5833333333333334, 0.3717948717948718, 0.375, 0.08333333333333331, 0.08333333333333325, 0.7291666666666667, 0.9901960784313726, 0.16666666666666666, 0.16666666666666666, 0.9649122807017544, 0.020833333333333332, 0.9705882352941176, 0.7083333333333334, 0.0, 0.6904761904761906, 0.375, 0.8055555555555556, 0.7222222222222223, 0.29166666666666663, 0.6785714285714286, 0.16666666666666669, 0.5833333333333334, 0.8666666666666667, 0.3214285714285714, 0.26666666666666666, 0.5, 0.020833333333333332, 0.7187500000000001, 0.358974358974359, 0.5555555555555556, 0.8333333333333334, 0.21052631578947367, 0.7708333333333334, 0.3611111111111111, 0.3627450980392157, 0.15, 0.3333333333333333, 0.07777777777777778, 0.08823529411764706, 0.5833333333333334, 0.8205128205128206, 0.7708333333333334, 0.5666666666666668, 0.20833333333333331, 0.962962962962963, 0.45555555555555555, 0.9393939393939393, 0.016666666666666663, 0.32352941176470584, 0.8214285714285715, 0.9242424242424243, 0.3125, 0.47435897435897434, 0.06666666666666667, 0.47222222222222215, 0.3214285714285714, 0.2857142857142857, 0.6833333333333335, 0.8055555555555556, 0.7179487179487181, 0.6904761904761906, 0.8205128205128206, 0.1388888888888889, 0.6428571428571429, 0.055555555555555566, 0.035714285714285726, 0.5925925925925927, 0.2156862745098039, 0.0, 0.16666666666666666, 0.0, 0.6944444444444445, 0.16666666666666666, 0.8333333333333334, 0.5, 0.6666666666666667, 0.3333333333333333, 0.0, 0.16666666666666666, 0.8333333333333334, 0.5, 0.6666666666666667, 0.3333333333333333, 0.0, 0.6666666666666667, 0.3333333333333333, 0.0, 0.16666666666666666, 0.8333333333333334, 0.5, 0.6666666666666667, 0.3333333333333333, 0.0, 0.6666666666666667, 0.3333333333333333, 0.0, 0.16666666666666666, 0.8333333333333334, 0.5, 0.16666666666666669, 0.8333333333333333, 0.5, 0.16666666666666666, 0.8333333333333334, 0.5, 0.0, 0.0, 0.0, 0.16666666666666666, 0.8333333333333334, 0.5, 0.16666666666666666, 0.8333333333333334, 0.5, 0.16666666666666666, 0.8333333333333334, 0.5, 0.6875, 0.6666666666666667, 0.21052631578947364, 1.0, 0.47368421052631576, 1.0, 0.95, 0.6842105263157894, 0.6666666666666666, 0.9285714285714285, 1.0, 0.7500000000000001, 0.09999999999999998, 0.888888888888889, 0.9444444444444444, 0.28571428571428564, 1.0, 1.0, 0.888888888888889, 0.8947368421052632, 0.6666666666666666, 0.25, 0.875, 0.8, 1.0, 0.9, 0.8, 0.7368421052631579, 1.0, 0.1250000000000001, 0.8333333333333334, 0.7368421052631579, 0.625, 0.8, 1.0, 1.0, 0.6842105263157894, 1.0, 0.25000000000000006, 1.0, 1.0, 0.8571428571428572, 0.9444444444444444, 0.5, 0.5, 0.75, 0.9444444444444444, 0.2222222222222223, 0.6842105263157894, 1.0, 0.8333333333333334, 0.5714285714285714, 0.9, 0.75, 0.9166666666666666, 0.6666666666666666, 0.9444444444444444, 0.875, 0.6875, 1.0, 0.9285714285714285, 1.0, 0.7500000000000001, 0.7368421052631579, 0.4666666666666666, 1.0, 1.0, 0.6842105263157894, 0.875, 0.8666666666666667, 0.6, 0.875, 1.0, 0.7368421052631579, 0.9, 0.8947368421052632, 0.16666666666666663, 0.625, 0.5, 0.6, 0.5, 0.5, 0.5, 0.5789473684210525, 0.5789473684210525, 0.5789473684210525, 0.8333333333333334, 0.8333333333333334, 0.8333333333333334, 0.25, 0.25, 0.25, 0.47368421052631576, 0.47368421052631576, 0.47368421052631576, 0.6666666666666666, 0.6666666666666666, 0.6666666666666666, 0.9, 0.9, 0.9, 0.5833333333333334, 0.5833333333333334, 0.5833333333333334, 0.33333333333333337, 0.33333333333333337, 0.33333333333333337, 0.3684210526315789, 0.3684210526315789, 0.3684210526315789, 0.8333333333333334, 0.8333333333333334, 0.8333333333333334, 0.0, 0.0, 0.0, 0.6666666666666667, 0.6666666666666667, 0.6666666666666667, 0.1250000000000001, 0.1250000000000001, 0.1250000000000001, 1.0, 1.0, 1.0, 0.8, 0.9, 0.95, 1.0, 0.95, 0.75, 1.0, 0.95, 0.6, 0.7, 0.8, 0.4, 1.0, 0.9, 0.9, 0.7, 0.1, 0.95, 0.9, 0.95, 0.6, 1.0, 0.8, 1.0, 0.3, 1.0, 0.25, 0.95, 0.7, 0.8, 0.3, 0.95, 0.8, 0.5, 0.8, 0.8, 0.95, 0.9, 0.4, 0.95, 0.8, 0.7, 0.9, 1.0, 0.6, 1.0, 0.9, 0.9, 0.95, 0.4, 0.3, 0.7, 1.0, 1.0, 0.6, 0.75, 0.9, 0.8, 0.8, 0.8, 0.7, 1.0, 0.8, 0.95, 0.75, 1.0, 0.9, 0.95, 0.8, 0.75, 1.0, 0.4, 0.3, 0.95, 1.0, 0.95, 0.6, 0.8, 0.5, 1.0, 0.1, 0.1, 0.1, 0.95, 0.95, 0.95, 0.6, 0.6, 0.6, 1.0, 1.0, 1.0, 0.95, 0.95, 0.95, 0.3, 0.3, 0.3, 0.5, 0.5, 0.5, 0.6, 0.6, 0.6, 0.9, 0.9, 0.9, 0.95, 0.95, 0.95, 0.3, 0.3, 0.3, 0.9, 0.9, 0.9, 0.9, 0.9, 0.9, 0.8, 0.8, 0.8, 0.75, 0.75, 0.75]), 8.881784197001252e-16);
    check_class(&it, "v27", "double");
    check_dims(&it, "v27", &[80, 3]);
    check(&it, "v27", &real(&[0.2399999999999996, 0.5, 0.39959999999999946, 0.0, 0.6, 0.7, 0.7325999999999998, 0.5040000000000003, 0.95, 0.9400000000000002, 0.2833333333333335, 0.25, 0.033333333333333326, 0.8739999999999999, 0.7513333333333334, 0.03500000000000003, 0.999, 0.4, 0.3333333333333335, 0.3, 0.6992999999999999, 0.5, 0.24, 0.25499999999999995, 0.6666666666666666, 1.0, 0.10086000000000007, 0.4, 0.0, 0.4920000000000003, 0.6666666666666666, 0.6599999999999999, 0.2, 0.1555555555555554, 0.40833333333333327, 0.9025000000000001, 0.999, 0.19999999999999996, 0.10454999999999999, 0.16666666666666666, 0.09225, 0.0, 0.75, 0.6666666666666666, 0.624375, 0.95, 0.16666666666666666, 0.7799999999999999, 0.123, 0.7, 0.95, 0.7, 0.25, 0.2, 0.5, 0.24, 0.16666666666666666, 0.2, 0.04166666666666667, 0.5, 0.45499999999999996, 0.16666666666666666, 0.00012300000000000012, 0.25, 0.293333333333333, 0.6569499999999999, 1.0, 0.7000000000000005, 0.0, 0.33333333333333337, 0.3466666666666667, 0.95, 0.999, 0.9, 0.4, 0.01999999999999993, 0.95, 0.999, 0.25, 0.5777777777777777, 0.1333333333333333, 0.54, 0.24975, 0.0, 0.6, 0.4, 0.666, 0.11999999999999997, 0.18999999999999995, 0.7, 0.08333333333333333, 0.3, 0.16666666666666666, 0.855, 0.8333333333333334, 0.7, 0.824508, 0.4, 1.0, 0.2214786, 0.999, 0.44166666666666665, 0.25, 0.3, 0.6666666666666666, 0.3350000000000001, 0.012299999999999997, 0.2666666666666667, 0.0, 0.059999999999999984, 0.5333333333333333, 0.6333333333333333, 0.16332000000000002, 0.11111111111111112, 0.11666666666666663, 0.7124999999999999, 0.16649999999999995, 0.19999999999999996, 0.123, 0.02777777777777777, 0.123, 0.0, 0.6000000000000001, 0.5846666666666667, 0.999, 0.09499999999999997, 0.08333333333333333, 0.75, 0.0861, 0.5899599999999999, 0.95, 0.175, 0.25, 0.2, 0.369, 0.2375, 0.16666666666666666, 0.2, 0.05, 0.6, 0.7, 0.16666666666666666, 0.123, 0.25, 0.2, 0.6139, 1.0, 0.16666666666666663, 0.0, 0.6000000000000003, 0.4, 0.04750000000000004, 0.7808849999999999, 0.75, 0.4, 0.0, 0.95, 0.2997, 0.25, 0.6666666666666666, 0.6666666666666666, 0.6, 0.999, 0.0, 0.2, 1.0, 0.999, 0.6, 0.19456000000000007, 1.0, 0.3333333333333333, 0.2, 0.0866666666666667, 0.95, 0.7308333333333333, 0.03500000000000003, 0.333, 0.4, 0.16666666666666663, 0.00030000000000000024, 0.999, 0.4166666666666667, 0.2375, 0.075, 0.39999999999999997, 0.050000000000000044, 0.123, 0.06666666666666665, 0.0, 0.6, 0.5733333333333334, 0.6666666666666666, 0.06000000000000001, 0.3333333333333333, 0.7, 0.95, 0.4162500000000005, 1.0, 0.0861, 0.11111111111111105, 0.10455, 0.0, 0.69, 0.6666666666666666, 0.24975, 0.95, 0.10833333333333339, 0.9, 0.10823999999999999, 0.27999999999999997, 0.38, 0.175, 0.07500000000000001, 0.2, 0.0, 0.25, 0.16666666666666666, 0.6, 0.05, 0.6, 0.21000000000000002, 0.14616666666666667, 0.00012300000000000012, 0.25, 0.6666666666666666, 0.7, 1.0, 0.8333333333333334, 0.0, 1.0, 0.13333333333333336, 0.04750000000000004, 0.16649999999999995, 0.7509, 0.039999999999999994, 0.1, 0.95, 0.999, 0.07500000000000001, 0.5555555555555556]), 8.881784197001252e-16);
}
#[test]
fn signed_zeros_match_matlab() {
    // MATLAB prints these as -0.
    let (it, errors) = run("a = besselj(-3, 0); b = erf(-0); c = legendre(2, 0); d = rat(-0.0625);\n");
    assert!(errors.is_empty(), "{errors:?}");
    let neg_zero = |name: &str, k: usize| {
        let Value::Num(m) = value(&it, name) else { panic!() };
        assert!(m.data[k] == 0.0 && m.data[k].is_sign_negative(), "{name}({}) = {}", k + 1, m.data[k]);
    };
    neg_zero("a", 0);
    neg_zero("b", 0);
    neg_zero("c", 1);
    check_text(&it, "d", &["-0 + 1/(-16)"]);
}
