@echo off
setlocal
cd /d "%~dp0"
if exist "GoogleAccountVault.exe" (
    start "" "GoogleAccountVault.exe"
) else if exist "src-tauri\target\release\google-account-vault.exe" (
    start "" "src-tauri\target\release\google-account-vault.exe"
) else (
    echo Œ¥’“µΩ“—±‡“Î≥Ã–Ú°£
    pause
)
