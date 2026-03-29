@echo off
setlocal

set IMAGE=sylinorg/zen-garden-ai-orchestrator
set TAG=latest
set CONTAINER=zen-garden-ai-orchestrator

:: Resolve workspace root (three levels up from this script)
set SCRIPT_DIR=%~dp0
pushd "%SCRIPT_DIR%..\..\..\"
set WORKSPACE=%CD%
popd

echo.
echo  Zen Garden AI Orchestrator
echo  ──────────────────────────
echo  Image:     %IMAGE%:%TAG%
echo  Workspace: %WORKSPACE%
echo.

:: Build the image (context = workspace root, dockerfile = this folder)
echo [1/2] Building image...
docker build -t %IMAGE%:%TAG% -f "%SCRIPT_DIR%Dockerfile" "%WORKSPACE%"
if errorlevel 1 (
    echo ERROR: Build failed.
    exit /b 1
)

:: Stop any existing container
docker rm -f %CONTAINER% >nul 2>&1

:: Run
echo [2/2] Starting container...
docker run -d ^
    --name %CONTAINER% ^
    -p 7190:7190 ^
    -p 21434:21434 ^
    -p 21435:21435 ^
    -p 21436:21436 ^
    -p 21437:21437 ^
    -p 21438:21438 ^
    -p 21439:21439 ^
    -v zen-garden-ai-data:/data ^
    --restart unless-stopped ^
    %IMAGE%:%TAG%

echo.
echo  Container started: %CONTAINER%
echo  Dashboard:  http://localhost:7190
echo  Ollama:     http://localhost:21434
echo  ComfyUI:    http://localhost:21435
echo  Whisper:    http://localhost:21436
echo  Speech:     http://localhost:21437
echo  Infinity:   http://localhost:21438
echo  Translate:  http://localhost:21439
echo.
