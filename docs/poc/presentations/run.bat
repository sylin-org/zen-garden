@echo off
title Zen Garden Diagrams
color 0A

echo.
echo  ===============================================================
echo                    ZEN GARDEN DIAGRAMS
echo            Animated visualizations for video
echo  ===============================================================
echo.

:: Check for Node.js
echo [1/4] Checking for Node.js...
where node >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo  ERROR: Node.js is not installed!
    echo.
    echo  Please install Node.js from: https://nodejs.org/
    echo  Download the LTS version, install it, then run this script again.
    echo.
    pause
    exit /b 1
)

:: Show Node version
for /f "tokens=*" %%i in ('node -v') do set NODE_VERSION=%%i
echo        Found Node.js %NODE_VERSION%

:: Check for npm
echo [2/4] Checking for npm...
where npm >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo  ERROR: npm is not installed!
    echo  This usually comes with Node.js. Please reinstall Node.js.
    echo.
    pause
    exit /b 1
)

for /f "tokens=*" %%i in ('npm -v') do set NPM_VERSION=%%i
echo        Found npm v%NPM_VERSION%

:: Install dependencies if needed
echo [3/4] Checking dependencies...
if not exist node_modules (
    echo        Installing dependencies - first run, may take a minute...
    call npm install
    if %ERRORLEVEL% NEQ 0 (
        echo.
        echo  ERROR: Failed to install dependencies!
        echo  Try running: npm install
        echo.
        pause
        exit /b 1
    )
    echo        Dependencies installed!
) else (
    echo        Dependencies already installed
)

:: Start the dev server
echo [4/4] Starting development server...
echo.
echo  ===============================================================
echo   Server starting at: http://localhost:5173
echo.
echo   - Use the menu to select diagrams
echo   - Press F11 for fullscreen when recording
echo   - Press Ctrl+C here to stop the server
echo  ===============================================================
echo.

:: Open browser after a short delay and start server
start "" http://localhost:5173
call npm run dev
