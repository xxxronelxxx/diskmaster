@echo off
set "SHARE=C:\Users\WDAGUtilityAccount\Desktop\share"
del "%SHARE%\done.flag" >nul 2>&1
del "%SHARE%\run.log" >nul 2>&1
del "%SHARE%\destructive-progress.log" >nul 2>&1

rem Watchdog: kill the script if it hangs for more than 900s (DISM is slow).
start "" /b cmd /c "ping -n 900 127.0.0.1 >nul & taskkill /f /im powershell.exe >nul 2>&1"

powershell -NoProfile -ExecutionPolicy Bypass -File "%SHARE%\destructive.ps1" > "%SHARE%\run.log" 2>&1
echo %errorlevel%> "%SHARE%\done.flag"
