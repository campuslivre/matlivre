//! containers.Map, dictionary and categorical against MATLAB R2023b.

use livre_lang::{Interpreter, OutputKind};

/// The displayed text of each statement (errors and printed text included).
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

fn check(src: &str, expected: &[&str]) {
    let out = shown(src);
    assert_eq!(out, expected, "{src}");
}

#[test]
fn map_basics() {
    check(
        "m = containers.Map();\nm('b') = 2;\nm('a') = [1 2 3];\nm\nk = keys(m)\nv = values(m)\nc = m.Count\nx = m('a')\ntf = isKey(m, {'a','zz'})\nremove(m, 'b');\nk2 = keys(m)\n",
        &[
            "m = \n  Map with properties:\n\n        Count: 2\n      KeyType: char\n    ValueType: any",
            "k = 1×2 cell\n\n    {'a'}    {'b'}",
            "v = 1×2 cell\n\n    {[1 2 3]}    {[2]}",
            "c = uint64\n   2",
            "x = 1×3\n\n     1     2     3",
            "tf = 1×2 logical array\n\n   1   0",
            "k2 = 1×1 cell\n\n    {'a'}",
        ],
    );
}

#[test]
fn map_types_order_and_errors() {
    check(
        concat!(
            "m2 = containers.Map({'one','two','three'}, [1 2 3]);\nv3 = values(m2)\ns = size(m2)\nm3 = containers.Map([3 1 2], {'c','a','b'});\nk3 = keys(m3)\nt3 = m3.ValueType\n",
            "try, m2('nope'), catch e, disp(e.message), end\ntry, m3('a'), catch e, disp(e.message), end\ntry, m2('one') = 'x'; catch e, disp(e.message), end\n",
            "m6 = m2; m6('four') = 4; c2 = m2.Count\nk = m2.keys\nvx = containers.Map({'a','b'}, {[1 2], 3}).ValueType\ncls = class(m2)\n",
        ),
        &[
            "v3 = 1×3 cell\n\n    {[1]}    {[3]}    {[2]}",
            "s = 1×2\n\n     3     1",
            "k3 = 1×3 cell\n\n    {[1]}    {[2]}    {[3]}",
            "t3 = 'char'",
            "The specified key is not present in this container.\n",
            "Specified key type does not match the type expected for this container.\n",
            "Specified value type does not match the type expected for this container.\n",
            "c2 = uint64\n   4",
            "k = 1×4 cell\n\n    {'four'}    {'one'}    {'three'}    {'two'}",
            "vx = 'any'",
            "cls = 'containers.Map'",
        ],
    );
}

#[test]
fn dictionary_basics() {
    check(
        concat!(
            "d = dictionary([\"b\" \"a\" \"c\"], [2 1 3]);\nd(\"z\") = 26\ny = d([\"a\" \"c\"])\nk = keys(d)\nn = numEntries(d)\n",
            "d = remove(d, \"b\");\nd(\"a\") = []\ne = dictionary\n[kt, vt] = types(d)\nd2 = dictionary([1 2 3], [\"one\" \"two\" \"three\"]);\nd2(2)\n",
            "d3 = dictionary(\"a\", {[1 2]})\nd4 = dictionary(string.empty, double.empty)\ntry, d(\"nope\"), catch err, disp(err.message), end\n",
        ),
        &[
            "d = \n  dictionary (string --> double) with 4 entries:\n\n    \"b\" --> 2\n    \"a\" --> 1\n    \"c\" --> 3\n    \"z\" --> 26",
            "y = 1×2\n\n     1     3",
            "k = 4×1 string array\n\n    \"b\"\n    \"a\"\n    \"c\"\n    \"z\"",
            "n = 4",
            "d = \n  dictionary (string --> double) with 2 entries:\n\n    \"c\" --> 3\n    \"z\" --> 26",
            "e = \n  dictionary with unset key and value types.",
            "kt = \"string\"",
            "vt = \"double\"",
            "ans = \"two\"",
            "d3 = \n  dictionary (string --> cell) with 1 entry:\n\n    \"a\" --> {[1 2]}",
            "d4 = \n  dictionary (string --> double) with no entries.",
            "Key not found.\n",
        ],
    );
}

#[test]
fn categorical_display_and_categories() {
    check(
        concat!(
            "c = categorical({'red','blue','red','green'})\ncats = categories(c)\nn = countcats(c)\nc2 = categorical([1 2 1 3], [1 2 3], {'lo','mid','hi'})\n",
            "s1 = c(2)\nc(5) = 'blue';\nc7 = mergecats(c, {'red','green'}, 'warm')\nsummary(c)\nb = categorical({'red';'green';'blue'})\n",
            "a = categorical({'a','bbb';'cc','d'})\ne = categorical({})\nm = categorical([1 NaN 2])\n",
        ),
        &[
            "c = 1×4 categorical array\n\n     red      blue      red      green ",
            "cats = 3×1 cell\n\n    {'blue' }\n    {'green'}\n    {'red'  }",
            "n = 1×3\n\n     1     1     2",
            "c2 = 1×4 categorical array\n\n     lo      mid      lo      hi ",
            "s1 = categorical\n     blue ",
            "c7 = 1×5 categorical array\n\n     warm      blue      warm      warm      blue ",
            "     blue      green      red \n     2         1          2   \n",
            "b = 3×1 categorical array\n\n     red \n     green \n     blue ",
            "a = 2×2 categorical array\n\n     a       bbb \n     cc      d   ",
            "e = \n  0×0 empty categorical array",
            "m = 1×3 categorical array\n\n     1      <undefined>      2 ",
        ],
    );
}

#[test]
fn categorical_operations() {
    check(
        concat!(
            "c = categorical({'red','blue','red','green','blue'});\neq = c == 'red'\nd = double(c)\nx = categorical([\"a\" \"b\" \"a\"]);\ncc = [c x]\n",
            "uu = unique(c)\nt = setcats(c, {'red','blue'})\nz = categorical({'a';'b';'a'});\n[g, gn] = findgroups(z)\n",
            "c3 = categorical({'s','m','l','m'}, {'s','m','l'}, 'Ordinal', true);\ntf = c3 > 'm'\ntry, c3(1) = 'zz'; catch er, disp(er.message), end\n",
            "q = categorical({'a','b'}); q(2) = 'new';\ncq = categories(q)\nr = c(c == 'blue')\ns.f = c(1:3);\ns.g = c(1)\ncel = {c(1)}\n",
        ),
        &[
            "eq = 1×5 logical array\n\n   1   0   1   0   0",
            "d = 1×5\n\n     3     1     3     2     1",
            "cc = 1×8 categorical array\n\n     red      blue      red      green      blue      a      b      a ",
            "uu = 1×3 categorical array\n\n     blue      green      red ",
            "t = 1×5 categorical array\n\n     red      blue      red      <undefined>      blue ",
            "g = 3×1\n\n     1\n     2\n     1",
            "gn = 2×1 categorical array\n\n     a \n     b ",
            "tf = 1×4 logical array\n\n   0   0   1   0",
            "Cannot add a new category 'zz' to this categorical array because its categories are protected.  Use ADDCATS to add the new category.\n",
            "cq = 3×1 cell\n\n    {'a'  }\n    {'b'  }\n    {'new'}",
            "r = 1×2 categorical array\n\n     blue      blue ",
            "s = struct with fields:\n\n    f: [red    blue    red]\n    g: red",
            "cel = 1×1 cell\n\n    {[red]}",
        ],
    );
}
