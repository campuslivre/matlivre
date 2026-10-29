@echo off
rem Runs bench.m in matlivre, GNU Octave and MATLAB (one after another), then compares.
rem Set OCTAVE / MATLAB to override the default install paths; a missing engine is skipped.
setlocal
cd /d "%~dp0"
if not exist results mkdir results

if not defined OCTAVE set "OCTAVE=%LOCALAPPDATA%\Programs\GNU Octave\Octave-11.3.0\mingw64\bin\octave-cli.exe"
if not defined MATLAB set "MATLAB=C:\Program Files\MATLAB\R2023b\bin\matlab.exe"

echo === matlivre
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
pushd ..
cargo build --release -p livre-lang --bin matlivre-cli || exit /b 1
popd
..\target\release\matlivre-cli.exe bench.m > results\matlivre.txt

if exist "%OCTAVE%" (
    echo === Octave
    "%OCTAVE%" --no-gui --quiet --norc --eval "bench" > results\octave.txt
) else echo Octave not found, skipping.

if exist "%MATLAB%" (
    echo === MATLAB
    "%MATLAB%" -sd "%CD%" -batch "bench" > results\matlab.txt
) else echo MATLAB not found, skipping.

powershell -NoProfile -ExecutionPolicy Bypass -File compare.ps1
