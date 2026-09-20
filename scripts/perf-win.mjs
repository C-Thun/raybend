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
 * ## 连接方式（WSL → Windows 宿主，NAT 下 `localhost` 不通 Windows 侧）
 *
 * exe 需要带着这两个参数起 WebView2（`--launch` 已代劳）：
 *
 * ```bash
 * WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9333 --remote-debugging-address=0.0.0.0'
 * ```
 *
 * `0.0.0.0` 是给 WSL 从 NAT 网关过来的（Chrome 系的调试口默认只听 127.0.0.1）。
 * 连不上的常见原因：① 另一个 RayBend 实例先起了 WebView2（浏览器进程是共享的，
 * 后起的实例带不上调试口 —— **先把别的窗口关掉**）；② Windows 防火墙拦了 WSL 子网
 * （可以用 `PERF_WIN_HOST` 指定别的地址试）。
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
const EXE = process.env.PERF_WIN_EXE ?? "/mnt/c/rb-target/raybend/debug/raybend-desktop.exe";
const LAUNCH = process.argv.includes("--launch");
/** 报告落点：默认崔总的临时目录（机器特定的测量物，不进仓库） */
const REPORT =
  process.env.PERF_WIN_REPORT ??
  join(
    "/mnt/c/src/tmp",
    `perf-win-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, "").slice(0, 15)}.json`,
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

/** 找到调试口在哪台地址上（每个候选 1.5s 探测） */
async function probeHost(host) {
  try {
    const response = await fetch(`http://${host}:${PORT}/json/version`, {
      signal: AbortSignal.timeout(1500),
    });
    return response.ok ? await response.json() : null;
  } catch {
    return null;
  }
}

/** `--launch`：从 WSL 拉起 Windows exe，经 WSLENV 把 CDP 参数带给 WebView2（§5.3 第 2 条） */
function launchExe() {
  if (!existsSync(EXE)) {
    throw new Error(`exe 不存在：${EXE}（先跑 pnpm debug:win，或用 PERF_WIN_EXE 指定路径）`);
  }
  const browserArguments = `--remote-debugging-port=${PORT} --remote-debugging-address=0.0.0.0`;
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
  if (LAUNCH) launchExe();

  let host = null;
  let version = null;
  const deadline = Date.now() + (LAUNCH ? 90_000 : 15_000);
  while (Date.now() < deadline && host === null) {
    for (const candidate of windowsHosts()) {
      const probed = await probeHost(candidate);
      if (probed) {
        host = candidate;
        version = probed;
        break;
      }
    }
    if (host === null) await sleep(1000);
  }
  if (host === null) {
    console.error(`✗ 没找到调试口（试过 ${windowsHosts().join("、")} 的 :${PORT}）`);
    console.error("  ① 先关掉所有已打开的 RayBend 窗口（WebView2 浏览器进程共享，后起的带不上调试口）");
    console.error("  ② 用 pnpm perf:win --launch 重跑（脚本会带 CDP 参数拉起 exe）");
    console.error("  ③ 还不行多半是 Windows 防火墙拦了 WSL：用 PERF_WIN_HOST=<宿主IP> 再试");
    process.exit(1);
  }
  console.log(`连上 ${host}:${PORT}（${version?.Browser ?? "?"}）`);

  const cdp = await connectCdp(PORT, { host, timeoutMs: 20_000 });
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
      port: PORT,
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
