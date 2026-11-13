@echo off
REM ==============================================================================
REM FantasTech — Windows Installer Builder
REM ==============================================================================
setlocal enabledelayedexpansion

echo ==============================================================================
echo [FantasTech] Compiling Windows Executable and Installer...
echo ==============================================================================

cd /d "%~dp0..\.."

echo [1/2] Compiling FantasTech release binary with Cargo...
cargo build --release --manifest-path core\Cargo.toml --bin fantastech
if !ERRORLEVEL! neq 0 (
    echo [ERROR] Cargo compilation failed.
    exit /b !ERRORLEVEL!
)

echo [2/2] Generating Windows Setup Installer (.exe)...

if not exist dist mkdir dist

set "ISCC_BIN="
where iscc >nul 2>nul
if %ERRORLEVEL% equ 0 (
    set "ISCC_BIN=iscc"
) else if exist "%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe" (
    set "ISCC_BIN=%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe"
) else if exist "%ProgramFiles%\Inno Setup 6\ISCC.exe" (
    set "ISCC_BIN=%ProgramFiles%\Inno Setup 6\ISCC.exe"
) else if exist "%LocalAppData%\Programs\Inno Setup 6\ISCC.exe" (
    set "ISCC_BIN=%LocalAppData%\Programs\Inno Setup 6\ISCC.exe"
) else if exist "C:\Inno Setup 6\ISCC.exe" (
    set "ISCC_BIN=C:\Inno Setup 6\ISCC.exe"
) else if exist "C:\InnoSetup\ISCC.exe" (
    set "ISCC_BIN=C:\InnoSetup\ISCC.exe"
)

REM If Inno Setup compiler was not found, attempt automatic installation via winget
if not defined ISCC_BIN (
    echo [INFO] Inno Setup not found in standard locations. Checking winget...
    where winget >nul 2>nul
    if !ERRORLEVEL! equ 0 (
        echo [INFO] Installing Inno Setup 6 via winget...
        winget install --id JRSoftware.InnoSetup --exact --silent --accept-source-agreements --accept-package-agreements >nul 2>nul
        if exist "%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe" set "ISCC_BIN=%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe"
        if exist "%ProgramFiles%\Inno Setup 6\ISCC.exe" set "ISCC_BIN=%ProgramFiles%\Inno Setup 6\ISCC.exe"
        if exist "%LocalAppData%\Programs\Inno Setup 6\ISCC.exe" set "ISCC_BIN=%LocalAppData%\Programs\Inno Setup 6\ISCC.exe"
    )
)

if defined ISCC_BIN (
    echo [INFO] Using Inno Setup Compiler: !ISCC_BIN!
    "!ISCC_BIN!" distribution\windows\installer.iss
    if !ERRORLEVEL! neq 0 (
        echo [ERROR] Inno Setup compilation failed.
        exit /b !ERRORLEVEL!
    )
    echo [SUCCESS] Windows Inno Setup installer generated in dist\FantasTech-Setup-0.1.0.exe
) else (
    echo [INFO] Inno Setup compiler unavailable. Generating native standalone Windows installer...
    powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0make-standalone-installer.ps1"
    if !ERRORLEVEL! neq 0 (
        echo [ERROR] Standalone installer generation failed.
        exit /b !ERRORLEVEL!
    )
    echo [SUCCESS] Standalone Windows installer generated in dist\FantasTech-Setup-0.1.0.exe
)

echo ==============================================================================
echo [SUCCESS] Windows Installer ready: dist\FantasTech-Setup-0.1.0.exe
echo ==============================================================================
