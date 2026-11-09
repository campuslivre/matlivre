% Nested loops around ode45 with results collected in growing lists (the shape of issue #1:
% a reactor start-up simulated over a grid of two feed flows). A small stand-in model.
FA = 0.86e-3; CA0 = 2;
y0 = [0.1875, 0, 0, 0, 0, 293.15, 293.15];
FB_list = linspace(0, 4e-3, 3);
FH_list = linspace(0, 1e-2, 3);
rhs = @(y, FB, FH) [ ...
    0; ...
    (FA*CA0- (FA+FB)*y(2))/y(1) - 0.05*exp(-2000/y(6))*y(2)*y(3); ...
    (FB*1.5 - (FA+FB)*y(3))/y(1) - 0.05*exp(-2000/y(6))*y(2)*y(3); ...
    0.05*exp(-2000/y(6))*y(2)*y(3) - (FA+FB)*y(4)/y(1); ...
    0.01*y(4) - (FA+FB)*y(5)/y(1); ...
    (FA+FB)*(293.15 - y(6))/y(1) + 50*0.05*exp(-2000/y(6))*y(2)*y(3) + 2e-3*(y(7) - y(6)); ...
    FH*(350 - y(7))/0.05 - 2e-3*(y(7) - y(6))];
xa = []; fb = []; fh = []; nsteps = [];
for j = 1:numel(FB_list)
    for h = 1:numel(FH_list)
        [t, y] = ode45(@(t, y) rhs(y, FB_list(j), FH_list(h)), [0 200], y0);
        xA = (FA*CA0 - (FA + FB_list(j))*y(:, 2)) ./ (FA*CA0) * 100;
        xa = [xa, xA(end)];
        fb = [fb, FB_list(j)];
        fh = [fh, FH_list(h)];
        nsteps = [nsteps, numel(t)];
    end
    fprintf('Done %d of %d\n', j, numel(FB_list));
end
% The raw trajectories (t, y, xA) differ from MATLAB's in the last bits (the step sizes are
% computed with different rounding); compare what was derived from them, to 6 decimals.
clear t y xA
xa = round(xa, 6)
nsteps

% The map of the issue: points below / above the target conversion in red / green. The threshold
% is 5 here so that this stand-in model has points on both sides.
figure('Name', 'Konverzió 2D térkép', 'NumberTitle', 'off');
hold on;
rossz_idx = xa < 5;
jo_idx = xa >= 5;
s1 = scatter(fb(rossz_idx), fh(rossz_idx), 20, 'r', 'filled');
s2 = scatter(fb(jo_idx), fh(jo_idx), 20, 'g', 'filled');
xlabel('B betáplálás (F_B) [m^3/s]');
ylabel('Fűtés (F_{H,be}) [m^3/s]');
title(['Konverzió térkép, V = ', num2str(y0(1)), ' m^3']);
lg = legend('X < 5%', 'X \geq 5%', 'Location', 'best');
grid on;
hold off;
ax = gca;
good = xa >= 5
