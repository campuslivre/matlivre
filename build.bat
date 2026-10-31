@echo off
rem Builds MatLivre. Usage: build.bat [debug|fast]
rem Release output: target\release\matlivre.exe  (debug: target\debug\matlivre.exe,
rem fast: target\fast\matlivre.exe, optimised but much quicker to rebuild than release)
setlocal
cd /d "%~dp0"

rem rustup installs cargo here; add it in case this shell's PATH predates the install.
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where cargo >nul 2>nul || (
    echo cargo was not found. Install Rust from https://rustup.rs and try again.
    exit /b 1
)

if /i "%~1"=="debug" (
    cargo build -p livre-app || exit /b 1
    echo.
    echo Built target\debug\matlivre.exe
) else if /i "%~1"=="fast" (
    cargo build --profile fast -p livre-app || exit /b 1
    echo.
    echo Built target\fast\matlivre.exe
) else (
    cargo build --release -p livre-app || exit /b 1
    echo.
    echo Built target\release\matlivre.exe
)
