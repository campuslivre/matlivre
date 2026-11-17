% The command-window display of each kind of value
a = int8([1 2 3]);
b = int32(7);
c = uint8([1 2; 3 4]);
d = single(2.5);
e = single([1.5 2.5]);
z1 = 3 + 4i;
z2 = [1+2i 3-4i];
l1 = true;
l2 = [true false true];
l3 = [true false; false true];
s1 = 'hello';
s2 = '';
e1 = [];
e2 = zeros(0, 3);
t1 = "text";
t2 = ["a" "bc"; "def" "g"];
c1 = {1, 'two', [3 4 5]};
c2 = {1, {2, 'x'}; 'y', []};
st = struct('x', 1, 'y', 'abc', 'z', {{1, 2}});
sa = struct('p', {1, 2, 3});
f1 = @sin;
f2 = @(x) x + 1;
n1 = -5;
n2 = [1.5 -2.5; 100 0.001];
n3 = 1e10;
n4 = [1 1e6];
n6 = [1 Inf -Inf];
i1 = int16([-5 10 300]);
u1 = uint8(200);
disp(5)
disp([1 2 3])
disp([1.5 2.5; 3.5 4.5])
disp('text')
disp(true)
