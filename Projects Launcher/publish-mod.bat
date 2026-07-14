@echo off
REM Projects Launcher - AFK "mod" tek tikla yayinlama
REM build + imza + FTP publish + version-afk.php senkron (mantik publish-mod.ps1 icinde)
setlocal
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0publish-mod.ps1"
set RC=%ERRORLEVEL%
echo.
if "%RC%"=="0" (echo Bitti.) else (echo Hata kodu: %RC%)
pause
endlocal
