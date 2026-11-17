% A new figure has no axes until something needs them
chk = @(label, x) disp(sprintf('%-18s | %-8s | %-5s | %s', label, class(x), mat2str(size(x)), mat2str(double(x))));
chkt = @(label, x) disp(sprintf('%-18s | %-8s | %-5s | ''%s''', label, class(x), mat2str(size(x)), char(x)));
f = figure;
chk('new children', numel(f.Children));
chk('new findobj', numel(findobj(f, 'Type', 'axes')));
chk('new CurrentAxes', isempty(f.CurrentAxes));
chkt('new Visible', f.Visible);
ax = axes;
chk('axes children', numel(f.Children));
chk('axes findobj', numel(findobj(f, 'Type', 'axes')));
chk('axes is child', f.Children == ax);
% axes with properties
g = figure;
ax2 = axes('Position', [0.1 0.1 0.5 0.5]);
chk('pos children', numel(g.Children));
chk('pos', ax2.Position);
% the other ways of making the first axes
h1 = figure; plot(1:3); chk('plot', numel(h1.Children));
h2 = figure; hold on; chk('hold on', numel(h2.Children));
h3 = figure; xlim([0 1]); chk('xlim', numel(h3.Children));
h4 = figure; title('t'); chk('title', numel(h4.Children));
h5 = figure; a5 = gca; chk('gca', numel(h5.Children));
% axes(fig) targets a figure
k = figure;
ax3 = axes(k);
chk('axes(fig)', numel(k.Children));
chk('axes(fig) findobj', numel(findobj(k, 'Type', 'axes')));
close all
clear f ax g ax2 h1 h2 h3 h4 h5 a5 k ax3 chk chkt
