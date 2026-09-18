#!/usr/bin/env node
/**
 * 渲染 spike 的一键脚本（`PLAN.md` A.2 的 7.6）：**构建 → 开窗 → 测量 → 落盘**。
 *
 * 目标是把人的操作压到「点几下 + 目视填两格」——渲染、适配器信息、帧统计、
 * 坐标往返偏差、设备丢失恢复成败全部由程序算完并写进报告。
 *
 * 用法（在 WSL 里）：
 *
 * ```bash
 * pnpm spike:win
 * ```
 *
 * 它会依次做四件事：
 *
 * 1. `pnpm build` —— **先重建 dist**。这一步不省：`dist/` 是编译期嵌进 exe 的，
 *    忘了它就会让人拿着旧界面白测一轮（`AGENTS.md` §5.3 第 7 条，血的教训）。
 * 2. 在 Windows 侧 `cargo build -p raybend-desktop -p raybend --features custom-protocol`
 *    （产物落在 `C:\rb-target\raybend`，**不能**放在 9p 共享上：增量编译的锁语义不支持）。
 * 3. `pnpm check:win` —— 既比时间也比内容，不合格直接停。
 * 4. 启动 exe（带 `RAYBEND_SPIKE=1`，应用启动时就把 spike 窗口开出来）。
 *
 * 然后**人就去做 `plans/M2-W1-windows-gpu.md` 里的那几件事**，最后在 spike 窗口里
 * 点「写报告」，把 `spike-report.md` 发回来。
 *
 * `--dry-run`：只做前三步（构建 + 核对），**不开窗**。给自己验证这条路径用，
 * 免得为了确认脚本没写错而在别人桌面上弹一个窗口。
 *
 * 退出码：0 = 构建与启动都完成；1 = 某一步失败（会打印补救命令）。
 */

import { spawn, spawnSync } from "node:child_process";
import { existsSync, openSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const TARGET_DIR = process.env.WIN_TARGET_DIR ?? "C:\\rb-target\\raybend";
const TARGET_DIR_WSL = process.env.WIN_TARGET_DIR_WSL ?? "/mnt/c/rb-target/raybend";
const EXE = process.env.WIN_EXE ?? join(TARGET_DIR_WSL, "debug", "raybend-desktop.exe").replaceAll("\\", "/");
const REPORT_DIR_WSL = "/mnt/c/rb-target/spike-report";

const DRY_RUN = process.argv.includes("--dry-run");
/** 追加模式打开日志文件，返回可写的 fd（给子进程的 stdout/stderr 用）。 */
const logFd = (path) => openSync(path, "a");
const TOTAL = DRY_RUN ? 3 : 4;
const step = (n, text) => console.log(`\n[${n}/${TOTAL}] ${text}`);
const fail = (message, remedy) => {
  console.error(`\n✗ ${message}`);
  if (remedy) console.error(`  补救：\n${remedy}`);
  process.exit(1);
};

const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, { cwd: ROOT, stdio: "inherit", ...options });
  if (result.error) fail(`执行 ${command} 失败：${result.error.message}`);
  return result.status === 0;
};

/* ── 0. 环境自检 ───────────────────────────────────────────── */
if (process.platform !== "linux") {
  fail(
    `这个脚本要在 WSL 里跑（当前平台 ${process.platform}）`,
    "在 WSL 里执行：pnpm spike:win",
  );
}
const distro = process.env.WSL_DISTRO_NAME;
if (!distro) {
  fail(
    "读不到 WSL_DISTRO_NAME —— 无法拼出给 Windows 侧用的 UNC 仓库路径",
    "在 WSL 的登录 shell 里跑（不是 ssh 的裸环境）：echo $WSL_DISTRO_NAME 应当有输出",
  );
}
const uncRepo = `\\\\wsl.localhost\\${distro}\\${ROOT.replace(/^\//, "").replaceAll("/", "\\")}`;

/* ── 1. 前端产物 ──────────────────────────────────────────── */
step(1, "重建前端产物（pnpm build）—— dist 是编译期嵌进 exe 的，不能省");
if (!run("pnpm", ["build"])) {
  fail("pnpm build 失败", "先修前端构建错误再跑本脚本：pnpm build");
}

/* ── 2. Windows 侧构建 ────────────────────────────────────── */
step(2, `构建 Windows 产物（产物落 ${TARGET_DIR}，不走 9p 共享）`);
const buildCmd = `pushd ${uncRepo} & cargo build -p raybend-desktop -p raybend --features custom-protocol`;
const built = run("cmd.exe", ["/c", buildCmd], {
  env: {
    ...process.env,
    // 跨 WSL→Windows 传环境变量走 WSLENV（cmd 的 `set VAR=x & …` 会把空格吃进值里）
    CARGO_TARGET_DIR: TARGET_DIR,
    WSLENV: process.env.WSLENV ? `${process.env.WSLENV}:CARGO_TARGET_DIR` : "CARGO_TARGET_DIR",
  },
});
if (!built) {
  fail(
    "Windows 侧构建失败",
    [
      "常见原因：",
      `  · 忘了 --features custom-protocol（会白屏，且这一步不会报错）`,
      `  · 产物路径不在 Windows 本地盘（9p 共享不支持 rustc 增量编译的锁语义）`,
      `  · 直接手跑一次看详细报错：`,
      `      export CARGO_TARGET_DIR='${TARGET_DIR}'`,
      `      export WSLENV='CARGO_TARGET_DIR'`,
      `      cmd.exe /c 'pushd ${uncRepo} & cargo build -p raybend-desktop -p raybend --features custom-protocol'`,
    ].join("\n"),
  );
}

/* ── 3. 产物一致性 ────────────────────────────────────────── */
step(3, "核对产物（比时间也比内容）—— check:win 不合格就不许往下走");
if (!run("pnpm", ["check:win"], { env: { ...process.env, WIN_EXE: EXE } })) {
  fail(
    "产物与 dist 不一致（exe 比 dist 旧，或内嵌资源对不上）",
    "别硬着头皮测：重跑本脚本即可（第 1 步会重新 build）",
  );
}
if (!existsSync(EXE)) {
  fail(`构建过了却找不到产物：${EXE}`, "检查 WIN_EXE / WIN_TARGET_DIR 环境变量");
}

/* ── 4. 开窗 ──────────────────────────────────────────────── */
if (DRY_RUN) {
  console.log(`
────────────────────────────────────────────────────────────────
--dry-run：构建与核对都过了，**没有开窗**。
真跑请去掉 --dry-run（或者直接双击/命令行跑 exe）。
────────────────────────────────────────────────────────────────`);
  process.exit(0);
}
step(4, "启动应用（带 --spike=1 与 RAYBEND_SPIKE=1 —— 启动时就把 spike 窗口开出来）");
/*
 * **两个标记都给**：脚本是经 WSL→Windows 起进程的，环境变量能不能透传看互操作层，
 * 命令行参数则是硬的 —— 哪个到了都能开窗（判定见 `src-tauri/src/lib.rs` 的 `spike_requested_from`）。
 *
 * 子进程的 stdout/stderr 落到日志文件而不是丢掉：开窗失败时 Rust 侧只 `eprintln!` 一行，
 * 丢了就只能猜（上一版就是这样：窗口没开出来，而脚本还在印「已经开了」）。
 */
const APP_LOG = join(tmpdir(), "raybend-desktop.log");
/*
 * ⚠️ WSL→Windows **只转 `WSLENV` 里列出的变量**（本仓血的教训，AGENTS.md §5.3 第 2 条）。
 *
 * 2026-09-19 实测：`WGPU_BACKEND=dx12 pnpm spike:win` 起出来的窗口仍是 Vulkan，
 * 报告里写着「`WGPU_BACKEND`：(未设置 → 走默认优先级)」—— 变量在互操作层被丢了，
 * 而下面那段帮助文字还在教人这么用。所以把后端实验要用的变量显式并进 `WSLENV`。
 * （`RAYBEND_SPIKE` 不需要：脚本同时传了 `--spike=1` 这个**参数**，参数不受互操作限制。）
 */
const wslEnv = ["WGPU_BACKEND", process.env.WSLENV].filter(Boolean).join(":");
const child = spawn(EXE, ["--spike=1"], {
  detached: true,
  stdio: ["ignore", logFd(APP_LOG), logFd(APP_LOG)],
  env: { ...process.env, RAYBEND_SPIKE: "1", ...(wslEnv ? { WSLENV: wslEnv } : {}) },
});
child.unref();

console.log(`
────────────────────────────────────────────────────────────────
已经开了。接下来请照 plans/M2-W1-windows-gpu.md 里的表格逐项走：

  · 表格里「怎么做 → 看什么 → 填什么」三步，能程序算的已经算完了
  · 最后在 spike 窗口右栏填那两三个主观项，点「写报告」
  · 报告落在：${REPORT_DIR_WSL}/
      spike-report.md   ← 把这个发回来（人看的那份）
      spike-report.json ← 同数据的机器版

  从 WSL 看：  cat ${REPORT_DIR_WSL}/spike-report.md

  窗口没出来时：  cat ${APP_LOG}   （Rust 侧把失败原因写在这儿）
────────────────────────────────────────────────────────────────

提示：
  · 换后端重测（A.2 的回退实验）：先设 WGPU_BACKEND，再重跑本脚本
  ·    （WSL 侧设的变量经互操作会丢，脚本已把 WGPU_BACKEND 并进 WSLENV 转发，故这三条有效）：
      WGPU_BACKEND=dx12 pnpm spike:win
      WGPU_BACKEND=vulkan pnpm spike:win
      WGPU_BACKEND=gl pnpm spike:win
  · 只想重开窗口、不重新构建：直接跑
      RAYBEND_SPIKE=1 ${EXE}
  · DPI 实验：把系统缩放改成 100% / 125% / 150%，各重开一次窗口再看那张表
`);
