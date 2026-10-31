function bench_live_matlab()
% Times bench_live.mlx in MATLAB's Live Editor engine (executeAndSave: run every section,
% capture outputs and figures, save them into the file), and the same code as a plain script.
here = fileparts(mfilename('fullpath'));
mlx = fullfile(here, 'bench_live.mlx');
tmp = fullfile(tempdir, 'matlivre_bench_live.mlx');
runs = 4;
live = zeros(1, runs);
for k = 1:runs
    copyfile(mlx, tmp, 'f');
    t0 = tic;
    matlab.internal.liveeditor.executeAndSave(tmp);
    live(k) = toc(t0);
end
% run() refuses bench_live.m next to bench_live.mlx (same name), so run a copy.
script = fullfile(tempdir, 'matlivre_bench_plain.m');
copyfile(fullfile(here, 'bench_live.m'), script, 'f');
plain = zeros(1, runs);
for k = 1:runs
    close all force
    t0 = tic;
    runPlain(script);
    plain(k) = toc(t0);
end
close all force
fprintf('RESULT mlx_first %.4f\n', live(1));
fprintf('RESULT mlx_best %.4f\n', min(live(2:end)));
fprintf('RESULT m_first %.4f\n', plain(1));
fprintf('RESULT m_best %.4f\n', min(plain(2:end)));
end

function runPlain(path)
% In its own workspace, so the script's variables do not clash with the timer's.
evalc('run(path)');
end
