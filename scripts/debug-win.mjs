#!/usr/bin/env node
/**
 * 一键 Windows debug 构建（人类 2026-09-20 要求：「日常就反复编译 debug 这一个需求，
 * 不要反复打带环境变量的长指令」）。
 *
 * 用法：`pnpm debug:win` —— 一条命令做完三步，任何一步失败立刻停：
 *
 *   ① `pnpm build`     —— 前端必须先出 `dist/`（它是**编译期嵌进 exe** 的，不重建就是旧界面）；
 *   ② Windows 侧 cargo —— 产物落 `C:\rb-target\raybend`（9p 共享不支持 rustc 锁语义，
 *      见 `AGENTS.md` §5.3 血泪规），`CARGO_TARGET_DIR` 经 `WSLENV` 透传给 cmd.exe，
 *      **`--features custom-protocol` 必须带**（否则窗口出来页面打不开，§5.3.1）；
 *   ③ `pnpm check:win` —— 比时间戳也比资源名，防止「拿旧 exe 白测一轮」（§5.3 第 7 条）。
 *
 * 结束打印 exe 路径 —— 直接跑它即可：
 *   /mnt/c/rb-target/raybend/debug/raybend-desktop.exe
 *
 * ## 本脚本封装的原命令（`AGENTS.md` §5.3，不再需要手打，排障时对照用）
 *
 * ```bash
 * pnpm build
 * export CARGO_TARGET_DIR='C:\rb-target\raybend'
 * export WSLENV='CARGO_TARGET_DIR'
 * cmd.exe /c 'pushd \\wsl.localhost\Ubuntu-24.04\home\andares\repos\c-thun\raybend & cargo build -p raybend-desktop --features custom-protocol'
 * pnpm check:win
 * ```
 *
 * 只服务本机（WSL → Windows）这一条开发路径；发版走 `pnpm release`，与此无关。
 */

import { spawnSync } from "node:child_process";
import { execSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

/** 仓库根（脚本自身位置向上一级） */
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** Windows 侧产物目录（`AGENTS.md` §5.3：必须落 Windows 本地盘） */
const CARGO_TARGET_DIR = "C:\\rb-target\\raybend";
/** 对应的 WSL 侧路径（check:win 与打印用） */
const EXE = "/mnt/c/rb-target/raybend/debug/raybend-desktop.exe";

/** 仓库根的 Windows UNC 形式（`\\wsl.localhost\<distro>\...`）—— 用 wslpath 现算，不写死发行版名 */
function windowsRepoPath() {
  try {
    return execSync("wslpath -w .", { cwd: ROOT, encoding: "utf8" }).trim();
  } catch {
    console.error("✗ 算不出仓库的 Windows 路径（wslpath -w 失败）—— 这个脚本只在 WSL 里跑");
    process.exit(2);
  }
}

function run(title, command, args, options = {}) {
  console.log(`── ${title}`);
  const result = spawnSync(command, args, {
    stdio: "inherit",
    cwd: ROOT,
    ...options,
  });
  if (result.status !== 0) {
    console.error(`✗ ${title} 失败（退出码 ${result.status}）`);
    process.exit(result.status ?? 1);
  }
}

run("① 前端构建（dist/ 嵌进 exe，必须最新）", "pnpm", ["build"]);

run("② Windows 侧 cargo 构建（debug + custom-protocol）", "cmd.exe", [
  "/c",
  /*
   * 与 `AGENTS.md` §5.3 逐字一致的形式：UNC 路径**不加引号** —— 引号经 WSL→cmd
   * 的参数互操作会被搅掉，pushd 收到残引号就失败（实测）。pushd 对 UNC 会自动映射
   * 盘符（Z:），那个「UNC 路径不受支持」的警告是预期噪音。仓库路径不含空格。
   */
  `pushd ${windowsRepoPath()} & cargo build -p raybend-desktop --features custom-protocol`,
], {
  env: { ...process.env, CARGO_TARGET_DIR, WSLENV: "CARGO_TARGET_DIR" },
});

run("③ 产物核对（时间戳 + 资源名）", "node", ["scripts/check-win-artifact.mjs"]);

console.log(`\n✓ Windows debug 构建完成：${EXE}`);
