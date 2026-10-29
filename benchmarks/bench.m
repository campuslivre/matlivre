% matlivre benchmark suite.
% Runs unchanged in MATLAB, GNU Octave and matlivre. Each case reports the best of REPS runs
% as "RESULT <name> <seconds>" and a checksum as "CHECK <name> <value>" so results can be
% compared across engines.

REPS = 3;

%% 1. Scalar arithmetic in a for loop
best = inf;
for r = 1:REPS
    tic;
    s = 0;
    for k = 1:2000000
        s = s + k * k;
    end
    best = min(best, toc);
end
fprintf('RESULT scalar_loop %.6f\n', best);
fprintf('CHECK scalar_loop %.10g\n', s);

%% 2. While loop with branches (Collatz step counts)
best = inf;
for r = 1:REPS
    tic;
    total = 0;
    for n = 1:30000
        m = n;
        while m ~= 1
            if mod(m, 2) == 0
                m = m / 2;
            else
                m = 3 * m + 1;
            end
            total = total + 1;
        end
    end
    best = min(best, toc);
end
fprintf('RESULT while_branch %.6f\n', best);
fprintf('CHECK while_branch %.10g\n', total);

%% 3. Recursive function calls
best = inf;
for r = 1:REPS
    tic;
    f = bench_fib(20);
    best = min(best, toc);
end
fprintf('RESULT fib_recursive %.6f\n', best);
fprintf('CHECK fib_recursive %.10g\n', f);

%% 4. Growing an array element by element
best = inf;
for r = 1:REPS
    tic;
    v = [];
    for k = 1:100000
        v(end + 1) = k;
    end
    best = min(best, toc);
end
fprintf('RESULT array_growth %.6f\n', best);
fprintf('CHECK array_growth %.10g\n', sum(v));

%% 5. Indexed assignment into a preallocated array
best = inf;
for r = 1:REPS
    tic;
    x = zeros(1, 1000000);
    for k = 1:1000000
        x(k) = k / 2;
    end
    best = min(best, toc);
end
fprintf('RESULT indexed_assign %.6f\n', best);
fprintf('CHECK indexed_assign %.10g\n', sum(x));

%% 6. Vectorized elementwise math
best = inf;
for r = 1:REPS
    tic;
    x = linspace(0, 10, 2000000);
    y = sin(x).^2 + cos(x).^2 + sqrt(x) .* exp(-x);
    s = sum(y);
    best = min(best, toc);
end
fprintf('RESULT vector_math %.6f\n', best);
fprintf('CHECK vector_math %.10g\n', s);

%% 7. Dense matrix multiply (300x300)
A = reshape(mod((1:90000) * 7, 13), 300, 300) / 13;
best = inf;
for r = 1:REPS
    tic;
    B = A * A;
    best = min(best, toc);
end
fprintf('RESULT matmul_300 %.6f\n', best);
fprintf('CHECK matmul_300 %.10g\n', sum(B(:)));

%% 8. Linear solve (300x300)
M = A + 300 * eye(300);
b = ones(300, 1);
best = inf;
for r = 1:REPS
    tic;
    sol = M \ b;
    best = min(best, toc);
end
fprintf('RESULT linsolve_300 %.6f\n', best);
fprintf('CHECK linsolve_300 %.10g\n', sum(sol));

%% 9. Sorting a million numbers
data = mod((1:1000000) * 7919, 1000003);
best = inf;
for r = 1:REPS
    tic;
    srt = sort(data);
    best = min(best, toc);
end
fprintf('RESULT sort_1e6 %.6f\n', best);
fprintf('CHECK sort_1e6 %.10g\n', srt(500000));

%% 10. Building a string in a loop
best = inf;
for r = 1:REPS
    tic;
    str = '';
    for k = 1:20000
        str = [str sprintf('%d,', k)];
    end
    best = min(best, toc);
end
fprintf('RESULT string_build %.6f\n', best);
fprintf('CHECK string_build %.10g\n', length(str));

%% 11. Struct field updates
best = inf;
for r = 1:REPS
    tic;
    p = struct('x', 0, 'y', 0);
    for k = 1:500000
        p.x = p.x + 1;
        p.y = p.y + p.x;
    end
    best = min(best, toc);
end
fprintf('RESULT struct_fields %.6f\n', best);
fprintf('CHECK struct_fields %.10g\n', p.y);

%% 12. Cell array writes and reads
best = inf;
for r = 1:REPS
    tic;
    c = cell(1, 100000);
    for k = 1:100000
        c{k} = k;
    end
    t = 0;
    for k = 1:100000
        t = t + c{k};
    end
    best = min(best, toc);
end
fprintf('RESULT cell_ops %.6f\n', best);
fprintf('CHECK cell_ops %.10g\n', t);
