#!/usr/bin/env node
/**
 * 「库非空时进浏览」不炸 —— 端到端回归检查（开发机、零依赖）。
 *
 * **为什么需要它**：2026-09-17 人类机器上「一进浏览就报错」，根因是
 * `components/ui/thumb-queue.ts` 的 `clear()` 把 `entries` 读进了调用它的 effect 的依赖，
 * 紧接着的 `setEntries({})` 让那个 effect **自我失效** → 无限自激 → 爆栈
 * （`RangeError: Maximum call stack size exceeded`）。
 *
 * 这条路径**三个既有闸门都覆盖不到**：
 *   * `cargo test` / `pnpm test`：Node 里 `solid-js` 走 SSR 构建，没有响应式，成不了环；
 *   * `pnpm smoke:ui`：浏览器里没有后端 → 库列表恒为空 → 调用方那个 effect 提前 return；
 *   * `tsc`：纯粹的类型问题才归它管。
 * 只有「假后端给出一个**在线**的库 + 真前端渲染」才能复现。
 *
 * 用法：
 *   pnpm dev                      # 另开一个终端起开发服务器
 *   pnpm check:browse             # 默认打 http://localhost:1420/
 *
 * 退出码：左列出现「读库失败」或控制台有 RangeError → 1。
 *
 * 说明：假后端只覆盖「进浏览」这一刻要用的那几条命令（库列表 + 目录列表），
 * 别的命令一律返回 `undefined` —— 我们只关心**启动不炸**，不假装能把整个后端演完。
 * 想把这里扩成完整的浏览 E2E，那属于人类的 E2E 地盘（`AGENTS.md` §2.8）。
 */

import { spawn } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const PORT = 9467;
const APP = process.env.APP_URL ?? "http://localhost:1420/";

function findChrome() {
  if (process.env.CHROME_BIN) return process.env.CHROME_BIN;
  const cache = join(homedir(), ".cache", "ms-playwright");
  if (existsSync(cache)) {
    for (const dir of readdirSync(cache)) {
      const candidates = [
        join(cache, dir, "chrome-linux64", "chrome"),
        join(cache, dir, "chrome-linux", "chrome"),
        join(cache, dir, "chrome-headless-shell-linux64", "chrome-headless-shell"),
      ];
      for (const candidate of candidates) if (existsSync(candidate)) return candidate;
    }
  }
  for (const candidate of ["/usr/bin/google-chrome", "/usr/bin/chromium", "/usr/bin/chromium-browser"]) {
    if (existsSync(candidate)) return candidate;
  }
  return undefined;
}

/*
 * 假后端：**5 个**在线库。形状与 `RepositoryView` DTO 对齐（camelCase）。
 *
 * 为什么是 5 个而不是 1 个：库列表的紧缩/展开规则（`BROWSE.md` §4.2）只在 >3 个库时才生效 ——
 * 一个库的 fixture 根本测不到「查看所有库」那张伪卡片。
 */
const REPOSITORIES = [
  {
    id: "repro0000000000",
    name: "示例库",
    importTemplate: ":CYEAR-:CMONTH-:CDAY/MY:FILENAME",
    createdAt: 1_700_000_000_000,
    lastOpenedAt: null,
    online: true,
    root: "C:\\Photos\\demo",
    displayPath: "C:\\Photos\\demo",
    paths: [
      {
        path: "C:\\Photos\\demo",
        pathFolded: "c:/photos/demo",
        addedAt: 1_700_000_000_000,
        lastSeenAt: 1_700_000_000_000,
        status: "unknown",
      },
    ],
    photoCount: 8,
    triesPathsPlaceholder: undefined,
    triedPaths: 0,
  },
  ...["第二个库", "第三个库", "第四个库", "第五个库"].map((name, i) => ({
    id: `repro000000000${i + 1}`,
    name,
    importTemplate: ":CYEAR-:CMONTH-:CDAY/MY:FILENAME",
    createdAt: 1_700_000_000_000,
    lastOpenedAt: null,
    online: true,
    root: `C:\\Photos\\demo${i + 1}`,
    displayPath: `C:\\Photos\\demo${i + 1}`,
    paths: [],
    photoCount: 3,
    triesPathsPlaceholder: undefined,
    triedPaths: 0,
  })),
];

/*
 * 三张合成照片（形状与 `AssetItem` DTO 对齐）。
 *
 * 为什么要给数据：没有照片就测不到「双击进看图 / 状态栏 / Tab 三态」这条链 ——
 * 而那正是 M2-W2 阶段 1 的主要交付。
 */
const DEMO_ITEMS = [1, 2, 3, 4, 5, 6].map((i) => ({
  id: i,
  relPath: `photos/2026-08-15/MY00${i}.JPG`,
  fileName: `MY00${i}.JPG`,
  ext: "JPG",
  isRaw: false,
  takenAt: 1_789_000_000_000 + i * 1000,
  takenAtOffsetMin: null,
  rating: i === 1 ? 3 : 0,
  colorLabel: i === 1 ? "red" : null,
  likeState: i === 1 ? "like" : null,
  lockLevel: i === 2 ? 1 : 0,
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
  sizeBytes: 1000,
  missing: false,
}));

const FIXTURES = {
  repositories_list: REPOSITORIES,
  browse_page: { total: DEMO_ITEMS.length, offset: 0, items: DEMO_ITEMS },
  browse_timeline: {
    total: DEMO_ITEMS.length,
    entries: DEMO_ITEMS.map((i) => ({ id: i.id, relPath: i.relPath, takenAt: i.takenAt })),
  },
  browse_facets: { ratings: [], colors: [], likes: [], locks: [] },
  browse_markings: [],
  // 标签词典（3.2）：搜索与创建各给一份
  tag_list: [
    { id: 1, name: "婚礼", useCount: 3 },
    { id: 2, name: "外景", useCount: 1 },
  ],
  tag_ensure: { id: 9, name: "新标签", useCount: 0 },
  // 删除：一次把三条路径都造出来（删到 1 张、被锁挡住 1 张、失败 1 张）
  browse_delete: {
    deleted: 1,
    blockedLocked: [9],
    alreadyGone: 0,
    failed: [{ path: "C:/Photos/demo/photos/2026-08-15/MY006.JPG", reason: "文件被占用" }],
  },
  // 3.1 的边界：假装有两张被锁挡住 —— 界面上必须说出来
  // 打标先回「被锁挡住」（3.1 的边界提示），之后由 undo/redo 那几条断言接手
  browse_mark: {
    changed: 0,
    skippedLocked: [9, 11],
    undoLabel: null,
    redoLabel: null,
    canUndo: false,
    canRedo: false,
  },
  browse_flags_get: { total: 0, picks: [], rejects: [] },
  // 打旗标之后假装库里真有了一面旗 —— 这样「移除所有旗标」才是可点的，
  // 才验得到「必须先确认」这条（3.1）
  flags_set: { total: 1, picks: [1], rejects: [] },
  flags_clear: { total: 0, picks: [], rejects: [] },
};

/** 1×1 的透明 PNG：缩略图/大图都拿它当字节（只看链路通不通，不看画质）。 */
const ONE_PIXEL_PNG =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

/*
 * 目录列表按**路径**回答（这是 2026-09-18 三条口径的观测点）：
 *   * 库根 → 里面只有一个 `photos/`（它**不该**出现在树上）
 *   * `photos/` → 一个日期目录 + 一个保留目录 `_RAW`（后者**不该**出现在树上）
 *   其它路径 → 空
 */
const DEMO_PHOTOS = {
  date: { name: "2026-08-15", path: "C:/Photos/demo/photos/2026-08-15" },
  raw: { name: "_RAW", path: "C:/Photos/demo/photos/_RAW" },
};

const chromePath = findChrome();
if (!chromePath) {
  console.error("✗ 没找到 Chromium。设置 CHROME_BIN，或先用 Playwright 装一个浏览器。");
  process.exit(2);
}

try {
  const probe = await fetch(APP, { method: "HEAD" });
  if (!probe.ok && probe.status !== 405) throw new Error(String(probe.status));
} catch {
  console.error(`✗ ${APP} 打不开 —— 先另开一个终端跑 \`pnpm dev\`。`);
  process.exit(2);
}

const chrome = spawn(
  chromePath,
  [
    "--headless=new",
    "--no-sandbox",
    "--disable-gpu",
    "--disable-dev-shm-usage",
    `--remote-debugging-port=${PORT}`,
    "--window-size=1440,900",
    "about:blank",
  ],
  { stdio: ["ignore", "ignore", "pipe"] },
);
chrome.stderr.on("data", () => {});

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const problems = [];

async function pageTarget() {
  for (let i = 0; i < 60; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      const page = list.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      /* 还没起来 */
    }
    await sleep(250);
  }
  throw new Error("Chrome 调试端口没起来");
}

try {
  const ws = new WebSocket(await pageTarget());
  let nextId = 1;
  const pending = new Map();
  const consoleErrors = [];

  ws.addEventListener("message", (event) => {
    const msg = JSON.parse(event.data);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
      return;
    }
    if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error") {
      consoleErrors.push(msg.params.args.map((a) => a.description ?? a.value ?? "").join(" "));
    }
  });
  await new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve);
    ws.addEventListener("error", reject);
  });

  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`${method}: 20 秒没有回应（页面可能卡死了）`));
      }, 20_000);
      pending.set(id, (msg) => {
        clearTimeout(timer);
        if (msg.error) reject(new Error(JSON.stringify(msg.error)));
        else resolve(msg.result);
      });
      ws.send(JSON.stringify({ id, method, params }));
    });

  await send("Runtime.enable");
  await send("Page.enable");
  await send("Page.addScriptToEvaluateOnNewDocument", {
    source: `
      Error.stackTraceLimit = 60;
      // 抓「未处理的 Promise 拒绝」与运行期错误：Solid 里这类异常常常只留在控制台，
      // 而静默失败会让冒烟只能看到「后面全都不对」这种二手症状
      window.__REJECTIONS = [];
      window.addEventListener("unhandledrejection", (event) => {
        const reason = event.reason;
        const stack = reason && reason.stack ? String(reason.stack).slice(0, 400) : "";
        window.__REJECTIONS.push("unhandledrejection: " + String(reason) + " @ " + stack);
      });
      window.addEventListener("error", (event) => {
        window.__REJECTIONS.push("error: " + String(event.message));
      });
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
        transformCallback: (cb) => cb,
        convertFileSrc: (p) => p,
        invoke: (cmd, args) => {
          window.__INVOKE_LOG = window.__INVOKE_LOG || [];
          window.__INVOKE_LOG.push(cmd);
          const fixtures = ${JSON.stringify(FIXTURES)};
          if (cmd === "thumb_get" || cmd === "view_image") {
            // 前端 toBytes() 认 ArrayBuffer / Uint8Array / number[]，给哪个都行
            const raw = atob(${JSON.stringify(ONE_PIXEL_PNG)});
            const bytes = new Uint8Array(raw.length);
            for (let i = 0; i < raw.length; i += 1) bytes[i] = raw.charCodeAt(i);
            return Promise.resolve(bytes.buffer);
          }
          if (cmd === "image_histogram") {
            // 直方图的形状不能像 PNG 那样乱编：界面会按 max 归一化柱高，
            // 给一组能分出形状的计数（24 桶的中段鼓起来一个包）
            const bins = 24;
            const shape = Array.from({ length: bins }, (_, i) =>
              Math.round(400 * Math.exp(-((i - 11) ** 2) / 18)),
            );
            return Promise.resolve({
              bins,
              r: shape.map((v, i) => (i > 16 ? 0 : v)),
              g: shape,
              b: shape.map((v, i) => (i < 4 ? 0 : v)),
              max: Math.max(...shape),
            });
          }
          if (cmd === "dir_list") {
            const path = String((args && args.path) || "").replace(/\\\\/g, "/");
            const photos = ${JSON.stringify(DEMO_PHOTOS)};
            const root = "C:/Photos/demo";
            if (path === root + "/photos") {
              return Promise.resolve([photos.date, photos.raw]);
            }
            return Promise.resolve([]);
          }
          return Promise.resolve(
            Object.prototype.hasOwnProperty.call(fixtures, cmd) ? fixtures[cmd] : undefined,
          );
        },
      };
    `,
  });
  await send("Page.navigate", { url: APP });

  for (let i = 0; i < 80; i++) {
    const ready = await send("Runtime.evaluate", {
      expression: `(document.querySelector("#root")?.childElementCount ?? 0) > 0`,
      returnByValue: true,
    });
    if (ready.result?.value === true) break;
    await sleep(250);
  }
  await sleep(1200);

  const clicked = await send("Runtime.evaluate", {
    expression: `(() => {
      const label = [...document.querySelectorAll("label")]
        .find((n) => /浏览|Browse/.test(n.textContent ?? ""));
      if (!label) return "没找到「浏览」标签";
      label.click();
      return "ok";
    })()`,
    returnByValue: true,
  });
  if (clicked.result?.value !== "ok") problems.push(String(clicked.result?.value));
  await sleep(2000);

  const left = await send("Runtime.evaluate", {
    expression: `[...document.querySelectorAll("aside")].map((a) => a.innerText ?? "").join(" | ")`,
    returnByValue: true,
  });
  const leftText = String(left.result?.value ?? "");

  if (/读库失败|Could not read libraries/.test(leftText)) {
    problems.push("左列显示「读库失败」—— 读库那条链路上抛了异常");
  }
  if (!/示例库/.test(leftText)) {
    problems.push(`左列没渲染出那个库（实测文字：「${leftText.slice(0, 120)}」）`);
  }

  /*
   * 2026-09-18 的三条口径（人类上报后定的）：
   *   ① 点库不铺整库照片 —— 没选目录时**一次 browse_page 都不该发**
   *   ② 树的根是 `photos/` 之内 —— 树上不出现 `photos` 这一行
   *   ③ 保留目录 `_RAW` 不显示
   */
  const tree = await send("Runtime.evaluate", {
    expression: `(() => {
      const lines = [...document.querySelectorAll("aside")]
        .map((a) => a.innerText ?? "")
        .join("\\n")
        .split("\\n")
        .map((s) => s.trim())
        .filter(Boolean);
      return {
        hasPhotosRow: lines.some((l) => l.toLowerCase() === "photos"),
        hasRawRow: lines.some((l) => l.includes("_RAW")),
        hasDateRow: lines.some((l) => l.includes("2026-08-15")),
        pickHint: /挑一个目录|Pick a folder/.test(document.body.innerText ?? ""),
        mainText: (document.querySelector("main")?.innerText ?? "").slice(0, 160),
        pageCalls: (window.__INVOKE_LOG || []).filter((c) => c === "browse_page").length,
      };
    })()`,
    returnByValue: true,
  });
  const saw = tree.result?.value ?? {};
  if (saw.hasPhotosRow === true) {
    problems.push("目录树上出现了 `photos` 这一行 —— 树的根应当是 `photos/` 之内");
  }
  if (saw.hasRawRow === true) {
    problems.push("目录树上出现了保留目录 `_RAW` —— 它不该显示");
  }
  if (saw.hasDateRow !== true) {
    problems.push("目录树上没看到 `photos/` 下的日期目录（假后端给了 2026-08-15）");
  }
  if (saw.pickHint !== true) {
    problems.push(
      `没选目录时网格没显示「挑一个目录」的空态（中间列实测文字：「${saw.mainText ?? "(空)"}」）`,
    );
  }
  if (saw.pageCalls !== 0) {
    problems.push(`没选目录就发了 ${saw.pageCalls} 次 browse_page —— 点库不该铺整库照片`);
  }

  /*
   * 库列表的紧缩态（`BROWSE.md` §4.2）：5 个库 ⇒ 第 4 位是「查看所有库」的伪卡片。
   * ≤3 个库时它**不该**出现（那种情况另有一条断言在下面「点开之后」的检查里覆盖不到，
   * 所以这里只说「>3 时必须出现」）。
   */
  const compactView = await send("Runtime.evaluate", {
    expression: `(() => {
      const asides = [...document.querySelectorAll("aside")];
      const text = asides.map((a) => a.innerText ?? "").join("\\n");
      const all = [...asides].flatMap((a) => [...a.querySelectorAll("button")])
        .find((b) => /查看所有库|Show all libraries/.test(b.textContent ?? ""));
      return {
        hasShowAll: Boolean(all),
        // 缩起态只该露 3 张真卡片
        repoCards: (text.match(/库/g) ?? []).length,
      };
    })()`,
    returnByValue: true,
  });
  const compactSaw = compactView.result?.value ?? {};
  if (compactSaw.hasShowAll !== true) {
    problems.push("5 个库时没出现「查看所有库」的伪卡片");
  }

  /* 点伪卡片 → 展开：全部库都该看得见 */
  const clickedAll = await send("Runtime.evaluate", {
    expression: `(() => {
      const btn = [...document.querySelectorAll("aside button")]
        .find((b) => /查看所有库|Show all libraries/.test(b.textContent ?? ""));
      if (!btn) return "没找到伪卡片";
      btn.click();
      return "ok";
    })()`,
    returnByValue: true,
  });
  if (clickedAll.result?.value !== "ok") {
    problems.push(`点「查看所有库」失败：${clickedAll.result?.value}`);
  }
  await sleep(600);
  const expandedText = await send("Runtime.evaluate", {
    expression: `[...document.querySelectorAll("aside")].map((a) => a.innerText ?? "").join("\\n")`,
    returnByValue: true,
  });
  const expanded = String(expandedText.result?.value ?? "");
  for (const name of ["第五个库", "第四个库"]) {
    if (!expanded.includes(name)) {
      problems.push(`展开后没看到「${name}」（展开应当显示全部库）`);
    }
  }
  if (!/2026-08-15/.test(expanded)) {
    problems.push("展开后目录树被藏掉了 —— 规则是「缩到最小值」，不是隐藏");
  }

  /* 第二步：点那个目录 —— 这时才该去读库 */
  const clickedDir = await send("Runtime.evaluate", {
    expression: `(() => {
      const row = [...document.querySelectorAll("aside button")]
        .find((b) => (b.textContent ?? "").includes("2026-08-15"));
      if (!row) return "没找到目录行";
      row.click();
      return "ok";
    })()`,
    returnByValue: true,
  });
  if (clickedDir.result?.value !== "ok") {
    problems.push(`点目录失败：${clickedDir.result?.value}`);
  }
  await sleep(1200);
  const afterPick = await send("Runtime.evaluate", {
    expression: `(window.__INVOKE_LOG || []).filter((c) => c === "browse_page").length`,
    returnByValue: true,
  });
  if ((afterPick.result?.value ?? 0) < 1) {
    problems.push("点了目录之后没有去读库（browse_page 没被调用）");
  }
  /*
   * ── 阶段 1 的那条链：双击进看图 → 状态栏 → Tab 三态 → Esc 退回 ──
   *
   * 这一段是对 `plans/M2-W2.md` 1.2/1.3/1.4 的端到端冒烟：
   * 网格 → 看图件 → 外壳三态，三个模块的接线错一处这里就红。
   */
  /*
   * 3.1：标记动作的边界提示 —— 后端说「有 2 张被锁挡住」，界面必须说出来
   *（照片上一个像素都不会变，不说用户完全看不出来）。
   * 顺带：这一步之前 tile 已被点选过（下面那条链会再点一次，无妨）。
   */
  await send("Runtime.evaluate", {
    expression: `(() => {
      const tile = document.querySelector('[role="option"]');
      tile?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return Boolean(tile);
    })()`,
    returnByValue: true,
  });
  await sleep(200);
  const markNotice = await send("Runtime.evaluate", {
    expression: `(() => {
      const star = document.querySelector('button[aria-label="3 星"]');
      star?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return Boolean(star);
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  const noticeShown = await send("Runtime.evaluate", {
    expression: `(() => {
      const note = document.querySelector("[data-toast]");
      return {
        present: Boolean(note),
        tone: note?.getAttribute("data-toast") ?? null,
        text: (note?.textContent ?? "").slice(0, 40),
      };
    })()`,
    returnByValue: true,
  });
  const noticeState = noticeShown.result?.value ?? {};
  if (markNotice.result?.value !== true) {
    problems.push("冒烟里没找到第 3 颗星按钮（aria-label 变了？）");
  } else if (noticeState.tone !== "danger") {
    problems.push(`被锁挡住时应当弹一条「危险」提示（实测 ${JSON.stringify(noticeState)}）`);
  }
  // 提示自己是「不阻塞操作」的：容器不能吃掉点击
  const toastLayer = await send("Runtime.evaluate", {
    expression: `(() => {
      const host = document.querySelector("[data-toast-host]");
      return host ? getComputedStyle(host).pointerEvents : null;
    })()`,
    returnByValue: true,
  });
  if (toastLayer.result?.value !== "none") {
    problems.push(
      `提示容器必须是 pointer-events: none（不遮挡操作，实测 ${JSON.stringify(toastLayer.result?.value)}）`,
    );
  }

  /*
   * 3.1：清旗标是**全局动作**，点它必须先确认（`easy destroy` 范式；
   * 与删照片不同 —— 那条支持批量，所以不给 Shift 快通道，见 5.1）。
   * 断言分两步：点了之后**出现确认框**，而且此时旗标**还在一面**（没被直接清掉）。
   */
  const pickClick = await send("Runtime.evaluate", {
    expression: `(() => {
      const pick = document.querySelector('button[aria-label="留下"]');
      const before = { found: Boolean(pick), disabled: pick ? pick.disabled : null };
      pick?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return before;
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  const clearGate = await send("Runtime.evaluate", {
    expression: `(() => {
      const buttons = [...document.querySelectorAll("button")];
      const clear = buttons.find((b) => b.textContent && b.textContent.includes("移除所有旗标"));
      if (!clear) return { found: false };
      if (clear.disabled) return { found: true, disabled: true };
      clear.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return { found: true, disabled: false };
    })()`,
    returnByValue: true,
  });
  await sleep(300);
  const gateState = await send("Runtime.evaluate", {
    expression: `(() => ({
      dialog: Boolean(document.querySelector('[role="dialog"]')),
      dialogText: (document.querySelector('[role="dialog"]')?.textContent ?? "").slice(0, 60),
    }))()`,
    returnByValue: true,
  });
  const gate = clearGate.result?.value ?? {};
  const gateShown = gateState.result?.value ?? {};
  if (gate.found !== true || gate.disabled === true) {
    problems.push(
      `「移除所有旗标」应当是可点的（实测 ${JSON.stringify(gate)}；打旗标按钮=${JSON.stringify(
        pickClick.result?.value ?? {},
      )}；invoke=${JSON.stringify((await send("Runtime.evaluate", {
        expression: "JSON.stringify((window.__INVOKE_LOG || []).slice(-6))",
        returnByValue: true,
      })).result?.value ?? "")}）`,
    );
  } else if (gateShown.dialog !== true) {
    problems.push(`清空旗标必须先弹确认（实测 ${JSON.stringify(gateShown)}）`);
  } else {
    // 确认框里点「取消」—— 保证后面的断言不被这个模态挡住
    await send("Runtime.evaluate", {
      expression: `(() => {
        const dialog = document.querySelector('[role="dialog"]');
        const cancel = [...(dialog?.querySelectorAll("button") ?? [])].find(
          (b) => b.textContent && b.textContent.includes("取消"),
        );
        cancel?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        return Boolean(cancel);
      })()`,
      returnByValue: true,
    });
    await sleep(200);
  }

  /*
   * 3.2：标签弹窗（单张）—— 打开 → 输入即搜（0.6s 防抖）→ 创建新标签 → 保存落库。
   * 这条链跨了 Rust 的两个命令（tag_list / tag_ensure）与 browse_mark 的 attachTags。
   */
  const openTags = await send("Runtime.evaluate", {
    expression: `(() => {
      const button = [...document.querySelectorAll("button")].find(
        (b) => b.textContent && b.textContent.trim() === "标签",
      );
      if (!button) return { found: false };
      if (button.disabled) return { found: true, disabled: true };
      button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return { found: true, disabled: false };
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  const tagOpenState = await send("Runtime.evaluate", {
    expression: `(() => ({
      dialog: document.querySelector("[data-tag-dialog]")?.getAttribute("data-tag-dialog") ?? null,
      input: Boolean(document.querySelector("[data-tag-dialog] input")),
      results: document.querySelectorAll("[data-tag-result]").length,
    }))()`,
    returnByValue: true,
  });
  const tagOpen = tagOpenState.result?.value ?? {};
  const tagButton = openTags.result?.value ?? {};
  if (tagButton.found !== true || tagButton.disabled === true) {
    problems.push(`「标签」按钮应当可点（实测 ${JSON.stringify(tagButton)}）`);
  } else if (tagOpen.dialog !== "single") {
    problems.push(`单张时应当开「单张」形态的标签弹窗（实测 ${JSON.stringify(tagOpen)}）`);
  } else if (tagOpen.results < 1) {
    problems.push(`弹窗打开时应当先列出一批常用标签（实测 ${JSON.stringify(tagOpen.results)}）`);
  }

  if (tagOpen.dialog === "single") {
    // 输入即搜：等过防抖
    await send("Runtime.evaluate", {
      expression: `(() => {
        const input = document.querySelector("[data-tag-dialog] input");
        if (!input) return false;
        // 故意用一个**词典里没有**的名字：走创建路径（已有同名时应当复用，不建重复的）
        input.value = "新标签";
        input.dispatchEvent(new Event("input", { bubbles: true }));
        return true;
      })()`,
      returnByValue: true,
    });
    await sleep(1000);
    const searchState = await send("Runtime.evaluate", {
      expression: `(() => ({
        results: document.querySelectorAll("[data-tag-result]").length,
        create: Boolean(document.querySelector("[data-tag-create]")),
        log: (window.__INVOKE_LOG || []).filter((c) => c.startsWith("tag_")).length,
      }))()`,
      returnByValue: true,
    });
    const search = searchState.result?.value ?? {};
    if (search.results < 1 || search.create !== true) {
      problems.push(`输入之后应当出搜索结果与「创建」行（实测 ${JSON.stringify(search)}）`);
    }
    // 词典里已有的名字点「创建」**不该**再建一个（去重是硬要求）
    const dedupeCheck = await send("Runtime.evaluate", {
      expression: `(() => {
        const input = document.querySelector("[data-tag-dialog] input");
        if (!input) return { typed: false };
        input.value = "婚礼";
        input.dispatchEvent(new Event("input", { bubbles: true }));
        return { typed: true };
      })()`,
      returnByValue: true,
    });
    if (dedupeCheck.result?.value?.typed === true) {
      await sleep(1000);
      const before = await send("Runtime.evaluate", {
        expression: `(window.__INVOKE_LOG || []).filter((c) => c === "tag_ensure").length`,
        returnByValue: true,
      });
      await send("Runtime.evaluate", {
        expression: `document.querySelector("[data-tag-create]")?.dispatchEvent(new MouseEvent("click", { bubbles: true }))`,
        returnByValue: true,
      });
      await sleep(300);
      const after = await send("Runtime.evaluate", {
        expression: `(() => ({
          ensures: (window.__INVOKE_LOG || []).filter((c) => c === "tag_ensure").length,
          chips: document.querySelectorAll("[data-tag-chip]").length,
        }))()`,
        returnByValue: true,
      });
      const dedup = after.result?.value ?? {};
      if (dedup.ensures !== before.result?.value) {
        problems.push("词典里已有的标签不该再建一个（tag_ensure 不该被调用）");
      }
      if (typeof dedup.chips !== "number" || dedup.chips < 1) {
        problems.push(`复用已有标签时也该出现在待保存列表里（实测 ${JSON.stringify(dedup)}）`);
      }
      // 把输入还原成「新标签」，继续验创建路径
      await send("Runtime.evaluate", {
        expression: `(() => {
          const input = document.querySelector("[data-tag-dialog] input");
          input.value = "新标签";
          input.dispatchEvent(new Event("input", { bubbles: true }));
          return true;
        })()`,
        returnByValue: true,
      });
      await sleep(1000);
    }

    // 点「创建」行 → tag_ensure + 一个待保存的 chip
    await send("Runtime.evaluate", {
      expression: `document.querySelector("[data-tag-create]")?.dispatchEvent(new MouseEvent("click", { bubbles: true }))`,
      returnByValue: true,
    });
    await sleep(400);
    const stagedState = await send("Runtime.evaluate", {
      expression: `(() => ({
        chips: document.querySelectorAll("[data-tag-chip]").length,
        staged: document.querySelectorAll("[data-tag-staged='true']").length,
        ensured: (window.__INVOKE_LOG || []).includes("tag_ensure"),
      }))()`,
      returnByValue: true,
    });
    const staged = stagedState.result?.value ?? {};
    if (staged.ensured !== true) {
      problems.push("「创建」应当调用 tag_ensure 先把标签建出来");
    }
    if (staged.staged < 1) {
      problems.push(`创建后应当出现一个待保存的标签（实测 ${JSON.stringify(staged)}）`);
    }

    // 保存 → 关窗 + browse_mark 带上 attachTags
    await send("Runtime.evaluate", {
      expression: `(() => {
        const dialog = document.querySelector('[role="dialog"]');
        const save = [...(dialog?.querySelectorAll("button") ?? [])].find(
          (b) => b.textContent && b.textContent.trim() === "保存",
        );
        save?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        return Boolean(save);
      })()`,
      returnByValue: true,
    });
    await sleep(500);
    const savedState = await send("Runtime.evaluate", {
      expression: `(() => ({
        closed: !document.querySelector("[data-tag-dialog]"),
        marks: (window.__INVOKE_LOG || []).filter((c) => c === "browse_mark").length,
      }))()`,
      returnByValue: true,
    });
    const saved = savedState.result?.value ?? {};
    if (saved.marks < 1) {
      problems.push("保存标签应当落到 browse_mark（attachTags）");
    }
    if (saved.closed !== true) {
      problems.push(`保存之后弹窗应当关掉（实测 ${JSON.stringify(saved)}）`);
    }
  }

  /*
   * 4.1：撤销 / 重做按钮 —— 未做过动作时禁用；按钮文案里带后端的动作名。
   */
  const undoUi = await send("Runtime.evaluate", {
    expression: `(() => {
      const find = (text) =>
        [...document.querySelectorAll("button")].find((b) => b.textContent?.trim() === text);
      const undo = find("撤销");
      const redo = find("重做");
      return {
        found: Boolean(undo && redo),
        undoDisabled: undo ? undo.disabled : null,
        redoDisabled: redo ? redo.disabled : null,
      };
    })()`,
    returnByValue: true,
  });
  const undoState = undoUi.result?.value ?? {};
  if (undoState.found !== true) {
    problems.push(`工具条上应当有撤销 / 重做按钮（实测 ${JSON.stringify(undoState)}）`);
  } else if (undoState.undoDisabled !== true) {
    problems.push("还没做过任何动作时「撤销」应当是禁用的");
  }

  /*
   * 6.1：左右列宽度可拖拽 —— 两根手柄都在，键盘微调真的改了列宽。
   *
   * 用键盘（而不是合成 pointer 拖拽）验这条：拖拽的数学在 `lib/column-resize.ts` 有单测，
   * 这里要验的是**接线**（手柄 → 宽度 → 落盘），键盘走的是同一条夹取路径。
   */
  const widthBefore = await send("Runtime.evaluate", {
    expression: `(() => {
      const handles = {
        left: Boolean(document.querySelector('[data-col-resizer="left"]')),
        right: Boolean(document.querySelector('[data-col-resizer="right"]')),
      };
      const aside = document.querySelector("aside");
      return { handles, width: aside ? Math.round(aside.getBoundingClientRect().width) : null };
    })()`,
    returnByValue: true,
  });
  const beforeResize = widthBefore.result?.value ?? {};
  if (beforeResize.handles?.left !== true) {
    problems.push(`左列应当有拖拽手柄（实测 ${JSON.stringify(beforeResize.handles)}）`);
  } else if (beforeResize.handles?.right === true) {
    // 人类 2026-09-19 定：**右列宽度固定，不给把手** —— 回归时应当报出来
    problems.push("右列不该有拖拽手柄（右列宽度固定，人类 2026-09-19 定）");
  } else {
    await send("Runtime.evaluate", {
      expression: `(() => {
        const handle = document.querySelector('[data-col-resizer="left"]');
        handle?.focus();
        handle?.dispatchEvent(
          new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true, cancelable: true }),
        );
        return true;
      })()`,
      returnByValue: true,
    });
    await sleep(300);
    const afterWidth = await send("Runtime.evaluate", {
      expression: `(() => {
        const aside = document.querySelector("aside");
        return aside ? Math.round(aside.getBoundingClientRect().width) : null;
      })()`,
      returnByValue: true,
    });
    const widened = afterWidth.result?.value ?? null;
    if (widened === null || beforeResize.width === null || widened <= beforeResize.width) {
      problems.push(
        `按方向键应当把左列调宽（原来 ${JSON.stringify(beforeResize.width)}，现在 ${JSON.stringify(widened)}）`,
      );
    }
    // 调回去，别影响后面的布局断言
    await send("Runtime.evaluate", {
      expression: `(() => {
        const handle = document.querySelector('[data-col-resizer="left"]');
        handle?.dispatchEvent(
          new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true, cancelable: true }),
        );
        return true;
      })()`,
      returnByValue: true,
    });
    await sleep(200);
  }

  /*
   * 5.2：键盘评片流 —— 网格里 `→` 移动「当前那张」、数字打星、`Esc` 取消选择。
   */
  const beforeKey = await send("Runtime.evaluate", {
    // ⚠️ 别用 aria-label 判断「哪一张」：`Tile` 根节点上**没有** aria-label
    // （它的无障碍名来自内容），拿它比较会得到两次 null —— 2026-09-19 踩过。
    // 用「在所有 tile 里排第几」更稳。
    expression: `(() => {
      const tiles = [...document.querySelectorAll('[role="option"]')];
      return tiles.findIndex((tile) => tile.getAttribute("aria-selected") === "true");
    })()`,
    returnByValue: true,
  });
  await send("Runtime.evaluate", {
    expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true, cancelable: true }))`,
    returnByValue: true,
  });
  await sleep(300);
  const afterKey = await send("Runtime.evaluate", {
    expression: `(() => {
      const tiles = [...document.querySelectorAll('[role="option"]')];
      return {
        selectedAt: tiles.findIndex((tile) => tile.getAttribute("aria-selected") === "true"),
        count: tiles.filter((tile) => tile.getAttribute("aria-selected") === "true").length,
        marked: (window.__INVOKE_LOG || []).filter((c) => c === "browse_mark").length,
      };
    })()`,
    returnByValue: true,
  });
  const moved = afterKey.result?.value ?? {};
  if (moved.count !== 1) {
    problems.push(`方向键移动「当前那张」之后应当仍然只有一张被选中（实测 ${JSON.stringify(moved)}）`);
  } else if (moved.selectedAt === beforeKey.result?.value) {
    problems.push(
      `方向键应当把「当前那张」挪到下一张（原来是第 ${JSON.stringify(
        beforeKey.result?.value,
      )} 个，现在还是第 ${JSON.stringify(moved.selectedAt)} 个）`,
    );
  }

  /*
   * tiles 的状态条：**两侧同一个组件**（人类 2026-09-19：tiles 在业务上不可分割）。
   *
   * 为什么这里值得断言：统一之前浏览侧那条是**内联**在 `BrowseWorkspace` 里的另一份实现，
   * 于是「导入侧加了 `信息` 按钮、浏览侧没有」这种漂移天天发生，而且没人会报 ——
   * 谁都以为「另一个视图大概也有」。判据就用共享组件自己的标记 `[data-tiles-control-bar]`。
   */
  const tilesBar = await send("Runtime.evaluate", {
    expression: `(() => {
      const bar = document.querySelector("main [data-tiles-control-bar]");
      if (bar === null) return null;
      const text = bar.innerText.replace(/\\s+/g, " ");
      return {
        info: bar.querySelector("[data-tile-info]")?.getAttribute("data-tile-info") ?? null,
        sort: bar.querySelector("[data-sort]") !== null,
        zoom: bar.querySelector('[role="slider"]') !== null,
        byTime: text.includes("按时间"),
        // 计数与选中数是同一段文案里的两个数字（人类要求：两边都要有选中计数）
        count: text.slice(0, 40),
      };
    })()`,
    returnByValue: true,
  });
  const bar = tilesBar.result?.value ?? null;
  if (bar === null) {
    problems.push("浏览侧没用上共享的 tiles 状态条（[data-tiles-control-bar] 不在）");
  } else {
    if (bar.info === null) problems.push("状态条上缺「信息」三态开关（两侧应当都有）");
    if (!bar.sort) problems.push("浏览侧状态条应当有排序（它是可配置项，但浏览现在开着）");
    if (!bar.zoom) problems.push("状态条上缺缩放滑块");
    if (!bar.byTime) problems.push("状态条上缺「按时间」");
  }

  // 数字键打星（只在网格里生效）
  await send("Runtime.evaluate", {
    expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "3", bubbles: true, cancelable: true }))`,
    returnByValue: true,
  });
  await sleep(400);
  const rated = await send("Runtime.evaluate", {
    expression: `(window.__INVOKE_LOG || []).filter((c) => c === "browse_mark").length`,
    returnByValue: true,
  });
  if ((rated.result?.value ?? 0) <= (moved.marked ?? 0)) {
    problems.push("按数字键应当打星（browse_mark 没被调用）");
  }

  /*
   * 5.1：Delete → 必须先弹确认（人类 2026-09-19 的批注：删除能批量，所以不给 easy destroy / Shift 快通道）。
   * 确认之后：成功的走提示、被锁与失败的分别说清楚，失败清单进模态。
   */
  await send("Runtime.evaluate", {
    expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true, cancelable: true }))`,
    returnByValue: true,
  });
  await sleep(300);
  const deleteGate = await send("Runtime.evaluate", {
    expression: `(() => {
      const dialog = document.querySelector('[role="dialog"]');
      return {
        dialog: Boolean(dialog),
        text: (dialog?.textContent ?? "").slice(0, 60),
        deletedYet: (window.__INVOKE_LOG || []).includes("browse_delete"),
      };
    })()`,
    returnByValue: true,
  });
  const deleteGateState = deleteGate.result?.value ?? {};
  if (deleteGateState.dialog !== true) {
    problems.push(`Delete 必须先弹确认（实测 ${JSON.stringify(deleteGateState)}）`);
  }
  if (deleteGateState.deletedYet === true) {
    problems.push("还没确认就已经删了 —— 确认弹窗必须挡住删除");
  }
  if (deleteGateState.dialog === true) {
    await send("Runtime.evaluate", {
      expression: `(() => {
        const dialog = document.querySelector('[role="dialog"]');
        const confirm = [...(dialog?.querySelectorAll("button") ?? [])].find(
          (b) => b.textContent && b.textContent.trim() === "确定",
        );
        confirm?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        return Boolean(confirm);
      })()`,
      returnByValue: true,
    });
    await sleep(600);
    const deleteResult = await send("Runtime.evaluate", {
      expression: `(() => ({
        tones: [...document.querySelectorAll("[data-toast]")].map((t) => t.getAttribute("data-toast")),
        failures: Boolean(document.querySelector("[data-delete-failures]")),
        failureText: (document.querySelector("[data-delete-failures]")?.textContent ?? "").slice(0, 60),
        dialogs: document.querySelectorAll('[role="dialog"]').length,
        // 诊断用：删除接口到底回了什么、删完还剩几张被选中
        log: (window.__INVOKE_LOG || []).filter((c) => c === "browse_delete").length,
        tail: (window.__INVOKE_LOG || []).slice(-8),
        pages: (window.__INVOKE_LOG || []).filter((c) => c === "browse_page").length,
        rejections: (window.__REJECTIONS || []).slice(0, 3),
        selectedText: (document.querySelector("main")?.innerText ?? "").match(/已选 \d+ 张/)?.[0] ?? null,
      }))()`,
      returnByValue: true,
    });
    const del = deleteResult.result?.value ?? {};
    if (!Array.isArray(del.tones) || !del.tones.includes("success")) {
      problems.push(`删除成功要有提示（实测 ${JSON.stringify(del.tones)}）`);
    }
    if (!Array.isArray(del.tones) || !del.tones.includes("danger")) {
      problems.push(`被锁挡住 / 失败要有「危险」提示（实测 ${JSON.stringify(del.tones)}）`);
    }
    if (del.failures !== true || !String(del.failureText).includes("MY006")) {
      problems.push(`删除失败清单要逐条列出来（实测 ${JSON.stringify(del)}）`);
    }
    // 关掉失败清单，别挡住后面的断言
    await send("Runtime.evaluate", {
      expression: `(() => {
        const dialog = [...document.querySelectorAll('[role="dialog"]')].find((d) =>
          d.querySelector("[data-delete-failures]"),
        );
        const close = [...(dialog?.querySelectorAll("button") ?? [])].find(
          (b) => b.textContent && b.textContent.trim() === "关闭",
        );
        close?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        return Boolean(close);
      })()`,
      returnByValue: true,
    });
    await sleep(300);
  }

  // 再选中一张，供后面的筛选断言用（删除把选中清掉了）
  await send("Runtime.evaluate", {
    expression: `(() => {
      const tile = document.querySelector('[role="option"]');
      tile?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return Boolean(tile);
    })()`,
    returnByValue: true,
  });
  await sleep(300);

  /*
   * 3.3–3.5：筛选开关打开后 —— 结果区出 chips（从选中照片「取同类」）、
   * 两个以上条件出「任一/全部」、chip 能单条摘掉、排序控件在控制条上。
   */
  const filterOn = await send("Runtime.evaluate", {
    expression: `(() => {
      const button = [...document.querySelectorAll("button")].find(
        (b) => b.getAttribute("aria-label") === "开启后，后面的标记都变成筛选条件",
      );
      if (!button) return { found: false };
      button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return { found: true };
    })()`,
    returnByValue: true,
  });
  await sleep(500);
  const filterState = await send("Runtime.evaluate", {
    expression: `(() => {
      const bar = document.querySelector('[data-filter-bar="open"]');
      return {
        bar: Boolean(bar),
        chips: document.querySelectorAll("[data-filter-chip]").length,
        count: Boolean(document.querySelector("[data-filter-count]")),
        combinator: document.querySelectorAll('[role="radiogroup"]').length,
      };
    })()`,
    returnByValue: true,
  });
  const filterUi = filterState.result?.value ?? {};
  if (filterOn.result?.value?.found !== true) {
    problems.push("冒烟里没找到「筛选」开关（aria-label 变了？）");
  } else if (filterUi.bar !== true) {
    problems.push(`打开筛选后应有结果区（实测 ${JSON.stringify(filterUi)}）`);
  } else {
    // 选中的是 MY001：3 星 + 红色 + 喜欢 ⇒ 三条同类条件
    if (filterUi.chips !== 3) {
      problems.push(`筛选条件应当从选中照片取同类（3 星/红色/喜欢 → 3 条，实测 ${filterUi.chips}）`);
    }
    if (filterUi.count !== true) {
      problems.push("结果区应当显示「共 N 张」");
    }
    if (filterUi.combinator < 1) {
      problems.push("两个以上条件时应当出现「任一 / 全部」切换");
    }

    // 摘掉一条 chip：只少一条，别的条件不动
    const removed = await send("Runtime.evaluate", {
      expression: `(() => {
        const chip = document.querySelector("[data-filter-chip]");
        const key = chip?.getAttribute("data-filter-chip") ?? null;
        chip?.querySelector("button")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        return key;
      })()`,
      returnByValue: true,
    });
    await sleep(400);
    const afterRemove = await send("Runtime.evaluate", {
      expression: `(() => ({
        chips: document.querySelectorAll("[data-filter-chip]").length,
        stillThere: Boolean(document.querySelector('[data-filter-chip="${removed.result?.value}"]')),
      }))()`,
      returnByValue: true,
    });
    const after = afterRemove.result?.value ?? {};
    if (after.chips !== 2 || after.stillThere !== false) {
      problems.push(`摘掉一条 chip 应当只少那一条（实测 ${JSON.stringify(after)}）`);
    }
  }

  // 排序：控制条上有控件；换一个键会重查
  const sortUi = await send("Runtime.evaluate", {
    expression: `(() => {
      const box = document.querySelector("[data-sort]");
      const buttons = box ? [...box.querySelectorAll("button")] : [];
      return { box: Boolean(box), buttons: buttons.length };
    })()`,
    returnByValue: true,
  });
  const sortState = sortUi.result?.value ?? {};
  if (sortState.box !== true || sortState.buttons < 2) {
    problems.push(`底部控制条上应当有排序控件（键 + 方向，实测 ${JSON.stringify(sortState)}）`);
  }

  // 关掉筛选：结果区收起来，条件也清干净
  await send("Runtime.evaluate", {
    expression: `(() => {
      const button = [...document.querySelectorAll("button")].find(
        (b) => b.getAttribute("aria-label") === "开启后，后面的标记都变成筛选条件",
      );
      button?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return Boolean(button);
    })()`,
    returnByValue: true,
  });
  await sleep(500);
  const filterOffState = await send("Runtime.evaluate", {
    expression: `(() => ({
      bar: Boolean(document.querySelector('[data-filter-bar="open"]')),
      chips: document.querySelectorAll("[data-filter-chip]").length,
    }))()`,
    returnByValue: true,
  });
  const off = filterOffState.result?.value ?? {};
  if (off.bar !== false || off.chips !== 0) {
    problems.push(`关掉筛选应当收起结果区并清干净条件（实测 ${JSON.stringify(off)}）`);
  }

  const openViewer = await send("Runtime.evaluate", {
    expression: `(() => {
      const tile = document.querySelector('[role="option"]');
      if (!tile) return "没有 tile";
      // 真实交互是 click →（再点一下）→ dblclick；只丢一个 dblclick 不是用户会做的事
      tile.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      tile.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
      return "ok";
    })()`,
    returnByValue: true,
  });
  if (openViewer.result?.value !== "ok") {
    problems.push(`双击进看图失败：${openViewer.result?.value}`);
  }
  await sleep(1200);

  /*
   * 双击没开就用回车再试一次 —— 两条路都能进看图（1.2 都要求），
   * 而且分开报能一眼看出是「tile 没接 dblclick」还是「打开的定位逻辑不对」。
   */
  const viaDbl = await send("Runtime.evaluate", {
    expression: `Boolean(document.querySelector('[data-viewer="open"]'))`,
    returnByValue: true,
  });
  if (viaDbl.result?.value !== true) {
    const enter = await send("Runtime.evaluate", {
      expression: `(() => {
        const tile = document.querySelector('[role="option"]');
        if (!tile) return "没有 tile";
        tile.focus();
        tile.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
        return "ok";
      })()`,
      returnByValue: true,
    });
    if (enter.result?.value !== "ok") problems.push(`回车进看图失败：${enter.result?.value}`);
    await sleep(900);
    const viaEnter = await send("Runtime.evaluate", {
      expression: `Boolean(document.querySelector('[data-viewer="open"]'))`,
      returnByValue: true,
    });
    if (viaEnter.result?.value === true) {
      problems.push("双击没能进看图（回车可以）—— `Tile` 没接住 `onDblClick`，要像导入网格那样在外面包一层");
      // 既然开了，后面的断言继续跑；结束时再把它关掉
    }
  }
  await sleep(300);

  const viewerState = await send("Runtime.evaluate", {
    expression: `(() => {
      const main = document.querySelector("main[data-chrome]");
      return {
        viewer: Boolean(document.querySelector('[data-viewer="open"]')),
        status: Boolean(document.querySelector('[data-viewer-status="open"]')),
        readout: Boolean(document.querySelector('[data-viewer-readout="open"]')),
        strip: document.querySelectorAll("[data-strip-item]").length,
        stripCurrent: document.querySelector("[data-strip-item][data-current='true']")?.getAttribute("data-strip-item") ?? null,
        histogram: document.querySelector('[data-histogram]')?.getAttribute("data-histogram") ?? null,
        chrome: main?.getAttribute("data-chrome") ?? null,
        sides: [...document.querySelectorAll("aside")].filter((a) => !a.classList.contains("hidden")).length,
      };
    })()`,
    returnByValue: true,
  });
  const shown = viewerState.result?.value ?? {};
  if (shown.viewer !== true) {
    // 失败时把现场带出来：没有 tile、tile 是占位、还是点击没接上 —— 三种原因差别很大
    const why = await send("Runtime.evaluate", {
      expression: `(() => ({
        tiles: document.querySelectorAll('[role="option"]').length,
        selected: document.querySelectorAll('[role="option"][aria-selected="true"]').length,
        mainText: (document.querySelector("main")?.innerText ?? "").slice(0, 80),
        anyViewer: document.querySelectorAll("[data-viewer]").length,
        mainTail: (document.querySelector("main")?.outerHTML ?? "").slice(-120),
        log: (window.__INVOKE_LOG || []).slice(-8),
      }))()`,
      returnByValue: true,
    });
    problems.push(`双击之后看图件没打开（现场：${JSON.stringify(why.result?.value)}）`);
  }
  if (shown.status !== true) problems.push("看图态底部状态栏没出现（[data-viewer-status=\"open\"] 不在）");
  if (shown.readout !== true) problems.push("右栏没换成预览 + 直方图（[data-viewer-readout=\"open\"] 不在）");
  // 2026-09-19 起直方图是**三条填充曲线**（加色叠加），不再是柱子
  if (shown.histogram !== "curves") {
    problems.push(`直方图没画出曲线（data-histogram=${JSON.stringify(shown.histogram)}）`);
  }
  if (shown.strip !== 6) {
    problems.push(`胶片带应当有 6 张缩略（实测 ${JSON.stringify(shown.strip)}）`);
  }
  if (shown.stripCurrent !== "0") {
    problems.push(`刚进看图时当前那张应当是第 1 张（实测 ${JSON.stringify(shown.stripCurrent)}）`);
  }

  /* 胶片带：点第 2 张 → 看的就是它，底部状态栏跟着走（BROWSE.md §5.7） */
  await send("Runtime.evaluate", {
    expression: `(() => {
      const item = document.querySelector('[data-strip-item="1"]');
      item?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
      return Boolean(item);
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  const stripAfter = await send("Runtime.evaluate", {
    expression: `(() => ({
      current: document.querySelector("[data-strip-item][data-current='true']")?.getAttribute("data-strip-item") ?? null,
      status: document.querySelector('[data-viewer-status="open"]')?.innerText ?? "",
      selected: document.querySelectorAll("[data-strip-item]").length,
    }))()`,
    returnByValue: true,
  });
  const stripState = stripAfter.result?.value ?? {};
  if (stripState.current !== "1") {
    problems.push(`点胶片带第 2 张之后当前那张应当是它（实测 ${JSON.stringify(stripState.current)}）`);
  }
  if (!String(stripState.status).includes("MY002")) {
    problems.push(`点胶片带之后底部状态栏文件名没跟着走（实测 ${JSON.stringify(String(stripState.status).slice(0, 60))}）`);
  }
  if (shown.chrome !== "default") problems.push(`进看图时三态应当从默认开始，实测 ${shown.chrome}`);

  /* Tab：①默认 → ②关左右 → ③关胶片带 → ① */
  /* 对比态：Ctrl 多选 → 自然进入对比；反选 → 自然退出（BROWSE.md §5.5） */
  await send("Runtime.evaluate", {
    expression: `(() => {
      const item = document.querySelector('[data-strip-item="2"]');
      item?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true }));
      return Boolean(item);
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  const compareOn = await send("Runtime.evaluate", {
    expression: `(() => ({
      compare: Boolean(document.querySelector('[data-compare="open"]')),
      frames: document.querySelectorAll("[data-compare-frame]").length,
      baseline: document.querySelector("[data-compare-frame][data-baseline='true']")?.getAttribute("data-compare-frame") ?? null,
      viewer: Boolean(document.querySelector('[data-viewer="open"]')),
      selected: document.querySelectorAll("[data-strip-item]").length,
    }))()`,
    returnByValue: true,
  });
  const compareState = compareOn.result?.value ?? {};
  if (compareState.compare !== true) {
    problems.push(`Ctrl 多选之后应当自然进入对比（实测 ${JSON.stringify(compareState)}）`);
  } else {
    if (compareState.frames !== 2) {
      problems.push(`对比应当有 2 幅画幅（实测 ${JSON.stringify(compareState.frames)}）`);
    }
    if (compareState.baseline !== "0") {
      problems.push(`第一幅应当是基准（带主色描边）（实测 ${JSON.stringify(compareState.baseline)}）`);
    }
    if (compareState.viewer !== false) {
      problems.push("对比态下不该同时出现单张看图件");
    }
  }

  /*
   * 2.5：在对比里**点某一幅画幅** = 把它当「当前那张」（BROWSE.md §5.7）——
   * 底部状态栏与右栏都跟着它，但**选择集合不变**（否则对比当场散掉）。
   */
  const beforeFocus = await send("Runtime.evaluate", {
    expression: `(() => ({
      rightColumn: (document.querySelector("aside:last-of-type")?.innerText ?? "").slice(0, 80),
      frameCurrent: document.querySelector("[data-compare-frame][data-current='true']")?.getAttribute("data-compare-frame") ?? null,
    }))()`,
    returnByValue: true,
  });
  await send("Runtime.evaluate", {
    expression: `(() => {
      const frame = document.querySelector('[data-compare-frame="0"]');
      frame?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
      return Boolean(frame);
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  const afterFocus = await send("Runtime.evaluate", {
    expression: `(() => ({
      status: document.querySelector('[data-viewer-status="open"]')?.innerText ?? "",
      rightColumn: (document.querySelector("aside:last-of-type")?.innerText ?? "").slice(0, 80),
      frameCurrent: document.querySelector("[data-compare-frame][data-current='true']")?.getAttribute("data-compare-frame") ?? null,
      frames: document.querySelectorAll("[data-compare-frame]").length,
      stripCurrent: document.querySelector("[data-strip-item][data-current='true']")?.getAttribute("data-strip-item") ?? null,
    }))()`,
    returnByValue: true,
  });
  const focusState = afterFocus.result?.value ?? {};
  if (focusState.frameCurrent !== "0") {
    problems.push(`点了第一幅之后它应当是「当前那张」（实测 ${JSON.stringify(focusState.frameCurrent)}）`);
  }
  if (!String(focusState.status).includes("MY002")) {
    problems.push(`点画幅之后底部状态栏应当跟着它（实测 ${JSON.stringify(String(focusState.status).slice(0, 60))}）`);
  }
  if (!String(focusState.rightColumn).includes("MY002")) {
    problems.push(`点画幅之后右栏应当显示它的信息（实测 ${JSON.stringify(String(focusState.rightColumn).slice(0, 60))}）`);
  }
  if (String(beforeFocus.result?.value?.rightColumn ?? "").includes("MY002")) {
    problems.push("点画幅**之前**右栏就已经是 MY002 了 —— 这条断言区分不出「跟着走」");
  }
  if (focusState.frames !== 2) {
    problems.push(`点画幅不该改变参与对比的张数（实测 ${JSON.stringify(focusState.frames)}）`);
  }

  /*
   * 2.4：对比态下**再按一次回车** → 胶片带只显示参与对比的图（整条主色细边框）。
   */
  await send("Runtime.evaluate", {
    expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }))`,
    returnByValue: true,
  });
  await sleep(300);
  const stripCompare = await send("Runtime.evaluate", {
    expression: `(() => {
      const strip = document.querySelector("[data-filmstrip]");
      return {
        mode: strip?.getAttribute("data-strip-mode") ?? null,
        items: document.querySelectorAll("[data-strip-item]").length,
        border: strip ? getComputedStyle(strip).borderTopWidth : null,
      };
    })()`,
    returnByValue: true,
  });
  const stripState2 = stripCompare.result?.value ?? {};
  if (stripState2.mode !== "compare") {
    problems.push(`对比态按回车后胶片带应当是「只看对比图」（实测 ${JSON.stringify(stripState2)}）`);
  }
  if (stripState2.items !== 2) {
    problems.push(`「只看对比图」时胶片带应当只有 2 张（实测 ${JSON.stringify(stripState2.items)}）`);
  }
  if (stripState2.border !== "1px") {
    problems.push(`「只看对比图」时胶片带要有 1px 主色边框（实测 ${JSON.stringify(stripState2.border)}）`);
  }

  /*
   * 2.4：在这个状态里**不按 Ctrl** 点一张 = 把它移出对比；只剩一幅时退出并恢复全部。
   */
  await send("Runtime.evaluate", {
    expression: `(() => {
      const item = document.querySelector('[data-strip-item="2"]');
      item?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
      return Boolean(item);
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  const afterLeave = await send("Runtime.evaluate", {
    expression: `(() => {
      const strip = document.querySelector("[data-filmstrip]");
      return {
        compare: Boolean(document.querySelector('[data-compare="open"]')),
        mode: strip?.getAttribute("data-strip-mode") ?? null,
        items: document.querySelectorAll("[data-strip-item]").length,
        current: document.querySelector("[data-strip-item][data-current='true']")?.getAttribute("data-strip-item") ?? null,
        viewer: Boolean(document.querySelector('[data-viewer="open"]')),
      };
    })()`,
    returnByValue: true,
  });
  const leaveState = afterLeave.result?.value ?? {};
  if (leaveState.compare !== false) {
    problems.push("只剩一幅时应当退出对比（实测仍在对比）");
  }
  if (leaveState.mode !== "all" || leaveState.items !== 6) {
    problems.push(`退出对比后胶片带应当恢复显示全部 6 张（实测 ${JSON.stringify(leaveState)}）`);
  }
  if (leaveState.current !== "1") {
    problems.push(`退出后应当定位到最后那张（实测 current=${JSON.stringify(leaveState.current)}）`);
  }

  /* 反选（Ctrl 路径）：再 Ctrl 点一张进对比、再 Ctrl 点它自己 → 只剩一张 → 自然退出 */
  await send("Runtime.evaluate", {
    expression: `(() => {
      const item = document.querySelector('[data-strip-item="2"]');
      item?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true }));
      return Boolean(item);
    })()`,
    returnByValue: true,
  });
  await sleep(300);
  await send("Runtime.evaluate", {
    expression: `(() => {
      const item = document.querySelector('[data-strip-item="2"]');
      item?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true }));
      return Boolean(item);
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  const compareOff = await send("Runtime.evaluate", {
    expression: `(() => ({
      compare: Boolean(document.querySelector('[data-compare="open"]')),
      viewer: Boolean(document.querySelector('[data-viewer="open"]')),
    }))()`,
    returnByValue: true,
  });
  const offState = compareOff.result?.value ?? {};
  if (offState.compare !== false || offState.viewer !== true) {
    problems.push(`反选之后应当自然退回单张看图（实测 ${JSON.stringify(offState)}）`);
  }

  /*
   * 0.4 + 0.6：选中超过 4 张时**只对比最近选中的 4 张**（人类 2026-09-19 定的规则），
   * 布局走 2×2；被挤掉的是**最早选中**的那张，基准 = 窗口里最早的那张。
   */
  for (const at of [2, 3, 4, 5]) {
    await send("Runtime.evaluate", {
      expression: `(() => {
        const item = document.querySelector('[data-strip-item="${at}"]');
        item?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true }));
        return Boolean(item);
      })()`,
      returnByValue: true,
    });
    await sleep(120);
  }
  await sleep(400);
  const windowState = await send("Runtime.evaluate", {
    expression: `(() => {
      const host = document.querySelector('[data-compare="open"]');
      return {
        count: host?.getAttribute("data-compare-count") ?? null,
        cols: host?.getAttribute("data-compare-cols") ?? null,
        rows: host?.getAttribute("data-compare-rows") ?? null,
        baselineName: document.querySelector('[data-compare-frame="0"]')?.getAttribute("aria-label") ?? null,
        last: document.querySelector('[data-compare-frame="3"]')?.getAttribute("aria-label") ?? null,
        note: Boolean(document.querySelector('[data-compare-note="open"]')),
      };
    })()`,
    returnByValue: true,
  });
  const win = windowState.result?.value ?? {};
  if (win.count !== "4") {
    problems.push(`选中 5 张时只该对比 4 张（实测 ${JSON.stringify(win.count)}）`);
  }
  if (win.cols !== "2" || win.rows !== "2") {
    problems.push(`4 张画幅应当走 2×2（实测 ${JSON.stringify({ cols: win.cols, rows: win.rows })}）`);
  }
  if (win.baselineName !== "MY003.JPG") {
    problems.push(`4 张窗口的基准应当是窗口里最早选中的 MY003（实测 ${JSON.stringify(win.baselineName)}）`);
  }
  if (win.last !== "MY006.JPG") {
    problems.push(`窗口最右应当是最后选中的 MY006（实测 ${JSON.stringify(win.last)}）`);
  }
  if (win.note !== true) {
    problems.push("挤掉图时应当有一句说明（[data-compare-note]）");
  }

  /*
   * 顺带验一条**窗口滚动**：Ctrl 点掉当前窗口里**最新**的那张（MY006）——
   * 之前被挤出去的 MY002 会**回到窗口**，所以仍是 4 幅（不是 3 幅），基准变回 MY002。
   */
  await send("Runtime.evaluate", {
    expression: `(() => {
      const item = document.querySelector('[data-strip-item="5"]');
      item?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true }));
      return Boolean(item);
    })()`,
    returnByValue: true,
  });
  await sleep(300);
  const shrunk = await send("Runtime.evaluate", {
    expression: `(() => ({
      count: document.querySelector('[data-compare="open"]')?.getAttribute("data-compare-count") ?? null,
      cols: document.querySelector('[data-compare="open"]')?.getAttribute("data-compare-cols") ?? null,
      baseline: document.querySelector('[data-compare-frame="0"]')?.getAttribute("aria-label") ?? null,
      note: Boolean(document.querySelector('[data-compare-note="open"]')),
    }))()`,
    returnByValue: true,
  });
  const shrunkState = shrunk.result?.value ?? {};
  if (shrunkState.count !== "4" || shrunkState.cols !== "2" || shrunkState.baseline !== "MY002.JPG") {
    problems.push(
      `移出最新那张后应当仍对比 4 幅、基准回到 MY002（实测 ${JSON.stringify(shrunkState)}）`,
    );
  }
  if (shrunkState.note) {
    problems.push("选中的总数回到 4 张以内时，那句「只对比最近选中的 4 张」的说明该收起来");
  }

  const chromeCycle = [];
  for (let i = 0; i < 4; i += 1) {
    const state = await send("Runtime.evaluate", {
      expression: `(() => {
        window.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true }));
        const main = document.querySelector("main[data-chrome]");
        return {
          chrome: main?.getAttribute("data-chrome") ?? null,
          film: main?.getAttribute("data-film") ?? null,
          sides: [...document.querySelectorAll("aside")].filter((a) => !a.classList.contains("hidden")).length,
          strip: Boolean(document.querySelector('[data-filmstrip="open"]')),
        };
      })()`,
      returnByValue: true,
    });
    chromeCycle.push(state.result?.value ?? {});
    await sleep(200);
  }
  const expected = [
    { chrome: "film-only", sides: 0, film: "on", strip: true },
    { chrome: "view-only", sides: 0, film: "off", strip: false },
    { chrome: "default", sides: 2, film: "on", strip: true },
    { chrome: "film-only", sides: 0, film: "on", strip: true },
  ];
  for (let i = 0; i < expected.length; i += 1) {
    const want = expected[i];
    const got = chromeCycle[i] ?? {};
    if (
      got.chrome !== want.chrome ||
      got.sides !== want.sides ||
      got.film !== want.film ||
      got.strip !== want.strip
    ) {
      problems.push(
        `Tab 第 ${i + 1} 下应当是 ${JSON.stringify(want)}，实测 ${JSON.stringify(got)}`,
      );
    }
  }

  /*
   * 对比态的那**一组**控件（人类 2026-09-19：整个对比区只有一组「左上返回 + 右下缩放」，
   * 不是每幅画幅各来一组）。顺带守住那个真实踩过的坑 ——
   * 抽共享组件时返回按钮接的是「通知外面」的回调、没有关 store，症状是**点了没反应**
   * （`pnpm smoke:ui` 的看图演示先抓到，这里再对对比态守一遍）。
   */
  const compareControls = await send("Runtime.evaluate", {
    expression: `(() => {
      const host = document.querySelector('[data-compare="open"]');
      const back = host?.querySelector('button[aria-label="返回"]') ?? null;
      return {
        frames: document.querySelectorAll("[data-compare-frame]").length,
        back: back !== null,
        zoom: host?.querySelectorAll('[data-viewer-controls="zoom"]').length ?? 0,
        backTotal: document.querySelectorAll('button[aria-label="返回"]').length,
      };
    })()`,
    returnByValue: true,
  });
  const controls = compareControls.result?.value ?? {};
  if (!controls.back) problems.push("对比态缺左上角「返回」（整个对比区应当有一组）");
  if (controls.zoom !== 1) {
    problems.push(`对比态的缩放控件应当只有一组（实测 ${JSON.stringify(controls.zoom)}）`);
  }
  if (controls.backTotal !== 1) {
    problems.push(`对比态不该有多组返回控件（实测 ${JSON.stringify(controls.backTotal)}）`);
  }
  if (controls.back) {
    await send("Runtime.evaluate", {
      expression: `(() => {
        const host = document.querySelector('[data-compare="open"]');
        const back = host?.querySelector('button[aria-label="返回"]');
        back?.click();
        return Boolean(back);
      })()`,
      returnByValue: true,
    });
    await sleep(500);
    const afterBack = await send("Runtime.evaluate", {
      expression: `(() => ({
        viewer: Boolean(document.querySelector('[data-viewer="open"]')),
        compare: Boolean(document.querySelector('[data-compare="open"]')),
        chrome: document.querySelector("main[data-chrome]")?.getAttribute("data-chrome") ?? null,
      }))()`,
      returnByValue: true,
    });
    const backState = afterBack.result?.value ?? {};
    if (backState.viewer || backState.compare) {
      problems.push(`对比态点返回没退出看图（实测 ${JSON.stringify(backState)}）`);
    }
    if (backState.chrome !== "default") {
      problems.push(`对比态点返回之后三态应当复位（实测 ${JSON.stringify(backState.chrome)}）`);
    }
  }

  /* Esc 退回 tiles：看图件关掉、左右栏必定回来 */
  await send("Runtime.evaluate", {
    expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }))`,
    returnByValue: true,
  });
  await sleep(500);
  const closed = await send("Runtime.evaluate", {
    expression: `(() => {
      const main = document.querySelector("main[data-chrome]");
      return {
        viewer: Boolean(document.querySelector('[data-viewer="open"]')),
        chrome: main?.getAttribute("data-chrome") ?? null,
        sides: [...document.querySelectorAll("aside")].filter((a) => !a.classList.contains("hidden")).length,
      };
    })()`,
    returnByValue: true,
  });
  const after = closed.result?.value ?? {};
  if (after.viewer !== false) problems.push("Esc 之后看图件没关掉");
  if (after.chrome !== "default" || after.sides !== 2) {
    problems.push(`退回 tiles 时左右栏必须回来，实测 ${JSON.stringify(after)}`);
  }

  const stackOverflow = consoleErrors.find((text) => /Maximum call stack/.test(text));
  if (stackOverflow) {
    problems.push("控制台出现爆栈（很可能是响应式自激）：" + stackOverflow.split("\n")[0]);
  }
  const otherErrors = consoleErrors.filter((text) => !/Maximum call stack/.test(text));
  if (otherErrors.length > 0) {
    problems.push(`控制台有 ${otherErrors.length} 条错误，例如：` + otherErrors[0].split("\n")[0]);
  }
} finally {
  chrome.kill("SIGKILL");
}

if (problems.length > 0) {
  console.error("✗ 「库非空时进浏览」检查没过：");
  for (const problem of problems) console.error("  · " + problem);
  process.exit(1);
}
console.log("✓ 库非空时进浏览正常（左列渲染出库、树根是 photos 之内、无 _RAW、点目录才读库、无爆栈、无控制台错误）");
