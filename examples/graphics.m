%% Graphics in MatLivre
% Figures are drawn inline, right below the code that makes them. Run everything with F5.

%% A lit surface
[X, Y] = meshgrid(linspace(-3, 3, 50));
Z = peaks(X, Y);
surf(X, Y, Z)
shading interp
camlight
title('peaks with a light')
colorbar

%% Several charts in a tiled layout
t = tiledlayout(2, 2);
title(t, 'Tiled layout')
nexttile
contourf(peaks(60), 10)
title('contourf')
nexttile
histogram(randn(1, 3000), 30)
title('histogram')
nexttile
pie([35 25 20 15 5])
title('pie')
nexttile
x = linspace(0, 4*pi, 300);
plot(x, sin(x), x, cos(x), 'LineWidth', 1.5)
legend('sin', 'cos')
grid on
title('lines')
