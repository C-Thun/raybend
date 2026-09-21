#!/usr/bin/env node
/**
 * 浏览模式**真机性能采样**（`plans/M2-W3.md` 步骤 6.4 —— M2 收口的最后一项编码活）。
 *
 * ```bash
 * pnpm perf:win --launch     # 脚本自己拉起 Windows exe（带 CDP 参数），测完窗口留着，手动关
 * pnpm perf:win              # exe 已经开着（带 CDP 参数）就直连复测
 * ```
 *
 * ## 与 `perf:browse` 的分工（数字才可比）
 *
 * | | `perf:browse`（无头） | `perf:win`（本脚本，真机） |
 * | --- | --- | --- |
 * | 渲染 | 软件渲染（`--disable-gpu`） | **真 GPU 合成** —— 60fps 的裁决在这里 |
 * | 后端 | 假后端（10 万条合成） | **真 Tauri IPC + SQLite**（用什么库就量什么库） |
 * | 滚动采样 | 同一份探针（`lib/perf-probes.mjs`） | 同一份探针 |
 * | 判据 | 筛选数据路径 < 100ms 硬判 | **只上报**，60fps 归人类目视（`AGENTS.md` §2.8） |
 *
 * ## 它量什么
 *
 * 1. **滚动帧间隔**：进浏览 → 点开目录树 → 程序化滚动 3 秒采 `requestAnimationFrame` 间隔；
 * 2. **一次筛选计时**：开筛选 → 点「3 星」→ 首屏 tile 变化（真机端到端：IPC + 查询 + 绘制）；
 * 3. **JS 堆 / DOM 节点 / 视口 / DPR**（`AGENTS.md` §7.9：环境事实必须随报告带上）。
 *
 * ## 连接方式（WSL → Windows 宿主：直连优先，不通则自动反向隧道）
 *
 * exe 需要带调试参数起 WebView2（`--launch` 已代劳，经 WSLENV 透传）：
 *
 * ```bash
 * WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9333'
 * ```
 *
 * ⚠️ **Chromium（WebView2 153 实测）把调试口硬绑在 Windows 的 `127.0.0.1`**——
 * `--remote-debugging-address=0.0.0.0` 会被无视（headed 模式下的安全限制），
 * 而 WSL NAT 下 `localhost` 不通 Windows 侧、防火墙又拦 WSL 子网的入站。
 * 所以脚本走**反向隧道**：WSL 起中继口（`127.0.0.1:9334`），Windows 侧 PowerShell
 * **拨出**到 WSL（出站不受防火墙限制）并同时拨本机 `127.0.0.1:9333`，两头对拷。
 * 整条链路零管理员权限、不改防火墙。
 *
 * 连不上的排查：① `netstat` 里没有 `127.0.0.1:9333` → 调试口没开（多半是另一个
 * RayBend 实例先占了 WebView2 浏览器进程——先把别的窗口关掉再 `--launch`）；
 * ② Windows 拨不到 WSL（罕见）→ 用 `PERF_WIN_RELAY_IP` 手工指定 WSL IP。
 *
 * ## 库用哪个
 *
 * 60fps 问的是 GPU 合成，屏幕上就那几十个 tile，**用什么库都能量出帧率**——
 * 直接用您平时那个库就行。筛选计时那条在真机上只是端到端 sanity（无头 10 万条的
 * 硬判数字在 `perf:browse` + `query-bench`，已实测 < 100ms）。
 */

import { spawn } from "node:child_process";
import { execSync } from "node:child_process";
import net from "node:net";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { realpathSync } from "node:fs";
import { dirname, join } from "node:path";

import { connectCdp, sleep } from "./lib/cdp.mjs";
import { SCROLL_PROBE, VIEWPORT_PROBE } from "./lib/perf-probes.mjs";

const PORT = Number(process.env.PERF_WIN_PORT ?? 9333);
/** 反向隧道在本机（WSL）这一侧的中继口 */
const RELAY_PORT = Number(process.env.PERF_WIN_RELAY_PORT ?? 9334);
const EXE = process.env.PERF_WIN_EXE ?? "/mnt/c/rb-target/raybend/debug/raybend-desktop.exe";
const LAUNCH = process.argv.includes("--launch");
/** 报告落点：默认崔总的临时目录（机器特定的测量物，不进仓库） */
const REPORT =
  process.env.PERF_WIN_REPORT ??
  join(
    "/mnt/c/src/tmp",
    `perf-win-${new Date().toISOString().slice(0, 19).replace(/[-:T]/g, "")}.json`,
  );

/** 两套语言包里这几个控件的 aria-label（真机界面语言不确定，两份都认） */
export const LABELS = {
  filterHint: ["开启后，后面的标记都变成筛选条件", "When on, the marks to the right become filter conditions"],
  stars3: ["3 星", "3 stars"],
  expand: ["展开", "Expand"],
};

/*
 * 页内探针：选目录 + 筛选计时。必须自洽（见 perf-probes.mjs 的说明），
 * 所以 LABELS 用 JSON 注入而不是闭包。导出给验证脚本用（对着同一份假后端真跑一遍）。
 */
export const PICK_DIR_PROBE = `
(async (labels) => {
  const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const visible = (node) => Boolean(node.offsetParent);
  const tilesNow = () => document.querySelectorAll('main [data-virtual-scroller] [role="option"]').length;

  /* 展开目录树的前几层（导入模板是 photos/日期/，展开根那一层就够见到日期行；
   * 每轮限量、总量封顶，别把大树整个摊开——那是在测页面造一万行，不是测滚动） */
  for (let round = 0; round < 4; round += 1) {
    const collapsed = [...document.querySelectorAll("aside button[aria-label]")]
      .filter((b) => labels.expand.includes(b.getAttribute("aria-label")) && visible(b))
      .slice(0, 40);
    if (collapsed.length === 0) break;
    for (const b of collapsed) b.click();
    await pause(600);
  }

  /* 候选 = 目录名按钮（title 是库内相对路径，photos/ 开头），深的优先（日期级比库根有料） */
  const candidates = [...document.querySelectorAll('aside button[title^="photos"]')]
    .filter(visible)
    .sort((a, b) => b.title.length - a.title.length)
    .slice(0, 12);
  for (const row of candidates) {
    row.click();
    for (let wait = 0; wait < 14; wait += 1) {
      await pause(200);
      if (tilesNow() > 0) return { ok: true, scope: row.title };
    }
  }
  return { error: "目录树里没点出任何有照片的目录", candidates: candidates.map((c) => c.title).slice(0, 8) };
})`;

export const FILTER_TIMING_PROBE = `
(async (labels) => {
  const frame = () => new Promise((resolve) => requestAnimationFrame(() => resolve()));
  const state = () => ({
    count: document.querySelectorAll('main [data-virtual-scroller] [role="option"]').length,
    first: document.querySelector('main [data-virtual-scroller] [role="option"]')?.textContent?.slice(0, 20) ?? null,
    filterText: document.querySelector("[data-filter-count]")?.textContent?.trim() ?? null,
  });
  const buttonByLabel = (candidates) =>
    [...document.querySelectorAll("button[aria-label]")].find(
      (b) => candidates.includes(b.getAttribute("aria-label")) && b.offsetParent,
    );
  const before = state();
  const filterToggle = buttonByLabel(labels.filterHint);
  if (!filterToggle) return { error: "找不到筛选开关" };
  if (filterToggle.getAttribute("aria-pressed") !== "true") {
    filterToggle.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await frame();
    await frame();
  }
  const star = buttonByLabel(labels.stars3);
  if (!star) return { error: "找不到 3 星按钮" };
  window.__PERF_WIN_FILTERED = false;
  const clickAt = performance.now();
  star.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  let changedAt = null;
  for (let i = 0; i < 300 && changedAt === null; i += 1) {
    await frame();
    const after = state();
    if (
      after.count !== before.count ||
      after.first !== before.first ||
      (after.filterText !== null && after.filterText !== before.filterText)
    ) {
      changedAt = Math.round((performance.now() - clickAt) * 10) / 10;
    }
  }
  return {
    perceptualMs: changedAt,
    before: { tiles: before.count, first: before.first },
    after: state(),
    emptyResult: state().count === 0,
  };
})`;

/** WSL NAT 下 Windows 宿主的地址：默认网关（`ip route show default` 的 via 那个） */
function windowsHosts() {
  const hosts = [];
  if (process.env.PERF_WIN_HOST) hosts.push(process.env.PERF_WIN_HOST);
  hosts.push("127.0.0.1"); // mirrored 网络模式 / Windows 侧自己跑
  try {
    const route = execSync("ip route show default", { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
    const via = /via\s+([0-9.]+)/.exec(route);
    if (via) hosts.push(via[1]);
  } catch {
    /* 没有 ip 命令就算了 */
  }
  return [...new Set(hosts)];
}

/** 本机（WSL）的 eth0 地址 —— Windows 侧拨出过来用的目标 */
function wslIp() {
  if (process.env.PERF_WIN_RELAY_IP) return process.env.PERF_WIN_RELAY_IP;
  try {
    const out = execSync("ip -4 -o addr show scope global", {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    const lines = out.split("\n").filter((line) => /inet\s+([0-9.]+)/.test(line));
    const eth = lines.find((line) => /eth/.test(line)) ?? lines[0];
    return eth ? (/inet\s+([0-9.]+)/.exec(eth))[1] : null;
  } catch {
    return null;
  }
}

/** Windows 侧 netstat：调试口是否已在 Windows 本机监听（只绑 127.0.0.1 也算）。
 *  管道别经 WSL 的 sh（findstr 不在 WSL 的 PATH 上，exec 会 127）——整个拿回来 JS 里滤。 */
function windowsCdpListening() {
  try {
    const out = execSync("cmd.exe /c netstat -ano", {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
      timeout: 20_000,
      maxBuffer: 16 * 1024 * 1024,
    });
    return out
      .split("\n")
      .some((line) => line.includes(":" + PORT) && line.includes("LISTENING"));
  } catch {
    return false;
  }
}

/**
 * 反向隧道：本机（WSL）起中继口；每个连进来的连接，临时起一个随机口，让 Windows 侧
 * PowerShell **拨出**（Windows→WSL 不经防火墙）并同拨 Windows 本机的调试口，两头对拷。
 * CDP 的 HTTP 与 WebSocket 都走这条链，客户端（`connectCdp`）完全无感。
 */
function startReverseRelay(wslAddr) {
  const children = new Set();
  const dbg = (...args) => {
    if (process.env.PERF_WIN_DEBUG) console.error("[relay]", ...args);
  };
  const server = net.createServer((sock) => {
    dbg("front 连入");
    let done = false;
    let child = null;
    const back = net.createServer();
    let dialTimer = null;
    const cleanup = () => {
      if (done) return;
      done = true;
      if (dialTimer !== null) clearTimeout(dialTimer);
      sock.destroy();
      back.close();
      if (child) {
        children.delete(child);
        try {
          child.kill();
        } catch {
          /* 已经退了 */
        }
      }
    };
    sock.on("error", cleanup);
    let bridged = false;
    back.on("connection", (bridge) => {
      bridged = true;
      dbg("Windows 已拨入，桥接");
      bridge.on("error", (error) => dbg("bridge 错误", error.code));
      sock.pipe(bridge);
      bridge.pipe(sock);
      sock.on("close", cleanup);
      bridge.on("close", cleanup);
    });
    back.on("listening", () => {
      const backPort = back.address().port;
      const ps =
        `$ErrorActionPreference='Stop';try{` +
        `$a=[System.Net.Sockets.TcpClient]::new('127.0.0.1',${PORT});` +
        `$b=[System.Net.Sockets.TcpClient]::new('${wslAddr}',${backPort});` +
        `$as=$a.GetStream();$bs=$b.GetStream();` +
        `$t1=$as.CopyToAsync($bs);$t2=$bs.CopyToAsync($as);` +
        `[void][System.Threading.Tasks.Task]::WaitAny(@($t1,$t2));` +
        `$a.Close();$b.Close()}catch{}`;
      const encoded = Buffer.from(ps, "utf16le").toString("base64");
      child = spawn("powershell.exe", ["-NoProfile", "-EncodedCommand", encoded], { stdio: "ignore" });
      children.add(child);
      child.on("close", (code) => dbg("powershell 退出 code=", code));
      child.on("close", cleanup);
    });
    back.on("error", cleanup);
    back.listen(0, "0.0.0.0");
    // 「Windows 拨号 8 秒不到就放弃」——拨到了就不动它，别把正在用的连接拆了
    dialTimer = setTimeout(() => {
      if (!bridged) {
        dbg("8 秒没等到 Windows 拨入，放弃");
        cleanup();
      }
    }, 8_000);
  });
  return {
    server,
    children,
    listen: () =>
      new Promise((resolve, reject) => {
        server.once("error", reject);
        server.listen(RELAY_PORT, "127.0.0.1", () => resolve());
      }),
    stop: () => {
      server.close();
      for (const child of children) {
        try {
          child.kill();
        } catch {
          /* 已退 */
        }
      }
    },
  };
}

/** `--launch`：从 WSL 拉起 Windows exe，经 WSLENV 把 CDP 参数带给 WebView2（§5.3 第 2 条） */
function launchExe() {
  if (!existsSync(EXE)) {
    throw new Error(`exe 不存在：${EXE}（先跑 pnpm debug:win，或用 PERF_WIN_EXE 指定路径）`);
  }
  /* 只带端口参数：0.0.0.0 会被 Chromium 153 无视（调试口硬绑 127.0.0.1），跨机靠反向隧道 */
  const browserArguments = `--remote-debugging-port=${PORT}`;
  const child = spawn(EXE, [], {
    detached: true,
    stdio: "ignore",
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: browserArguments,
      // WSL→Windows 只转发 WSLENV 里列出的变量（血的教训，AGENTS.md §7.9）
      WSLENV: [...(process.env.WSLENV ?? "").split(":"), "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"]
        .filter(Boolean)
        .join(":"),
    },
  });
  child.unref();
  console.log(`已拉起 ${EXE}（CDP ${PORT}）—— 等它起来…`);
}

async function main() {
  /* `--launch` 前先看调试口是不是已经开着（exe 可能已在跑，别重复拉起第二个实例） */
  const already = windowsCdpListening();
  if (LAUNCH && !already) launchExe();
  else if (LAUNCH) console.log(`调试口已在 Windows 侧监听（:9333）—— 复用现有实例，不重复拉起`);

  /* 路线 1：直连（mirrored 网络 / PERF_WIN_HOST 指定了可达地址时成立） */
  let host = null;
  let version = null;
  const directHosts = windowsHosts();
  for (const candidate of directHosts) {
    try {
      const response = await fetch(`http://${candidate}:${PORT}/json/version`, {
        signal: AbortSignal.timeout(1500),
      });
      if (response.ok) {
        host = candidate;
        version = await response.json();
        break;
      }
    } catch {
      /* 这个候选不通，试下一个 */
    }
  }

  /* 路线 2：反向隧道（NAT + 防火墙下的常规路线；Windows 侧 PowerShell 拨出，零权限） */
  let relay = null;
  if (host === null) {
    if (!windowsCdpListening()) {
      console.error(`✗ Windows 侧没有调试口在听（:${PORT}）`);
      console.error("  ① 先关掉所有已打开的 RayBend 窗口（WebView2 浏览器进程共享，后起的带不上调试参数）");
      console.error("  ② 再用 pnpm perf:win --launch 重跑（脚本会带 CDP 参数拉起 exe）");
      process.exit(1);
    }
    const addr = wslIp();
    if (addr === null) {
      console.error("✗ Windows 侧调试口只绑了 127.0.0.1，且本机拿不到 WSL IP（无法建反向隧道）");
      console.error("  用 PERF_WIN_RELAY_IP=<WSL的eth0 IP> 手工指定后重跑");
      process.exit(1);
    }
    relay = startReverseRelay(addr);
    await relay.listen();
    host = "127.0.0.1";
  }

  /* 等调试口就绪（launch 后 app 起来要几秒；经隧道探测） */
  const probePort = host === "127.0.0.1" && relay !== null ? RELAY_PORT : PORT;
  const deadline = Date.now() + (LAUNCH && !already ? 90_000 : 20_000);
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`http://${host}:${probePort}/json/version`, {
        signal: AbortSignal.timeout(3000),
      });
      if (response.ok) {
        version = await response.json();
        break;
      }
    } catch {
      /* 还没起来 */
    }
    await sleep(1000);
  }
  if (version === null) {
    relay?.stop();
    console.error(`✗ 调试口等了 ${(LAUNCH ? 90 : 20)} 秒也没就绪（${host}:${probePort}）`);
    process.exit(1);
  }
  const via = relay !== null ? "反向隧道（Windows 侧调试口只绑了 127.0.0.1）" : "直连";
  console.log(`连上 ${host}:${probePort}（${version?.Browser ?? "?"}，${via}）`);

  const cdp = await connectCdp(probePort, { host, timeoutMs: 20_000 });
  try {
    /* 等应用真的挂上（标题栏/正文出现；生产包是 http://tauri.localhost，资产是嵌入的，快） */
    let booted = false;
    for (let attempt = 0; attempt < 60 && !booted; attempt += 1) {
      await sleep(500);
      const probe = await cdp.send("Runtime.evaluate", {
        expression: `Boolean(document.querySelector("aside, header"))`,
        returnByValue: true,
      });
      booted = probe.result?.value === true;
    }
    if (!booted) throw new Error("页面 20 秒没挂上（连上的可能不是 RayBend？）");

    const viewport = (await cdp.send("Runtime.evaluate", {
      expression: VIEWPORT_PROBE,
      returnByValue: true,
    })).result?.value;
    const notes = [];
    if (!`${viewport.inner[0]}x${viewport.inner[1]}`.startsWith("2560x1440")) {
      notes.push(
        `视口是 ${viewport.inner[0]}×${viewport.inner[1]}，不是 DoD 的 2560×1440 —— ` +
          "把窗口拉到那台屏上最大化（或 F11）后重跑，帧率数字才算数",
      );
    }

    /* 切到浏览（命令体系自己的键位 Ctrl+2） */
    for (const type of ["keyDown", "keyUp"]) {
      await cdp.send("Input.dispatchKeyEvent", {
        type,
        key: "2",
        code: "Digit2",
        windowsVirtualKeyCode: 50,
        modifiers: 2,
        text: "",
      });
    }
    await sleep(1200);

    /* 挑目录：展开树 → 逐个候选点、等到网格真有照片 */
    const picked = (await cdp.send("Runtime.evaluate", {
      expression: `${PICK_DIR_PROBE}(${JSON.stringify(LABELS)})`,
      awaitPromise: true,
      returnByValue: true,
    })).result?.value;
    if (picked?.ok !== true) {
      throw new Error(`没选到目录：${JSON.stringify(picked)}；控制台 ${JSON.stringify(cdp.consoleErrors.slice(0, 3))}`);
    }
    console.log(`目录：${picked.scope}`);
    await sleep(2500); // 等缩略图那波进来，别把首屏加载混进滚动采样

    /* 指标 1：滚动帧间隔（与 perf:browse 同一份探针 —— 无头量 JS 预算，这里量 GPU 合成） */
    const scroll = (await cdp.send("Runtime.evaluate", {
      expression: `${SCROLL_PROBE}(3000)`,
      awaitPromise: true,
      returnByValue: true,
    })).result?.value;
    if (scroll?.error) throw new Error(scroll.error);

    /* 指标 2：一次筛选的端到端计时（真 IPC + SQLite + 绘制） */
    const filter = (await cdp.send("Runtime.evaluate", {
      expression: `${FILTER_TIMING_PROBE}(${JSON.stringify(LABELS)})`,
      awaitPromise: true,
      returnByValue: true,
    })).result?.value;
    if (filter?.error) {
      notes.push(`筛选没量到：${filter.error}`);
    } else if (filter.emptyResult) {
      notes.push("筛选结果是空的（这个目录没有 3 星照片）—— 计时是「清空首屏」那一下，仍有效");
    }

    /* 指标 3：内存 + 环境事实 */
    const heap = await cdp.send("Runtime.getHeapUsage", {});
    const metrics = await cdp.send("Performance.getMetrics", {});
    const metricOf = (name) => metrics.metrics?.find((metric) => metric.name === name)?.value ?? null;
    const memory = {
      jsHeapMb: heap.usedSize === undefined ? null : Math.round(heap.usedSize / 1024 / 1024),
      domNodes: metricOf("Nodes"),
      jsListeners: metricOf("JSEventListeners"),
    };

    const report = {
      when: new Date().toISOString(),
      machine: "Windows 真机（人类跑）",
      host,
      port: probePort,
      via,
      viewport,
      scope: picked.scope,
      scroll,
      filter,
      memory,
      notes,
      consoleErrors: cdp.consoleErrors.slice(0, 3),
    };
    mkdirSync(dirname(REPORT), { recursive: true });
    writeFileSync(REPORT, `${JSON.stringify(report, null, 2)}\n`);

    const fps = (ms) => (ms > 0 ? Math.round(1000 / ms) : null);
    console.log("\n浏览真机采样（只上报；60fps 的裁决归人类目视，AGENTS.md §2.8）");
    console.log(`视口           ${viewport.inner[0]}×${viewport.inner[1]}  dpr ${viewport.dpr}`);
    console.log(`目录           ${picked.scope}`);
    console.log(`滚动帧 p50/p95 ${scroll.p50} / ${scroll.p95} ms   最差 ${scroll.worst} ms`);
    console.log(`               ≈ ${fps(scroll.p50)} fps（p50）/ ${fps(scroll.p95)} fps（p95），${scroll.frames} 帧`);
    if (filter?.perceptualMs != null) {
      console.log(`筛选端到端     ${filter.perceptualMs} ms（点 3 星 → 首屏变化；无头 10 万条的硬判在 perf:browse）`);
    }
    console.log(`JS 堆          ${memory.jsHeapMb ?? "?"} MB   DOM 节点 ${memory.domNodes ?? "?"}`);
    for (const note of notes) console.log(`⚠ ${note}`);
    if (report.consoleErrors.length > 0) console.log(`控制台错误     ${JSON.stringify(report.consoleErrors)}`);
    console.log(`报告           ${REPORT}`);
    console.log("\n✓ 采样完成 —— 窗口可以关了；60fps 与体感结论由人类填写（plans/M2-W3.md §5）");
  } finally {
    cdp.close();
    relay?.stop();
  }
}

// node --check 按 CommonJS 解析，顶层不能有裸 await —— 全部收进 main()。
/* 只在直接跑（node scripts/perf-win.mjs）时执行；被 import 时只提供探针 */
const realArgv1 = (() => {
  try {
    return process.argv[1] ? realpathSync(process.argv[1]) : null;
  } catch {
    return null;
  }
})();
const realSelf = (() => {
  try {
    return realpathSync(new URL(import.meta.url).pathname);
  } catch {
    return null;
  }
})();
if (realArgv1 !== null && realArgv1 === realSelf) {
  main().catch((error) => {
    console.error(`✗ ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  });
}
