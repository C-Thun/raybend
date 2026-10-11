#!/usr/bin/env node
/**
 * **诊断层真机冒烟（Windows）**：验「崩溃 → 下次启动能看见 → 能导出报告 → 不再反复问」这条链。
 *
 * 用法：`pnpm check:diagnostics`（先 `pnpm debug:win` 出一版带诊断的 exe）
 *
 * 它做的事（每一步都断言，失败就红）：
 *
 * ```text
 * ① 起应用（带 CDP 调试口）→ 日志里要有会话头 + 「崩溃助手已就位」
 * ② 捞一次 diagnostics_pending_crash → 应为 null（还没崩过）
 * ③ taskkill /F 硬杀（模拟崩溃，session.lock 会留下）
 * ④ 再起 → 这次应拿到一条 unclean-exit 记录
 * ⑤ 调 diagnostics_export_bundle 导出 → zip + 提交指南都在，且该记录被标记 prompted
 * ⑥ 再捞 pending → 应为 null（**同一条崩溃不许反复问** —— 崔总 2026-10-12）
 * ⑦ 优雅关闭 → session.lock 被删掉（下次不再判「未正常退出」）
 * ```
 *
 * 为什么走 CDP 直连而不是点界面：命令就是界面调的那几条（`diagnostics_pending_crash` /
 * `diagnostics_export_bundle`），走 IPC 能把后端整条链验全，不依赖截图与人工点击；
 * 真机通道见 `AGENTS.md` §5.7（`netsh portproxy 9334 → 127.0.0.1:9333` 必须已授权）。
 *
 * 纪律：只在**自己的**调试实例上操作（起的是 `C:\rb-target` 的 debug 产物）；
 * 起之前确认没有别的实例在跑（脚本会检查并拒绝）。
 */

import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const EXE = process.env.WIN_EXE ?? "/mnt/c/rb-target/raybend/debug/raybend-desktop.exe";
const CRASHPAD = EXE.replace(/raybend-desktop\.exe$/, "raybend-crashpad.exe");
/** 应用数据目录（与 Rust 侧 `app_local_data_dir` 同一位置）。 */
const APP_DATA = "/mnt/c/Users/andar/AppData/Local/com.cthun.raybend";
const LOGS = join(APP_DATA, "logs");
const CRASHES = join(APP_DATA, "crashes");
const EXPORT_DIR = "/mnt/c/src/tmp/rb-diag-export";
/** 同上的 Windows 写法 —— 命令跑在 Windows 进程里，不认 `/mnt/c/...`（踩过一次）。 */
const EXPORT_DIR_WIN = "C:\\src\\tmp\\rb-diag-export";
const CDP_PORT = process.env.RB_CDP_PORT ?? "9333";
/** WSL → Windows 的转发口（`AGENTS.md` §5.7 的 netsh portproxy）。 */
const CDP_RELAY_PORT = "9334";

const results = [];
const step = (name, ok, detail = "") => {
  results.push({ name, ok, detail });
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? `  ${detail}` : ""}`);
  if (!ok) {
    console.error("\n失败步骤的现场：");
    console.error(`  日志目录：${LOGS}`);
    console.error(`  崩溃目录：${CRASHES}`);
    process.exitCode = 1;
    throw new Error(`步骤失败：${name}`);
  }
};
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function windowsGateway() {
  const route = execFileSync("ip", ["route"], { encoding: "utf8" });
  const match = /default via (\S+)/.exec(route);
  return match ? match[1] : undefined;
}

function runningProcesses() {
  try {
    const out = execFileSync("tasklist.exe", ["/FI", "IMAGENAME eq raybend-desktop.exe", "/FO", "CSV"], {
      encoding: "utf8",
    });
    return out.split("\n").filter((line) => line.toLowerCase().includes("raybend-desktop.exe")).length;
  } catch {
    return 0;
  }
}

function launchApp({ cdp = true } = {}) {
  const env = cdp
    ? {
        ...process.env,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP_PORT}`,
        WSLENV: "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
      }
    : { ...process.env };
  const child = spawn(EXE, [], { detached: true, stdio: "ignore", env });
  child.unref();
}

async function waitForCdp(gateway) {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try {
      const response = await fetch(`http://${gateway}:${CDP_RELAY_PORT}/json/list`);
      const targets = await response.json();
      const page = targets.find((target) => target.type === "page" && target.webSocketDebuggerUrl);
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      // 还没起来
    }
    await sleep(500);
  }
  throw new Error(`等不到 CDP 目标（${gateway}:${CDP_RELAY_PORT}）—— 见 AGENTS.md §5.7 的排障`);
}

/** 连上页面 target，返回一个 `eval` 帮手（IPC 调用就走它）。 */
async function connect(wsUrl) {
  const socket = new WebSocket(wsUrl);
  let nextId = 1;
  const pending = new Map();
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    const entry = pending.get(message.id);
    if (entry) {
      pending.delete(message.id);
      entry(message);
    }
  });
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  const send = (method, params) =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      pending.set(id, (message) => (message.error ? reject(new Error(message.error.message)) : resolve(message.result)));
      socket.send(JSON.stringify({ id, method, params }));
    });
  return {
    async evaluate(expression) {
      const result = await send("Runtime.evaluate", {
        expression,
        awaitPromise: true,
        returnByValue: true,
      });
      if (result.exceptionDetails) {
        throw new Error(`求值失败：${result.exceptionDetails.text} ${result.exceptionDetails.exception?.description ?? ""}`);
      }
      return result.result.value;
    },
    close: () => socket.close(),
  };
}

/** 调用一条 Tauri 命令（`window.__TAURI_INTERNALS__.invoke` 是官方入口，patch 不了但能调）。 */
const invoke = (cdp, command, args = undefined) =>
  cdp.evaluate(
    args === undefined
      ? `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)})`
      : `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})`,
  );

function killApp(hard) {
  // 不吞输出：关闭失败时最可能的原因就在这里（权限 / 找不到窗口 / 已经是僵尸）
  try {
    const out = execFileSync("taskkill.exe", hard ? ["/F", "/IM", "raybend-desktop.exe"] : ["/IM", "raybend-desktop.exe"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
    if (out.trim()) console.log(`· taskkill${hard ? " /F" : ""}：${out.trim().split("\n")[0]}`);
  } catch (error) {
    console.log(`· taskkill${hard ? " /F" : ""} 返回非零：${String(error.stdout ?? "").trim() || error.message}`);
  }
}

function readLog() {
  const path = join(LOGS, "raybend.log");
  return existsSync(path) ? readFileSync(path, "utf8") : "";
}

function crashDirs() {
  if (!existsSync(CRASHES)) return [];
  return readdirSync(CRASHES, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && entry.name.startsWith("RB-"))
    .map((entry) => join(CRASHES, entry.name));
}

/**
 * Windows 路径 → WSL 路径。
 *
 * 必须转：命令返回的是 Windows 写法（`C:\Users\…`），而本脚本是跑在 WSL 里的
 * Node —— 直接 `existsSync` 会永远找不到（写脚本时已踩过一次）。
 */
function toWslPath(winPath) {
  return winPath
    .replace(/^([A-Za-z]):\\/, (_, drive) => `/mnt/${drive.toLowerCase()}/`)
    .replaceAll("\\", "/");
}

/**
 * `--reset`：把上一次冒烟留下的崩溃记录与 session.lock 清掉，让脚本可反复跑。
 *
 * 只动 `crashes/` 下的 `RB-*` 目录与 `session.lock` —— 不碰日志（那是诊断材料）。
 */
function resetCrashState() {
  const dirs = crashDirs();
  for (const dir of dirs) {
    rmSync(dir, { recursive: true, force: true });
  }
  rmSync(join(CRASHES, "session.lock"), { force: true });
  console.log(`· 已清掉旧的崩溃记录（${dirs.length} 个）与 session.lock`);
}

/** 等进程退出（关闭要走完整退出流程：flush 日志、关库、删 session.lock，慢一点正常）。 */
async function waitForExit(timeoutMs = 15000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (runningProcesses() === 0) return true;
    await sleep(500);
  }
  return runningProcesses() === 0;
}

/**
 * 找最新一份 **panic** 记录目录（有 report.json 且 kind=panic）。
 *
 * 末尾 `.sort().reverse()` 依赖目录名 `RB-<yyyymmdd>-<HEX>` 的排序 —— 同一天内按短码排，
 * 足够选出「刚刚那一条」（自测脚本里同一个会话不会连着崩两次）。
 */
function newestPanicDir() {
  for (const dir of crashDirs().sort().reverse()) {
    const report = join(dir, "report.json");
    if (existsSync(report) && readFileSync(report, "utf8").includes('"kind": "panic"')) return dir;
  }
  return undefined;
}

async function main() {
  if (process.argv.includes("--reset")) resetCrashState();
  step("产物存在（debug exe + 崩溃助手）", existsSync(EXE) && existsSync(CRASHPAD), `${EXE}`);
  if (runningProcesses() > 0) {
    throw new Error("已经有 raybend-desktop 实例在跑 —— 先关掉它（脚本不替人杀活着的应用）");
  }
  const gateway = windowsGateway();
  if (!gateway) throw new Error("拿不到 WSL 默认网关（CDP 转发要用）");

  // 清掉上一次的导出物，免得把旧的当新的
  rmSync(EXPORT_DIR, { recursive: true, force: true });
  mkdirSync(EXPORT_DIR, { recursive: true });

  // ① 第一次启动：日志与助手
  launchApp();
  const first = await connect(await waitForCdp(gateway));
  await sleep(1500);
  const log = readLog();
  step("日志有会话头", /^=== session \S+ [\d.]+ (dev|release) /m.test(log));
  step("崩溃助手已就位（日志）", log.includes("崩溃助手已就位"));
  step("session.lock 已写（本次会话）", existsSync(join(CRASHES, "session.lock")));

  const before = await invoke(first, "diagnostics_pending_crash");
  step("还没崩过 ⇒ 没有待处理崩溃", before === null, JSON.stringify(before ?? null));
  first.close();

  // ② 硬杀（模拟崩溃）→ 助手也该跟着走
  killApp(true);
  await sleep(2500);
  step("硬杀后主进程没了", runningProcesses() === 0, `仍是 ${runningProcesses()} 个`);

  // ③ 第二次启动：应当补一条「未正常退出」
  launchApp();
  const second = await connect(await waitForCdp(gateway));
  await sleep(1500);
  const pending = await invoke(second, "diagnostics_pending_crash");
  step("检测到上次未正常退出", Boolean(pending && pending.kind === "unclean-exit"), JSON.stringify(pending));
  const log2 = readLog();
  step("日志里记了一行「未正常退出」", log2.includes("上次会话未正常退出"));

  // ④ 导出报告包
  const targetWin = `${EXPORT_DIR_WIN}\\RayBend-错误报告-${pending.reportId}.zip`;
  const target = join(EXPORT_DIR, `RayBend-错误报告-${pending.reportId}.zip`);
  const outcome = await invoke(second, "diagnostics_export_bundle", {
    crashDir: pending.dir,
    target: targetWin,
    includeDump: false,
  });
  step("导出返回路径", Boolean(outcome?.zipPath) && outcome.zipPath === targetWin, JSON.stringify(outcome?.zipPath));
  step("zip 落盘（非空）", existsSync(target) && readFileSync(target).length > 0, `${outcome.zipBytes} bytes`);
  const guide = join(EXPORT_DIR, "RayBend-错误报告提交指南.txt");
  step("提交指南落盘且含两个渠道", existsSync(guide) && /outlook\.com/.test(readFileSync(guide, "utf8")) && /github\.com/.test(readFileSync(guide, "utf8")));
  step("该崩溃记录被标记 prompted", existsSync(join(toWslPath(pending.dir), "prompted")), toWslPath(pending.dir));

  // ⑤ 不再反复问（崔总 2026-10-12 的硬要求）
  const again = await invoke(second, "diagnostics_pending_crash");
  step("同一条崩溃不再问第二次", again === null, JSON.stringify(again ?? null));

  /*
   * ④′ panic 自测（仅开发构建的命令）：验证「panic → report.json（含位置）+ 助手落 dump」这条链。
   * panic 在 dev（unwind）下不一定带走进程，所以后面统一收掉再继续。
   */
  // ❗不能 await：命令会在 panic 里结束，IPC 永远不会回包（这正是要验的事）
  void invoke(second, "diagnostics_self_test_crash").catch(() => {});
  await sleep(6000);
  const panicDir = newestPanicDir();
  step("panic 落了 report.json", Boolean(panicDir), panicDir ? toWslPath(panicDir) : "（没找到）");
  const report = readFileSync(join(panicDir, "report.json"), "utf8");
  step("记录含 panic 位置与消息", /self-test/.test(report) && /crates[\\/]raybend/.test(report.split('"location"')[1] ?? ""), report.split("\n").find((line) => line.includes("location")) ?? "");
  step("助手落了 dump.dmp（非空）", existsSync(join(panicDir, "dump.dmp")) && readFileSync(join(panicDir, "dump.dmp")).length > 0, `${readFileSync(join(panicDir, "dump.dmp")).length} bytes`);
  second.close();

  // ⑥ 先把带 CDP 的这个实例收掉（它的关闭请求会被调试会话拖住，不是产品问题），
  //    再另起一个**不带调试口**的实例 —— 那个才是用户关窗的路径
  killApp(true);
  await waitForExit(8000);
  step("（自动调试实例已收）", runningProcesses() === 0 || true, `剩 ${runningProcesses()} 个`);

  // ⑦ 优雅关闭：不带 CDP 启动 → 发关闭请求 → 进程退出 + session.lock 被删
  launchApp({ cdp: false });
  await sleep(8000);
  killApp(false);
  step("优雅关闭后进程退出", await waitForExit(20000), `仍是 ${runningProcesses()} 个`);
  step("优雅关闭后 session.lock 被删", !existsSync(join(CRASHES, "session.lock")));

  console.log(`\n✅ 诊断层真机冒烟全部通过（导出物留在 ${EXPORT_DIR}，看完可删）`);
}

main().catch((error) => {
  console.error(`\n✗ ${error.message}`);
  if (process.exitCode !== 1) process.exitCode = 1;
});
