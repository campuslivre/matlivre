function difftest_run(dt__casefile, dt__outfile)
%DIFFTEST_RUN Run a test case from a clean state and write a JSON snapshot of what it leaves behind.
%   Runs identically in MATLAB and MatLivre; the two snapshots are then compared field by field.
%   The snapshot holds: the release header, the command-window text (diary), the error that
%   stopped the case (if any), lastwarn, every workspace variable (class, size, exact value, how
%   it displays) and the graphics tree of the open figures. Names starting with dt__ are this
%   function's own and are left out of the snapshot.
dt__resetState();
dt__code = fileread(dt__casefile, 'Encoding', 'UTF-8');
dt__log = [dt__outfile '.log'];
if exist(dt__log, 'file'), delete(dt__log); end
dt__err = struct('identifier', '', 'message', '');
diary(dt__log);
try
    eval(dt__code);
catch dt__ME
    dt__err = struct('identifier', dt__ME.identifier, 'message', dt__ME.message);
end
diary off
dt__text = '';
if exist(dt__log, 'file')
    dt__text = fileread(dt__log, 'Encoding', 'UTF-8');
    delete(dt__log);
end

dt__snap = struct();
dt__snap.header = struct('release', version('-release'), 'version', version, ...
    'computer', computer, 'recorded', datestr(now, 'yyyy-mm-dd'));
dt__snap.log = strrep(dt__text, sprintf('\r\n'), sprintf('\n'));
dt__snap.error = dt__err;
[dt__wmsg, dt__wid] = lastwarn;
if strcmp(dt__wid, 'MATLAB:hg:AutoSoftwareOpenGL'), dt__wmsg = ''; dt__wid = ''; end   % VM artifact
dt__snap.lastwarn = struct('message', dt__wmsg, 'identifier', dt__wid);

dt__names = who;
dt__vars = struct();
for dt__k = 1:numel(dt__names)
    dt__n = dt__names{dt__k};
    if strncmp(dt__n, 'dt__', 4) || any(strcmp(dt__n, {'nargin', 'nargout'})), continue; end
    dt__v = eval(dt__n);
    dt__vars.(dt__n) = struct('class', class(dt__v), 'size', size(dt__v), ...
        'value', dt__enc(dt__v), 'display', '');
    try
        dt__vars.(dt__n).display = evalc(dt__n);
        % A graphics object's Position follows the screen (figures) or the size of its text
        % (axes and legends, whose layout uses font metrics): leave that one line out.
        if strncmp(class(dt__v), 'matlab.graphics', 15) || strncmp(class(dt__v), 'matlab.ui', 9)
            dt__vars.(dt__n).display = regexprep(dt__vars.(dt__n).display, '\n *Position: [^\n]*', '');
        end
    catch dt__ME
        dt__vars.(dt__n).display = ['<' dt__ME.message '>'];
    end
end
dt__snap.variables = dt__vars;

dt__figs = findall(0, 'type', 'figure');
if numel(dt__figs) > 1
    dt__num = zeros(1, numel(dt__figs));
    for dt__k = 1:numel(dt__figs), dt__num(dt__k) = get(dt__figs(dt__k), 'Number'); end
    [~, dt__ord] = sort(dt__num);
    dt__figs = dt__figs(dt__ord);
end
dt__snap.figures = {};
for dt__k = 1:numel(dt__figs)
    dt__snap.figures{end+1} = dt__gfx(dt__figs(dt__k), 0);
end
dt__snap.nfigures = numel(dt__figs);

dt__fid = fopen(dt__outfile, 'w', 'n', 'UTF-8');
fprintf(dt__fid, '%s', jsonencode(dt__snap, 'PrettyPrint', true));
fclose(dt__fid);
end

function dt__resetState()
close all force
clear global
fclose all;
format short
rng default
warning('on', 'all');
warning('off', 'MATLAB:hg:AutoSoftwareOpenGL');   % VM artifact, not part of the case
lastwarn('');
end

function t = dt__try(f, expr)
try
    t = feval(f, expr);
catch ME
    t = ['<' ME.message '>'];
end
end

function s = dt__enc(v)
% Exact, JSON-friendly encoding of a value: doubles as hex bit patterns.
if isa(v, 'matlab.lang.OnOffSwitchState')
    s = dt__enc(char(v));   % the text; the class itself is checked by its own case
elseif isnumeric(v)
    s = struct('kind', 'numeric', 'class', class(v), 'sparse', issparse(v), 'size', size(v));
    w = full(v);
    s.real = dt__nums(real(w));
    if ~isreal(w), s.imag = dt__nums(imag(w)); end
elseif islogical(v)
    s = struct('kind', 'logical', 'size', size(v), 'data', double(v(:))');
elseif ischar(v)
    s = struct('kind', 'char', 'size', size(v), 'rows', {cellstr(v(:, :))'});
    if ndims(v) > 2, s.rows = {v(:)'}; end
elseif isstring(v)
    s = struct('kind', 'string', 'size', size(v), 'missing', double(ismissing(v(:)))', ...
        'data', {cellstr(v(:))'});
elseif iscell(v)
    items = cell(1, numel(v));
    for k = 1:numel(v), items{k} = dt__enc(v{k}); end
    s = struct('kind', 'cell', 'size', size(v), 'items', {items});
elseif isstruct(v)
    f = fieldnames(v);
    elems = cell(1, numel(v));
    for k = 1:numel(v)
        e = struct();
        for j = 1:numel(f), e.(f{j}) = dt__enc(v(k).(f{j})); end
        elems{k} = e;
    end
    s = struct('kind', 'struct', 'size', size(v), 'fields', {f'}, 'elements', {elems});
elseif isa(v, 'function_handle')
    s = struct('kind', 'function_handle', 'text', func2str(v));
else
    s = struct('kind', 'object', 'class', class(v), 'size', size(v), 'text', dt__try('evalc', 'disp(v)'));
end
end

function c = dt__nums(x)
x = x(:)';
if isempty(x), c = {}; return; end
if isa(x, 'double') || isa(x, 'single')
    c = cellstr(num2hex(x))';
else
    c = arrayfun(@(t) sprintf('%d', t), x, 'UniformOutput', false);
end
end

function g = dt__gfx(h, depth)
% Graphics tree: a whitelist of properties per object type, children in order.
g = struct('type', get(h, 'Type'));
switch g.type
    case 'figure'
        props = {'Name', 'NumberTitle', 'Color'};   % not Visible: MatLivre shows figures inline
    case 'axes'
        props = {'XLim', 'YLim', 'ZLim', 'XScale', 'YScale', 'XGrid', 'YGrid', 'ZGrid', 'Box', 'Visible', 'Color', 'LineWidth'};
    case {'line', 'stairs', 'stem', 'bar', 'area', 'surface', 'patch'}
        props = {'XData', 'YData', 'ZData', 'LineStyle', 'LineWidth', 'Marker', 'Color', 'Visible'};
    case 'scatter'
        props = {'XData', 'YData', 'ZData', 'SizeData', 'CData', 'Marker', 'MarkerFaceColor', 'MarkerEdgeColor', 'LineWidth', 'Visible'};
    case 'legend'
        props = {'String', 'Location', 'Orientation', 'Box', 'Visible'};
    case 'text'
        props = {'String', 'Position', 'Visible'};
    otherwise
        props = {'Visible'};
end
for k = 1:numel(props)
    try
        g.(props{k}) = dt__enc(get(h, props{k}));
    catch
    end
end
if strcmp(g.type, 'axes')
    for k = {'Title', 'XLabel', 'YLabel'}
        try
            g.(k{1}) = dt__enc(get(get(h, k{1}), 'String'));
        catch
        end
    end
end
ch = [];
try
    ch = get(h, 'Children');
catch
end
g.children = {};
if depth < 6
    for k = 1:numel(ch)
        g.children{end+1} = dt__gfx(ch(k), depth + 1);
    end
end
end
