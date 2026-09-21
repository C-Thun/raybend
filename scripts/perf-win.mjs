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
 * ## 连接方式（WSL → Windows 宿主：直连；需一次性 netsh 授权）
 *
 * exe 需要带调试参数起 WebView2（`--launch` 已代劳，经 WSLENV 透传）：
 *
 * ```bash
 * WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9333'
 * ```
 *
 * ⚠️ **Chromium（WebView2 153 实测）把调试口硬绑在 Windows 的 `127.0.0.1`**（headed
 * 模式下的安全限制，`--remote-debugging-address=0.0.0.0` 被无视）；WSL NAT 下
 * `localhost` 不通 Windows 侧，防火墙也拦 WSL 子网入站。所以需要**一次性**
 * 管理员授权（管理员 PowerShell 里跑，持久保存，之后永久直连）：
 *
 * ```powershell
 * netsh interface portproxy add v4tov4 listenaddress=0.0.0.0 listenport=9334 connectaddress=127.0.0.1 connectport=9333
 * netsh advfirewall firewall add rule name="RayBend CDP (WSL only)" dir=in action=allow protocol=TCP localport=9334 remoteip=172.16.0.0/12
 * ```
 *
 * （防火墙规则只放行 WSL 子网 172.16.0.0/12 的 9334 入站，不暴露给局域网。）
 * 脚本会依次试：`PERF_WIN_HOST` 指定地址 → 网关:9333 → 网关:9334（portproxy）
 * → 127.0.0.1（mirrored 网络）。连不上时会把上面两条命令再打印一遍。
 *
 * 另外的排查：`netstat` 里没有 `127.0.0.1:9333` → 调试口没开（多半是另一个
 * RayBend 实例先占了 WebView2 浏览器进程——先把别的窗口关掉再 `--launch`）。
 *
 * ## 库用哪个
 *
 * 60fps 问的是 GPU 合成，屏幕上就那几十个 tile，**用什么库都能量出帧率**——
 * 直接用您平时那个库就行。筛选计时那条在真机上只是端到端 sanity（无头 10 万条的
 * 硬判数字在 `perf:browse` + `query-bench`，已实测 < 100ms）。
 */

import { spawn } from "node:child_process";
import { execSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { realpathSync } from "node:fs";
import { dirname, join } from "node:path";

import { connectCdp, sleep } from "./lib/cdp.mjs";
import { SCROLL_PROBE, VIEWPORT_PROBE } from "./lib/perf-probes.mjs";

const PORT = Number(process.env.PERF_WIN_PORT ?? 9333);
/** 一次性 netsh portproxy 监听的端口（直连候选之一） */
const PROXY_PORT = Number(process.env.PERF_WIN_PROXY_PORT ?? 9334);
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
/** 直连候选（按优先级）：`PERF_WIN_HOST` → 网关:调试口 → 网关:portproxy → 本机（mirrored 网络） */
function directRoutes() {
  const routes = [];
  if (process.env.PERF_WIN_HOST) {
    routes.push({ host: process.env.PERF_WIN_HOST, port: PORT, via: "PERF_WIN_HOST 指定" });
  }
  try {
    const route = execSync("ip route show default", { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
    const via = /via\s+([0-9.]+)/.exec(route);
    if (via) {
      routes.push({ host: via[1], port: PORT, via: "网关直连" });
      routes.push({ host: via[1], port: PROXY_PORT, via: "portproxy（方案 A 的一次性 netsh）" });
    }
  } catch {
    /* 没有 ip 命令就算了 */
  }
  routes.push({ host: "127.0.0.1", port: PORT, via: "mirrored 网络" });
  return routes;
}

/** 一次性 netsh 授权（连不上时打印；人类在管理员 PowerShell 里跑一次，永久直连） */
function printOneTimeSetup() {
  console.error("  ① 在 Windows 上用**管理员** PowerShell 跑一次（持久保存，之后永久直连）：");
  console.error(
    `     netsh interface portproxy add v4tov4 listenaddress=0.0.0.0 listenport=${PROXY_PORT} connectaddress=127.0.0.1 connectport=${PORT}`,
  );
  console.error(
    `     netsh advfirewall firewall add rule name="RayBend CDP (WSL only)" dir=in action=allow protocol=TCP localport=${PROXY_PORT} remoteip=172.16.0.0/12`,
  );
  console.error("     （防火墙只放 WSL 子网 172.16.0.0/12 的入站，不暴露给局域网）");
  console.error("  ② 跑完重试 pnpm perf:win；若 netstat 里连 127.0.0.1:9333 都没有 → 先关掉所有 RayBend 窗口再 --launch");
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

/** `--launch`：从 WSL 拉起 Windows exe，经 WSLENV 把 CDP 参数带给 WebView2（§5.3 第 2 条） */
function launchExe() {
  if (!existsSync(EXE)) {
    throw new Error(`exe 不存在：${EXE}（先跑 pnpm debug:win，或用 PERF_WIN_EXE 指定路径）`);
  }
  /* 只带端口参数：0.0.0.0 会被 Chromium 153 无视（调试口硬绑 127.0.0.1），跨机靠一次性 netsh portproxy */
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

  /* 直连候选轮询：PERF_WIN_HOST → 网关:9333 → 网关:9334（portproxy）→ 127.0.0.1（mirrored） */
  let route = null;
  const deadline = Date.now() + (LAUNCH && !already ? 90_000 : 20_000);
  while (Date.now() < deadline && route === null) {
    for (const candidate of directRoutes()) {
      try {
        const response = await fetch(`http://${candidate.host}:${candidate.port}/json/version`, {
          signal: AbortSignal.timeout(2000),
        });
        if (response.ok) {
          route = { ...candidate, version: await response.json() };
          break;
        }
      } catch {
        /* 这个候选不通，试下一个 */
      }
    }
    if (route === null) await sleep(1000);
  }
  if (route === null) {
    if (!windowsCdpListening()) {
      console.error(`✗ Windows 侧没有调试口在听（:${PORT}）`);
      console.error("  先关掉所有已打开的 RayBend 窗口（WebView2 浏览器进程共享，后起的带不上调试参数），");
      console.error("  再用 pnpm perf:win --launch 重跑（脚本会带 CDP 参数拉起 exe）");
    } else {
      console.error(`✗ Windows 调试口在听（127.0.0.1:${PORT}）但 WSL 直连都不通 —— 缺一次性 netsh 授权：`);
      printOneTimeSetup();
    }
    process.exit(1);
  }
  console.log(`连上 ${route.host}:${route.port}（${route.version?.Browser ?? "?"}，${route.via}）`);

  const cdp = await connectCdp(route.port, { host: route.host, timeoutMs: 20_000 });
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
    /* DoD 的 2560×1440 是**物理分辨率**口径：CSS 尺寸 × DPR 才是真正的光栅面积
     * （例：125%×110% 屏上全屏窗口 CSS 约 1862×1048、dpr 1.375 → 物理 2560×1441，已达标） */
    const physical = [
      Math.round(viewport.inner[0] * viewport.dpr),
      Math.round(viewport.inner[1] * viewport.dpr),
    ];
    viewport.physical = physical;
    if (physical[0] < 2500 || physical[1] < 1400) {
      notes.push(
        `视口 CSS ${viewport.inner[0]}×${viewport.inner[1]}（dpr ${viewport.dpr}）= 物理 ${physical[0]}×${physical[1]}，` +
          "不到 DoD 的 2560×1440 —— 把窗口拉到那台屏上最大化（或 F11）后重跑，帧率数字才算数",
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
      host: route.host,
      port: route.port,
      via: route.via,
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
    console.log(
      `视口           ${viewport.inner[0]}×${viewport.inner[1]}  dpr ${viewport.dpr}  物理 ${physical[0]}×${physical[1]}`,
    );
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
