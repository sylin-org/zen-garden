@echo off
setlocal

set IMAGE=sylinorg/zen-garden-weaviate-orchestrator
set TAG=latest
set CONTAINER=zen-garden-weaviate-orchestrator

:: Resolve workspace root (three levels up from this script)
set SCRIPT_DIR=%~dp0
pushd "%SCRIPT_DIR%..\..\..\"
set WORKSPACE=%CD%
popd

echo.
echo  Zen Garden Weaviate Orchestrator
echo  ────────────────────────────────
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
    -p 7194:7194 ^
    -e KOI_ENDPOINT=http://host.docker.internal:5641 ^
    -v zen-garden-weaviate-data:/data ^
    --restart unless-stopped ^
    %IMAGE%:%TAG%

echo.
echo  Container started: %CONTAINER%
echo  Dashboard: http://localhost:7194
echo.
