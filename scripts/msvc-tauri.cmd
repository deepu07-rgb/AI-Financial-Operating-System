@echo off
setlocal
set "AIFOS_VSDEVCMD="

for %%E in (BuildTools Community Professional Enterprise) do (
  if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\2022\%%E\Common7\Tools\VsDevCmd.bat" (
    set "AIFOS_VSDEVCMD=%ProgramFiles(x86)%\Microsoft Visual Studio\2022\%%E\Common7\Tools\VsDevCmd.bat"
  )
)

if not defined AIFOS_VSDEVCMD (
  echo Visual Studio 2022 C++ Build Tools were not found. 1>&2
  echo Install Desktop development with C++ and a Windows SDK. 1>&2
  exit /b 1
)

call "%AIFOS_VSDEVCMD%" -no_logo -arch=x64 -host_arch=x64 >nul
if errorlevel 1 exit /b %errorlevel%

for %%R in ("%~dp0..") do set "CARGO_TARGET_DIR=%%~fR\target"
pnpm --filter @aifos/desktop tauri %*
exit /b %errorlevel%
