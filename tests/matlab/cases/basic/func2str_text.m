% func2str shows an anonymous function as typed: parentheses kept, none added, numbers as written
fs = { @(t) 2e-3*t, @(x) 1.50*x, @(x) 0.5*x, @(x) .5*x, @(x) 1e3+x, @(x) 1E5*x, @(x) 3i*x, @(x) 2.5j+x, ...
       @(x) 10*x, @(x) 1.0*x, @(x) x.^2+1, @(t) (t.^2)+1, @(a,b) a*b-sin(a), @(a,b) (a*b)-sin(a), ...
       @(x) x(1:2:end), @(x) [1 2.50 3e0], @(x) {1.0, 'a'}, @(x) 0.10*x, @(x) 1e-10, ...
       @(x) 5.0e+2*x, @(x) 1e0*x, @(x) 100000*x, @(x) -(x+1)*2, @(x) (x)', @(x) ((x+1))*2, ...
       @(x) a.b(1.0), @(x) x.^-2.0, @(x) 1.0:2:x };
for i = 1:numel(fs)
    disp(func2str(fs{i}))
end
g = str2func('@(x) 1.50*(x+1)');
disp(func2str(g))
f = @(x) 2e-3*x;
r = f(1000);
clear fs g f
