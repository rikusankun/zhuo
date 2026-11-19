@echo off
setlocal
title Zhuo
cd /d "%~dp0"
where node >nul 2>nul
if errorlevel 1 (
  echo Please install Node.js LTS first.
  pause
  exit /b 1
)
where cargo >nul 2>nul
if errorlevel 1 (
  echo Please install Rust first.
  pause
  exit /b 1
)
if not exist node_modules (
  call npm ci
  if errorlevel 1 exit /b 1
)
echo Open http://localhost:3020 after the services are ready.
echo Press Ctrl+C to stop Zhuo.
call npm run start
exit /b %errorlevel%
