@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
set "bukan_install_exit=%errorlevel%"
if not "%bukan_install_exit%"=="0" echo Bukan installation failed. See the message above.
pause
exit /b %bukan_install_exit%
