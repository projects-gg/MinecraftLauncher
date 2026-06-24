@echo off
REM Projects Launcher - tek tikla yayinlama
REM build + imza + FTP publish (mantik publish.ps1 icinde)
setlocal
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0publish.ps1"
set RC=%ERRORLEVEL%
echo.
if "%RC%"=="0" (echo Bitti.) else (echo Hata kodu: %RC%)
pause
endlocal
