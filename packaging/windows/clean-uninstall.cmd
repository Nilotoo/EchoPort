@echo off
rem EchoPort clean uninstaller (per-user MSI install).
rem Removes the app, settings, profiles and caches. Downloaded music in
rem %USERPROFILE%\Music\EchoPort is intentionally kept.
setlocal
if /i "%~1"=="--run" goto run

rem Copy ourselves out of the install directory first so the uninstaller can
rem delete it. Pass the original install directory as the second argument.
copy /y "%~f0" "%TEMP%\echoport-clean-uninstall.cmd" >nul 2>&1
start "" "%TEMP%\echoport-clean-uninstall.cmd" --run "%~dp0"
exit /b 0

:run
set "APPDIR=%~2"
title EchoPort - clean uninstall
echo Stopping EchoPort...
taskkill /IM echoport.exe /F >nul 2>&1
timeout /t 1 /nobreak >nul

echo Removing settings, profiles and caches...
rmdir /s /q "%APPDATA%\echoport" >nul 2>&1
rmdir /s /q "%LOCALAPPDATA%\echoport" >nul 2>&1

echo Uninstalling EchoPort...
set "PRODUCTCODE="
for /f "usebackq tokens=*" %%K in (`powershell -NoProfile -ExecutionPolicy Bypass -Command "$k=(Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -like 'EchoPort*' } | Select-Object -First 1).PSChildName; if($k){$k}"`) do set "PRODUCTCODE=%%K"
if exist "%APPDIR%unins000.exe" (
    echo Running Inno Setup uninstaller...
    start "" /wait "%APPDIR%unins000.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
) else if defined PRODUCTCODE (
    msiexec /x "%PRODUCTCODE%" /qn
) else if exist "%APPDIR%Uninstall.exe" (
    echo Running NSIS uninstaller...
    start "" /wait "%APPDIR%Uninstall.exe" /S
) else (
    echo No Inno/MSI/NSIS uninstall entry found; removing per-user install folder...
    rmdir /s /q "%LOCALAPPDATA%\EchoPort" >nul 2>&1
    rmdir /s /q "%LOCALAPPDATA%\Programs\EchoPort" >nul 2>&1
)

del /q "%~f0" >nul 2>&1
echo.
echo EchoPort has been removed. Your downloaded music is still in:
echo   %USERPROFILE%\Music\EchoPort
echo.
pause
