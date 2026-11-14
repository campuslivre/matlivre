% A handle's on/off properties are not plain text.
f = figure;
v = get(f, 'Visible');
c = class(v);
isc = ischar(v);
