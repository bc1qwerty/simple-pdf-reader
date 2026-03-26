@echo off
echo ============================================
echo   Simple PDF Reader - Windows Build
echo ============================================

:: Check Rust
where rustc >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Rust not found. Install from https://rustup.rs
    pause
    exit /b 1
)

:: Check LLVM/clang (needed for mupdf bindgen)
where clang >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo [INFO] LLVM not found. Installing via winget...
    winget install LLVM.LLVM -s winget --accept-package-agreements --accept-source-agreements
    if %ERRORLEVEL% NEQ 0 (
        echo [ERROR] Failed to install LLVM. Install manually: https://releases.llvm.org
        pause
        exit /b 1
    )
    set "PATH=%PATH%;C:\Program Files\LLVM\bin"
)

:: Set LIBCLANG_PATH
set "LIBCLANG_PATH=C:\Program Files\LLVM\lib"

echo.
echo [BUILD] Compiling release build...
cargo build --release
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Build failed!
    pause
    exit /b 1
)

:: Create desktop shortcut
echo [SHORTCUT] Creating desktop shortcut...
set "EXE_PATH=%CD%\target\release\SimplePDFReader.exe"
set "WORK_DIR=%CD%\target\release"

powershell -Command "$ws = New-Object -ComObject WScript.Shell; $sc = $ws.CreateShortcut([Environment]::GetFolderPath('Desktop') + '\Simple PDF Reader.lnk'); $sc.TargetPath = '%EXE_PATH%'; $sc.WorkingDirectory = '%WORK_DIR%'; $sc.IconLocation = '%EXE_PATH%,0'; $sc.Description = 'Simple PDF Reader'; $sc.Save()"

echo.
echo ============================================
echo   Build complete!
echo   Exe: target\release\SimplePDFReader.exe
echo   Shortcut: Desktop\Simple PDF Reader
echo ============================================
echo.
echo   Usage:
echo     SimplePDFReader.exe                - Open viewer
echo     SimplePDFReader.exe file.pdf       - Open a PDF
echo     SimplePDFReader.exe --register     - Set as default PDF viewer
echo     SimplePDFReader.exe --unregister   - Remove file association
echo.
pause
