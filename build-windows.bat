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
        echo         Or: choco install llvm
        pause
        exit /b 1
    )
    :: Refresh PATH
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

:: Copy to dist
if not exist dist mkdir dist
copy /Y target\release\SimplePDFReader.exe dist\PDFReader.exe

echo.
echo ============================================
echo   Build complete!
echo   Output: dist\PDFReader.exe
echo ============================================
echo.
echo   Usage:
echo     PDFReader.exe                    - Open viewer
echo     PDFReader.exe file.pdf           - Open a PDF
echo     PDFReader.exe --register         - Set as default PDF viewer
echo     PDFReader.exe --unregister       - Remove file association
echo.
pause
