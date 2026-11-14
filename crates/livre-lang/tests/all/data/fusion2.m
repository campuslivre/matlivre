N = 60000; x = linspace(0, 1, N); y = mod((1:N)*7919, 1013) / 1013 - 0.5; z = y; z(5) = NaN;
M = reshape(mod((1:N)*7, 101) / 101, 200, 300); rv = (1:300) / 300; cv = (1:200)' / 200;
l1 = x > 0.25; l2 = x < 0.75; a1 = l1 & l2;
a = x > 0.25 & x < 0.75; fprintf('g1 %d %s %d\n', isequal(a, a1), class(a), sum(a));
k1 = find(a1); k = find(x > 0.25 & x < 0.75); fprintf('g2 %d %s %d\n', isequal(k, k1), mat2str(size(k)), numel(k));
yc = y(:); kc = find(yc > 0.1 & yc < 0.3); kc1 = find((yc > 0.1) & (yc < 0.3)); fprintf('g3 %d %s\n', isequal(kc, kc1), mat2str(size(kc)));
ke = find(x > 2 & x < 3); fprintf('g4 %s\n', mat2str(size(ke)));
kce = find(yc > 2 & yc < 3); fprintf('g5 %s\n', mat2str(size(kce)));
km = find(M > 0.3 & M < 0.6); km1 = find((M > 0.3) & (M < 0.6)); fprintf('g6 %d %s\n', isequal(km, km1), mat2str(size(km)));
d1 = (x > 0.5); d2 = (x < 0.2); d3 = 2 * d2; d4 = d1 + d3; d = (x > 0.5) + 2 * (x < 0.2); fprintf('g7 %d %s\n', isequal(d, d4), class(d));
n1 = x > 0.5; n1 = ~n1; n2 = y < 0; n1 = n1 | n2; n = ~(x > 0.5) | (y < 0); fprintf('g8 %d %s\n', isequal(n, n1), class(n));
q1 = x == 0.5; q2 = y ~= 0; q3 = y >= 0.25; q4 = x <= 0.1; q1 = q1 | q2; q1 = q1 & q3; q1 = q1 | q4;
q = (x == 0.5 | y ~= 0) & y >= 0.25 | x <= 0.1; fprintf('g9 %d %d\n', isequal(q, q1), sum(q));
try, w = z > 0 & z < 1; r1 = 'none'; catch err, r1 = err.message; end
try, w1 = z > 0; w2 = z < 1; w3 = w1 & w2; r2 = 'none'; catch err, r2 = err.message; end
fprintf('g10 %d %s\n', isequal(r1, r2), r1);
try, kz = find(~z); r3 = 'none'; catch err, r3 = err.message; end
try, kz1 = ~z; r4 = 'none'; catch err, r4 = err.message; end
fprintf('g11 %d %s\n', isequal(r3, r4), r3);
s1 = sqrt(y) > 0.1; try, s2 = find(sqrt(y) > 0.1 & y > 0); s3 = 1; catch, s3 = 0; end
s4 = sqrt(y); s5 = s4 > 0.1; s6 = y > 0; s7 = find(s5 & s6); fprintf('g12 %d %d\n', isequal(s2, s7), isreal(s4));
b1 = M .* rv; b1 = b1 + cv; b1 = b1 - M;
b = M .* rv + cv - M; fprintf('g13 %d %s\n', isequal(b, b1), mat2str(size(b)));
o1 = cv .* rv; o1 = o1 + 1; o1 = o1 - 2 * cv;
o = cv .* rv + 1 - 2 * cv; fprintf('g14 %d %s\n', isequal(o, o1), mat2str(size(o)));
c1 = rv + 1; c2 = cv .^ 2; c2 = c2 + 1; c3 = M .* c1; c3 = c3 ./ c2; c3 = c3 - 0.5;
c = M .* (rv + 1) ./ (cv .^ 2 + 1) - 0.5; fprintf('g15 %d\n', isequal(c, c3));
e1 = M > rv; e2 = M < cv; e3 = e1 & e2;
e = M > rv & M < cv; fprintf('g16 %d %s %d\n', isequal(e, e3), class(e), sum(e(:)));
f1 = M .* rv; f1 = f1 > cv; f1 = find(f1);
f = find(M .* rv > cv); fprintf('g17 %d %s\n', isequal(f, f1), mat2str(size(f)));
h1 = rv + cv; h1 = h1 .* 2; h1 = h1 - 1;
h = (rv + cv) * 2 - 1; fprintf('g18 %d %s\n', isequal(h, h1), mat2str(size(h)));
try, bad = M + rv' + 1 - M; r5 = 'none'; catch err, r5 = err.message; end
try, bad1 = M + rv'; r6 = 'none'; catch err, r6 = err.message; end
fprintf('g19 %d\n', isequal(r5, r6));
sq = sqrt(cv - 0.5); sc1 = M .* rv + sq; sc2 = M .* rv; sc2 = sc2 + sqrt(cv - 0.5);
fprintf('g20 %d %d\n', isequal(sc1, sc2), isreal(sc1));
