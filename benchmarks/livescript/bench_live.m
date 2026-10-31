%% Live script benchmark
% A typical analysis notebook: computations with displayed results, a table, text and
% several figures. bench_live.mlx is this script converted by MATLAB's Live Editor.

%% Matrices and linear algebra
A = magic(6)
b = (1:6)'
x = A(1:5, 1:5) \ b(1:5)
e = eig(A(1:4, 1:4))
n = norm(A)

%% A loop with a running result
s = 0;
for k = 1:200000
    s = s + sin(k) / k;
end
s

%% Statistics
t = linspace(0, 10, 5000);
y = sin(t) .* exp(-t / 5) + 0.05 * cos(40 * t);
stats = [mean(y), std(y), min(y), max(y), median(y)]
c = cumsum(y);
c(end)

%% A table
names = ["alpha"; "beta"; "gamma"; "delta"; "epsilon"];
T = table(names, (1:5)', [3.5; 2.25; 8; 1.5; 6], 'VariableNames', {'Name', 'Id', 'Score'})
T = sortrows(T, 'Score', 'descend')

%% Text processing
words = split("the quick brown fox jumps over the lazy dog", " ")
lens = strlength(words)'
upper(join(words(1:3), "-"))
s = struct('name', 'probe', 'size', [3 4], 'ok', true)

%% Line plot
figure
plot(t, y, 'LineWidth', 1.5)
hold on
plot(t, exp(-t / 5), '--')
hold off
title('Damped signal')
xlabel('t')
ylabel('y')
legend('signal', 'envelope')
grid on

%% Surface
figure
surf(peaks(40))
colorbar
title('peaks')

%% Histogram and subplots
figure
histogram(y, 40)
title('Distribution')
figure
subplot(2, 2, 1); plot(t, sin(t));
subplot(2, 2, 2); bar([3 1 4 1 5]);
subplot(2, 2, 3); scatter(1:50, mod((1:50) * 7, 11));
subplot(2, 2, 4); stairs(1:10, cumsum(1:10));
