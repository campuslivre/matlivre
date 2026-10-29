//! sym() conversion of doubles and 32-digit vpa, against output recorded from MATLAB R2023b.

use livre_lang::{Interpreter, OutputKind};

fn run(src: &str) -> Vec<String> {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    it.take_outputs()
        .into_iter()
        .map(|o| match o.kind {
            OutputKind::Text(t) => t,
            OutputKind::Value { text, .. } => text,
            OutputKind::Error(e) => format!("Error: {e}"),
            OutputKind::Warning(w) => format!("Warning: {w}"),
            OutputKind::Figure(_) => String::new(),
        })
        .collect()
}

/// char() of each expression, one per line.
fn chars(exprs: &[&str]) -> Vec<String> {
    let src: String = exprs.iter().map(|e| format!("disp(char({e}))\n")).collect();
    run(&src).iter().flat_map(|s| s.lines().map(str::to_string).collect::<Vec<_>>()).filter(|l| !l.is_empty()).collect()
}

#[test]
fn sym_converts_doubles_as_matlab_does() {
    let cases = [
        ("sym(0.1)", "1/10"),
        ("sym(0.1 + 0.2)", "3/10"),
        ("sym(0.2385)", "477/2000"),
        ("sym(1/7 + 12*eps)", "1/7"),
        ("sym(1/7 + 16*eps)", "2573485501354633/18014398509481984"),
        ("sym(100000/3)", "100000/3"),
        ("sym(100001/3)", "4581344262051157/137438953472"),
        ("sym(1/100001)", "5902899074596311/590295810358705651712"),
        ("sym(123456.789)", "8483885939586761/68719476736"),
        ("sym(-10.000000001523087)", "-2814749767535271/281474976710656"),
        ("sym(1.7453292519943296e-05)", "1287825681435933/73786976294838206464"),
        ("sym(3*pi/4)", "(3*pi)/4"),
        ("sym(pi/100000)", "pi/100000"),
        ("sym(sqrt(2))", "2^(1/2)"),
        ("sym(sqrt(99))", "3*11^(1/2)"),
        ("sym(1e-7)", "1/10000000"),
        ("sym(1e23)", "100000000000000000000000"),
        ("sym(2^-100)", "1/1267650600228229401496703205376"),
        ("sym(3e-30)", "8563486156235759/2854495385411919762116571938898990272765493248"),
    ];
    let got = chars(&cases.iter().map(|c| c.0).collect::<Vec<_>>());
    for ((src, want), g) in cases.iter().zip(&got) {
        assert_eq!(g, want, "{src}");
    }
}

#[test]
fn vpa_prints_32_significant_digits_like_matlab() {
    let cases = [
        ("vpa(sym(1)/3)", "0.33333333333333333333333333333333"),
        ("vpa(sym(2)/3)", "0.66666666666666666666666666666667"),
        ("vpa(pi)", "3.1415926535897932384626433832795"),
        ("vpa(exp(sym(1)))", "2.7182818284590452353602874713527"),
        ("vpa(sqrt(sym(2)))", "1.4142135623730950488016887242097"),
        ("vpa(sym(5))", "5.0"),
        ("vpa(sym(-1)/2)", "-0.5"),
        ("vpa(sym(0))", "0.0"),
        ("vpa(sym(123456789)/1000)", "123456.789"),
        ("vpa(sym(2)^100)", "1267650600228229401496703205376.0"),
        ("vpa(sym(10)^40)", "1.0e+40"),
        ("vpa(sym(1e-40))", "1.0e-40"),
        ("vpa(sym(2)^-200)", "6.2230152778611417071440640537801e-61"),
        ("vpa(-sym(10)^35/3)", "-3.3333333333333333333333333333333e+34"),
        ("vpa(sym(1)/7*1e-5)", "0.0000014285714285714285714285714285714"),
        ("vpa(sym(10)^32/3)", "33333333333333333333333333333333.0"),
        ("vpa(sym(10)^33/3)", "333333333333333333333333333333330.0"),
        ("vpa(sym(1)/3*sym(10)^-32)", "3.3333333333333333333333333333333e-33"),
        ("vpa(sym(1)/10^31)", "0.0000000000000000000000000000001"),
        ("vpa(erf(sym(477)/2000*sym(-19.000000003046174)/10))", "-0.47838042524405578130631659319233"),
        ("vpa(sym(2)/3) + 1", "1.6666666666666666666666666666667"),
    ];
    let got = chars(&cases.iter().map(|c| c.0).collect::<Vec<_>>());
    for ((src, want), g) in cases.iter().zip(&got) {
        assert_eq!(g, want, "{src}");
    }
}

#[test]
fn vpa_keeps_variables_and_honours_digits() {
    let got = chars(&["vpa(sym('x')/3)", "vpa(sym(1)/3, 10)"]);
    assert_eq!(got[0], "0.33333333333333333333333333333333*x");
    assert_eq!(got[1], "0.3333333333");
    let out = run("old = digits(10); a = char(vpa(pi)); digits(old); b = char(vpa(pi)); disp(a); disp(b)");
    let text: Vec<&str> = out.iter().flat_map(|s| s.lines()).filter(|l| !l.is_empty()).collect();
    assert_eq!(text, ["3.141592654", "3.1415926535897932384626433832795"]);
}
