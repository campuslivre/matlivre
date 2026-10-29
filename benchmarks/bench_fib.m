function r = bench_fib(n)
% Naive recursive Fibonacci (function-call overhead benchmark).
    if n < 2
        r = n;
    else
        r = bench_fib(n - 1) + bench_fib(n - 2);
    end
end
