@echo off
setlocal enabledelayedexpansion
rem ============================================================
rem  raybend：在 Windows 侧构建 **dav1d 静态库**（AVIF 解码用）
rem
rem  什么时候要跑：**换机器 / 换盘 / 删过 C:\rb-deps 时跑一次**。
rem  产物（默认 C:\rb-deps\dav1d-1.5.0\）不在仓库里，但 Windows 侧的
rem  cargo 构建每次都要它（`scripts/lib/dav1d-win.mjs` 把位置告诉 system-deps）。
rem
rem  前置（winget 装，都是一次性的）：
rem    winget install --id mesonbuild.meson
rem    winget install --id Ninja-build.Ninja
rem    winget install --id nasm.nasm
rem    （另需 VS Build Tools 的 C++ 工具链 —— Rust 的 msvc 目标本来就要它）
rem
rem  版本固定 1.5.0：`dav1d-sys` 内部构建用的也是这个 tag，
rem  上游 README 写明「>= 1.3.0，> 1.5.0 不保证」。
rem
rem  产物位置可用 RAYBEND_DAV1D_WIN_DIR 覆盖（两边都要改，见 AGENTS.md §5.3）。
rem ============================================================
set "DAV1D_VER=1.5.0"
set "DEPS_ROOT=C:\rb-deps"
set "SRC_DIR=%DEPS_ROOT%\src\dav1d"
set "BUILD_DIR=%DEPS_ROOT%\build\dav1d-%DAV1D_VER%"
set "PREFIX=%DEPS_ROOT%\dav1d-%DAV1D_VER%"

rem ── ① VS 环境（ninja 调 cl.exe 要用；meson 自己会找 VS，ninja 不会）──
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%VSWHERE%" (
  echo [ERR] 找不到 vswhere.exe —— 装 Visual Studio Build Tools（含 C++ 工具链）后重试
  exit /b 1
)
for /f "usebackq tokens=*" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSPATH=%%i"
if not defined VSPATH (
  echo [ERR] vswhere 没找到 VC 工具链
  exit /b 1
)
call "%VSPATH%\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
if errorlevel 1 ( echo [ERR] vcvars64.bat 失败 & exit /b 1 )

rem ── ② 工具链上 PATH（winget 装完之后新开窗口才会有；这里显式补一次）──
set "PATH=%LOCALAPPDATA%\bin\NASM;%LOCALAPPDATA%\Microsoft\WinGet\Packages\Ninja-build.Ninja_Microsoft.Winget.Source_8wekyb3d8bbwe;C:\Program Files\Meson;%PATH%"
where nasm >nul 2>&1 || ( echo [ERR] 找不到 nasm —— winget install --id nasm.nasm & exit /b 1 )
where ninja >nul 2>&1 || ( echo [ERR] 找不到 ninja —— winget install --id Ninja-build.Ninja & exit /b 1 )
where meson >nul 2>&1 || ( echo [ERR] 找不到 meson —— winget install --id mesonbuild.meson & exit /b 1 )

rem ── ③ 取源码 ──
if not exist "%DEPS_ROOT%\src" mkdir "%DEPS_ROOT%\src"
if not exist "%SRC_DIR%\.git" (
  echo [1/4] git clone dav1d %DAV1D_VER%
  git clone --depth 1 -b %DAV1D_VER% https://code.videolan.org/videolan/dav1d.git "%SRC_DIR%" || exit /b 1
) else (
  echo [1/4] 源码已在 %SRC_DIR%
)

rem ── ④ 构建（release + 静态库；不打 tools/tests）──
echo [2/4] meson setup
if exist "%BUILD_DIR%" rmdir /s /q "%BUILD_DIR%"
meson setup --buildtype=release -Ddefault_library=static -Denable_tools=false -Denable_tests=false --prefix="C:/rb-deps/dav1d-%DAV1D_VER%" "%BUILD_DIR%" "%SRC_DIR%" || exit /b 1

echo [3/4] ninja
ninja -C "%BUILD_DIR%" || exit /b 1

echo [4/4] meson install
meson install -C "%BUILD_DIR%" || exit /b 1

echo.
echo ==== 产物 ====
dir /b "%PREFIX%\lib\*.a" 2>nul
if not exist "%PREFIX%\lib\libdav1d.a" (
  echo [ERR] 没看到 libdav1d.a —— 构建没成功
  exit /b 1
)
echo ✓ dav1d 静态库就绪：%PREFIX%
echo   接下来直接 pnpm debug:win 即可（脚本会把位置告诉 cargo）
