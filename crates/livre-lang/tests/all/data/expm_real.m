n = 12; A = reshape(mod((1:n*n)*7919, 1013), n, n) / 1013 - 0.5;
for s = [1e-3 0.01 0.1 0.3 1 5 30]
  E = expm(s*A); fprintf('e %g %.9g %.9g\n', s, sum(E(:)), trace(E));
end
B = reshape(mod((1:40*40)*104729, 1009), 40, 40) / 1009 - 0.5; F = expm(2*B);
fprintf('f %.9g %.9g %.9g\n', sum(F(:)), F(3,7), norm(F, 1));
Z = expm(zeros(9)); fprintf('z %d %d\n', isequal(Z, eye(9)), isreal(Z));
D = expm(diag(1:10) / 4); fprintf('d %.9g %.9g\n', D(10,10), D(3,3));
K = expm([0 1; 0 0] * 3); fprintf('k %g %g %g %g\n', K);
C = expm(A + 1i * A'); fprintf('c %.9g %.9g\n', real(trace(C)), imag(trace(C)));
