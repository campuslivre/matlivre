//! Differential tests: every snippet must leave identical outputs, errors and variables with
//! native compilation on and off, and the compiled path must actually be taken.

use livre_lang::{Interpreter, OutputKind};

fn outputs(it: &mut Interpreter) -> Vec<String> {
    it.take_outputs()
        .into_iter()
        .map(|o| match o.kind {
            OutputKind::Text(t) => t,
            OutputKind::Value { text, .. } => text,
            OutputKind::Figure(_) => "<figure>".into(),
            OutputKind::Warning(w) => format!("W:{w}"),
            OutputKind::Error(e) => format!("E{}:{e}", o.line),
        })
        .collect()
}

fn snapshot(it: &Interpreter) -> Vec<String> {
    it.workspace().into_iter().map(|v| format!("{} {} {} {}", v.name, v.class, v.size, v.value)).collect()
}

/// Runs `src` both ways; returns how many loops ran compiled.
fn same(src: &str) -> usize {
    // Deep interpreted recursion needs more than the default test-thread stack.
    let src = src.to_string();
    std::thread::Builder::new().stack_size(256 << 20).spawn(move || same_on_thread(&src)).unwrap().join().unwrap()
}

fn same_on_thread(src: &str) -> usize {
    let mut a = Interpreter::new();
    a.set_jit(false);
    let ra = a.run_source(src).map_err(|e| (e.message, e.line));
    let oa = outputs(&mut a);
    let mut b = Interpreter::new();
    b.set_jit(true);
    let rb = b.run_source(src).map_err(|e| (e.message, e.line));
    let ob = outputs(&mut b);
    assert_eq!(ra, rb, "result differs for:\n{src}");
    assert_eq!(oa, ob, "outputs differ for:\n{src}");
    assert_eq!(snapshot(&a), snapshot(&b), "variables differ for:\n{src}");
    b.jit_runs()
}

fn compiled(src: &str) {
    assert!(same(src) > 0, "expected the loop to compile:\n{src}");
}

#[test]
fn benchmark_loops() {
    compiled("s = 0;\nfor k = 1:200000\n  s = s + k * k;\nend\ns");
    compiled("total = 0;\nfor n = 1:2000\n  m = n;\n  while m ~= 1\n    if mod(m, 2) == 0\n      m = m / 2;\n    else\n      m = 3 * m + 1;\n    end\n    total = total + 1;\n  end\nend\ntotal");
    compiled("v = [];\nfor k = 1:1000\n  v(end + 1) = k;\nend\ns = sum(v)\nsize(v)");
    compiled("x = zeros(1, 1000);\nfor k = 1:1000\n  x(k) = k / 2;\nend\nsum(x)");
    compiled("p = struct('x', 0, 'y', 0);\nfor k = 1:1000\n  p.x = p.x + 1;\n  p.y = p.y + p.x;\nend\np");
    compiled("c = cell(1, 100);\nfor k = 1:100\n  c{k} = k;\nend\nt = 0;\nfor k = 1:100\n  t = t + c{k};\nend\nt\nclass(c{5})");
}

#[test]
fn recursion_and_functions() {
    compiled("f = 0;\nfor r = 1:2\n  f = fib(15);\nend\nf\nfunction r = fib(n)\n  if n < 2\n    r = n;\n  else\n    r = fib(n - 1) + fib(n - 2);\n  end\nend");
    compiled("s = 0;\nfor k = 1:50\n  s = s + sq(k) - twice(k);\nend\ns\nfunction y = sq(x)\n  y = x^2;\nend\nfunction y = twice(x)\n  y = 2 * x;\nend");
}

#[test]
fn direct_calls() {
    compiled("f = fib(18)\nfib(10)\nfunction r = fib(n)\n  if n < 2\n    r = n;\n  else\n    r = fib(n - 1) + fib(n - 2);\n  end\nend");
    compiled("y = down(300)\nfunction r = down(n)\n  if n <= 0\n    r = 0;\n  else\n    r = down(n - 1) + 1;\n  end\nend");
    compiled("y = bad(1)\nz = bad(-1)\nfunction r = bad(a)\n  if a > 0\n    r = a;\n  else\n    r = ~NaN + 1;\n  end\nend");
    // Non-scalar arguments are interpreted.
    assert_eq!(same("y = twice([1 2 3])\nfunction y = twice(x)\n  y = 2 * x;\nend"), 0);
}

#[test]
fn control_flow_and_semantics() {
    compiled("s = 0;\nfor k = 1:20\n  if k == 3\n    continue\n  elseif k > 10 && mod(k, 2) == 1\n    break\n  end\n  s = s + k;\nend\ns\nk");
    compiled("a = 0;\nfor k = 10:-3:-5\n  a = a + k;\nend\na\nk");
    compiled("n = 0;\nfor q = 0.1:0.1:1\n  n = n + 1;\nend\nn\nq");
    compiled("x = 0;\nfor k = 1:0\n  x = 1;\nend\nx\nk");
    compiled("m = [];\nr = 0;\nfor k = 1:5\n  r = max(r, mod(-k, 3)) + rem(-k, 3) + min(NaN, k) + max(k, NaN);\nend\nr");
    compiled("flag = false;\nfor k = 1:5\n  flag = flag | (k > 3);\nend\nflag\nclass(flag)");
    compiled("y = 0;\nfor k = 1:10\n  y = y + abs(sin(k)) + floor(k / 3) + round(k / 4) + fix(-k / 3) + sign(k - 5);\nend\ny");
    compiled("A = zeros(3, 4);\nfor i = 1:3\n  for j = 1:4\n    A(i, j) = i * 10 + j;\n  end\nend\nA\nA(5, 2) = 1;\nsize(A)");
    compiled("B = zeros(2, 2);\nfor k = 1:3\n  B(k, k) = k;\nend\nB");
    compiled("x = 5;\nwhile x > 0\n  x = x - 2;\nend\nx");
    compiled("total = 0;\nfor k = 1:5\n  if k > 2\n    newv = k;\n  end\nend\nexist('newv')");
}

#[test]
fn errors_match() {
    compiled("x = zeros(1, 3);\ns = 0;\nfor k = 1:5\n  s = s + x(k);\nend");
    compiled("x = zeros(1, 3);\nfor k = 1:5\n  x(k - 0.5) = 1;\nend");
    compiled("x = 1:5;\nfor k = 1:3\n  x(k - 1) = 0;\nend");
    compiled("A = zeros(2, 2);\nt = 0;\nfor k = 1:3\n  t = t + A(k, 1);\nend");
    compiled("n = 0;\nfor k = 1:3\n  n = n + 1;\n  y = ~NaN;\nend");
}

#[test]
fn falls_back_when_not_compilable() {
    // Display, strings, sqrt (may go complex) and whole-array use stay interpreted.
    assert_eq!(same("for k = 1:3\n  k\nend"), 0);
    assert_eq!(same("s = 0;\nfor k = -2:2\n  s = s + sqrt(k);\nend\ns"), 0);
    assert_eq!(same("x = [1 2 3];\nfor k = 1:2\n  y = sum(x);\nend\ny"), 0);
}
