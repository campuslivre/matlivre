@echo off
rem Builds an optimised matlivre for testing: target\fast\matlivre.exe
rem It runs at close to release speed but rebuilds in seconds instead of minutes, because it
rem skips the whole-program link-time optimisation of release builds. Use build.bat for the
rem exe you hand out.
call "%~dp0build.bat" fast %*
