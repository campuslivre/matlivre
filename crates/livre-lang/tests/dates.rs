//! Date functions and number display thresholds against MATLAB R2023b.

use livre_lang::{Interpreter, OutputKind, Value};

fn run(src: &str) -> (Interpreter, Vec<String>) {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    let out = it
        .take_outputs()
        .into_iter()
        .map(|o| match o.kind {
            OutputKind::Text(t) => t,
            OutputKind::Value { text, .. } => text,
            OutputKind::Error(e) => format!("Error: {e}"),
            OutputKind::Warning(w) => format!("Warning: {w}"),
            OutputKind::Figure(_) => String::new(),
        })
        .collect();
    (it, out)
}

fn num(it: &Interpreter, name: &str) -> Vec<f64> {
    match it.get_var(name) {
        Some(Value::Num(m)) => m.data.clone(),
        other => panic!("{name} = {other:?}"),
    }
}

fn text(it: &Interpreter, name: &str) -> String {
    it.get_var(name).unwrap().text().unwrap()
}

#[test]
fn datestr_and_datevec_match_matlab() {
    let (it, out) = run(concat!(
        "dn = datenum(2023, 10, 4, 13, 45, 30.25);\n",
        "s0 = datestr(dn); s1 = datestr(dn, 'yyyy-mm-dd HH:MM:SS.FFF'); s2 = datestr(dn, 1);\n",
        "s3 = datestr(dn, 'mmmm dd, yyyy (dddd) HH:MM PM'); s4 = datestr(datenum(2023, 10, 4));\n",
        "s5 = datestr([dn; dn + 1], 'yy/mm/dd'); s6 = datestr(datetime(2023, 10, 4, 1, 2, 3)); s8 = datestr([2023 1 2 3 4 5]);\n",
        "v = datevec(dn); v2 = datevec('2023-10-04 13:45:30', 'yyyy-mm-dd HH:MM:SS'); v3 = datevec('04-Oct-2023');\n",
        "[y, m, d] = datevec(dn); et = etime([2023 1 1 0 0 0], [2023 1 2 0 0 1]);\n",
        "ad = datestr(addtodate(dn, 2, 'month')); ad2 = datestr(addtodate(dn, -3, 'day'));\n",
        "ed = eomday(2024, 2); ed2 = eomday([2023 2024], [2 2]); em = datestr(eomdate(2023, 2)); wn = weeknum(datenum(2023, 10, 4));\n",
        "c = calendar(2023, 10); n0 = datenum('04-Oct-2023'); n1 = datenum(2023, 10, 4);\n",
    ));
    assert!(!out.iter().any(|o| o.starts_with("Error")), "{out:?}");
    assert_eq!(text(&it, "s0"), "04-Oct-2023 13:45:30");
    assert_eq!(text(&it, "s1"), "2023-10-04 13:45:30.250");
    assert_eq!(text(&it, "s2"), "04-Oct-2023");
    assert_eq!(text(&it, "s3"), "October 04, 2023 (Wednesday)  1:45 PM");
    assert_eq!(text(&it, "s4"), "04-Oct-2023");
    assert_eq!(it.get_var("s5").unwrap().dims(), [2, 8]);
    assert_eq!(text(&it, "s6"), "04-Oct-2023 01:02:03");
    assert_eq!(text(&it, "s8"), "02-Jan-2023 03:04:05");
    assert_eq!(num(&it, "v"), [2023.0, 10.0, 4.0, 13.0, 45.0, 30.25]);
    assert_eq!(num(&it, "v2"), [2023.0, 10.0, 4.0, 13.0, 45.0, 30.0]);
    assert_eq!(num(&it, "v3"), [2023.0, 10.0, 4.0, 0.0, 0.0, 0.0]);
    assert_eq!((num(&it, "y"), num(&it, "m"), num(&it, "d")), (vec![2023.0], vec![10.0], vec![4.0]));
    assert_eq!(num(&it, "et"), [-86401.0]);
    assert_eq!(text(&it, "ad"), "04-Dec-2023 13:45:30");
    assert_eq!(text(&it, "ad2"), "01-Oct-2023 13:45:30");
    assert_eq!(num(&it, "ed"), [29.0]);
    assert_eq!(num(&it, "ed2"), [28.0, 29.0]);
    assert_eq!(text(&it, "em"), "28-Feb-2023");
    assert_eq!(num(&it, "wn"), [40.0]);
    let c = num(&it, "c");
    // Column-major 6×7: c[6] is row 1, column 2 (2 October); 31 October is row 5, column 3.
    assert_eq!((c.len(), c[0], c[6], c[4 + 6 * 2], c[5]), (42, 1.0, 2.0, 31.0, 0.0));
    assert_eq!(num(&it, "n0"), [739163.0]);
    assert_eq!(num(&it, "n1"), [739163.0]);
}

#[test]
fn datetime_helpers_match_matlab() {
    let (it, out) = run(concat!(
        "t = datetime(2023, 10, 4, 13, 45, 30); [Y, M, D] = ymd(t); [h, mi, sec] = hms(t);\n",
        "iw = isweekend(datetime(2023, 10, [6 7 8])); ib = isbetween(t, datetime(2023, 1, 1), datetime(2023, 12, 31));\n",
        "yd = yyyymmdd(t); cv = convertTo(t, 'posixtime'); xm = x2mdate(45203); mo = months('01-Jan-2023', '15-Apr-2023'); wk = week(t);\n",
        "xt = exceltime(t); jd = juliandate(t);\n",
    ));
    assert!(!out.iter().any(|o| o.starts_with("Error")), "{out:?}");
    assert_eq!((num(&it, "Y"), num(&it, "M"), num(&it, "D")), (vec![2023.0], vec![10.0], vec![4.0]));
    assert_eq!((num(&it, "h"), num(&it, "mi"), num(&it, "sec")), (vec![13.0], vec![45.0], vec![30.0]));
    assert_eq!(num(&it, "iw"), [0.0, 1.0, 1.0]);
    assert_eq!(num(&it, "ib"), [1.0]);
    assert_eq!(num(&it, "yd"), [20231004.0]);
    assert_eq!(num(&it, "cv"), [1696427130.0]);
    assert_eq!(num(&it, "xm"), [739163.0]);
    assert_eq!(num(&it, "mo"), [3.0]);
    assert_eq!(num(&it, "wk"), [40.0]);
    assert!((num(&it, "xt")[0] - 45203.573263888889).abs() < 1e-9);
    assert!((num(&it, "jd")[0] - 2460222.0732638889).abs() < 1e-7);
}

#[test]
fn number_display_switches_where_matlab_does() {
    let (_, out) = run(concat!(
        "a = 999.5\nb = 1000.5\nc = 0.001\nd = 0.0011\ne = -1234.5\n",
        "format long\ng = 99.5\nh = 100.5\nformat short\n",
        "m2 = [1 1000.5]\nm5 = [0.0009 0.0001]\n",
    ));
    assert_eq!(&out[..7], ["a = 999.5000", "b = 1.0005e+03", "c = 1.0000e-03", "d = 0.0011", "e = -1.2345e+03", "g = 99.500000000000000", "h = 1.005000000000000e+02"]);
    assert!(out[7].contains("1.0e+03 *") && out[7].contains("1.0005"), "{:?}", out[7]);
    assert!(out[8].contains("1.0e-03 *") && out[8].contains("0.9000"), "{:?}", out[8]);
}
