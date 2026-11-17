% A legend is a child of its figure; its labels become the DisplayName of the plotted objects; scatter keeps SizeData
f = figure; x = 1:3;
h1 = plot(x, x); hold on; h2 = plot(x, 2*x);
fprintf('before legend: children=%d\n', numel(f.Children));
l = legend('first', 'second');
fprintf('after legend: children=%d, class(1)=%s, class(2)=%s\n', numel(f.Children), class(f.Children(1)), class(f.Children(2)));
fprintf('DisplayName: %s | %s\n', h1.DisplayName, h2.DisplayName);
fprintf('String: %s\n', strjoin(l.String, ','));
fprintf('Location: %s  Orientation: %s  Box: %s  Visible: %s\n', l.Location, l.Orientation, char(l.Box), char(l.Visible));
fprintf('ax.Legend empty: %d\n', isempty(gca().Legend));
fprintf('findobj legend: %d\n', numel(findobj(f, 'Type', 'legend')));
fprintf('axes children: %d\n', numel(gca().Children));
l2 = legend('first', 'second', 'Location', 'northwest');
fprintf('same object: %d  Location: %s\n', l2 == l, l2.Location);
l3 = legend('a', 'b', 'Location', 'southeast', 'Orientation', 'horizontal');
fprintf('Location: %s Orientation: %s\n', l3.Location, l3.Orientation);
legend('off'); fprintf('after off: children=%d\n', numel(f.Children));
legend('show'); fprintf('after show: children=%d\n', numel(f.Children));
l4 = legend; fprintf('legend() call: class=%s\n', class(l4));
g = figure; plot(1:3, 'DisplayName', 'dn'); l5 = legend; fprintf('String from DisplayName: %s\n', strjoin(l5.String, ','));
h = figure; p1 = plot(1:3); hold on; p2 = plot(3:-1:1); legend('x','y'); fprintf('DisplayName after: %s %s\n', p1.DisplayName, p2.DisplayName);
k = figure; s1 = scatter(1:3, 1:3); fprintf('scatter default SizeData=%g\n', s1.SizeData);
s2 = scatter(1:3, 1:3, 50); fprintf('scatter SizeData=%g\n', s2.SizeData);
s4 = scatter(1:3, 1:3); s4.SizeData = 100; fprintf('set SizeData=%g\n', s4.SizeData);
s5 = scatter(1:3, 1:3, 64); fprintf('SizeData=%g MarkerSize? n/a\n', s5.SizeData);
close all
clear f x h1 h2 l l2 l3 l4 g l5 h p1 p2 k s1 s2 s4 s5
