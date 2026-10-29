%% Language tour
% Complex numbers, integer classes, N-D arrays, string and struct arrays, classes, dates, tables and symbolic math.

%% Complex numbers
z = 3 + 4i
r = roots([1 0 4])
m = abs(z)

%% Integer classes saturate and round
a = int8(100) + int8(100)
b = uint8(7) / 2

%% N-D arrays
A = cat(3, [1 2; 3 4], [5 6; 7 8])
s = sum(A, 3)

%% String and struct arrays
names = ["Ada" "Grace" "Alan"];
greeting = "Hello, " + names
people = struct('name', {'Ada', 'Grace'}, 'born', {1815, 1906});
born = [people.born]

%% Classes from the script folder
p = Point(3, 4) + Point(1, 1)
p.R

%% Dates and durations
t = datetime(2026, 10, 28, 9, 30, 0)
later = t + hours(36)
span = between(t, later)

%% Tables
Name = ["Ada"; "Grace"; "Alan"];
Born = [1815; 1906; 1912];
T = table(Name, Born);
T = sortrows(T, 'Born', 'descend')

%% Symbolic math
syms x
f = x^3 - 6*x^2 + 11*x - 6
roots_of_f = solve(f == 0)
df = diff(f)
F = int(f)
fplot(f, [0 4])
grid on
title('f(x) = x^3 - 6x^2 + 11x - 6')
