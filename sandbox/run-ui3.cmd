@echo off
set "SHARE=C:\Users\WDAGUtilityAccount\Desktop\share"
del "%SHARE%\done.flag" >nul 2>&1
del "%SHARE%\run.log" >nul 2>&1
del "%SHARE%\ui3-progress.log" >nul 2>&1
start "" /b cmd /c "ping -n 300 127.0.0.1 >nul & taskkill /f /im powershell.exe >nul 2>&1"
powershell -NoProfile -ExecutionPolicy Bypass -File "%SHARE%\ui3.ps1" > "%SHARE%\run.log" 2>&1
echo %errorlevel%> "%SHARE%\done.flag"
