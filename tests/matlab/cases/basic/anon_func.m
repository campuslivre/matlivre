% Anonymous functions and their text.
f = @(t) t.^2 + 1;
g = @(a, b) a*b - sin(a);
h = @sin;
s1 = func2str(f);
s2 = func2str(g);
s3 = func2str(h);
