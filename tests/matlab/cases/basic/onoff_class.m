% A handle's on/off properties are not plain text.
f = figure;
v = get(f, 'Visible');
fprintf('%s %d\n', class(v), ischar(v));
disp(v)
fprintf('%s\n', char(v));
close all
clear f v
