#!/usr/bin/env perl
# Builds docs/matlab-coverage.md from:
#   documented.tsv  product<TAB>category path<TAB>name   (MATLAB R2023b function reference,
#                   help/<product>/helpfuncbycat.xml of the installation)
#   classes.tsv     category path<TAB>name               (classes the function reference omits)
#   scope.tsv       folder<TAB>name                      (public files of toolbox folders, for
#                   functions that exist but are not in the reference)
#   missing.txt     names MatLivre's exist() does not know
#   branch.txt      names provided by unmerged branches
# and prints the names implemented since the previous checklist.
use strict;
use warnings;

my ($dir, $missing_file, $branch_file, $doc) = @ARGV;

sub lines { my $f = shift; open my $h, '<:encoding(UTF-8)', $f or return (); chomp(my @l = <$h>); s/\r$// for @l; return @l }

my %missing = map { $_ => 1 } lines($missing_file);
my %branch = map { $_ => 1 } lines($branch_file);

# Out of scope for a live-script engine, by category of the function reference.
my @out_categories = (
    'App Building',
    'External Language Interfaces',
    'Environment and Settings > Add-Ons',
    'Environment and Settings > Desktop',
    'Environment and Settings > Help and Support',
    'Environment and Settings > Startup and Shutdown',
    'Software Development Tools > Background Processing',
    'Software Development Tools > Build Automation',
    'Software Development Tools > Debugging and Analysis',
    'Software Development Tools > Projects',
    'Software Development Tools > Source Control Integration',
    'Software Development Tools > Testing Frameworks',
    'Software Development Tools > Toolbox Distribution',
    'Data Import and Analysis > Large Files and Big Data',
    'Data Import and Analysis > Data Import and Export > Hardware and Network Communication',
    'Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data',
    'Data Import and Analysis > Data Import and Export > Web Access and Streaming',
    'Data Import and Analysis > Visual Exploration',
    'Mathematics > Quantum Computing',
    'MuPAD to MATLAB Migration',
);
# Audio and video: files are in scope, devices and players are not.
my %device = map { $_ => 1 } qw(audioplayer audiorecorder audiodevinfo audiodevreset sound soundsc beep getaudiodata getplayer isplaying isrecording
    play playblocking pause resume stop record recordblocking VideoReader VideoWriter hasFrame readFrame writeVideo mmfileinfo aviinfo
    mmreader avifile addframe);
my %keyword = map { $_ => 1 } qw(ans arguments break case catch classdef continue else elseif end for function global if otherwise parfor
    persistent return switch try while varargin varargout nargin nargout spmd);

my %entry;    # name => [category, status, reason]
sub out_reason {
    my ($cat, $name) = @_;
    for my $o (@out_categories) { return $o if index($cat, $o) == 0 || index($cat, " > $o") >= 0 }
    return 'Audio and video devices' if $cat =~ /Audio and Video Data/ && $device{$name};
    return undef;
}

my %product_label = (matlab => 'MATLAB', symbolic => 'Symbolic Math Toolbox');
my @rows = map { [split /\t/, $_, -1] } lines("$dir/documented.tsv");
push @rows, map { my @p = split /\t/, $_, -1; ['matlab', @p] } lines("$dir/classes.tsv");
for my $r (@rows) {
    my ($prod, $cat, $name) = @$r;
    next unless defined $name && length $name;
    my $full = "$product_label{$prod} > $cat";
    my $why = out_reason($cat, $name);
    my $e = $entry{$name};
    # A name listed in several categories counts as in scope if any of them is.
    if (!$e || ($e->[2] && !$why)) { $entry{$name} = [$full, undef, $why] }
}
# Undocumented public functions from the folder scan: keep only those not already out.
my %legacy_out;
for my $l (lines("$dir/legacy_out.tsv")) { my ($n, $r) = split /\t/, $l, 2; $legacy_out{$n} = $r }
for my $l (lines("$dir/scope.tsv")) {
    my ($folder, $name) = split /\t/, $l;
    next if $entry{$name};
    my $why = $legacy_out{$name} // ($folder =~ /symbolic/ ? undef : undef);
    $entry{$name} = ["Not in the function reference > $folder", undef, $why];
}

for my $name (keys %entry) {
    my $e = $entry{$name};
    if ($e->[2]) { $e->[1] = 'out' }
    elsif ($keyword{$name}) { $e->[1] = $missing{$name} ? 'keyword' : 'done' }
    elsif (!$missing{$name}) { $e->[1] = 'done' }
    elsif ($branch{$name}) { $e->[1] = 'branch' }
    else { $e->[1] = 'todo' }
}

# Newly implemented since the previous checklist.
my %was_todo;
for (lines($doc)) { $was_todo{$1} = 1 if /^- \[ \] `([^`]+)`/ }
my @new = sort grep { $was_todo{$_} && $entry{$_}[1] eq 'done' } keys %was_todo;
print "Newly implemented: ", scalar(@new), "\n@new\n";

# Group by the first two category levels below the product.
my (%group, %count);
for my $name (sort { lc $a cmp lc $b } keys %entry) {
    my ($cat, $st, $why) = @{$entry{$name}};
    my @p = split / > /, $cat;
    my $g = join(' > ', @p[0 .. ($#p < 2 ? $#p : 2)]);
    push @{$group{$g}{$st}}, $st eq 'out' ? "`$name` ($why)" : "`$name`";
    $count{$st}++;
}

open my $o, '>:encoding(UTF-8)', $doc or die "$doc: $!";
my @t = localtime;
my $date = sprintf '%04d-%02d-%02d', $t[5] + 1900, $t[4] + 1, $t[3];
print $o <<"HEAD";
# MATLAB coverage

Every function in the MATLAB R2023b and Symbolic Math Toolbox function references (the
installation's `help/*/helpfuncbycat.xml`), plus key classes and undocumented public functions
found in the toolbox folders, checked against MatLivre. Regenerate with
`tools/coverage/update.sh` after merging work ($date).

| Status | Count |
|---|---:|
| Implemented | @{[$count{done} // 0]} |
| In a branch not merged yet | @{[$count{branch} // 0]} |
| Language keyword (check it works) | @{[$count{keyword} // 0]} |
| **To implement** | **@{[$count{todo} // 0]}** |
| Out of scope | @{[$count{out} // 0]} |

Out of scope, by category of the function reference: app building, external language
interfaces (Python, Java, .NET, C/C++, COM, web services), desktop/help/add-on/startup
settings, software-development tools (projects, source control, test frameworks, debugging,
background pools, toolbox packaging), big data (tall arrays, datastores), hardware and network
communication, web access, scientific file formats (HDF4/HDF5, netCDF, CDF, FITS), audio/video
devices, interactive visual exploration, quantum computing and MuPAD migration helpers.

HEAD
for my $g (sort keys %group) {
    my $s = $group{$g};
    my $n = sub { scalar @{$s->{$_[0]} // []} };
    print $o "## $g\n\nImplemented: ", $n->('done'), ". To implement: ", $n->('todo'), ". In a branch: ", $n->('branch'),
        ". Out of scope: ", $n->('out'), ".\n\n";
    if ($s->{todo}) { print $o "To implement:\n\n", (map { "- [ ] $_\n" } @{$s->{todo}}), "\n" }
    print $o "In a branch: ", join(', ', @{$s->{branch}}), "\n\n" if $s->{branch};
    print $o "Language keywords to check: ", join(', ', @{$s->{keyword}}), "\n\n" if $s->{keyword};
    print $o "Implemented: ", join(', ', @{$s->{done}}), "\n\n" if $s->{done};
    print $o "Out of scope: ", join(', ', @{$s->{out}}), "\n\n" if $s->{out};
}
close $o;
printf "%d done, %d todo, %d branch, %d keyword, %d out\n", map { $count{$_} // 0 } qw(done todo branch keyword out);
