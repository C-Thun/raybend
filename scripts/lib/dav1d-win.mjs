/**
 * **Windows 侧构建要传给 cargo 的 dav1d 环境变量**（AVIF 解码，`memory/FUNCTION-IMAGING.md` §1）。
 *
 * # 为什么需要它
 *
 * `image` 的 `avif-native` feature 拉进 `dav1d` crate，而 `dav1d-sys` 的 build.rs 用
 * `system-deps` 找 dav1d 库。Windows 上没有 pkg-config，所以这里**直接告诉它库在哪**
 * （`system-deps` 的 `NO_PKG_CONFIG` + `SEARCH_NATIVE` 组合，已核对源码）：
 *
 * * `SYSTEM_DEPS_DAV1D_NO_PKG_CONFIG=1` —— 跳过 pkg-config 探测；
 * * `SYSTEM_DEPS_DAV1D_LIB=dav1d` + `LINK=static` —— 链静态库（**不打 DLL 进安装包**）；
 * * `SEARCH_NATIVE` / `INCLUDE` —— 库与头文件的位置。
 *
 * 静态库是**一次性构建**出来的，不在仓库里：
 * `scripts/build-dav1d-win.cmd`（meson + ninja + nasm + VS Build Tools，约 3 分钟），
 * 默认落 `C:\rb-deps\dav1d-1.5.0\`。换机器 / 换盘时先跑一次那个脚本，
 * 或者用 `RAYBEND_DAV1D_WIN_DIR` 指到别处。
 *
 * # WSL → Windows 的坑
 *
 * 只有写进 `WSLENV` 的变量才会跨过去（`AGENTS.md` §5.3 第 2 条）。
 * 所以这里同时给出「变量本体」和「拼好的 WSLENV」，调用方两个都要传。
 */

/** 一次性构建出来的静态库位置（可用 `RAYBEND_DAV1D_WIN_DIR` 覆盖）。 */
export const DAV1D_WIN_DIR =
  process.env.RAYBEND_DAV1D_WIN_DIR ?? "C:\\rb-deps\\dav1d-1.5.0";

/** 给 Windows 侧 cargo 的 dav1d 变量（值必须是 Windows 路径）。 */
export const DAV1D_WIN_ENV = dav1dWindowsEnv(DAV1D_WIN_DIR);

export function dav1dWindowsEnv(directory) { return {
  SYSTEM_DEPS_DAV1D_NO_PKG_CONFIG: "1",
  SYSTEM_DEPS_DAV1D_LIB: "dav1d",
  SYSTEM_DEPS_DAV1D_LINK: "static",
  SYSTEM_DEPS_DAV1D_SEARCH_NATIVE: `${directory}\\lib`,
  SYSTEM_DEPS_DAV1D_INCLUDE: `${directory}\\include`,
}; }

/**
 * 拼出这次 `cmd.exe` 调用要用的环境变量：
 * 原来的 `process.env` + dav1d 那几个 + `WSLENV`（把新变量并进去，别覆盖掉已有的）。
 *
 * @param {Record<string, string | undefined>} extra 额外要传的变量（如 `CARGO_TARGET_DIR`）
 * @param {string[]} extraWslEnv 额外要并进 `WSLENV` 的名字（如 `WGPU_BACKEND`）
 */
export function windowsBuildEnv(extra = {}, extraWslEnv = [], environment = process.env) {
  const vars = { ...dav1dWindowsEnv(environment.RAYBEND_DAV1D_WIN_DIR ?? DAV1D_WIN_DIR), ...extra };
  const names = [...Object.keys(vars), ...extraWslEnv];
  // 我们传的路径已经是 Windows 形式，不继承旧的 /p 翻译标志，否则会被 WSL 再翻一次。
  const existing = (environment.WSLENV ?? "").split(":").filter(item => item && !names.includes(item.split("/")[0]));
  const wslEnv = [...new Set([...existing, ...names])].join(":");
  return { ...environment, ...vars, WSLENV: wslEnv };
}
