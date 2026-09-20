#!/usr/bin/env node
/**
 * 浏览模式**前端性能基准**（`plans/M2-W3.md` 阶段 6 / 步骤 6.2）。
 *
 * ```bash
 * pnpm dev                 # 另开一个终端
 * pnpm perf:browse         # 默认 10 万条
 * pnpm perf:browse 300000
 * ```
 *
 * ## 量的是什么（以及**不**量什么）
 *
 * 假后端给 **10 万条**合成照片，然后量**前端那一段**：
 *
 * | 指标 | 判据 |
 * | --- | --- |
 * | **筛选感知延迟**：点下筛选条件 → 首屏 tile 画出来 | < 100ms（`PLAN.md` 的 DoD） |
 * | **滚动 JS 帧预算**：程序化滚动 3 秒，采 `requestAnimationFrame` 间隔 | p95 < 16.7ms |
 * | **JS 堆 / DOM 节点** | 只报数（内存的体感归人类） |
 *
 * ⚠️ **无头 Chrome 是软件渲染**：这里的帧数字只证明「前端脚本不拖后腿」，
 * **60fps@2560×1440 的裁决在 Windows 真机**（`AGENTS.md` §2.8）。
 * 数据库那一段（SQLite + IPC）由 `cargo run --release -p raybend --example query-bench` 覆盖。
 *
 * ## 假后端
 *
 * 页是**按需生成**的（10 万个对象一次性建出来会把测量对象自己拖垮）——
 * 与 `check-browse-boot.mjs` 的固定 fixture 不同，这里要的是「量级真实」。
 */

import { writeFileSync } from "node:fs";
import { realpathSync } from "node:fs";

import { connectCdp, launchChrome, requireServer, sleep } from "./lib/cdp.mjs";
import { SCROLL_PROBE } from "./lib/perf-probes.mjs";

const PORT = Number(process.env.PERF_PORT ?? 9511);
const APP = process.env.APP_URL ?? "http://localhost:1420/";
const ROWS = Number(process.argv[2] ?? 100_000);
const REPORT = process.env.PERF_REPORT ?? "";

/** 1×1 透明 PNG（缩略图/大图都拿它当字节 —— 只看链路通不通，不看画质） */
const ONE_PIXEL_PNG =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

const REPOSITORY = {
  id: "perf0000000000",
  name: "性能库",
  importTemplate: ":CYEAR-:CMONTH-:CDAY/MY:FILENAME",
  createdAt: 1_700_000_000_000,
  lastOpenedAt: null,
  online: true,
  root: "C:\\Photos\\perf",
  displayPath: "C:\\Photos\\perf",
  paths: [
    {
      path: "C:\\Photos\\perf",
      pathFolded: "c:/photos/perf",
      addedAt: 1_700_000_000_000,
      lastSeenAt: 1_700_000_000_000,
      status: "unknown",
    },
  ],
  photoCount: ROWS,
  triesPathsPlaceholder: undefined,
  triedPaths: 0,
};

const T0 = 1_789_000_000_000;

/** 第 `index` 张（0 基）：字段形状与 `AssetItem` 对齐 */
function itemAt(index) {
  /*
   * ⚠️ 这个函数会被 `toString()` **内联进页面**，所以它必须自洽 ——
   * 不能引用模块作用域的任何东西（`T0` 一度就是漏网的：页面里报 "T0 is not defined"）。
   */
  const T0 = 1_789_000_000_000;
  const id = index + 1;
  const day = String(1 + (id % 28)).padStart(2, "0");
  const month = String(1 + (id % 12)).padStart(2, "0");
  return {
    id,
    relPath: `photos/2026-${month}-${day}/PERF${String(id).padStart(6, "0")}.JPG`,
    fileName: `PERF${String(id).padStart(6, "0")}.JPG`,
    ext: "JPG",
    isRaw: false,
    hasRaw: false,
    author: null,
    description: null,
    gpsLat: null,
    gpsLon: null,
    country: null,
    provinceState: null,
    city: null,
    sublocation: null,
    createdMs: T0 + index * 60_000,
    takenAt: T0 + index * 60_000,
    takenAtOffsetMin: null,
    rating: id % 11 === 0 ? 5 : id % 5 === 0 ? 3 : 0,
    colorLabel: id % 9 === 0 ? "red" : null,
    likeState: id % 13 === 0 ? "like" : null,
    lockLevel: 0,
    cameraMake: "Panasonic",
    cameraModel: "DC-G9",
    lens: null,
    focalMm: null,
    fNumber: null,
    exposureMs: null,
    iso: null,
    width: 4000,
    height: 3000,
    orientation: 1,
    sizeBytes: 3_000_000,
    missing: false,
  };
}

/* 导出给探针验证脚本用（perf-win 的页内探针要对着同一份假后端跑一遍才算验证过） */
export const MOCK = `
  window.__PERF = { pageCalls: 0, thumbCalls: 0, calls: [], matchCache: {} };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    transformCallback: (cb) => cb,
    convertFileSrc: (p) => p,
    invoke: (cmd, args) => {
      window.__PERF.calls.push(cmd);
      const at = ${"(" + itemAt.toString() + ")"};

      /* ── 筛选：整数运算判匹配，结果按筛选条件缓存（页面里也要真的筛） ── */
      const ratingOf = (id) => (id % 11 === 0 ? 5 : id % 5 === 0 ? 3 : 0);
      const colorOf = (id) => (id % 9 === 0 ? "red" : null);
      const likeOf = (id) => (id % 13 === 0 ? "like" : null);
      const matches = (id, filter) => {
        if (!filter) return true;
        if (typeof filter.minRating === "number" && ratingOf(id) < filter.minRating) return false;
        if (Array.isArray(filter.colors) && filter.colors.length > 0 && !filter.colors.includes(colorOf(id) || "none")) return false;
        if (Array.isArray(filter.likes) && filter.likes.length > 0 && !filter.likes.includes(likeOf(id) || "none")) return false;
        if (Array.isArray(filter.locks) && filter.locks.length > 0 && !filter.locks.includes(0)) return false;
        return true;
      };
      const matchIds = (filter) => {
        const key = JSON.stringify(filter || {});
        if (window.__PERF.matchCache[key]) return window.__PERF.matchCache[key];
        const startedAt = performance.now();
        const ids = [];
        for (let id = 1; id <= ${ROWS}; id += 1) if (matches(id, filter)) ids.push(id);
        window.__PERF.lastQueryMs = Math.round((performance.now() - startedAt) * 100) / 100;
        window.__PERF.matchCache[key] = ids;
        return ids;
      };
      const filterOf = (args) => (args && args.query && args.query.filter) || (args && args.filter) || {};

      if (cmd === "thumb_get" || cmd === "view_image") {
        window.__PERF.thumbCalls += 1;
        const raw = atob(${JSON.stringify(ONE_PIXEL_PNG)});
        const bytes = new Uint8Array(raw.length);
        for (let i = 0; i < raw.length; i += 1) bytes[i] = raw.charCodeAt(i);
        return Promise.resolve(bytes.buffer);
      }
      if (cmd === "image_histogram") {
        return Promise.resolve({ bins: 4, r: [1, 2, 3, 4], g: [1, 2, 3, 4], b: [1, 2, 3, 4], max: 4 });
      }
      if (cmd === "repositories_list") return Promise.resolve([${JSON.stringify(REPOSITORY)}]);
      if (cmd === "recent_dirs_list") {
        return Promise.resolve([
          { path: "C:\\\\Photos\\\\perf", includeSubdirs: true, usedAt: ${T0}, useCount: 3 },
        ]);
      }
      if (cmd === "volumes_list") {
        return Promise.resolve([
          {
            path: "C:\\\\",
            name: "系统盘",
            kind: "fixed",
            totalBytes: 512000000000,
            freeBytes: 128000000000,
          },
        ]);
      }
      if (cmd === "dir_meta_ensure") return Promise.resolve([]);
      if (cmd === "dir_list") {
        const path = String((args && args.path) || "").replace(/\\\\/g, "/");
        if (path === "C:/Photos/perf/photos") {
          return Promise.resolve([{ name: "2026-08-15", path: "C:/Photos/perf/photos/2026-08-15" }]);
        }
        return Promise.resolve([]);
      }
      if (cmd === "source_paths_status") return Promise.resolve([true]);
      if (cmd === "browse_page") {
        window.__PERF.pageCalls += 1;
        const offset = Number((args && args.offset) || 0);
        const limit = Number((args && args.limit) || 200);
        const ids = matchIds(filterOf(args));
        const page = ids.slice(offset, offset + limit);
        window.__PERF.lastResponse = { total: ids.length, offset, returned: page.length };
        window.__PERF.lastPageAt = performance.now();
        return Promise.resolve({ total: ids.length, offset, items: page.map((id) => at(id - 1)) });
      }
      /*
       * 时间线 = **显示序**（浏览侧的网格顺序由它决定，不是由 page 决定）——
       * 假后端也必须给全，否则网格一行都不渲染（2026-09-20 踩过：光有 page 不够）。
       */
      if (cmd === "browse_timeline") {
        const ids = matchIds(filterOf(args));
        const entries = ids.map((id) => ({
          id,
          relPath: at(id - 1).relPath,
          takenAt: ${T0} + (id - 1) * 60000,
        }));
        return Promise.resolve({ total: entries.length, entries });
      }
      if (cmd === "browse_facets") {
        return Promise.resolve({ ratings: [], colors: [], likes: [], locks: [] });
      }
      if (cmd === "browse_markings") return Promise.resolve([]);
      if (cmd === "tag_list") return Promise.resolve([]);
      if (cmd === "browse_flags_get") return Promise.resolve({ total: 0, picks: [], rejects: [] });
      return Promise.resolve(null);
    },
  };
`;

async function main() {
  await requireServer(APP);
  const chrome = launchChrome({ port: PORT, windowSize: [2560, 1440] });
  const cdp = await connectCdp(PORT);
  try {
    await cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: MOCK });
    await cdp.send("Page.navigate", { url: APP });
    // 等首屏真的挂上（导入工作区有 aside；挂不上就直接报错，别让后面全是二手症状）。
    // 30s：跑了多天的 dev server 冷挂载实测 ~14s（250 个模块重新校验），15s 会卡在边界上
    let booted = false;
    for (let attempt = 0; attempt < 60 && !booted; attempt += 1) {
      await sleep(500);
      const probe = await cdp.send("Runtime.evaluate", {
        expression: `Boolean(document.querySelector("aside")) || (document.body?.innerText ?? "").length > 0`,
        returnByValue: true,
      });
      booted = probe.result?.value === true;
    }
    if (!booted) {
      const why = await cdp.send("Runtime.evaluate", {
        expression: `({ href: location.href, title: document.title, html: (document.documentElement?.innerHTML ?? "").slice(0, 200) })`,
        returnByValue: true,
      });
      throw new Error(`页面没挂上：${JSON.stringify(why.result?.value)}；控制台 ${JSON.stringify(cdp.consoleErrors.slice(0, 2))}`);
    }

    // 切到浏览（用命令体系自己的键位：Ctrl+2）
    await cdp.send("Input.dispatchKeyEvent", {
      type: "keyDown",
      key: "2",
      code: "Digit2",
      windowsVirtualKeyCode: 50,
      modifiers: 2,
      text: "",
    });
    await cdp.send("Input.dispatchKeyEvent", {
      type: "keyUp",
      key: "2",
      code: "Digit2",
      windowsVirtualKeyCode: 50,
      modifiers: 2,
    });
    await sleep(1200);

    /*
     * 点左列目录树里的那一天（浏览必须有 `scopePath` 才查）。
     *
     * 树是懒展开的：先逐层点开 `aria-expanded="false"` 的节点，直到出现目标行 ——
     * 与真人操作一致（`TreeNode` 的 aria 属性就是交互的地图）。
     */
    const clickedDir = await cdp.send("Runtime.evaluate", {
      expression: `(async () => {
        const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
        const aside = document.querySelector("aside");
        if (!aside) return { error: "没有左列" };
        const find = () => [...aside.querySelectorAll("button, [role=treeitem]")]
          .find((node) => (node.textContent || "").includes("2026-08-15"));
        for (let attempt = 0; attempt < 12; attempt += 1) {
          const row = find();
          if (row) {
            row.click();
            return { ok: true, attempts: attempt };
          }
          const collapsed = [...aside.querySelectorAll('[role=treeitem][aria-expanded="false"]')];
          if (collapsed.length === 0) {
            const cards = [...aside.querySelectorAll("button")];
            cards[0]?.click();
          } else {
            collapsed[0].click();
          }
          await pause(400);
        }
        return { error: "没找到 2026-08-15 那一行" };
      })()`,
      awaitPromise: true,
      returnByValue: true,
    });
    const dirPick = clickedDir.result?.value ?? {};
    if (dirPick.ok !== true) {
      // 失败时把现场带出来（工作流 / 渲染了什么 / 控制台）——不然只能猜
      const why = await cdp.send("Runtime.evaluate", {
        expression: `(() => ({
          flowChips: [...document.querySelectorAll("header, [data-flowbar]")].map((n) => (n.textContent ?? "").slice(0, 40)),
          asides: document.querySelectorAll("aside").length,
          body: (document.body?.innerText ?? "").slice(0, 160),
          invokes: Object.keys(window.__PERF ?? {}),
          tauri: typeof window.__TAURI_INTERNALS__,
          calls: (window.__PERF?.calls ?? []).slice(-12),
          readyState: document.readyState,
        }))()`,
        returnByValue: true,
      });
      throw new Error(
        `点目录失败：${dirPick.error ?? JSON.stringify(dirPick)}；现场 ${JSON.stringify(why.result?.value)}；` +
          `控制台 ${JSON.stringify(cdp.consoleErrors.slice(0, 2))}；` +
          `注入脚本异常 ${JSON.stringify(cdp.exceptions.slice(0, 2))}`,
      );
    }
    await sleep(3000);

    /*
     * ── 指标 1：筛选感知延迟 ──
     *
     * 拆成两段（否则在无头软件渲染里量到的是**光栅化**，不是应用的活）：
     *   * `dataMs`：点下条件 → **筛选后的那一页数据回来**（可判据的那一段）；
     *   * `paintMs`：再等到首屏 tile 真的换掉（上报，不判据 —— 无头是软件渲染）。
     */
    const filter = await cdp.send("Runtime.evaluate", {
      expression: `(async () => {
        const frame = () => new Promise((resolve) => requestAnimationFrame(() => resolve()));
        const tilesNow = () => document.querySelectorAll('main [data-virtual-scroller] [role="option"]').length;
        const firstIdNow = () => document.querySelector('main [data-virtual-scroller] [role="option"]')?.textContent?.slice(0, 20) ?? null;
        const before = { tiles: tilesNow(), first: firstIdNow() };
        const filterButton = [...document.querySelectorAll("button")].find(
          (b) => b.getAttribute("aria-label") === "开启后，后面的标记都变成筛选条件",
        );
        if (!filterButton) return { error: "找不到筛选开关" };
        filterButton.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        await frame();
        const star = document.querySelector('button[aria-label="3 星"]');
        if (!star) return { error: "找不到星级筛选按钮" };
        window.__PERF.lastPageAt = 0;
        const clickAt = performance.now();
        star.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        let dataMs = null;
        let paintMs = null;
        while (performance.now() - clickAt < 5000) {
          await frame();
          if (dataMs === null && window.__PERF.lastPageAt > clickAt) {
            dataMs = Math.round((window.__PERF.lastPageAt - clickAt) * 10) / 10;
          }
          const after = { tiles: tilesNow(), first: firstIdNow() };
          if (after.tiles > 0 && (after.first !== before.first || after.tiles !== before.tiles)) {
            paintMs = Math.round((performance.now() - clickAt) * 10) / 10;
            break;
          }
        }
        return {
          dataMs,
          paintMs,
          tilesBefore: before.tiles,
          tilesAfter: tilesNow(),
          total: document.querySelector("[data-filter-count]")?.textContent?.trim() ?? null,
          mockQueryMs: window.__PERF.lastQueryMs,
          pageCalls: window.__PERF.pageCalls,
          lastResponse: window.__PERF.lastResponse ?? null,
        };
      })()`,
      awaitPromise: true,
      returnByValue: true,
    });
    const filterStats = filter.result?.value ?? {};

    // ── 指标 2：滚动 JS 帧预算（程序化滚动 3 秒，采 rAF 间隔；探针与 perf-win 共用一份）──
    const scroll = await cdp.send("Runtime.evaluate", {
      expression: `${SCROLL_PROBE}(3000)
        .then((scrollStats) => ({
          ...scrollStats,
          pageCalls: window.__PERF.pageCalls,
          thumbCalls: window.__PERF.thumbCalls,
        }))`,
      awaitPromise: true,
      returnByValue: true,
    });
    const scrollStats = scroll.result?.value ?? {};

    // ── 指标 3：JS 堆 + DOM 节点 ──
    const heap = await cdp.send("Runtime.getHeapUsage", {});
    const metrics = await cdp.send("Performance.getMetrics", {});
    const metricOf = (name) =>
      metrics.metrics?.find((metric) => metric.name === name)?.value ?? null;
    const memory = {
      jsHeapMb: heap.usedSize === undefined ? null : Math.round(heap.usedSize / 1024 / 1024),
      domNodes: metricOf("Nodes"),
      jsListeners: metricOf("JSEventListeners"),
    };

    const report = {
      rows: ROWS,
      window: "2560x1440",
      filter: filterStats,
      scroll: scrollStats,
      memory,
      consoleErrors: cdp.consoleErrors.slice(0, 3),
    };
    console.log("\n浏览性能基准（无头 Chrome，软件渲染 —— 帧数字只代表 JS 侧预算）");
    console.log(`合成库：${ROWS.toLocaleString("en-US")} 张\n`);
    console.log(`筛选数据路径   ${filterStats.dataMs ?? "?"} ms（判据 < 100ms：从点下条件到筛选结果回来）`);
    console.log(`筛选首屏绘制   ${filterStats.paintMs ?? "?"} ms（无头软件渲染，仅上报；真机数字见 AGENTS.md §2.8）`);
    console.log(`假后端自报     查询 ${filterStats.mockQueryMs ?? "?"} ms（10 万条整数扫描，缓存后为 0）`);
    console.log(`筛选结果       ${filterStats.total ?? "?"}（可见 tile ${filterStats.tilesAfter ?? "?"}）`);
    console.log(`滚动帧 p50/p95 ${scrollStats.p50 ?? "?"} / ${scrollStats.p95 ?? "?"} ms   最差 ${scrollStats.worst ?? "?"} ms（仅上报）`);
    console.log(`滚动时取页/取图 ${scrollStats.pageCalls ?? "?"} / ${scrollStats.thumbCalls ?? "?"} 次，可见 tile ${scrollStats.tiles ?? "?"}`);
    console.log(`JS 堆          ${memory.jsHeapMb ?? "?"} MB   DOM 节点 ${memory.domNodes ?? "?"}   监听器 ${memory.jsListeners ?? "?"}`);
    if (report.consoleErrors.length > 0) {
      console.log(`控制台错误     ${JSON.stringify(report.consoleErrors)}`);
    }
    if (REPORT) {
      writeFileSync(REPORT, `${JSON.stringify(report, null, 2)}\n`);
      console.log(`报告已写入 ${REPORT}`);
    }

    const problems = [];
    if (typeof filterStats.dataMs !== "number") {
      problems.push(`筛选这条路没量到（${JSON.stringify(filterStats)}）`);
    } else if (filterStats.dataMs > 100) {
      problems.push(`筛选数据路径 ${filterStats.dataMs}ms 超过 100ms 判据`);
    }
    /*
     * 帧数字**不判据**：这是无头 + 软件渲染（`--disable-gpu`），量到的主要是光栅化。
     * 60fps 的裁决在 Windows 真机（人类目视 + 同样的采样），见 AGENTS.md §2.8。
     */
    if (cdp.consoleErrors.length > 0) problems.push(`控制台有错误：${cdp.consoleErrors[0]}`);
    if (problems.length > 0) {
      console.error("\n✗ 性能基准没过：");
      for (const problem of problems) console.error(`  · ${problem}`);
      process.exitCode = 1;
    } else {
      console.log("\n✓ 前端脚本没拖后腿（60fps 的真机裁决见 AGENTS.md §2.8）");
    }
  } finally {
    cdp.close();
    chrome.kill("SIGKILL");
  }
}

/* 只在直接跑（node scripts/perf-browse.mjs）时执行；被 import 时只提供 MOCK */
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
