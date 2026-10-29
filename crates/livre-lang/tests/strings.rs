//! Regular expressions, string utilities and base conversions against MATLAB R2023b.

use livre_lang::{Interpreter, OutputKind, Value};

fn run(src: &str) -> Interpreter {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    let errors: Vec<String> = it
        .take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Error(e) => Some(e),
            _ => None,
        })
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    it
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

/// The texts of a cell array or string array.
fn texts(it: &Interpreter, name: &str) -> Vec<String> {
    match it.get_var(name) {
        Some(Value::Cell(c)) => c.data.iter().map(|v| v.text().unwrap()).collect(),
        Some(Value::Str(s)) => s.data.iter().map(|t| t.clone().unwrap_or_default()).collect(),
        other => panic!("{name} = {other:?}"),
    }
}

fn dims(it: &Interpreter, name: &str) -> Vec<usize> {
    it.get_var(name).unwrap().dims()
}

#[test]
fn regexp_outputs_match_matlab() {
    let it = run(concat!(
        "s = 'ab12cd345e';\n[st, en, te, ma, to, nm, sp] = regexp(s, '\\d+');\n",
        "m = regexp(s, '[a-z]+', 'match'); t1 = regexp(s, '([a-z])(\\d)', 'tokens', 'once');\n",
        "n = regexp('x=12, y=7', '(?<key>\\w)=(?<val>\\d+)', 'names'); nn = regexp('none', '(?<k>\\d)', 'names');\n",
        "o = regexp(s, '\\d', 'match', 'once'); o0 = regexp(s, 'z', 'match', 'once'); e0 = regexp(s, 'z'); m0 = regexp(s, 'z', 'match');\n",
        "sp2 = regexp('a,b,,c', ',', 'split'); [a1, a2] = regexp(s, '\\d+', 'match', 'start');\n",
        "ci = regexpi('ABC abc', 'abc', 'match'); ss = regexp(\"a1b2\", '\\d', 'match');\n",
        "ml = regexp(sprintf('l1\\nl2'), '^l\\d$', 'match'); ml2 = regexp(sprintf('l1\\nl2'), '^l\\d$', 'match', 'lineanchors');\n",
        "dot = regexp(sprintf('a\\nb'), 'a.b', 'match'); wb = regexp('the cat sat', '\\<\\w', 'match');\n",
        "lz = regexp('<a><b>', '<.*?>', 'match'); lb = regexp('price: $42', '(?<=\\$)\\d+', 'match');\n",
        "em = regexp('abc', 'x*', 'match'); em2 = regexp('abc', 'x*', 'match', 'emptymatch');\n",
    ));
    assert_eq!(num(&it, "st"), [3.0, 7.0]);
    assert_eq!(num(&it, "en"), [4.0, 9.0]);
    assert_eq!(texts(&it, "ma"), ["12", "345"]);
    assert_eq!(texts(&it, "sp"), ["ab", "cd", "e"]);
    assert_eq!(texts(&it, "m"), ["ab", "cd", "e"]);
    assert_eq!(texts(&it, "t1"), ["b", "1"]);
    match it.get_var("n") {
        Some(Value::Struct(s)) => {
            assert_eq!(s.fields, ["key", "val"]);
            assert_eq!(s.dims, [1, 2]);
            assert_eq!(s.get_at(1, "val").unwrap().text().unwrap(), "7");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(dims(&it, "nn"), [0, 0]);
    assert_eq!(text(&it, "o"), "1");
    assert_eq!(dims(&it, "o0"), [0, 0]);
    assert_eq!(dims(&it, "e0"), [0, 0]);
    assert_eq!(dims(&it, "m0"), [0, 0]);
    assert_eq!(texts(&it, "sp2"), ["a", "b", "", "c"]);
    assert_eq!(texts(&it, "a1"), ["12", "345"]);
    assert_eq!(num(&it, "a2"), [3.0, 7.0]);
    assert_eq!(texts(&it, "ci"), ["ABC", "abc"]);
    assert!(matches!(it.get_var("ss"), Some(Value::Str(_))));
    assert_eq!(dims(&it, "ml"), [0, 0]);
    assert_eq!(texts(&it, "ml2"), ["l1", "l2"]);
    assert_eq!(texts(&it, "dot"), ["a\nb"]);
    assert_eq!(texts(&it, "wb"), ["t", "c", "s"]);
    assert_eq!(texts(&it, "lz"), ["<a>", "<b>"]);
    assert_eq!(texts(&it, "lb"), ["42"]);
    assert_eq!(dims(&it, "em"), [0, 0]);
    assert_eq!(texts(&it, "em2").len(), 4);
}

#[test]
fn regexprep_and_regexptranslate_match_matlab() {
    let it = run(concat!(
        "r1 = regexprep('hello world', 'o', '0'); r2 = regexprep('hello world', '(\\w+) (\\w+)', '$2 $1');\n",
        "r3 = regexprep('aaa', 'a', 'b', 'once'); r4 = regexprep('abc', 'B', 'x', 'ignorecase');\n",
        "r5 = regexprep({'a1', 'b2'}, '\\d', '#'); r6 = regexprep(\"x1\", '\\d', 'N'); r7 = regexprep('a.b', '\\.', '\\n');\n",
        "r8 = regexprep('abcabc', 'b', 'X', 2); r9 = regexprep('Hello', '(?<first>H)', '$<first>$<first>');\n",
        "tr = regexptranslate('escape', 'a.b*c'); tw = regexptranslate('wildcard', '*.m');\n",
    ));
    assert_eq!(text(&it, "r1"), "hell0 w0rld");
    assert_eq!(text(&it, "r2"), "world hello");
    assert_eq!(text(&it, "r3"), "baa");
    assert_eq!(text(&it, "r4"), "axc");
    assert_eq!(texts(&it, "r5"), ["a#", "b#"]);
    assert!(matches!(it.get_var("r6"), Some(Value::Str(_))));
    assert_eq!(text(&it, "r7"), "a\nb");
    assert_eq!(text(&it, "r8"), "abcaXc");
    assert_eq!(text(&it, "r9"), "HHello");
    assert_eq!(text(&it, "tr"), "a\\.b\\*c");
    assert_eq!(text(&it, "tw"), ".*\\.m");
}

#[test]
fn string_utilities_match_matlab() {
    let it = run(concat!(
        "[t, r] = strtok('  hello world  '); [t2, r2] = strtok('a,b,c', ',');\n",
        "sl = splitlines(sprintf('a\\nb\\r\\nc')); x1 = sscanf('3 4 5', '%d'); x2 = sscanf('1.5,2.5', '%f,'); x3 = sscanf('abc', '%s');\n",
        "[x4, n4] = sscanf('10 20 30 40', '%d', [2 2]); x5 = sscanf('12abc', '%d'); x6 = sscanf('ff 10', '%x');\n",
        "v1 = strvcat('a', '', 'abc'); v2 = str2mat('a', '', 'abc'); sm = strmatch('ab', {'abc'; 'xab'; 'ab'});\n",
        "sm2 = strmatch('ab', {'abc'; 'xab'; 'ab'}, 'exact'); fs = findstr('abcabc', 'bc');\n",
        "eb = eraseBetween('Hello World', 3, 5); rb = replaceBetween('Hello World', 1, 5, 'Howdy'); eb2 = eraseBetween(\"The quick fox\", \"The \", \" fox\");\n",
        "mt = matches({'apple', 'Apple', 'pear'}, 'apple'); mt2 = matches(\"Apple\", \"apple\", 'IgnoreCase', true);\n",
    ));
    assert_eq!((text(&it, "t"), text(&it, "r")), ("hello".into(), " world  ".into()));
    assert_eq!((text(&it, "t2"), text(&it, "r2")), ("a".into(), ",b,c".into()));
    assert!(matches!(it.get_var("sl"), Some(Value::Cell(_))));
    assert_eq!(texts(&it, "sl"), ["a", "b", "c"]);
    assert_eq!(num(&it, "x1"), [3.0, 4.0, 5.0]);
    assert_eq!(num(&it, "x2"), [1.5, 2.5]);
    assert_eq!(text(&it, "x3"), "abc");
    assert_eq!(num(&it, "x4"), [10.0, 20.0, 30.0, 40.0]);
    assert_eq!(num(&it, "n4"), [4.0]);
    assert_eq!(num(&it, "x5"), [12.0]);
    assert_eq!(num(&it, "x6"), [255.0, 16.0]);
    assert_eq!(dims(&it, "v1"), [2, 3]);
    assert_eq!(dims(&it, "v2"), [3, 3]);
    assert_eq!(num(&it, "sm"), [1.0, 3.0]);
    assert_eq!(num(&it, "sm2"), [3.0]);
    assert_eq!(num(&it, "fs"), [2.0, 5.0]);
    assert_eq!(text(&it, "eb"), "He World");
    assert_eq!(text(&it, "rb"), "Howdy World");
    assert_eq!(text(&it, "eb2"), "The  fox");
    assert_eq!(num(&it, "mt"), [1.0, 0.0, 0.0]);
    assert_eq!(num(&it, "mt2"), [1.0]);
}

#[test]
fn base_conversions_match_matlab() {
    let it = run(concat!(
        "b1 = dec2bin(10); b2 = dec2bin([1 5 10]); b3 = dec2bin(5, 8); h1 = dec2hex(255); h2 = dec2hex([1 255], 4);\n",
        "d1 = bin2dec('1010'); d2 = bin2dec({'1', '111'}); d3 = hex2dec('FF'); d4 = hex2dec({'a', 'FF'});\n",
        "db = dec2base(255, 16); bd = base2dec('zz', 36); nh = num2hex(1); hn = hex2num('3ff0000000000000'); nh2 = num2hex(single(1));\n",
    ));
    assert_eq!(text(&it, "b1"), "1010");
    assert_eq!(dims(&it, "b2"), [3, 4]);
    assert_eq!(text(&it, "b3"), "00000101");
    assert_eq!(text(&it, "h1"), "FF");
    assert_eq!(dims(&it, "h2"), [2, 4]);
    assert_eq!(num(&it, "d1"), [10.0]);
    assert_eq!(num(&it, "d2"), [1.0, 7.0]);
    assert_eq!(num(&it, "d3"), [255.0]);
    assert_eq!(num(&it, "d4"), [10.0, 255.0]);
    assert_eq!(text(&it, "db"), "FF");
    assert_eq!(num(&it, "bd"), [1295.0]);
    assert_eq!(text(&it, "nh"), "3ff0000000000000");
    assert_eq!(num(&it, "hn"), [1.0]);
    assert_eq!(text(&it, "nh2"), "3f800000");
}
