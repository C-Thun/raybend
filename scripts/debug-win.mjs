#!/usr/bin/env node
/**
 * 一键 Windows debug 构建（人类 2026-09-20 要求：「日常就反复编译 debug 这一个需求，
 * 不要反复打带环境变量的长指令」）。
 *
 * 用法：`pnpm debug:win` —— 一条命令做完四步，任何一步失败立刻停：
 *
 *   ⓪ 残留进程预检 —— 上一轮跑起来的 exe 还赖在内存里时，cargo 删不掉旧产物，只会丢一句
 *      `failed to remove file ... 拒绝访问 (os error 5)`。有窗口（真的开着）就停下请人关，
 *      没窗口（窗口早关了、进程不退的僵尸）就直接清掉；探针失败不阻塞构建。
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
/** 同一个产物的 Windows 侧路径（查进程与占用必须用它 —— WSL 路径 Windows 侧认不出来） */
const EXE_WIN = `${CARGO_TARGET_DIR}\\debug\\raybend-desktop.exe`;

/** 仓库根的 Windows UNC 形式（`\\wsl.localhost\<distro>\...`）—— 用 wslpath 现算，不写死发行版名 */
function windowsRepoPath() {
  try {
    return execSync("wslpath -w .", { cwd: ROOT, encoding: "utf8" }).trim();
  } catch {
    console.error("✗ 算不出仓库的 Windows 路径（wslpath -w 失败）—— 这个脚本只在 WSL 里跑");
    process.exit(2);
  }
}

/** 启动不到这么久的一律不动：可能还在闪屏阶段（主窗口 `visible:false`，要等前端报就绪） */
const FRESH_PROCESS_MS = 20_000;

/**
 * tao / 系统自己开的窗口，**不是**应用的界面 —— 判定「应用开着没」时必须排掉。
 *
 * `Tao Thread Event Target` 是事件循环的消息窗（tao 0.35 `event_loop.rs:627` 自己注册的）：
 * **只要进程活着它就存在**（哪怕一个界面窗口都没有），而且 `IsWindowVisible` 还返回 true。
 * 2026-09-23 那次排障就是靠它把「只剩一个消息窗的僵尸」认出来。
 *
 * `PseudoConsoleWindow` 是 **ConPTY 伪终端窗**（从控制台拉起进程时由 Windows 控制台宿主创建，
 * 归在应用 PID 名下、`IsWindowVisible` 也为真）—— 2026-09-24 崔总的实例就是从控制台起的，
 * 它把「已经关了窗口的僵尸」误判成「开着」，脚本于是拒绝清理。
 *
 * 反过来，应用**真正**的窗口类名是 `Tauri Window`（`tauri-runtime-wry-2.11.4/src/lib.rs:856`
 * 设的），不在这个表里 —— 所以「有可见的、类名不在这里的顶层窗」就等于「界面还开着」。
 */
const INTERNAL_WINDOW_CLASSES = new Set([
  "Tao Thread Event Target",
  "PseudoConsoleWindow",
  "IME",
  "MSFTIME UI",
  "Default IME",
]);

/**
 * PowerShell 探针：一次问出「所有 raybend-desktop.exe 进程 + 它们名下的顶层窗口 + exe 是否被独占占用」。
 *
 * 走 `-EncodedCommand`（UTF-16LE base64）：彻底绕开 WSL→Windows 的引号/转义地雷
 * （`AGENTS.md` §5.3 第 3 条），脚本里怎么写单双引号都不怕。输出只认标记之后的 JSON ——
 * 中文一律由本文件打印，不受控制台代码页影响（直接印 PowerShell 的中文会变乱码）。
 */
const PS_PROBE = String.raw`
$ErrorActionPreference = 'Stop'
# 关掉进度流：stderr 不是控制台时，PowerShell 5.1 会把进度记录序列化成 CLIXML 注到输出里
# （实测看到一坨「#< CLIXML ...」的 XML，内容其实是「准备首次使用模块」）。
# ⚠ 这个模板里**不能出现反引号**——它会提前结束 JS 的模板字面量（已经栽过一次）。
$ProgressPreference = 'SilentlyContinue'
# 输出一律 UTF-8：不设它，PS 5.1 按控制台代码页（936）写，经 WSL 回来就是乱码
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -Namespace Rb -Name Win -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr l);
public delegate bool EnumWindowsProc(IntPtr h, IntPtr l);
[DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
[DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr h);
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
[DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, System.Text.StringBuilder s, int n);
'@
$script:wins = New-Object System.Collections.ArrayList
$cb = [Rb.Win+EnumWindowsProc]{
  param($h, $l)
  $owner = 0
  [void][Rb.Win]::GetWindowThreadProcessId($h, [ref]$owner)
  $cls = New-Object System.Text.StringBuilder 256
  [void][Rb.Win]::GetClassName($h, $cls, 256)
  [void]$script:wins.Add([pscustomobject]@{
    pid = $owner
    cls = $cls.ToString()
    visible = [bool][Rb.Win]::IsWindowVisible($h)
    titleLen = [int][Rb.Win]::GetWindowTextLength($h)
  })
  return $true
}
[void][Rb.Win]::EnumWindows($cb, [IntPtr]::Zero)

$procs = @()
foreach ($p in @(Get-CimInstance Win32_Process -Filter "Name='raybend-desktop.exe'")) {
  $procs += [pscustomobject]@{
    pid = [int]$p.ProcessId
    path = $p.ExecutablePath
    startedAt = $p.CreationDate.ToString('o')
  }
}

$exe = '${EXE_WIN}'
$exists = Test-Path -LiteralPath $exe
$locked = $false
if ($exists) {
  try {
    $fs = [System.IO.File]::Open($exe, [System.IO.FileMode]::Open, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
    $fs.Close()
  } catch { $locked = $true }
}

'<<<RB-PROBE>>>' + (@{
  exe = $exe
  exists = [bool]$exists
  locked = $locked
  processes = $procs
  windows = @($script:wins)
} | ConvertTo-Json -Depth 6 -Compress)
`;

/** 跑探针，取回 JSON。失败一律抛错，由调用方决定「不阻塞构建」。 */
function probeWindowsState() {
  const encoded = Buffer.from(PS_PROBE, "utf16le").toString("base64");
  const result = spawnSync(
    "powershell.exe",
    ["-NoProfile", "-NonInteractive", "-EncodedCommand", encoded],
    { encoding: "utf8", cwd: ROOT, maxBuffer: 8 * 1024 * 1024, timeout: 60_000, stdio: ["ignore", "pipe", "pipe"] },
  );
  if (result.status !== 0) {
    /* 带上 PS 自己的最后一行（异常消息通常就在末行）——没它时只能看到一个退出码，白查 */
    const why = (result.stderr ?? "").trim().split("\n").slice(-1)[0] ?? "";
    throw new Error(`powershell 退出码 ${result.status ?? "（超时）"}${why ? `：${why}` : ""}`);
  }
  const marker = "<<<RB-PROBE>>>";
  const at = (result.stdout ?? "").indexOf(marker);
  if (at < 0) throw new Error("探针输出里没有 JSON 标记");
  /* 解析不了就换一句能看懂的话抛出去 —— 调用方（`reapStaleInstance`）会把「探针跑了」当成
     可降级的失败，而不是把构建卡死；这里只负责把原因说清楚。 */
  try {
    return JSON.parse(result.stdout.slice(at + marker.length).trim());
  } catch {
    throw new Error("探针输出不是合法 JSON");
  }
}

/** Windows 路径比对：大小写不敏感、分隔符统一（别用 `===` 比路径） */
function sameWinPath(value, expected) {
  const norm = (text) => String(text).replace(/[\\/]+/g, "\\").replace(/\\+$/, "").toLowerCase();
  return norm(value) === norm(expected);
}

/**
 * 步骤 ⓪：清掉「窗口早关了、进程却赖着不退」的残留实例 —— 它锁着 exe，cargo 就删不掉旧产物：
 *
 * ```
 * error: failed to remove file `C:\rb-target\raybend\debug\raybend-desktop.exe`
 * Caused by: 拒绝访问。 (os error 5)
 * ```
 *
 * 2026-09-23 崔总撞上这个（「我明明应用关着没开啊」）：窗口**确实**没了，可进程从 9/21 11:14
 * 一直挂到当天，锁着 exe 让每次构建都失败。最可能的来源是 `pnpm perf:win --launch` ——
 * 它 `detached: true` + `unref()` 拉起 exe，跑完**从不回收**（设计如此：复用现有实例）。
 *
 * 判定规则（宁可停下问人，也不乱杀）：
 *   · 有**可见且不是内部消息窗**的顶层窗 → 应用正开着 → 停下，请人自己关（不替人杀活着的应用）
 *   · 启动不到 20 秒 → 可能还在闪屏阶段 → 也停
 *   · 没有可见界面窗 → 僵尸 → 清掉
 *   · 探针本身失败 → **不阻塞构建**：「诊断工具挂了就编不了」比原问题更糟
 *
 * ⚠ 不拿「窗口标题非空」或「窗口数量」当判据：窗口类名才是硬事实。标题一为空就当成僵尸、
 * 或数错窗口个数，后果是**把开着窗口的应用杀了**——而误停只是麻烦一下。两边代价不对称，
 * 所以判据只取最保守的那个。
 *
 * 想留着样本排查：`RB_KEEP_STALE=1 pnpm debug:win`（会直接停下，不打这次构建）。
 */
function reapStaleInstance() {
  console.log("── ⓪ 残留进程预检（旧 exe 被还在跑的实例锁住时，cargo 只会丢一句看不懂的 os error 5）");

  let state;
  try {
    state = probeWindowsState();
  } catch (error) {
    console.error(`! 探针没跑通（${error.message}）—— 不阻塞构建，继续；若 cargo 报「拒绝访问」，就是残留进程。`);
    return;
  }

  const windows = Array.isArray(state.windows) ? state.windows : [];
  const processes = Array.isArray(state.processes) ? state.processes : [];
  /* 读不到路径的（提权进程之类）也算进来 —— 宁可报出来让人看一眼，也不要漏 */
  const ours = processes.filter((item) => !item.path || sameWinPath(item.path, EXE_WIN));

  if (ours.length === 0) {
    if (state.locked) {
      console.error("! exe 被占用，但找不到 raybend-desktop.exe 进程 —— 多半是杀软/索引在扫刚写出来的产物；");
      console.error("  先直接编，若仍报 os error 5 就重跑一次（真失败再看 tasklist | findstr raybend）。");
    } else {
      console.log("· 没有残留实例，exe 可写");
    }
    return;
  }

  const rows = ours.map((item) => {
    const visible = windows.filter((win) => win.pid === item.pid && win.visible);
    const live = visible.filter((win) => !INTERNAL_WINDOW_CLASSES.has(win.cls));
    const started = Date.parse(String(item.startedAt ?? "").replace(/(\.\d{3})\d+/, "$1"));
    const fresh = Number.isFinite(started) && Date.now() - started < FRESH_PROCESS_MS;
    /* 三态写明白，不整嵌套三元 —— 这一行就是要一眼看出「为什么停 / 为什么杀」 */
    let kind = "僵尸";
    if (live.length > 0) kind = "开着";
    else if (fresh) kind = "刚启动";
    return {
      ...item,
      live: live.length,
      visibleClasses: [...new Set(visible.map((win) => win.cls))],
      started,
      kind,
    };
  });

  for (const row of rows) {
    const at = Number.isFinite(row.started)
      ? new Date(row.started).toLocaleString("zh-CN", { hour12: false })
      : "（启动时间读不到）";
    console.log(`  · PID ${row.pid}  ${row.kind}  启动于 ${at}  ${row.path ?? "（路径读不到）"}`);
    /* 把「看到了什么窗口」也印出来：判错了人一眼就能反驳，不用再猜 */
    console.log(`      界面窗 ${row.live} 个｜可见窗类名：${row.visibleClasses.join(", ") || "（无）"}`);
  }

  if (process.env.RB_KEEP_STALE === "1") {
    console.error("✗ RB_KEEP_STALE=1：按你的要求留着它们 —— 但它们锁着 exe，这次构建必然失败，先停下。");
    process.exit(1);
  }

  if (rows.some((row) => row.kind !== "僵尸")) {
    console.error("✗ 上面有实例还开着窗口或刚启动 —— 脚本不替人杀活着的应用，请自己关掉它再重跑。");
    console.error("  若确认它已经卡死不再响应：taskkill /F /PID <上面的 PID>");
    process.exit(1);
  }

  console.log("  僵尸的来历：窗口关掉后进程不退（已知问题，待查）；`pnpm perf:win --launch` 是 detached 拉起、跑完不回收，最容易留下它。");
  console.log("  另一种已知来历（2026-09-24）：旧版全屏看图把 `Esc` 做成 hide —— 隐藏的 fullscreen-viewer 窗口也算一扇窗，");
  console.log("  于是关掉主窗口后进程不退；那一条已在 2026-09-24 的修复里改成真销毁。");
  for (const row of rows) {
    const killed = spawnSync("taskkill.exe", ["/F", "/T", "/PID", String(row.pid)], { encoding: "utf8" });
    if (killed.status === 0) console.log(`  → 已清掉 PID ${row.pid}`);
    else console.error(`  ! PID ${row.pid} 没清掉（taskkill 退出码 ${killed.status}）`);
  }

  let after = null;
  try {
    after = probeWindowsState();
  } catch {
    after = null;
  }
  const remain = (after?.processes ?? []).filter((item) => !item.path || sameWinPath(item.path, EXE_WIN));
  if (after === null || remain.length > 0 || after.locked) {
    console.error("✗ 清完仍被占用 —— 手工看一眼：tasklist | findstr raybend-desktop");
    process.exit(1);
  }
  console.log("✓ 已清干净，exe 可写");
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

reapStaleInstance();

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
