@echo off
setlocal

set IMAGE=sylinorg/zen-garden-ollama-orchestrator

:: Read version from orchestrator Cargo.toml
for /f "tokens=3 delims= " %%v in ('findstr /r "^version" "%~dp0Cargo.toml"') do (
    set RAW=%%v
    goto :found
)
:found
:: Strip quotes
set VERSION=%RAW:"=%

echo.
echo  Zen Garden Ollama Orchestrator — Push to Docker Hub
echo  ────────────────────────────────────────────────────
echo  Image:   %IMAGE%
echo  Version: %VERSION%
echo.

:: Resolve workspace root
set SCRIPT_DIR=%~dp0
pushd "%SCRIPT_DIR%..\..\..\"
set WORKSPACE=%CD%
popd

:: Build with version label baked in
echo [1/4] Building image...
docker build ^
    -t %IMAGE%:%VERSION% ^
    -t %IMAGE%:latest ^
    --label org.opencontainers.image.version=%VERSION% ^
    --label org.opencontainers.image.created=%DATE:~6,4%-%DATE:~3,2%-%DATE:~0,2% ^
    -f "%SCRIPT_DIR%Dockerfile" ^
    "%WORKSPACE%"
if errorlevel 1 (
    echo ERROR: Build failed.
    exit /b 1
)

:: Verify
echo [2/4] Verifying image...
docker inspect --format "{{.Config.Labels}}" %IMAGE%:%VERSION%
echo.

:: Push versioned tag
echo [3/4] Pushing %IMAGE%:%VERSION%...
docker push %IMAGE%:%VERSION%
if errorlevel 1 (
    echo ERROR: Push failed. Run "docker login" first.
    exit /b 1
)

:: Push latest tag
echo [4/4] Pushing %IMAGE%:latest...
docker push %IMAGE%:latest
if errorlevel 1 (
    echo ERROR: Push latest failed.
    exit /b 1
)

echo.
echo  Done. https://hub.docker.com/r/%IMAGE%
echo.
