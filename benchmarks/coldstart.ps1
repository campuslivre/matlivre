# Cold start: time from launching each program until it has run a line of code (or, for the
# MatLivre app, until its window is up and responding; its engine starts with the window).
#   powershell -NoProfile -ExecutionPolicy Bypass -File coldstart.ps1 [-Runs 3] [-Skip octave_gui] [-Only matlab_batch]
# The first start of Octave's GUI shows a welcome wizard that waits for clicks; open the GUI once
# by hand (or skip octave_gui) before measuring it.
# Prints "RESULT <case> <first seconds> <median seconds>". Set OCTAVE_DIR / MATLAB to override
# the install paths; a missing program is skipped.
param([int]$Runs = 3, [string[]]$Skip = @(), [string[]]$Only = @())
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$cli = Join-Path $root 'target\release\matlivre-cli.exe'
$app = Join-Path $root 'target\release\matlivre.exe'
$octDir = if ($env:OCTAVE_DIR) { $env:OCTAVE_DIR } else { "$env:LOCALAPPDATA\Programs\GNU Octave\Octave-11.3.0\mingw64\bin" }
$matlab = if ($env:MATLAB) { $env:MATLAB } else { 'C:\Program Files\MATLAB\R2023b\bin\matlab.exe' }

$work = Join-Path ([IO.Path]::GetTempPath()) 'matlivre-coldstart'
New-Item -ItemType Directory -Force $work | Out-Null
$hello = Join-Path $work 'hello.m'
Set-Content -Path $hello -Value 'x = 1 + 1;' -Encoding ascii
$marker = Join-Path $work 'ready.txt'
$markerM = $marker.Replace("'", "''")

function Measure-Run([scriptblock]$Block) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    & $Block | Out-Null
    $sw.Elapsed.TotalSeconds
}

# Polls $Test every 20 ms; returns the seconds elapsed on $Sw when it holds.
function Wait-Until([Diagnostics.Stopwatch]$Sw, [scriptblock]$Test, [int]$TimeoutSec = 300) {
    while (-not (& $Test)) {
        if ($Sw.Elapsed.TotalSeconds -gt $TimeoutSec) { throw 'timed out' }
        Start-Sleep -Milliseconds 20
    }
    $Sw.Elapsed.TotalSeconds
}

# Stops the processes with these names started since $Since (leaving any already open alone).
function Stop-Since([string[]]$Names, [datetime]$Since) {
    foreach ($n in $Names) {
        Get-Process -Name $n -ErrorAction SilentlyContinue | Where-Object { $_.StartTime -ge $Since } | Stop-Process -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Seconds 2
}

function Report([string]$Name, [double[]]$Times) {
    $sorted = $Times | Sort-Object
    $median = $sorted[[int][math]::Floor(($sorted.Count - 1) / 2)]
    'RESULT {0} {1:N3} {2:N3}' -f $Name, $Times[0], $median
}

$cases = [ordered]@{}
if (Test-Path $cli) {
    $cases['matlivre_cli'] = { Measure-Run { & $cli $hello } }
}
if (Test-Path $app) {
    $cases['matlivre_app'] = {
        $since = Get-Date
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $p = Start-Process $app -PassThru
        $t = Wait-Until $sw { $p.Refresh(); $p.MainWindowHandle -ne 0 -and $p.Responding }
        Stop-Since @('matlivre') $since
        $t
    }
}
$octCli = Join-Path $octDir 'octave-cli.exe'
if (Test-Path $octCli) {
    $cases['octave_cli'] = { Measure-Run { & $octCli --no-gui --quiet --norc --eval 'x = 1 + 1;' } }
    $octGui = Join-Path $octDir 'octave.exe'
    $cases['octave_gui'] = {
        Remove-Item $marker -ErrorAction SilentlyContinue
        $since = Get-Date
        $sw = [Diagnostics.Stopwatch]::StartNew()
        Start-Process $octGui -ArgumentList '--gui', '--norc', '--persist', '--eval', "`"fclose(fopen('$markerM', 'w'));`"" | Out-Null
        $t = Wait-Until $sw { Test-Path $marker }
        Stop-Since @('octave', 'octave-gui', 'octave-11.3.0') $since
        $t
    }
}
if (Test-Path $matlab) {
    $cases['matlab_batch'] = { Measure-Run { & $matlab -batch 'x = 1 + 1;' } }
    $cases['matlab_desktop'] = {
        Remove-Item $marker -ErrorAction SilentlyContinue
        $since = Get-Date
        $sw = [Diagnostics.Stopwatch]::StartNew()
        Start-Process $matlab -ArgumentList '-r', "`"fclose(fopen('$markerM', 'w'));`"" | Out-Null
        $t = Wait-Until $sw { Test-Path $marker }
        Stop-Since @('MATLAB') $since
        $t
    }
}

foreach ($name in $cases.Keys) {
    if ($Skip -contains $name -or ($Only.Count -gt 0 -and $Only -notcontains $name)) { continue }
    $times = @()
    for ($i = 0; $i -lt $Runs; $i++) {
        $times += [double](& $cases[$name])
        Start-Sleep -Seconds 2
    }
    Report $name $times
}
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
