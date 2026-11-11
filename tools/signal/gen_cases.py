"""Builds tests/all/data/signal_cases.m from tools/signal/cases.txt.

The script defines a few test signals, runs every case inside try/catch and prints each result
with %.17g (class, size, real parts, imaginary parts) or the error message. The same script runs
in MATLAB, to record signal_matlab.txt, and in MatLivre, from the test signal_matlab.rs:

    python tools/signal/gen_cases.py
    matlab -batch "run('crates/livre-lang/tests/all/data/signal_cases.m')" > out.txt
    # keep only the result lines (those starting with <case>[.<n>]|) as signal_matlab.txt
"""
import os

root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
src = os.path.join(root, "tools", "signal", "cases.txt")
dst = os.path.join(root, "crates", "livre-lang", "tests", "all", "data", "signal_cases.m")

cases = [line.rstrip("\n") for line in open(src, encoding="utf-8")]
cases = [c for c in cases if c.strip() and not c.startswith("#")]

LOOP = r"""for i = 1:numel(cases)
  try
    r = 'unset';
    eval(cases{i});
    if iscell(r), items = r; else, items = {r}; end
    for k = 1:numel(items)
      v = items{k};
      if isnumeric(v) || islogical(v) || ischar(v)
        v2 = double(v(:)).';
        fprintf('%d.%d|%s|%s|%s|%s\n', i, k, class(v), sprintf('%d ', size(v)), sprintf('%.17g ', real(v2)), sprintf('%.17g ', imag(v2)));
      else
        fprintf('%d.%d|%s\n', i, k, class(v));
      end
    end
  catch ME
    fprintf('%d|ERR|%s\n', i, strrep(ME.message, char(10), ' '));
  end
end"""

out = [
    "x = sin(0.3*(1:100)) + 0.5*cos(1.1*(1:100));",
    "y = cos(0.9*(1:100)) + 0.3*sin(2.4*(1:100));",
    "y2 = y;",
    "t = linspace(0,4*pi,13);",
    "cases = {",
]
out += ["'" + c.replace("'", "''") + "'" for c in cases]
out.append("};")
out.append(LOOP)
with open(dst, "w", encoding="utf-8", newline="\n") as f:
    f.write("\n".join(out) + "\n")
print(len(cases), "cases ->", dst)
