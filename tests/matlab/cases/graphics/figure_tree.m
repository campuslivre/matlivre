% Open figures are compared as a tree: figure, legend and axes, and their objects
x = linspace(0, 1, 4);
f1 = figure('Name', 'lines');
plot(x, x, 'r-'); hold on;
plot(x, x.^2, 'b--o');
title('Two lines'); grid on; box off;
legend('line', 'curve', 'Location', 'northwest');
f2 = figure;
scatter(x, x.^2, 50);
xlabel('x');
f3 = figure;
stairs(x, 2*x);
