% matlab.lang.OnOffSwitchState: how the on/off properties read as text and as logical values
chk = @(label, x) disp(sprintf('%-14s | %-8s | %-5s | %s', label, class(x), mat2str(size(x)), mat2str(double(x))));
chkt = @(label, x) disp(sprintf('%-14s | %-8s | %-5s | ''%s''', label, class(x), mat2str(size(x)), char(x)));
f = figure;
ax = axes(f);
vis = ax.Visible;
box = ax.Box;
chkt('class vis', class(vis));
chkt('class box', class(box));
% text comparisons
chk('strcmp', strcmp(vis, 'on'));
chk('strcmpi', strcmpi(vis, 'ON'));
chk('strncmp', strncmp(vis, 'on', 1));
chk('isequal text', isequal(vis, 'on'));
chk('isequal self', isequal(vis, vis));
chk('isequal true', isequal(vis, true));
chk('ismember yes', ismember(vis, {'on'}));
chk('ismember no', ismember(vis, {'off'}));
chk('== text', vis == 'on');
chk('== other text', vis == 'off');
chk('~= text', vis ~= 'off');
% switch on the property
switch vis
    case 'on'
        chkt('switch vis', 'on branch');
    otherwise
        chkt('switch vis', 'otherwise branch');
end
switch box
    case true
        chkt('switch box', 'true branch');
    otherwise
        chkt('switch box', 'otherwise branch');
end
% building text
chkt('sprintf s', sprintf('%s', vis));
chkt('sprintf d', sprintf('%d', box));
chkt('sprintf mixed', sprintf('%s=%d', vis, box));
chkt('concat before', ['v=' vis]);
chkt('concat after', [vis 'x']);
chkt('char', char(vis));
chkt('string', string(vis));
chkt('cellstr', cellstr(vis));
chkt('mat2str', mat2str(vis));
disp(vis)
fprintf('%s\n', vis);
% logical and numeric use
chk('== true', vis == true);
chk('box == true', box == true);
chk('box == 1', box == 1);
chk('~box', ~box);
chk('box & true', box & true);
chk('box && true', box && true);
chk('vis || false', vis || false);
chk('double', double(box));
chk('logical', logical(box));
chk('int8', int8(vis));
chk('num2str', num2str(box));
chk('box + 1', box + 1);
chk('any', any(box));
chk('xor', xor(box, true));
chk('islogical', islogical(box));
chk('isnumeric', isnumeric(box));
chk('ischar', ischar(vis));
chk('isa', isa(box, 'matlab.lang.OnOffSwitchState'));
chk('isempty', isempty(vis));
chk('size', size(vis));
if box, chkt('if box', 'on'); else, chkt('if box', 'off'); end
if vis, chkt('if vis', 'on'); else, chkt('if vis', 'off'); end
% setting a property from another one
ax.Box = vis;
chk('box after', ax.Box);
ax.Visible = 'off';
chkt('off state', ax.Visible);
chk('off is off', strcmp(ax.Visible, 'off'));
close all
clear f ax vis box chk chkt
