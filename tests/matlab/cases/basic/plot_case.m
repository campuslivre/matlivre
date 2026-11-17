% A figure with one line, a title and labels.
x = 0:0.5:2;
h = plot(x, x.^2);
title('Squares'); xlabel('x'); ylabel('y');
ax = gca;
