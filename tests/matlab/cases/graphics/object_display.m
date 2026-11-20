% How a graphics object shows in the command window
x = linspace(0, 2, 5);
h = plot(x, x.^2);
ax = gca;
ax
h
title('Squares');
ax
hold on;
h2 = plot(x, x, 'r--o', 'DisplayName', 'line');
h2
clear x ax
