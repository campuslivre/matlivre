@echo off
rem Builds (if needed) and starts matlivre. Usage: run.bat [debug|fast] [file.mlx|file.m]
setlocal

set "PROFILE=--release"
if /i "%~1"=="debug" (
    set "PROFILE="
    shift
) else if /i "%~1"=="fast" (
    set "PROFILE=--profile fast"
    shift
)

rem Resolve a file argument against the caller's folder before moving to the repo.
set "FILE="
if not "%~1"=="" set "FILE=%~f1"

cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where cargo >nul 2>nul || (
    echo cargo was not found. Install Rust from https://rustup.rs and try again.
    exit /b 1
)

if defined FILE (
    cargo run %PROFILE% -p livre-app -- "%FILE%"
) else (
    cargo run %PROFILE% -p livre-app
)
