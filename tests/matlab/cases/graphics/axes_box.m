% Box of the axes: on for the axes a plotting command makes, off for the others
names = {'plot','semilogx','semilogy','loglog','bar','barh','stairs','stem','area','errorbar','histogram','fill','imagesc','image','contour','quiver','scatter','plot3','surf','mesh','line','xline'};
calls = {'plot(1:3)','semilogx(1:3,1:3)','semilogy(1:3,1:3)','loglog(1:3,1:3)','bar(1:3)','barh(1:3)','stairs(1:3)','stem(1:3)','area(1:3)','errorbar(1:3,1:3,[.1 .1 .1])','histogram([1 2 2 3])','fill([0 1 1],[0 0 1],''r'')','imagesc(magic(3))','image(magic(3))','contour(peaks(8))','quiver(1:3,1:3,1:3,1:3)','scatter(1:3,1:3)','plot3(1:3,1:3,1:3)','surf(peaks(5))','mesh(peaks(5))','line([0 1],[0 1])','xline(1)'};
for k = 1:numel(calls)
    figure; eval([calls{k} ';']);
    fprintf('%-9s Box=%s\n', names{k}, char(gca().Box));
end
close all
% axes made by something else start with Box off
figure; hold on;      fprintf('hold      Box=%s\n', char(gca().Box));
figure; a = gca;      fprintf('gca       Box=%s\n', char(a.Box));
figure; a = axes;     fprintf('axes      Box=%s\n', char(a.Box));
figure; subplot(1,2,1); fprintf('subplot   Box=%s\n', char(gca().Box));
figure; title('t');   fprintf('title     Box=%s\n', char(gca().Box));
figure; xlim([0 1]);  fprintf('xlim      Box=%s\n', char(gca().Box));
close all
% the box command and the property
figure; plot(1:3);
box off; fprintf('box off   Box=%s\n', char(gca().Box));
box on;  fprintf('box on    Box=%s\n', char(gca().Box));
box;     fprintf('box       Box=%s\n', char(gca().Box));
box;     fprintf('box       Box=%s\n', char(gca().Box));
ax = gca; ax.Box = 'off'; fprintf('set off   Box=%s\n', char(ax.Box));
set(ax, 'Box', 'on');     fprintf('set on    Box=%s\n', char(ax.Box));
% replotting and clearing keep it
cla;                      fprintf('cla       Box=%s\n', char(ax.Box));
plot(3:-1:1);             fprintf('replot    Box=%s\n', char(ax.Box));
hold on; plot(1:3);       fprintf('2 plots   Box=%s\n', char(ax.Box));
% a new plot in a cleared figure makes new axes
clf; plot(1:3);           fprintf('clf+plot  Box=%s\n', char(gca().Box));
% grid turns all three grids on
ax = gca;
fprintf('grid off  X=%s Y=%s Z=%s\n', char(ax.XGrid), char(ax.YGrid), char(ax.ZGrid));
grid on;
fprintf('grid on   X=%s Y=%s Z=%s\n', char(ax.XGrid), char(ax.YGrid), char(ax.ZGrid));
% a plotting command that replaces the contents sets Box again; with hold on it stays
figure; ax = axes; plot(1:3);              fprintf('axes; plot            Box=%s\n', char(ax.Box));
figure; plot(1:3); box off; plot(1:3);     fprintf('plot; box off; plot   Box=%s\n', char(gca().Box));
figure; hold on; plot(1:3);                fprintf('hold on; plot         Box=%s\n', char(gca().Box));
figure; plot(1:3); box off; hold on; plot(3:-1:1); fprintf('box off; hold; plot   Box=%s\n', char(gca().Box));
figure; ax = axes; bar(1:3);               fprintf('axes; bar             Box=%s\n', char(ax.Box));
figure; plot(1:3); scatter(1:3,1:3);       fprintf('plot; scatter        Box=%s\n', char(gca().Box));
figure; plot(1:3); surf(peaks(5));         fprintf('plot; surf           Box=%s\n', char(gca().Box));
figure; plot(1:3); plot3(1:3,1:3,1:3);     fprintf('plot; plot3          Box=%s\n', char(gca().Box));
figure; plot(1:3); line([0 1],[0 1]);      fprintf('plot; line          Box=%s\n', char(gca().Box));
figure; plot(1:3); hold on; scatter(1:3,1:3); fprintf('plot; hold; scatter  Box=%s\n', char(gca().Box));
figure; scatter(1:3,1:3); plot(1:3);       fprintf('scatter; plot        Box=%s\n', char(gca().Box));
figure; plot(1:3); box off; cla; plot(1:3); fprintf('box off; cla; plot   Box=%s\n', char(gca().Box));
figure; subplot(1,2,1); plot(1:3);         fprintf('subplot; plot       Box=%s\n', char(gca().Box));
close all
close all
clear ax a names calls k
