%% Getting Started with matlivre
% A live script mixes formatted text, code, and results. Click in a section and press Ctrl+Enter to run it, or press F5 to run everything. Results appear right below the code.

%% Variables and matrices
% Statements without a semicolon show their value.
a = 3
b = [1 2 3; 4 5 6]
c = b' * a;
total = sum(c(:))

%% Plotting
% Figures are drawn inline. Hover a plot to read values off the crosshair.
x = linspace(0, 2*pi, 200);
plot(x, sin(x), 'LineWidth', 2)
hold on
plot(x, cos(x), '--', 'LineWidth', 2)
hold off
grid on
title('Sine and cosine')
xlabel('x (radians)')
ylabel('Amplitude')
legend('sin', 'cos')

%% Loops, conditions, and formatted text
for k = 1:5
    if mod(k, 2) == 0
        fprintf('%d is even\n', k);
    else
        fprintf('%d is odd\n', k);
    end
end

%% Solving a linear system
A = [4 -2 1; -2 4 -2; 1 -2 4];
rhs = [11; -16; 17];
sol = A \ rhs
residual = norm(A*sol - rhs)

%% Local functions
% Functions defined at the end of the script can be called from any section.
n = 1:10;
f = arrayfun(@fib, n)
bar(n, f)
title('Fibonacci numbers')
grid on

function r = fib(n)
    if n <= 2
        r = 1;
    else
        r = fib(n - 1) + fib(n - 2);
    end
end
