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
 * 为什么要给数据：没有照片就测不到「双击进看图 / 状态栏 / Tab 四态」这条链 ——
 * 而那正是 M2-W2 阶段 1 的主要交付。
 */
const DEMO_ITEMS = [1, 2, 3, 4, 5, 6].map((i) => ({
  id: i,
  relPath: `photos/2026-08-15/MY00${i}.JPG`,
  fileName: `MY00${i}.JPG`,
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
  createdMs: 1_789_000_000_000 + i * 1000,
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
  /*
   * 最后两张**故意不给宽高**：老库（2026-09-18 之前的导入）里 `assets.width/height`
   * 就是 NULL —— 界面必须靠 `dir_meta_ensure` 补读兜住（tile 比例与对比尺寸都靠它）。
   */
  width: i >= 5 ? null : 4000,
  height: i >= 5 ? null : 3000,
  orientation: 1,
  sizeBytes: 1000,
  missing: false,
}));

const FIXTURES = {
  /*
   * 左列那两条命令（人类 2026-09-19：「假后端缺命令，导入左列显示『读不了这个位置』」）：
   * 形参按 `src/api/types.ts` 的 `RecentDir` / `Volume` 给全，
   * 缺字段会让视图在 `.slice()` 上炸 —— 那种失败是工装的，不是产品的。
   */
  recent_dirs_list: [
    { path: "C:\\Photos\\demo", includeSubdirs: true, usedAt: 1_789_000_000_000, useCount: 3 },
  ],
  volumes_list: [
    {
      path: "C:\\",
      name: "系统盘",
      kind: "fixed",
      totalBytes: 512_000_000_000,
      freeBytes: 128_000_000_000,
    },
  ],
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
  // 打旗标之后假装库里真有了一面旗 —— 这样「清空旗标」才是可点的，
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
    /*
     * **无头 Chrome 默认没有鼠标**：`(hover: hover)` / `(pointer: fine)` 都是 false，
     * 于是 Tailwind 里所有 `hover:*` / `group-hover:*` 变体整条失效
     *（那是「真机有鼠标」才成立的媒体特性）。`Emulation.setEmulatedMedia` 试过、不生效，
     * 正确做法是启动时把 blink 的悬停/指针能力直接声明出来
     *（HoverType::kHover = 2，PointerType::kFine = 4）。
     *
     * 不修这个的后果：冒烟里怎么移动鼠标，信息条都不浮出 —— 会误判成 CSS 写错
     *（2026-09-20 真踩过）。
     */
    "--blink-settings=primaryHoverType=2,availableHoverTypes=2,primaryPointerType=4,availablePointerTypes=4",
    `--remote-debugging-port=${PORT}`,
    "--window-size=1440,900",
    "about:blank",
  ],
  { stdio: ["ignore", "ignore", "pipe"] },
);
chrome.stderr.on("data", () => {});

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const problems = [];

/**
 * **真双击**：用 CDP 发两对 press/release（第二对 `clickCount: 2`），
 * 浏览器由此自己合成 dblclick —— 走的正是真人那条路（含 pointer capture / 命中测试）。
 *
 * 合成 `new MouseEvent("dblclick")` 只能证明「处理器在」，证明不了「这条路通」：
 * 对比里双击失效那几次全是在这条真路上暴露的（2026-09-20）。
 */
async function realDoubleClick(sendFn, x, y) {
  await sendFn("Input.dispatchMouseEvent", { type: "mousePressed", button: "left", buttons: 1, clickCount: 1, x, y });
  await sendFn("Input.dispatchMouseEvent", { type: "mouseReleased", button: "left", buttons: 0, clickCount: 1, x, y });
  await sendFn("Input.dispatchMouseEvent", { type: "mousePressed", button: "left", buttons: 1, clickCount: 2, x, y });
  await sendFn("Input.dispatchMouseEvent", { type: "mouseReleased", button: "left", buttons: 0, clickCount: 2, x, y });
}

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
        const error = event.error;
        const stack = error && error.stack ? String(error.stack).slice(0, 800) : "";
        window.__REJECTIONS.push("error: " + String(event.message) + (stack ? " @ " + stack : ""));
      });
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
        transformCallback: (cb) => cb,
        convertFileSrc: (p) => p,
        invoke: (cmd, args) => {
          window.__INVOKE_LOG = window.__INVOKE_LOG || [];
          window.__INVOKE_LOG.push(cmd);
          if (cmd === "setting_set") {
            window.__SETTING_CALLS = window.__SETTING_CALLS || [];
            window.__SETTING_CALLS.push(args);
          }
          const fixtures = ${JSON.stringify(FIXTURES)};
          if (cmd === "thumb_get" || cmd === "view_image") {
            // 前端 toBytes() 认 ArrayBuffer / Uint8Array / number[]，给哪个都行
            const raw = atob(${JSON.stringify(ONE_PIXEL_PNG)});
            const bytes = new Uint8Array(raw.length);
            for (let i = 0; i < raw.length; i += 1) bytes[i] = raw.charCodeAt(i);
            return Promise.resolve(bytes.buffer);
          }
          if (cmd === "image_histogram") {
            /*
             * 直方图的形状不能像 PNG 那样乱编：界面会按 max 归一化高度。
             * **52 桶**（= 0..255 每 5 级一个点，人类 2026-09-19 的口径）：
             * 三个通道给**互相错开**的包，好让分层取色（单通道 / 两两重叠 / 三色重叠）
             * 真的都被画出来 —— 三条完全一样的曲线是测不出分层的。
             */
            const bins = 52;
            const shape = (center, width, height) =>
              Array.from({ length: bins }, (_, i) =>
                Math.round(height * Math.exp(-((i - center) ** 2) / width)),
              );
            const r = shape(20, 60, 400);
            const g = shape(26, 90, 360);
            const b = shape(33, 120, 320);
            return Promise.resolve({
              bins,
              r,
              g,
              b,
              max: 400,
            });
          }
          if (cmd === "dir_meta_ensure") {
            /*
             * 补读宽高（老库兜底路径）：按请求的文件名逐个回答 ——
             * 回来一份「真实的」尺寸，界面应当据此把比例与对比尺寸都补齐。
             *
             * ⚠️ 故意让 **MY006 变成竖图且更小**（2400×3200，7.68M）而其他都是
             * 4000×3000（12M）：这样 ① 老库补读路径照旧被覆盖；
             * ② 一组里的图**比例不同、像素数也不同** —— 虚拟画布（宽 4000 × 高 3200）、
             * 居中留白、以及「按像素数最小的那张算适合窗口」三条都能在真浏览器里量到。
             */
            const files = (args && args.files) || [];
            return Promise.resolve(
              files.map((file) => {
                const relative = String(file.relative);
                const portrait = /MY006/i.test(relative);
                return {
                  relative,
                  width: portrait ? 2400 : 4000,
                  height: portrait ? 3200 : 3000,
                  orientation: 1,
                };
              }),
            );
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
          /*
           * 打标的**参数**要留痕：赞/踩「再点一次 = 取消」这条只能靠参数验
           * （2026-09-20 的 bug 就是第二次仍然发 value="like"，后端判定无改动 → 弹「没有需要改动的照片」）。
           */
          if (cmd === "browse_mark") {
            window.__MARK_ARGS = window.__MARK_ARGS || [];
            window.__MARK_ARGS.push(args);
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
   * tiles 下的多选 → **回车进对比**（人类 2026-09-19 报：只能在 film 里 Ctrl 多选，
   * tiles 里多选按回车没反应）。顺带把「老库宽高为 NULL」那条兜底路径也验掉：
   * 最后两张（MY005/MY006）在 fixture 里没有宽高，进对比必须能看到图，
   * 而不是那句「还没读到这张的尺寸」。
   */
  // ① 先普通点一张（清掉之前的选中），等一帧让「收起库列表」落地，
  // 再从**当前 DOM**重新命中第二张 Ctrl 加选 ⇒ 目录里共 2 张选中。
  // 不长期持有第一次查询到的节点：真人的第二次点击也会重新做命中测试。
  await send("Runtime.evaluate", {
    expression: `(() => {
      const tiles = [...document.querySelectorAll('main [data-virtual-scroller] [role="option"]')];
      if (tiles.length < 6) return "tile 不够";
      tiles[4].dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
      return "ok";
    })()`,
    returnByValue: true,
  });
  await sleep(50);
  await send("Runtime.evaluate", {
    expression: `(() => {
      const tiles = [...document.querySelectorAll('main [data-virtual-scroller] [role="option"]')];
      if (tiles.length < 6) return "tile 不够";
      tiles[5].dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true }));
      return "ok";
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  const tilesSelected = await send("Runtime.evaluate", {
    expression: `document.querySelectorAll('main [data-virtual-scroller] [role="option"][aria-selected="true"]').length`,
    returnByValue: true,
  });
  if (tilesSelected.result?.value !== 2) {
    problems.push(
      `tiles 里 Ctrl 点两张应当有 2 张选中（实测 ${JSON.stringify(tilesSelected.result?.value)}）`,
    );
  }

  // ② 回车 → 直接进对比（对比是选择状态的派生值）
  await send("Runtime.evaluate", {
    expression: `(() => {
      const tiles = [...document.querySelectorAll('main [data-virtual-scroller] [role="option"]')];
      const last = tiles[tiles.length - 1];
      if (!last) return false;
      last.focus();
      last.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
      return true;
    })()`,
    returnByValue: true,
  });
  await sleep(900);
  const tilesCompare = await send("Runtime.evaluate", {
    expression: `(() => {
      const frames = [...document.querySelectorAll("[data-compare-frame]")];
      /*
       * **混合比例**的一组（人类 2026-09-20 报的那个「横图被裁成竖图」）：
       * MY005 是 4000×3000（横）、MY006 补读成 2400×3200（竖且更小）。
       * 虚拟画布应当是 4000×3200 —— 两张图都按**原尺寸居中**贴进去，谁都不裁。
       */
      const details = frames.map((frame) => {
        const canvasEl = frame.querySelector("[data-compare-canvas]");
        const img = frame.querySelector("img");
        const canvasBox = canvasEl?.getBoundingClientRect();
        const imgBox = img?.getBoundingClientRect();
        const paneBox = frame.getBoundingClientRect();
        return canvasBox && imgBox
          ? {
              name: frame.getAttribute("aria-label"),
              fillW: imgBox.width / canvasBox.width,
              fillH: imgBox.height / canvasBox.height,
              offsetX: (imgBox.left - canvasBox.left) / canvasBox.width,
              offsetY: (imgBox.top - canvasBox.top) / canvasBox.height,
              insideCanvas:
                imgBox.width <= canvasBox.width + 0.5 && imgBox.height <= canvasBox.height + 0.5,
              insidePane:
                imgBox.width <= paneBox.width + 0.5 && imgBox.height <= paneBox.height + 0.5,
              canvas: { width: canvasBox.width, height: canvasBox.height },
            }
          : null;
      });
      return {
        compare: Boolean(document.querySelector('[data-compare="open"]')),
        frames: frames.length,
        images: frames.filter((f) => f.querySelector("img") !== null).length,
        noSize: (document.body.innerText || "").includes("还没读到这张的尺寸"),
        details,
      };
    })()`,
    returnByValue: true,
  });
  const tilesCmp = tilesCompare.result?.value ?? {};
  if (tilesCmp.compare !== true) {
    problems.push(`tiles 里选中 2 张后按回车应当直接进对比（实测 ${JSON.stringify(tilesCmp)}）`);
  } else {
    if (tilesCmp.frames !== 2) {
      problems.push(`tiles 进对比后应当有 2 幅画幅（实测 ${JSON.stringify(tilesCmp.frames)}）`);
    }
    if (tilesCmp.images !== 2) {
      problems.push(
        `对比画幅都要出图 —— 老库（宽高为 NULL）必须靠补读兜住（实测出图 ${JSON.stringify(tilesCmp.images)} 幅）`,
      );
    }
    if (tilesCmp.noSize) {
      problems.push("对比里出现了「还没读到这张的尺寸」—— 说明宽高没补齐就进画幅了");
    }

    /*
     * 混合比例：虚拟画布 = 最大宽 × 最大高 = 4000×3200；
     * 横图（4000×3000）上下各留 100px（占比 0.03125），竖图（2400×3200）左右各留 800px（0.2）。
     * 关键：**两张都不被裁**（`insideCanvas`），而且**没有任何一张被拉伸**（fill 值就是原比例）。
     */
    const details = tilesCmp.details ?? [];
    const landscape = details.find((d) => d !== null && /MY005/i.test(String(d.name)));
    const portrait = details.find((d) => d !== null && /MY006/i.test(String(d.name)));
    if (landscape === undefined || portrait === undefined) {
      problems.push(`量不到两张混合比例的图（实测 ${JSON.stringify(details)}）`);
    } else {
      if (Math.abs(landscape.fillW - 1) > 0.01 || Math.abs(landscape.fillH - 3000 / 3200) > 0.01) {
        problems.push(`横图在画布里的占比不对（实测 ${JSON.stringify(landscape)}）`);
      }
      if (Math.abs(portrait.fillW - 2400 / 4000) > 0.01 || Math.abs(portrait.fillH - 1) > 0.01) {
        problems.push(`竖图在画布里的占比不对（实测 ${JSON.stringify(portrait)}）`);
      }
      if (Math.abs(landscape.offsetX) > 0.005 || Math.abs(landscape.offsetY - 100 / 3200) > 0.01) {
        problems.push(`横图应当在画布上下留白（居中），实测 ${JSON.stringify(landscape)}`);
      }
      if (Math.abs(portrait.offsetY) > 0.005 || Math.abs(portrait.offsetX - 0.2) > 0.01) {
        problems.push(`竖图应当在画布左右留白（居中），实测 ${JSON.stringify(portrait)}`);
      }
      if (landscape.insideCanvas !== true || portrait.insideCanvas !== true) {
        problems.push(
          `虚拟画布不许裁图（人类报的正是「横图被裁成竖比例」），实测 ${JSON.stringify({ landscape, portrait })}`,
        );
      }
      /*
       * 「适合窗口」按**像素数最小**的那张算：竖图 2400×3200 = 7.68M 比横图 12M 小，
       * 所以它必须完整待在栏区里（横图可以溢出栏区被窗口裁 —— 那是窗口的事，不是画布的事）。
       */
      if (portrait.insidePane !== true) {
        problems.push(`「适合窗口」应当按最小的那张（竖图）算，它必须整张可见（实测 ${JSON.stringify(portrait)}）`);
      }
      if (Math.abs(landscape.canvas.width - portrait.canvas.width) > 0.5) {
        problems.push("两格的画布必须是同一张（尺寸一致）");
      }
    }
  }

  // ③ 收尾：退出看图 + 把选中缩回一张（后面的单张看图断言不能被对比态干扰）
  await send("Runtime.evaluate", {
    expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }))`,
    returnByValue: true,
  });
  await sleep(500);
  await send("Runtime.evaluate", {
    expression: `(() => {
      const tile = document.querySelector('main [data-virtual-scroller] [role="option"]');
      tile?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
      return true;
    })()`,
    returnByValue: true,
  });
  await sleep(400);

  /*
   * ── 阶段 1 的那条链：双击进看图 → 状态栏 → Tab 四态 → Esc 退回 ──
   *
   * 这一段是对 `plans/M2-W2.md` 1.2/1.3/1.4 的端到端冒烟：
   * 网格 → 看图件 → 外壳四态，三个模块的接线错一处这里就红。
   */
  /*
   * 3.1：标记动作的边界提示 —— 后端说「有 2 张被锁挡住」，界面必须说出来
   *（照片上一个像素都不会变，不说用户完全看不出来）。
   * 顺带：这一步之前 tile 已被点选过（下面那条链会再点一次，无妨）。
   */
  await send("Runtime.evaluate", {
    expression: `(() => {
      const tile = document.querySelector('main [data-virtual-scroller] [role="option"]');
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
      // 人类 2026-09-19：标记态只剩**一个**旗标开关（实心旗），「留下/丢弃」的说法已废
      const flag = document.querySelector('button[aria-label="旗标"]');
      const before = { found: Boolean(flag), disabled: flag ? flag.disabled : null };
      flag?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return before;
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  const clearGate = await send("Runtime.evaluate", {
    expression: `(() => {
      const buttons = [...document.querySelectorAll("button")];
      const clear = buttons.find((b) => b.textContent && b.textContent.includes("清空旗标"));
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
      `「清空旗标」应当是可点的（实测 ${JSON.stringify(gate)}；打旗标按钮=${JSON.stringify(
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
   * 色标：**六色都在**（红黄绿青蓝紫）+ 无色那个空心圈，且青色点得动（人类 2026-09-19 加了青）。
   *
   * 为什么值得断言：色标取值原先散在六处各写一份映射，加一个色漏掉一处
   * 就表现为「某个视图上这颗色标是空的」——而且不报错。
   */
  const colorDots = await send("Runtime.evaluate", {
    expression: `(() => {
      const bar = document.querySelector("[data-toolsbar]");
      const dots = [...(bar?.querySelectorAll("button[aria-label]") ?? [])].filter((b) =>
        ["红色", "黄色", "绿色", "青色", "蓝色", "紫色", "无色"].includes(b.getAttribute("aria-label")),
      );
      const cyan = dots.find((b) => b.getAttribute("aria-label") === "青色");
      return {
        count: dots.length,
        names: dots.map((b) => b.getAttribute("aria-label")),
        cyanDisabled: cyan ? cyan.disabled : null,
      };
    })()`,
    returnByValue: true,
  });
  const colors = colorDots.result?.value ?? {};
  if (colors.count !== 7) {
    problems.push(`工具条上应当有 7 个色标按钮（六色 + 无色），实测 ${JSON.stringify(colors)}`);
  } else if (!colors.names.includes("青色")) {
    problems.push(`色标里缺「青」（实测 ${JSON.stringify(colors.names)}）`);
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
      const tiles = [...document.querySelectorAll('main [data-virtual-scroller] [role="option"]')];
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
      const tiles = [...document.querySelectorAll('main [data-virtual-scroller] [role="option"]')];
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
   * 赞/踩：**重复点同一个 = 取消**（人类 2026-09-20 报「取消不了，右上角提示没有需要改动的照片」）。
   *
   * 根因：`markLike` 无条件是 `value`，第二次仍送同一个值 → 后端判定无改动。
   * 验证方式取最直接的一条：选一张**已经喜欢**的照片（fixture 里 MY002 的 `likeState = like`），
   * 点「喜欢」必须送出 `value = null`（取消）。这样不依赖「打完标再读回来」那条回路
   *（假后端的 `browse_markings` 是固定的空数组，回路本来就闭不上）。
   */
  const likeSetup = await send("Runtime.evaluate", {
    expression: `(() => {
      const tiles = [...document.querySelectorAll('[data-virtual-scroller] [role="option"]')];
      // fixture 里唯一「已喜欢」的就是带标记的那一张（rating 3 + 红标 + likeState=like）
      const target = tiles.find((t) => t.querySelector('[data-tile-bar="marks"] svg') !== null)
        ?? tiles[0]
        ?? null;
      target?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      const button = [...document.querySelectorAll("button")].find(
        (b) => b.getAttribute("aria-label") === "喜欢",
      );
      return {
        tile: target ? (target.innerText || "").trim().slice(0, 12) : null,
        pressed: button?.getAttribute("aria-pressed") ?? null,
        disabled: button ? button.disabled : null,
      };
    })()`,
    returnByValue: true,
  });
  await sleep(350);
  const likeSetupState = likeSetup.result?.value ?? {};
  await send("Runtime.evaluate", {
    expression: `window.__MARK_ARGS = []`,
    returnByValue: true,
  });
  const likeClick = await send("Runtime.evaluate", {
    expression: `(() => {
      const button = [...document.querySelectorAll("button")].find(
        (b) => b.getAttribute("aria-label") === "喜欢",
      );
      button?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return { pressed: button?.getAttribute("aria-pressed") ?? null, disabled: button ? button.disabled : null };
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  const likeArgs = await send("Runtime.evaluate", {
    // 参数形状：`{ repositoryId, ids, action }`，动作在 `action` 里
    expression: `(window.__MARK_ARGS || []).filter((a) => a && a.action && a.action.kind === "like")`,
    returnByValue: true,
  });
  const likes = likeArgs.result?.value ?? [];
  if (likeSetupState.pressed !== "true") {
    problems.push(
      `没选中一张「已喜欢」的照片，取消这条验不了（实测 ${JSON.stringify(likeSetupState)}）`,
    );
  }
  if (likes.length === 0) {
    problems.push(`点「喜欢」应当发一次 browse_mark（实测 ${JSON.stringify(likes)}）`);
  } else if (likes[likes.length - 1].action.value !== null) {
    problems.push(
      `已经喜欢的那张再点「喜欢」必须**取消**（值应当是 null，实测 ${JSON.stringify(likes[likes.length - 1].action)}）`,
    );
  }
  if (likeClick.result?.value?.disabled === true) {
    problems.push("「喜欢」按钮不该是禁用的（有选中就该能点）");
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

  /*
   * 点空白取消选择（人类 2026-09-19 报的判定 bug）：
   * 早先只认「容器本身 / 留白层」，于是一行里**右侧空着的槽位**点不动 ——
   * 那一下命中的是**行元素**，被判成「点在行上」。
   * 这里就按真实位置点：取该行最后一张图右边 8px 的那个点（用 elementFromPoint 取真实命中元素）。
   */
  await send("Runtime.evaluate", {
    expression: `(() => {
      const tile = document.querySelector('main [data-virtual-scroller] [role="option"]');
      tile?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return Boolean(tile);
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  /*
   * 点空白取消选择（人类 2026-09-19 报的判定 bug）……
   *
   * ⚠️ 纵坐标要避开**右上角的 toast 区**：toast 宿主是 `fixed` 的 320 宽一列，
   * 卡片自己 `pointer-events-auto` —— 点在那里会被它吃掉（2026-09-20 真撞上：
   * 删除测试留下的「成功 / 失败」两条 toast 正好盖住那一格）。所以下面
   * 从上往下试几个点，取第一个不被 toast 盖住的。
   */
  const blank = await send("Runtime.evaluate", {
    expression: `(() => {
      const tile = document.querySelector('main [data-virtual-scroller] [role="option"]');
      const row = tile?.closest('[role="option"]')?.parentElement?.parentElement ?? null;
      const inRow = row ? [...row.querySelectorAll('[role="option"]')] : [];
      const last = inRow[inRow.length - 1] ?? null;
      const rect = row ? row.getBoundingClientRect() : null;
      const lastRect = last ? last.getBoundingClientRect() : null;
      if (!row || !rect || !lastRect) return { ok: false };
      const x = Math.min(rect.right - 4, lastRect.right + 8);
      // 避开 toast（右上角 fixed 那一列）
      const candidates = [
        lastRect.top + Math.min(lastRect.height / 2, 10),
        lastRect.bottom - 6,
        lastRect.top + lastRect.height / 2,
      ];
      let el = null;
      let y = 0;
      for (const candidate of candidates) {
        const hit = document.elementFromPoint(x, candidate);
        if (hit && hit.closest("[data-toast-host]") === null) {
          el = hit;
          y = candidate;
          break;
        }
      }
      if (!el) return { ok: false, reason: "elementFromPoint 空" };
      const inTile = el.closest('[role="option"]') !== null;
      const chain = [];
      for (let node = el; node !== null && chain.length < 6; node = node.parentElement) {
        chain.push(node.tagName + (node.className && typeof node.className === "string" ? "." + node.className.split(" ").slice(0, 2).join(".") : ""));
      }
      const scroller = document.querySelector("main [data-virtual-scroller]");
      el.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return {
        ok: true,
        inTile,
        x: Math.round(x),
        y: Math.round(y),
        row: { l: Math.round(rect.left), r: Math.round(rect.right) },
        last: { l: Math.round(lastRect.left), r: Math.round(lastRect.right) },
        scroller: scroller ? { l: Math.round(scroller.getBoundingClientRect().left), r: Math.round(scroller.getBoundingClientRect().right) } : null,
        chain,
        target: el.tagName + (el.getAttribute("role") ? "[" + el.getAttribute("role") + "]" : ""),
      };
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  const blankState = await send("Runtime.evaluate", {
    expression: `(() => {
      const tiles = [...document.querySelectorAll('main [data-virtual-scroller] [role="option"]')];
      return {
        selected: tiles.filter((t) => t.getAttribute("aria-selected") === "true").length,
        status: (document.querySelector("main")?.innerText ?? "").match(/已选 \d+ 张/)?.[0] ?? null,
      };
    })()`,
    returnByValue: true,
  });
  const blankHit = blank.result?.value ?? {};
  const blankAfter = blankState.result?.value ?? {};
  if (blankHit.ok === true && blankHit.inTile === false && blankAfter.selected !== 0) {
    problems.push(
      `点一行里空着的槽位应当取消选择（实测命中的是 ${JSON.stringify(blankHit.target)}，选中仍是 ${blankAfter.selected}；几何 ${JSON.stringify(blankHit)}）`,
    );
  }

  /*
   * tiles 的信息条（人类 2026-09-20 报的两条）：
   *   ① 悬浮/选中时**顶部标记条**与**底部文件名背景条**都没了；
   *   ② 信息档位（含第 1 级）无效 —— 根因是顶部条从来没渲染过（`Tile` 的 `inLibrary()`
   *      一直没人满足：网格没传 `context="library"`）。
   *
   * 现在要守住的口径（人类 2026-09-20 定）：
   *   * **未选中/未悬浮** + 开了信息档位 → 强制显示：**无底纹**、文字加**反色勾边**；
   *   * **选中/悬浮** → 回到标准方案：**半透背景条** + `fg-1`，**不要勾边**；
   *   * `marks` 档只管顶部标记；`marks-name` 档连底部文件名一起强制显示。
   *
   * 量法：`data-tile-bar` 有四个值 —— `marks-forced` / `marks` / `name-forced` / `name`；
   * 用 `opacity` 与 `backgroundColor` / `textShadow` 分辨「哪一层在显示、用的哪套方案」。
   * 内容用「有几个 svg + 无障碍名」判：星标与色标都是图标，`innerText` 本来就是空的。
   */
  /*
   * 这个冒烟跑在**无头 Chrome** 里，它的 `(hover: hover)` 默认是 false（见启动参数的
   * `--blink-settings`）：拿不到鼠标能力时，Tailwind 的 `group-hover:*` 变体整条失效，
   * 后面的 hover 断言会变成假绿/假红 —— 所以先确认环境真的「有鼠标」。
   */
  const hoverMedia = await send("Runtime.evaluate", {
    expression: `matchMedia("(hover: hover)").matches`,
    returnByValue: true,
  });
  if (hoverMedia.result?.value !== true) {
    problems.push(
      `冒烟环境没能模拟出「有鼠标」的媒体特性（hover=${JSON.stringify(hoverMedia.result?.value)}）—— hover 断言不可信`,
    );
  }

  const readBars = `(() => {
    const all = [...document.querySelectorAll('[data-virtual-scroller] [role="option"]')];
    // 找**有标记**的那一格（星标/色标都在标准层里）；找不到就退回第一格
    const tile = all.find((t) => t.querySelector('[data-tile-bar="marks"] svg') !== null) ?? all[0] ?? null;
    if (tile === null) return null;
    const read = (name) => {
      const el = tile.querySelector('[data-tile-bar="' + name + '"]');
      if (el === null) return null;
      const style = getComputedStyle(el);
      return {
        opacity: style.opacity,
        background: style.backgroundColor,
        shadow: style.textShadow,
        svgs: el.querySelectorAll("svg").length,
        aria: el.querySelector("[aria-label]")?.getAttribute("aria-label") ?? null,
      };
    };
    return {
      hasForcedMarks: tile.querySelector('[data-tile-bar="marks-forced"]') !== null,
      hasForcedName: tile.querySelector('[data-tile-bar="name-forced"]') !== null,
      marks: read("marks"),
      name: read("name"),
      forcedMarks: read("marks-forced"),
      forcedName: read("name-forced"),
    };
  })()`;

  const infoBars = await send("Runtime.evaluate", { expression: readBars, returnByValue: true });
  const bars0 = infoBars.result?.value ?? null;
  const transparent = (value) => value === "rgba(0, 0, 0, 0)" || value === "transparent";
  const showsMarks = (bar) => bar !== null && (bar.svgs > 0 || (bar.aria ?? "") !== "");
  if (bars0 === null) {
    problems.push("找不到 tiles 的格子（信息条那几条断言没法量）");
  } else {
    if (bars0.marks === null) {
      problems.push("tiles 顶上那条**标记信息条**不在 DOM 里（人类 2026-09-20 报的第一条）");
    } else {
      // 标准层**始终**带着半透底（只是用 opacity 藏着）—— 「背景条」本身不该丢
      if (transparent(bars0.marks.background)) {
        problems.push(`标准层应当带着半透背景条（实测 ${JSON.stringify(bars0.marks)}）`);
      }
      if (bars0.marks.opacity !== "0") {
        problems.push(`默认（未选中未悬浮、未开信息档位）标准层应当藏着（实测 ${JSON.stringify(bars0.marks)}）`);
      }
      if (bars0.marks.shadow !== "none") {
        problems.push(`默认（未选中未悬浮）不该有勾边（实测 ${JSON.stringify(bars0.marks)}）`);
      }
    }
    if (bars0.name === null) {
      problems.push("tiles 底下那条**文件名条**不在 DOM 里");
    } else if (transparent(bars0.name.background)) {
      problems.push(`文件名条的**半透背景**不该丢（实测 ${JSON.stringify(bars0.name)}）`);
    }
    if (bars0.hasForcedMarks || bars0.hasForcedName) {
      problems.push("没开信息档位时不该有强制显示层");
    }
  }

  /* 第 1 档（`marks`）：未选中 → 顶部标记**强制显示**（无底纹 + 勾边），底部文件名不跟出来 */
  await send("Runtime.evaluate", {
    expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "i", bubbles: true, cancelable: true }))`,
    returnByValue: true,
  });
  await sleep(300);
  const infoMarks = await send("Runtime.evaluate", {
    expression: `(() => {
      const bar = document.querySelector("[data-tiles-control-bar] [data-tile-info]");
      const all = [...document.querySelectorAll('[data-virtual-scroller] [role="option"]')];
      const tile = all.find((t) => t.querySelector('[data-tile-bar="marks"] svg') !== null) ?? all[0] ?? null;
      const forced = tile?.querySelector('[data-tile-bar="marks-forced"]');
      const style = forced ? getComputedStyle(forced) : null;
      return {
        level: bar?.getAttribute("data-tile-info") ?? null,
        forced: forced !== null,
        opacity: style?.opacity ?? null,
        background: style?.backgroundColor ?? null,
        shadow: style?.textShadow ?? null,
        svgs: forced ? forced.querySelectorAll("svg").length : 0,
        aria: forced?.querySelector("[aria-label]")?.getAttribute("aria-label") ?? null,
        hasForcedName: tile?.querySelector('[data-tile-bar="name-forced"]') !== null,
      };
    })()`,
    returnByValue: true,
  });
  const marks1 = infoMarks.result?.value ?? {};
  if (marks1.level !== "marks") {
    problems.push(`按一次 i 应当是「标记」档（实测 ${JSON.stringify(marks1.level)}）`);
  }
  if (marks1.forced !== true || marks1.opacity !== "1") {
    problems.push(`「标记」档下未选中的格子应当强制显示顶部标记（实测 ${JSON.stringify(marks1)}）`);
  } else {
    if (!transparent(marks1.background)) {
      problems.push(`强制显示层不该有底纹（实测 ${JSON.stringify(marks1.background)}）`);
    }
    if (marks1.shadow === "none") {
      problems.push(`强制显示层要有反色勾边（人类说的「加边框」，实测 ${JSON.stringify(marks1)}）`);
    }
    if (!showsMarks(marks1)) {
      problems.push(
        `强制显示的标记条是空的 —— MY002 上有 3 星 + 红标，应当看得见（实测 ${JSON.stringify(marks1)}）`,
      );
    }
  }
  if (marks1.hasForcedName) {
    problems.push("「标记」档不该把底部文件名也强制显示（那是第 2 档的事）");
  }

  /* 指向（真鼠标悬停）：强制层淡出、标准层淡入（回到半透底 + 无勾边） */
  const hoverTarget = await send("Runtime.evaluate", {
    expression: `(() => {
      const all = [...document.querySelectorAll('[data-virtual-scroller] [role="option"]')];
      const tile = all.find((t) => t.querySelector('[data-tile-bar="marks"] svg') !== null) ?? all[0] ?? null;
      const rect = tile?.getBoundingClientRect();
      return rect ? { x: Math.round(rect.left + rect.width / 2), y: Math.round(rect.top + rect.height / 2) } : null;
    })()`,
    returnByValue: true,
  });
  const hoverAt = hoverTarget.result?.value ?? null;
  if (hoverAt === null) {
    problems.push("量不到要悬停的格子");
  } else {
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: hoverAt.x, y: hoverAt.y });
    await sleep(400);
    const hovered = await send("Runtime.evaluate", {
      expression: `(() => {
        const tile = document.querySelector('[data-virtual-scroller] [role="option"]:hover');
        if (tile === null) return { hovered: false };
        const standard = tile.querySelector('[data-tile-bar="marks"]');
        const forced = tile.querySelector('[data-tile-bar="marks-forced"]');
        const style = standard ? getComputedStyle(standard) : null;
        return {
          hovered: true,
          standardOpacity: style?.opacity ?? null,
          standardBackground: style?.backgroundColor ?? null,
          standardShadow: style?.textShadow ?? null,
          forcedOpacity: forced ? getComputedStyle(forced).opacity : null,
        };
      })()`,
      returnByValue: true,
    });
    const hoverState = hovered.result?.value ?? {};
    if (hoverState.hovered !== true) {
      problems.push("鼠标移到格子上之后浏览器并不认为那一格被悬停（环境问题）");
    } else {
      if (hoverState.standardOpacity !== "1") {
        problems.push(`指向时标准信息条要浮出（实测 ${JSON.stringify(hoverState)}）`);
      }
      if (transparent(hoverState.standardBackground ?? "rgba(0, 0, 0, 0)")) {
        problems.push(`指向时标准信息条要有半透底（实测 ${JSON.stringify(hoverState)}）`);
      }
      if (hoverState.standardShadow !== "none") {
        problems.push("有底纹时不该再描边（人类 2026-09-20：有背景条就不需要描边）");
      }
      if (hoverState.forcedOpacity !== "0") {
        problems.push(`指向时强制显示层应当淡出（实测 ${JSON.stringify(hoverState)}）`);
      }
    }
  }

  /* 第 2 档（`marks-name`）：顶 + 底都强制显示；选中之后回到标准方案（两层都带底） */
  // 先把鼠标移开：上一段刚把那格悬停着，悬停时强制层本来就该淡出
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 4, y: 4 });
  await sleep(250);
  await send("Runtime.evaluate", {
    expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "i", bubbles: true, cancelable: true }))`,
    returnByValue: true,
  });
  await sleep(300);
  const infoName = await send("Runtime.evaluate", {
    expression: `(() => {
      const all = [...document.querySelectorAll('[data-virtual-scroller] [role="option"]')];
      const tile = all.find((t) => t.querySelector('[data-tile-bar="marks"] svg') !== null) ?? all[0] ?? null;
      const bar = document.querySelector("[data-tiles-control-bar] [data-tile-info]");
      const name = tile?.querySelector('[data-tile-bar="name-forced"]');
      return {
        level: bar?.getAttribute("data-tile-info") ?? null,
        hasForcedName: name !== null,
        nameOpacity: name ? getComputedStyle(name).opacity : null,
        nameShadow: name ? getComputedStyle(name).textShadow : null,
        nameText: (name?.innerText || "").trim(),
      };
    })()`,
    returnByValue: true,
  });
  const nameState = infoName.result?.value ?? {};
  if (nameState.level !== "marks-name") {
    problems.push(`按两次 i 应当是「标记 + 文件名」档（实测 ${JSON.stringify(nameState.level)}）`);
  }
  if (nameState.hasForcedName !== true || nameState.nameOpacity !== "1") {
    problems.push(`第 2 档下未选中的格子应当把文件名也强制显示（实测 ${JSON.stringify(nameState)}）`);
  } else {
    if (nameState.nameShadow === "none") {
      problems.push(`强制显示的文件名要有勾边（实测 ${JSON.stringify(nameState)}）`);
    }
    if (nameState.nameText === "") {
      problems.push("强制显示的文件名条是空的（应当有 MY002 的文件名）");
    }
  }

  /* 选中：强制层**不渲染**，标准层常亮且带半透底 */
  await send("Runtime.evaluate", {
    expression: `(() => {
      const all = [...document.querySelectorAll('[data-virtual-scroller] [role="option"]')];
      const tile = all.find((t) => t.querySelector('[data-tile-bar="marks"] svg') !== null) ?? all[0] ?? null;
      tile?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return Boolean(tile);
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  const selectedBars = await send("Runtime.evaluate", {
    expression: `(() => {
      const tile = document.querySelector('[data-virtual-scroller] [role="option"][aria-selected="true"]');
      if (tile === null) return null;
      const standard = (name) => {
        const el = tile.querySelector('[data-tile-bar="' + name + '"]');
        return el === null
          ? null
          : {
              opacity: getComputedStyle(el).opacity,
              background: getComputedStyle(el).backgroundColor,
              shadow: getComputedStyle(el).textShadow,
            };
      };
      return {
        forcedMarks: tile.querySelector('[data-tile-bar="marks-forced"]') !== null,
        forcedName: tile.querySelector('[data-tile-bar="name-forced"]') !== null,
        marks: standard("marks"),
        name: standard("name"),
      };
    })()`,
    returnByValue: true,
  });
  const selBars = selectedBars.result?.value ?? null;
  if (selBars === null) {
    problems.push("点一下格子应当选中它（信息条那条断言没得量）");
  } else {
    if (selBars.forcedMarks || selBars.forcedName) {
      problems.push("选中之后不该再有强制显示层（那时走标准方案）");
    }
    if (selBars.marks === null || selBars.marks.opacity !== "1") {
      problems.push(`选中时顶部标记条应当常亮（实测 ${JSON.stringify(selBars.marks)}）`);
    }
    if (selBars.marks !== null && transparent(selBars.marks.background)) {
      problems.push(`选中时顶部条要有半透背景（实测 ${JSON.stringify(selBars.marks)}）`);
    }
    if (selBars.marks !== null && selBars.marks.shadow !== "none") {
      problems.push("选中时有背景条，不该再描边");
    }
    if (selBars.name === null || selBars.name.opacity !== "1") {
      problems.push(`选中时底部文件名条应当常亮（实测 ${JSON.stringify(selBars.name)}）`);
    }
    if (selBars.name !== null && transparent(selBars.name.background)) {
      problems.push(`选中时文件名条要有半透背景（实测 ${JSON.stringify(selBars.name)}）`);
    }
  }

  /*
   * **持久化**（人类 2026-09-20：「按时间」在 import 与 browse 之间都要持久化，
   * 浏览侧以前每次进都重置；信息显示级别也要持久化）。
   *
   * 这条不走「重启应用」（冒烟里做不到），而是查**落盘内容**：设备级偏好写在
   * `localStorage["raybend.display.v2"]`（`lib/display-prefs.ts`），
   * 两个工作区各读自己的分域值 —— 只要 browse 值在里面、且下一次读回来就是它，就算持久化成立
   *（读回来的路径由 `display-prefs.test.ts` 与模块初始化的同步读覆盖）。
   */
  const persisted = await send("Runtime.evaluate", {
    expression: `(() => {
      const byTime = [...document.querySelectorAll("button")].find(
        (b) => b.getAttribute("aria-label") === "按时间",
      );
      byTime?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return Boolean(byTime);
    })()`,
    returnByValue: true,
  });
  if (persisted.result?.value !== true) {
    problems.push("状态条上没找到「按时间」开关（持久化这条验不了）");
  }
  await sleep(400);
  const storedPrefs = await send("Runtime.evaluate", {
    expression: `(() => {
      const raw = localStorage.getItem("raybend.display.v2");
      let parsed = null;
      try { parsed = raw === null ? null : JSON.parse(raw); } catch { parsed = "unparsable"; }
      const button = [...document.querySelectorAll("button")].find(
        (b) => b.getAttribute("aria-label") === "按时间",
      );
      return { raw, parsed, pressed: button?.getAttribute("aria-pressed") ?? null };
    })()`,
    returnByValue: true,
  });
  const prefsState = storedPrefs.result?.value ?? {};
  if (prefsState.pressed !== "true") {
    problems.push(`点「按时间」之后按钮应当是按下态（实测 ${JSON.stringify(prefsState)}）`);
  }
  if (!prefsState.parsed || prefsState.parsed === "unparsable" || prefsState.parsed.browse?.byTime !== true) {
    problems.push(
      `「按时间」必须落盘（键 raybend.display.v2 里 browse.byTime 应当是 true，实测 ${JSON.stringify(prefsState)}）`,
    );
  }
  // 信息档位是 browse 分域里的同一份记录：盘上的值必须等于**界面上当前的档位**（不写死某一档）
  const liveInfoLevel = await send("Runtime.evaluate", {
    expression: `document.querySelector("[data-tiles-control-bar] [data-tile-info]")?.getAttribute("data-tile-info") ?? null`,
    returnByValue: true,
  });
  const liveLevel = liveInfoLevel.result?.value ?? null;
  if (typeof prefsState.parsed?.browse?.infoMode !== "string") {
    problems.push(`显示偏好里缺 infoMode（实测 ${JSON.stringify(prefsState)}）`);
  } else if (liveLevel !== null && prefsState.parsed.browse.infoMode !== liveLevel) {
    problems.push(
      `信息档位应当跟着落盘（界面 ${JSON.stringify(liveLevel)}，盘上 ${JSON.stringify(prefsState.parsed.browse.infoMode)}）`,
    );
  }
  // 收尾：把「按时间」关回去，免得影响后面的分组断言
  await send("Runtime.evaluate", {
    expression: `(() => {
      const byTime = [...document.querySelectorAll("button")].find(
        (b) => b.getAttribute("aria-label") === "按时间",
      );
      if (byTime?.getAttribute("aria-pressed") === "true") {
        byTime.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      }
      return true;
    })()`,
    returnByValue: true,
  });
  await sleep(300);

  /* 收尾：鼠标移开 + 档位转回 `off` + 清掉选中，后面的断言从干净状态开始 */
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 4, y: 4 });
  await send("Runtime.evaluate", {
    expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "i", bubbles: true, cancelable: true }))`,
    returnByValue: true,
  });
  await sleep(200);
  /*
   * 顶部条上的**赞/踩**与**图标描边**（人类 2026-09-20 的两条新要求）。
   *
   * 判据：
   *   * 赞/踩 —— fixture 里那张带标记的照片 `likeState = "like"`，
   *     所以它的顶部条里应当有一个 `tabler-icon-thumb-up-filled` 的 svg；
   *   * 描边 —— 强制层（未选中未悬浮）的图标要带 `.tile-info-icon`（`paint-order: stroke`），
   *     标准层（有半透底）**不带**（有底纹就不需要描边）。
   */
  const marksLevelBefore = await send("Runtime.evaluate", {
    expression: `(() => {
      const button = document.querySelector("[data-tiles-control-bar] [data-tile-info]");
      const before = button?.getAttribute("data-tile-info") ?? null;
      // ① 先把选择清掉：强制显示层只在**未选中**时渲染（选中走标准层）
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
      // ② 档位切到「标记」，强制层才会在顶部条里出现
      if (before !== "marks") {
        window.dispatchEvent(new KeyboardEvent("keydown", { key: "i", bubbles: true, cancelable: true }));
      }
      return before;
    })()`,
    returnByValue: true,
  });
  const levelBefore = marksLevelBefore.result?.value ?? null;
  await sleep(350);
  const marksDetail = await send("Runtime.evaluate", {
    expression: `(() => {
      const all = [...document.querySelectorAll('[data-virtual-scroller] [role="option"]')];
      const tile = all.find((t) => t.querySelector('[data-tile-bar="marks"] svg') !== null) ?? all[0] ?? null;
      if (tile === null) return null;
      const forced = tile.querySelector('[data-tile-bar="marks-forced"]');
      const standard = tile.querySelector('[data-tile-bar="marks"]');
      const classesOf = (bar) =>
        bar === null ? null : [...bar.querySelectorAll("svg")].map((svg) => svg.getAttribute("class") ?? "");
      return {
        likeIcon: (tile.innerHTML || "").includes("thumb-up-filled")
          || (tile.innerHTML || "").includes("thumb-down-filled"),
        forcedIcons: classesOf(forced),
        standardIcons: classesOf(standard),
        standardDot: standard?.querySelector(".tile-info-dot") !== null,
      };
    })()`,
    returnByValue: true,
  });
  const marks = marksDetail.result?.value ?? null;
  if (marks === null) {
    problems.push("量不到带标记的格子（赞踩/描边这两条没法验）");
  } else {
    if (marks.likeIcon !== true) {
      problems.push("顶部信息条上应当显示赞/踩（人类 2026-09-20；fixture 那张是已喜欢）");
    }
    const forcedHasOutline =
      Array.isArray(marks.forcedIcons) &&
      marks.forcedIcons.length > 0 &&
      marks.forcedIcons.every((cls) => String(cls).includes("tile-info-icon"));
    if (!forcedHasOutline) {
      problems.push(
        `强制显示层的图案要带描边（.tile-info-icon，实测 ${JSON.stringify(marks.forcedIcons)}）`,
      );
    }
    const standardHasOutline =
      Array.isArray(marks.standardIcons) &&
      marks.standardIcons.some((cls) => String(cls).includes("tile-info-icon"));
    if (standardHasOutline) {
      problems.push("标准层（有半透底）的图案不该再描边");
    }
  }

  /*
   * `Ctrl/Cmd + A` = **全选**（人类 2026-09-20），而且必须是**整个范围**的全选 ——
   * 「即使未显示的部分也要设置选中状态」。
   */
  await send("Runtime.evaluate", {
    expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }))`,
    returnByValue: true,
  });
  await sleep(250);
  const selectAll = await send("Runtime.evaluate", {
    expression: `(() => {
      const dispatched = window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "a", ctrlKey: true, bubbles: true, cancelable: true }),
      );
      const tiles = [...document.querySelectorAll('[data-virtual-scroller] [role="option"]')];
      const selected = tiles.filter((t) => t.getAttribute("aria-selected") === "true").length;
      const status = (document.querySelector("main")?.innerText ?? "").match(/已选 (\d+) 张/)?.[1] ?? null;
      return {
        notPrevented: dispatched,
        rendered: tiles.length,
        renderedSelected: selected,
        selectedTotal: status === null ? null : Number(status),
        textSelection: String(window.getSelection()?.toString() ?? "").length,
      };
    })()`,
    returnByValue: true,
  });
  const selectState = selectAll.result?.value ?? {};
  if (selectState.notPrevented !== false) {
    problems.push("Ctrl+A 必须 preventDefault（否则浏览器会把整页文字选中变蓝）");
  }
  if (selectState.textSelection !== 0) {
    problems.push(`Ctrl+A 之后不该有文本选择（实测 ${JSON.stringify(selectState.textSelection)} 字符）`);
  }
  if (!(Number(selectState.renderedSelected) > 0)) {
    problems.push(`Ctrl+A 应当选中（实测 ${JSON.stringify(selectState)}）`);
  }
  if (selectState.renderedSelected !== selectState.rendered) {
    problems.push(
      `Ctrl+A 之后**当前渲染出来的每一格**都该是选中态（实测 ${JSON.stringify(selectState)}）`,
    );
  }
  /*
   * 「未显示的部分也要选中」这条在**冒烟里量不了**（fixture 只有 6 张、全都渲染出来了），
   * 所以它由单测钉住：`features/browse/store.test.ts` 的
   * 「selectAll 覆盖整个 scope（时间线全量），不只是已加载的页」。
   */
  // 收尾：清掉选择 + 把档位还原成进来时的样子（后面那段 cleanup 依赖它）
  await send("Runtime.evaluate", {
    expression: `(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
      /*
       * 档位是**三态循环**（off → marks → marks-name → off），所以「还原」要按到位为止，
       * 不能只按一次（按一次只是往前推一格）。
       */
      const want = ${JSON.stringify(levelBefore)};
      for (let i = 0; i < 4; i += 1) {
        const level = document.querySelector("[data-tiles-control-bar] [data-tile-info]")?.getAttribute("data-tile-info");
        if (level === want) break;
        window.dispatchEvent(new KeyboardEvent("keydown", { key: "i", bubbles: true, cancelable: true }));
      }
      return document.querySelector("[data-tiles-control-bar] [data-tile-info]")?.getAttribute("data-tile-info") ?? null;
    })()`,
    returnByValue: true,
  });
  await sleep(300);

  const infoOff = await send("Runtime.evaluate", {
    expression: `(() => {
      const bar = document.querySelector("[data-tiles-control-bar] [data-tile-info]");
      return {
        level: bar?.getAttribute("data-tile-info") ?? null,
        forced: document.querySelector('[data-tile-bar="marks-forced"], [data-tile-bar="name-forced"]') !== null,
      };
    })()`,
    returnByValue: true,
  });
  const infoOffState = infoOff.result?.value ?? {};
  if (infoOffState.level !== "off") {
    problems.push(`再按一次 i 应当回到「无」档（实测 ${JSON.stringify(infoOffState.level)}）`);
  }
  if (infoOffState.forced) {
    problems.push("回到「无」档后不该还有强制显示层");
  }
  await send("Runtime.evaluate", {
    expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }))`,
    returnByValue: true,
  });
  await sleep(200);

  // 再选中一张，供后面的筛选断言用（删除把选中清掉了）
  await send("Runtime.evaluate", {
    expression: `(() => {
      const tile = document.querySelector('main [data-virtual-scroller] [role="option"]');
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

  /*
   * 人类 2026-09-19 定的新语义（筛选态）：
   *   ① 工具条上的标记**不再跟随选中的照片**，它就是「筛选条件」本身 ——
   *      选另一张照片不会再改条件；② 旗标区是**两个**按钮「有旗标 / 无旗标」；
   *   ③ 星标是**阈值**语义（chip 文案写 `≥N 星`）。
   */
  const filterToolbar = await send("Runtime.evaluate", {
    expression: `(() => {
      const byLabel = (text) => document.querySelector('button[aria-label="' + text + '"]');
      const chipsBefore = document.querySelectorAll("[data-filter-chip]").length;
      // 换个「选中」：点一个别的 tile（挑评分不同的那张）
      const tiles = [...document.querySelectorAll('main [data-virtual-scroller] [role="option"]')];
      const other = tiles.find((tile) => tile.getAttribute("aria-selected") !== "true") ?? tiles[0];
      other?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return {
        chipsBefore,
        withFlag: Boolean(byLabel("有旗标")),
        withoutFlag: Boolean(byLabel("无旗标")),
        legacyKeep: Boolean(byLabel("留下")),
        legacyDrop: Boolean(byLabel("丢弃")),
      };
    })()`,
    returnByValue: true,
  });
  await sleep(600);
  const afterSelect = await send("Runtime.evaluate", {
    expression: `(() => ({
      chips: document.querySelectorAll("[data-filter-chip]").length,
      count: document.querySelector('[data-filter-count]')?.textContent ?? "",
    }))()`,
    returnByValue: true,
  });
  const ft = filterToolbar.result?.value ?? {};
  const fa = afterSelect.result?.value ?? {};
  if (ft.withFlag !== true || ft.withoutFlag !== true) {
    problems.push(`筛选态旗标区应当是「有旗标 / 无旗标」两个按钮（实测 ${JSON.stringify(ft)}）`);
  }
  if (ft.legacyKeep === true || ft.legacyDrop === true) {
    problems.push("「留下 / 丢弃」的说法已废（人类 2026-09-19），工具条上不该再有这两个按钮");
  }
  if (fa.chips !== ft.chipsBefore) {
    problems.push(
      `筛选态下改「选中」不该动筛选条件（前 ${ft.chipsBefore} 条 → 后 ${fa.chips} 条）`,
    );
  }

  // 星标是阈值：chip 文案必须写「≥」（选中 3 星 ⇒ 4/5 星也要出来）
  const ratingChip = await send("Runtime.evaluate", {
    expression: `(() => {
      const chip = [...document.querySelectorAll("[data-filter-chip]")]
        .find((c) => (c.getAttribute("data-filter-chip") ?? "").startsWith("rating:"));
      return chip ? chip.textContent.trim() : null;
    })()`,
    returnByValue: true,
  });
  const ratingText = ratingChip.result?.value;
  if (typeof ratingText === "string" && !ratingText.includes("≥")) {
    problems.push(`星标筛选是阈值语义，chip 应当写「≥N 星」（实测 ${JSON.stringify(ratingText)}）`);
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

  /*
   * 筛选态下控件要显示**条件本身**（人类 2026-09-20 报「赞和踩被选中时不显示状态」）。
   *
   * 根因：工具条的按下态以前只读「选中照片的三态」，而筛选条件一变 `store.reload()`
   * 就会清空选中 ⇒ 永远显示无值。色标 / 锁早就是读 `store.filter()` 的口径，
   * 星标与赞踩是漏掉的两处（这次一起修）。
   *
   * 此刻的条件：色标红 + 喜欢（星标那条 chip 已被上一步摘掉）。
   *
   * 先把「选中」清掉再读：不然旧代码可能**碰巧**读出一张恰好是「喜欢」的照片
   * （前面的步骤刚点过一张），让这条断言假绿 —— 筛选态本来就不该看选中。
   */
  await send("Runtime.evaluate", {
    expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }))`,
    returnByValue: true,
  });
  await sleep(300);
  const filterPressed = await send("Runtime.evaluate", {
    expression: `(() => {
      const byLabel = (text) => document.querySelector('button[aria-label="' + text + '"]');
      const pressed = (b) => (b ? b.getAttribute("aria-pressed") : null);
      return {
        chips: document.querySelectorAll("[data-filter-chip]").length,
        like: pressed(byLabel("喜欢")),
        dislike: pressed(byLabel("不喜欢")),
      };
    })()`,
    returnByValue: true,
  });
  const fp = filterPressed.result?.value ?? {};
  if (fp.chips < 2) {
    problems.push(`筛选态按下态这条没复现（此刻应当还有两条条件，实测 ${JSON.stringify(fp)}）`);
  }
  if (fp.like !== "true") {
    problems.push(
      `筛选条件里有「喜欢」时，喜欢按钮必须是按下态（实测 ${JSON.stringify(fp)}）`,
    );
  }
  if (fp.dislike === "true") {
    problems.push("没筛「不喜欢」时它不该是按下态");
  }

  /** 读两个赞踩按钮的按下态（这段要读很多次，写成一处） */
  const readLikeButtons = async () => {
    const r = await send("Runtime.evaluate", {
      expression: `(() => {
        const byLabel = (text) => document.querySelector('button[aria-label="' + text + '"]');
        const pressed = (b) => (b ? b.getAttribute("aria-pressed") : null);
        return { like: pressed(byLabel("喜欢")), dislike: pressed(byLabel("不喜欢")) };
      })()`,
      returnByValue: true,
    });
    return r.result?.value ?? {};
  };

  // 点「不喜欢」→ 条件换成它（互斥），按下态跟着换；再点一次 = 取消
  await send("Runtime.evaluate", {
    expression: `(() => {
      const byLabel = (text) => document.querySelector('button[aria-label="' + text + '"]');
      byLabel("不喜欢")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return true;
    })()`,
    returnByValue: true,
  });
  await sleep(500);
  const afterDislike = await readLikeButtons();
  if (afterDislike.dislike !== "true" || afterDislike.like === "true") {
    problems.push(`筛选态点「不喜欢」应当把条件换成它（实测 ${JSON.stringify(afterDislike)}）`);
  }
  await send("Runtime.evaluate", {
    expression: `(() => {
      const byLabel = (text) => document.querySelector('button[aria-label="' + text + '"]');
      byLabel("不喜欢")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return true;
    })()`,
    returnByValue: true,
  });
  await sleep(500);
  const afterDislikeOff = await readLikeButtons();
  if (afterDislikeOff.dislike === "true" || afterDislikeOff.like === "true") {
    problems.push(
      `筛选态再点一次已选中的「不喜欢」应当取消条件（实测 ${JSON.stringify(afterDislikeOff)}）`,
    );
  }

  // 星标阈值：此刻没有星级条件 ⇒ 点第 3 颗 = 「≥3 星」，1..3 颗点亮、4/5 不亮
  await send("Runtime.evaluate", {
    expression: `(() => {
      const b = document.querySelector('button[aria-label="3 星"]');
      b?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return Boolean(b);
    })()`,
    returnByValue: true,
  });
  await sleep(500);
  const starThreshold = await send("Runtime.evaluate", {
    expression: `(() => {
      const on = (n) => {
        const b = document.querySelector('button[aria-label="' + n + ' 星"]');
        return b === null ? null : (b.innerHTML || "").includes("star-filled");
      };
      return { s1: on(1), s2: on(2), s3: on(3), s4: on(4), s5: on(5) };
    })()`,
    returnByValue: true,
  });
  const st = starThreshold.result?.value ?? {};
  if (st.s1 !== true || st.s2 !== true || st.s3 !== true || st.s4 === true || st.s5 === true) {
    problems.push(
      `筛选态星标是阈值（≥3 星 ⇒ 1..3 颗点亮、4/5 不亮，实测 ${JSON.stringify(st)}）`,
    );
  }
  // 收尾：再点一次第 3 颗取消阈值，免得给后面的断言留条件
  await send("Runtime.evaluate", {
    expression: `(() => {
      const b = document.querySelector('button[aria-label="3 星"]');
      b?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return true;
    })()`,
    returnByValue: true,
  });
  await sleep(400);

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
      const tile = document.querySelector('main [data-virtual-scroller] [role="option"]');
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
        const tile = document.querySelector('main [data-virtual-scroller] [role="option"]');
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
        status: Boolean(document.querySelector('main [data-tiles-control-bar][data-tiles-bar-mode="view"]')),
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
        tiles: document.querySelectorAll('main [data-virtual-scroller] [role="option"]').length,
        selected: document.querySelectorAll('main [data-virtual-scroller] [role="option"][aria-selected="true"]').length,
        mainText: (document.querySelector("main")?.innerText ?? "").slice(0, 80),
        anyViewer: document.querySelectorAll("[data-viewer]").length,
        mainTail: (document.querySelector("main")?.outerHTML ?? "").slice(-120),
        log: (window.__INVOKE_LOG || []).slice(-8),
      }))()`,
      returnByValue: true,
    });
    problems.push(`双击之后看图件没打开（现场：${JSON.stringify(why.result?.value)}）`);
  }
  if (shown.status !== true) {
    problems.push(
      "看图态中列底部状态栏没出现（main 里的 [data-tiles-bar-mode=\"view\"] 不在 —— 看图那条应当就是 tiles 那条）",
    );
  }

  /*
   * 结构红线（人类 2026-09-20）：**workspace 只有纵向分列，没有跨列行，每一列各自到底**。
   *
   * 以前看图那条信息条是全宽、在三列下面（`ViewerStatusBar`）—— 既跨列又与 tiles 状态栏
   * 重复。现在它必须是**中列（`main`）自己的最后一段**：横向不跨出 main，纵向在胶片带下面、
   * 贴着 main 的底边。
   */
  const barGeometry = await send("Runtime.evaluate", {
    expression: `(() => {
      const main = document.querySelector("main[data-chrome]");
      const bar = document.querySelector('main [data-tiles-control-bar][data-tiles-bar-mode="view"]');
      const strip = document.querySelector('[data-filmstrip="open"]');
      if (!main || !bar) return { main: Boolean(main), bar: Boolean(bar) };
      const mb = main.getBoundingClientRect();
      const bb = bar.getBoundingClientRect();
      const sb = strip ? strip.getBoundingClientRect() : null;
      return {
        mode: bar.getAttribute("data-tiles-bar-mode"),
        insideMain: bar.closest("main") !== null,
        leftInside: bb.left >= mb.left - 1,
        rightInside: bb.right <= mb.right + 1,
        bottomFlush: Math.abs(bb.bottom - mb.bottom) <= 1,
        belowStrip: sb === null ? null : bb.top >= sb.bottom - 1,
      };
    })()`,
    returnByValue: true,
  });
  const barGeo = barGeometry.result?.value ?? {};
  if (barGeo.insideMain !== true || barGeo.leftInside !== true || barGeo.rightInside !== true) {
    problems.push(
      `看图状态栏必须在**中列之内**、不跨列（实测 ${JSON.stringify(barGeo)}）—— workspace 没有跨列行`,
    );
  }
  if (barGeo.bottomFlush !== true) {
    problems.push(`看图状态栏应当是中列最底那一格（实测 ${JSON.stringify(barGeo)}）`);
  }
  if (barGeo.belowStrip === false) {
    problems.push(`看图状态栏要排在胶片带**下面**（实测 ${JSON.stringify(barGeo)}）`);
  }

  /*
   * 胶片带**装得下就居中**（人类 2026-09-20）：内层那一行是 `w-max mx-auto`。
   * fixture 里只有 6 张（远窄于窗口）⇒ 第一张的左边缘应当明显离开胶片带左边缘。
   */
  const stripCentered = await send("Runtime.evaluate", {
    expression: `(() => {
      const strip = document.querySelector('[data-filmstrip="open"]');
      const first = strip?.querySelector("[data-strip-item]");
      const row = first?.parentElement ?? null;
      if (!strip || !first || !row) return null;
      const sb = strip.getBoundingClientRect();
      const rb = row.getBoundingClientRect();
      const fb = first.getBoundingClientRect();
      return {
        scrollWidth: strip.scrollWidth,
        clientWidth: strip.clientWidth,
        leftGap: Math.round(rb.left - sb.left),
        firstGap: Math.round(fb.left - sb.left),
        rowWidth: Math.round(rb.width),
      };
    })()`,
    returnByValue: true,
  });
  const stripFit = stripCentered.result?.value ?? null;
  if (stripFit === null) {
    problems.push("量不到胶片带的内层行（居中这条验不了）");
  } else if (stripFit.scrollWidth <= stripFit.clientWidth && stripFit.leftGap <= 4) {
    problems.push(
      `胶片带装得下时应当居中（实测 ${JSON.stringify(stripFit)}）—— 内层行是 w-max mx-auto`,
    );
  }

  /*
   * 胶片带 2026-09-20 的三条硬规矩（人类）：
   *   ① 默认 tile 偏小（实际 158×132），上下各 8px，因此整条 148px；
   *      **没有原生横向滚动条**（`scrollbar-width: none`）——
 *      它时有时无会把缩略图挤得忽大忽小；
   *   ② Ctrl + 滚轮走 17 档隐藏缩放，默认第 4 档，向上滚一格到第 5 档；
   *   ③ 普通滚轮要能横向滚，位置由 2px 的 `SubtleScrollbar` 指示（左滚到头宽 0、
 *      右滚到头占满全宽）。装得下时验不了滚动，这里把视口临时压窄来验。
   */
  const stripChrome = await send("Runtime.evaluate", {
    expression: `(() => {
      const strip = document.querySelector('[data-filmstrip="open"]');
      if (!strip) return null;
      const first = strip.querySelector("[data-strip-item]");
      return {
        height: Math.round(strip.getBoundingClientRect().height),
        step: strip.getAttribute("data-filmstrip-step"),
        tileWidth: first === null ? null : Math.round(first.getBoundingClientRect().width),
        tileHeight: first === null ? null : Math.round(first.getBoundingClientRect().height),
        scrollbarWidth: getComputedStyle(strip).scrollbarWidth,
        indicator: Boolean(document.querySelector("[data-subtle-scrollbar]")),
      };
    })()`,
    returnByValue: true,
  });
  const stripChromeState = stripChrome.result?.value ?? null;
  if (stripChromeState === null) {
    problems.push("量不到胶片带（尺寸/滚动条这条验不了）");
  } else {
    if (
      stripChromeState.height !== 148 ||
      stripChromeState.step !== "4" ||
      stripChromeState.tileWidth !== 158 ||
      stripChromeState.tileHeight !== 132
    ) {
      problems.push(
        `胶片带默认应当是第 4 档、tile 158×132、总高 148，实测 ${JSON.stringify(stripChromeState)}`,
      );
    }
    if (stripChromeState.scrollbarWidth !== "none") {
      problems.push(
        `胶片带不该有原生滚动条（scrollbar-width 应为 none），实测 ${JSON.stringify(stripChromeState.scrollbarWidth)}`,
      );
    }
    if (stripChromeState.indicator !== true) {
      problems.push("胶片带底部应有 2px 无感滚动条（[data-subtle-scrollbar] 不在）");
    }
  }
  const stripZoom = await send("Runtime.evaluate", {
    expression: `(() => {
      const strip = document.querySelector('[data-filmstrip="open"]');
      if (!strip) return false;
      strip.dispatchEvent(new WheelEvent("wheel", {
        deltaY: -120,
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }));
      return true;
    })()`,
    returnByValue: true,
  });
  await sleep(300);
  const stripZoomed = await send("Runtime.evaluate", {
    expression: `(() => {
      const strip = document.querySelector('[data-filmstrip="open"]');
      const first = strip?.querySelector("[data-strip-item]") ?? null;
      if (!strip) return null;
      const state = {
        step: strip.getAttribute("data-filmstrip-step"),
        height: Math.round(strip.getBoundingClientRect().height),
        tileWidth: first === null ? null : Math.round(first.getBoundingClientRect().width),
        tileHeight: first === null ? null : Math.round(first.getBoundingClientRect().height),
      };
      return state;
    })()`,
    returnByValue: true,
  });
  const stripZoomState = stripZoomed.result?.value ?? null;
  if (stripZoom.result?.value !== true || stripZoomState === null) {
    problems.push("量不到胶片带 Ctrl + 滚轮隐藏缩放");
  } else if (
    stripZoomState.step !== "5" ||
    stripZoomState.height !== 157 ||
    stripZoomState.tileWidth !== 169 ||
    stripZoomState.tileHeight !== 141
  ) {
    problems.push(
      `胶片带 Ctrl + 向上滚一格应到第 5 档、tile 169×141、总高 157，实测 ${JSON.stringify(stripZoomState)}`,
    );
  }
  // 变化后 2 秒才写：模拟 Tauri 环境直接检查 setting_set 的参数（真实环境落 app.db）。
  await sleep(2200);
  const persistedFilmStep = await send("Runtime.evaluate", {
    expression: `(() => ({ calls: (window.__SETTING_CALLS || []).slice() }))()`,
    returnByValue: true,
  });
  const persistedFilm = persistedFilmStep.result?.value ?? {};
  const browseFilmWrites = (persistedFilm.calls ?? []).filter(
    (call) => call?.key === "filmstrip.browse_tile_step",
  );
  const importFilmWrites = (persistedFilm.calls ?? []).filter(
    (call) => call?.key === "filmstrip.import_tile_step",
  );
  if (browseFilmWrites.length !== 1 || browseFilmWrites[0]?.value !== "5" || importFilmWrites.length !== 0) {
    problems.push(`browse 胶片带档位应在静止 2 秒后独立落盘为 5，实测 ${JSON.stringify(persistedFilm)}`);
  }
  await send("Runtime.evaluate", {
    expression: `(() => {
      const strip = document.querySelector('[data-filmstrip="open"]');
      strip?.dispatchEvent(new WheelEvent("wheel", {
        deltaY: 120,
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }));
      return true;
    })()`,
    returnByValue: true,
  });
  await sleep(2200);
  await send("Emulation.setDeviceMetricsOverride", { width: 620, height: 900, deviceScaleFactor: 1, mobile: false });
  await sleep(500);
  await send("Runtime.evaluate", {
    expression: `(() => {
      const strip = document.querySelector('[data-filmstrip="open"]');
      strip?.dispatchEvent(new WheelEvent("wheel", { deltaY: 240, bubbles: true, cancelable: true }));
      return true;
    })()`,
    returnByValue: true,
  });
  await sleep(300);
  const stripScrollState = await send("Runtime.evaluate", {
    expression: `(() => {
      const strip = document.querySelector('[data-filmstrip="open"]');
      const bar = document.querySelector("[data-subtle-scrollbar] > div");
      if (!strip) return null;
      const max = strip.scrollWidth - strip.clientWidth;
      const left = strip.scrollLeft;
      return { max: Math.round(max), left: Math.round(left), barWidth: bar ? bar.style.width : null };
    })()`,
    returnByValue: true,
  });
  await send("Emulation.clearDeviceMetricsOverride");
  await sleep(500);
  const stripScroll = stripScrollState.result?.value ?? null;
  if (stripScroll === null || stripScroll.max <= 1) {
    problems.push("压窄视口后胶片带应当溢出（滚轮这条验不了）");
  } else {
    if (stripScroll.left <= 0) {
      problems.push(`纵向滚轮必须能横向滚动胶片带（实测 left=${stripScroll.left}）`);
    }
    const ratio = stripScroll.barWidth === null ? null : Number.parseFloat(stripScroll.barWidth);
    if (ratio === null || Math.abs(ratio - (stripScroll.left / stripScroll.max) * 100) > 1) {
      problems.push(`无感滚动条应当指示位置（实测 ${JSON.stringify(stripScroll)}）`);
    }
  }
  if (shown.readout !== true) problems.push("右栏没换成预览 + 直方图（[data-viewer-readout=\"open\"] 不在）");
  /*
   * 预览框是**固定 4:3**（人类 2026-09-20：「比例改成 4:3，不要 3:2，这样对纵图支持更好」）。
   * 量外框的宽高比，容差 2%（子像素与内边距）。
   */
  const previewFrame = await send("Runtime.evaluate", {
    expression: `(() => {
      const section = document.querySelector('[data-viewer-readout="open"]');
      const frame = section?.querySelector("div");
      if (!frame) return null;
      const r = frame.getBoundingClientRect();
      return { w: Math.round(r.width), h: Math.round(r.height), ratio: r.height === 0 ? 0 : r.width / r.height };
    })()`,
    returnByValue: true,
  });
  const frame = previewFrame.result?.value ?? null;
  if (frame === null) {
    problems.push("量不到右栏预览框（4:3 这条验不了）");
  } else if (Math.abs(frame.ratio - 4 / 3) > 0.03) {
    problems.push(
      `右栏预览框应当是 4:3（实测 ${JSON.stringify(frame)}）—— 不要 3:2，也不要跟着照片比例走`,
    );
  }
  // 2026-09-20 起直方图是**逐点填充折线**（加色分层），不再做三次曲线拟合
  if (shown.histogram !== "lines") {
    problems.push(`直方图没画出折线（data-histogram=${JSON.stringify(shown.histogram)}）`);
  }
  if (shown.strip !== 6) {
    problems.push(`胶片带应当有 6 张缩略（实测 ${JSON.stringify(shown.strip)}）`);
  }
  if (shown.stripCurrent !== "0") {
    problems.push(`刚进看图时当前那张应当是第 1 张（实测 ${JSON.stringify(shown.stripCurrent)}）`);
  }

  /*
   * 直方图：**三条完整通道填充 path + 背景 4 根等分虚线**。
   *
   * 为什么要断言到这种程度：旧的七块硬切会在亚像素交界处留白；现在由浏览器把三条
   * 完整曲线叠加，既不留几何缝隙，也能在实时输入时只更新三条 path。
   */
  const histogramShape = await send("Runtime.evaluate", {
    expression: `(() => {
      const host = document.querySelector('[data-histogram="lines"]');
      if (host === null) return null;
      const paths = [...host.querySelectorAll("path")];
      const gridLines = [...host.querySelectorAll("span")].filter((el) =>
        (el.getAttribute("class") ?? "").includes("--hist-grid"),
      );
      return {
        paths: paths.length,
        channels: paths.filter((p) => (p.getAttribute("class") ?? "").includes("histogram-channel")).length,
        channelButtons: host.querySelectorAll('button[aria-pressed]').length,
        channel: host.getAttribute("data-channel"),
        straight: paths.every((p) => !(p.getAttribute("d") ?? "").includes(" C")),
        gridLines: gridLines.length,
      };
    })()`,
    returnByValue: true,
  });
  const hist = histogramShape.result?.value ?? null;
  if (hist === null) {
    problems.push("看图右栏里没有画出来的直方图（[data-histogram=\"lines\"] 不在）");
  } else {
    if (hist.paths !== 3 || hist.channels !== 3) {
      problems.push(`直方图应当只有三条完整 RGB 通道 path，实测 ${JSON.stringify(hist)}`);
    }
    if (hist.channelButtons !== 3 || hist.channel !== "all") {
      problems.push(`直方图默认应当全通道显示，并内置 R/G/B 三个互斥按钮，实测 ${JSON.stringify(hist)}`);
    }
    if (!hist.straight) problems.push("直方图必须逐点直连，不能再出现三次曲线段");
    if (hist.gridLines !== 4) {
      problems.push(`背景等分虚线应当是 4 根（纵 3 + 横 1），实测 ${JSON.stringify(hist.gridLines)}`);
    }
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
      status: document.querySelector('main [data-tiles-control-bar][data-tiles-bar-mode="view"]')?.innerText ?? "",
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
  if (shown.chrome !== "default") problems.push(`进看图时四态应当从默认开始，实测 ${shown.chrome}`);

  /* Tab：①默认 → ②只关左 → ③关两侧 → ④仅 view → ① */
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
    expression: `(() => {
      const frames = [...document.querySelectorAll("[data-compare-frame]")];
      const sources = frames.map((frame) => frame.querySelector("img")?.getAttribute("src") ?? null);
      /*
       * **虚拟画布**（人类 2026-09-20）：所有格子里的画布必须一模一样（尺寸 + 变换），
       * 差别只在里面放的是哪张图。画布比例 = 最大宽 / 最大高，与单张图的比例无关。
       */
      const canvases = frames.map((frame) => {
        const canvasEl = frame.querySelector("[data-compare-canvas]");
        const rect = canvasEl?.getBoundingClientRect();
        return rect ? { width: rect.width, height: rect.height } : null;
      });
      const images = frames.map((frame) => {
        const canvasEl = frame.querySelector("[data-compare-canvas]");
        const img = frame.querySelector("img");
        const canvasRect = canvasEl?.getBoundingClientRect();
        const imgRect = img?.getBoundingClientRect();
        return canvasRect && imgRect
          ? {
              width: imgRect.width / canvasRect.width,
              height: imgRect.height / canvasRect.height,
              offsetX: (imgRect.left - canvasRect.left) / canvasRect.width,
              offsetY: (imgRect.top - canvasRect.top) / canvasRect.height,
            }
          : null;
      });
      return {
        compare: Boolean(document.querySelector('[data-compare="open"]')),
        frames: frames.length,
        images: sources.filter(Boolean).length,
        uniqueSources: new Set(sources.filter(Boolean)).size,
        canvases,
        images2: images,
        viewer: Boolean(document.querySelector('[data-viewer="open"]')),
      };
    })()`,
    returnByValue: true,
  });
  const compareState = compareOn.result?.value ?? {};
  if (compareState.compare !== true) {
    problems.push(`Ctrl 多选之后应当自然进入对比（实测 ${JSON.stringify(compareState)}）`);
  } else {
    if (compareState.frames !== 2) {
      problems.push(`对比应当有 2 幅画幅（实测 ${JSON.stringify(compareState.frames)}）`);
    }
    if (compareState.images !== 2 || compareState.uniqueSources !== 2) {
      problems.push(`对比的每幅画面必须载入自己的图（实测 ${JSON.stringify(compareState)}）`);
    }
    const canvasSizes = compareState.canvases ?? [];
    if (
      canvasSizes.length !== 2 ||
      canvasSizes.some(
        (size) =>
          size === null ||
          Math.abs(size.width - canvasSizes[0].width) > 0.5 ||
          Math.abs(size.height - canvasSizes[0].height) > 0.5,
      )
    ) {
      problems.push(
        `每格的虚拟画布必须一样大（同一张画布的几个副本），实测 ${JSON.stringify(canvasSizes)}`,
      );
    }
    if (compareState.viewer !== false) {
      problems.push("对比态下不该同时出现单张看图件");
    }
  }

  /*
   * 2026-09-20 的新口径（人类）：进入对比**不自动**切「只看对比图」——
   * 胶片带仍显示全目录，好继续选第三张；再按一次回车才切（下面的 2.4 那段验的就是切过去）。
   */
  const stripOnEnter = await send("Runtime.evaluate", {
    expression: `document.querySelector("[data-filmstrip]")?.getAttribute("data-strip-mode") ?? null`,
    returnByValue: true,
  });
  if (stripOnEnter.result?.value !== "all") {
    problems.push(
      `进入对比时胶片带应当仍是全目录（不该自动切「只看对比图」），实测 ${JSON.stringify(stripOnEnter.result?.value)}`,
    );
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
      status: document.querySelector('main [data-tiles-control-bar][data-tiles-bar-mode="view"]')?.innerText ?? "",
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
    { chrome: "film-right", sides: 1, film: "on", strip: true },
    { chrome: "film-only", sides: 0, film: "on", strip: true },
    { chrome: "view-only", sides: 0, film: "off", strip: false },
    { chrome: "default", sides: 2, film: "on", strip: true },
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
   * 对比：**栏区是窗口 + 一张虚拟画布**（人类 2026-09-20 的新方案）。
   *
   * 要守住的三件事：
   *   1. 栏区是可见边界（`overflow: hidden`），不是「图片自己的小盒子」；
   *   2. 画布**不裁**任何一张图：每张图按原尺寸居中贴进画布（小的图两头留空）；
   *   3. **双击 = 适合窗口 ↔ 100%**（100% 时画布就是 1:1 的原图像素）。
   *
   * 这一组对比的两张图 fixture 里都是 4000×3000（同尺寸），所以画布 = 4000×3000，
   * 两张图都应当铺满画布（100% / 100%）—— 混合比例那组在 tiles 进对比那一段量。
   */
  const compareWindow = await send("Runtime.evaluate", {
    expression: `(() => {
      const pane = document.querySelector('[data-compare-frame="0"]');
      const canvas = pane?.querySelector("[data-compare-canvas]");
      const paneRect = pane?.getBoundingClientRect();
      const canvasRect = canvas?.getBoundingClientRect();
      const host = document.querySelector('[data-compare="open"]');
      const zoom = Number(host?.getAttribute("data-compare-zoom") ?? "NaN");
      const fills = [...document.querySelectorAll("[data-compare-frame]")].map((frame) => {
        const canvasEl = frame.querySelector("[data-compare-canvas]");
        const img = frame.querySelector("img");
        const canvasBox = canvasEl?.getBoundingClientRect();
        const imgBox = img?.getBoundingClientRect();
        return canvasBox && imgBox
          ? {
              width: imgBox.width / canvasBox.width,
              height: imgBox.height / canvasBox.height,
              inside:
                imgBox.width <= canvasBox.width + 0.5 && imgBox.height <= canvasBox.height + 0.5,
            }
          : null;
      });
      const clip = pane ? getComputedStyle(pane).overflow : null;
      return {
        pane: paneRect ? { width: paneRect.width, height: paneRect.height } : null,
        canvas: canvasRect ? { width: canvasRect.width, height: canvasRect.height } : null,
        zoom,
        fills,
        clip,
      };
    })()`,
    returnByValue: true,
  });
  const fitWindow = compareWindow.result?.value ?? {};
  if (fitWindow.clip !== "hidden") {
    problems.push(`栏区必须是可见边界（overflow: hidden），实测 ${JSON.stringify(fitWindow.clip)}`);
  }
  if (
    fitWindow.pane === null ||
    fitWindow.canvas === null ||
    fitWindow.canvas.width > fitWindow.pane.width + 0.5 ||
    fitWindow.canvas.height > fitWindow.pane.height + 0.5
  ) {
    problems.push(`「适合窗口」时画布应当落在栏区之内（实测 ${JSON.stringify(fitWindow)}）`);
  }
  /*
   * 画布不许裁任何一张图（`inside`），也不能把图拉出画布（`fill ≤ 1`）。
   * 这一组里都是 4000×3000 的图，所以每张都应当**正好铺满**画布。
   *
   * 只统计**已经出图**的格子：取图是异步的，某一格晚一拍不该算失败
   * （「每格都有图」那条已经在前面单独断言过了）。
   */
  const fills = (fitWindow.fills ?? []).filter((fill) => fill !== null);
  if (fills.length === 0) {
    problems.push(`量不到任何画幅的图片占位（实测 ${JSON.stringify(fitWindow.fills)}）`);
  }
  if (
    fills.some(
      (fill) =>
        fill.inside !== true ||
        fill.width > 1.005 ||
        fill.height > 1.005 ||
        Math.abs(fill.width - 1) > 0.01 ||
        Math.abs(fill.height - 1) > 0.01,
    )
  ) {
    problems.push(
      `同尺寸的一组：每张图都应当正好铺满画布且不被裁（实测 ${JSON.stringify(fills)}）`,
    );
  }

  /*
   * 双击栏区 → 100%：画布就是原图像素 1:1（4000×3000 的 fixture ⇒ 画布 4000×3000）。
   *
   * ⚠️ 用**真鼠标事件**（`realDoubleClick`），不用合成 MouseEvent ——
   * 人类两次报「双击不行」，两次都是这条真路上的问题被合成的假事件盖住了。
   */
  const paneCenter = await send("Runtime.evaluate", {
    expression: `(() => {
      const rect = document.querySelector('[data-compare-frame="0"]')?.getBoundingClientRect();
      return rect
        ? { x: Math.round(rect.left + rect.width / 2), y: Math.round(rect.top + rect.height / 2) }
        : null;
    })()`,
    returnByValue: true,
  });
  const paneAt = paneCenter.result?.value ?? null;
  if (paneAt === null) {
    problems.push("量不到对比第一格的中心（双击这条没法验）");
  } else {
    await realDoubleClick(send, paneAt.x, paneAt.y);
  }
  await sleep(350);
  const zoomed = await send("Runtime.evaluate", {
    expression: `(() => {
      const pane = document.querySelector('[data-compare-frame="0"]');
      const canvas = pane?.querySelector("[data-compare-canvas]");
      const canvasRect = canvas?.getBoundingClientRect();
      const host = document.querySelector('[data-compare="open"]');
      const zoomLabel = document.querySelector('[data-viewer-controls="zoom"] button:nth-child(2)')?.textContent?.trim() ?? null;
      return {
        canvas: canvasRect ? { width: canvasRect.width, height: canvasRect.height } : null,
        zoom: Number(host?.getAttribute("data-compare-zoom") ?? "NaN"),
        fit: host?.getAttribute("data-compare-fit") ?? null,
        debug: (globalThis.__cd ?? []).slice(-12),
        paneId: pane?.getAttribute("data-compare-photo-id") ?? null,
        current: document.querySelector("[data-compare-frame][data-current='true']")?.getAttribute("data-compare-photo-id") ?? null,
        zoomLabel,
      };
    })()`,
    returnByValue: true,
  });
  const zoomState = zoomed.result?.value ?? {};
  if (Math.abs(Number(zoomState.zoom) - 1) > 0.001) {
    problems.push(`双击后倍率应当是 100%（1:1），实测 ${JSON.stringify(zoomState)}`);
  }
  if (
    zoomState.canvas === null ||
    Math.abs(zoomState.canvas.width - 4000) > 2 ||
    Math.abs(zoomState.canvas.height - 3000) > 2
  ) {
    problems.push(
      `100% 时画布应当按原图像素显示（4000×3000），实测 ${JSON.stringify(zoomState.canvas)}`,
    );
  }
  if (!String(zoomState.zoomLabel ?? "").includes("100%")) {
    problems.push(`双击后读数应当是 100%（实测 ${JSON.stringify(zoomState.zoomLabel)}）`);
  }

  /* 右下角「适配」键：连点两次 —— 先回适配、再去 100%（人类 2026-09-20 报「第二次点不到 100%」） */
  const fitButtonClick = async () =>
    send("Runtime.evaluate", {
      expression: `(() => {
        const button = document.querySelector('[data-compare="open"] [data-viewer-controls="zoom"] button:nth-child(2)');
        button?.click();
        return button?.textContent?.trim() ?? null;
      })()`,
      returnByValue: true,
    });
  await fitButtonClick();
  await sleep(300);
  const afterFitClick = await send("Runtime.evaluate", {
    expression: `(() => {
      const host = document.querySelector('[data-compare="open"]');
      const canvas = document.querySelector('[data-compare-frame="0"] [data-compare-canvas]');
      const pane = document.querySelector('[data-compare-frame="0"]');
      const canvasRect = canvas?.getBoundingClientRect();
      const paneRect = pane?.getBoundingClientRect();
      return {
        zoom: Number(host?.getAttribute("data-compare-zoom") ?? "NaN"),
        label: document.querySelector('[data-compare="open"] [data-viewer-controls="zoom"] button:nth-child(2)')?.textContent?.trim() ?? null,
        inside:
          canvasRect && paneRect
            ? canvasRect.width <= paneRect.width + 0.5 && canvasRect.height <= paneRect.height + 0.5
            : null,
      };
    })()`,
    returnByValue: true,
  });
  const fitState1 = afterFitClick.result?.value ?? {};
  if (fitState1.inside !== true || !(Number(fitState1.zoom) < 1)) {
    problems.push(`「适配」键第一次点应当回到适合窗口（实测 ${JSON.stringify(fitState1)}）`);
  }
  await fitButtonClick();
  await sleep(300);
  const afterFitClick2 = await send("Runtime.evaluate", {
    expression: `(() => {
      const host = document.querySelector('[data-compare="open"]');
      return {
        zoom: Number(host?.getAttribute("data-compare-zoom") ?? "NaN"),
        label: document.querySelector('[data-compare="open"] [data-viewer-controls="zoom"] button:nth-child(2)')?.textContent?.trim() ?? null,
      };
    })()`,
    returnByValue: true,
  });
  const fitState2 = afterFitClick2.result?.value ?? {};
  if (Math.abs(Number(fitState2.zoom) - 1) > 0.001) {
    problems.push(
      `「适配」键**再点一次**应当切到 100%（与单张看图同一条口径，实测 ${JSON.stringify(fitState2)}）`,
    );
  }

  /* 再双击一次 → 回到适合窗口（倍率变小、画布重新落回栏区内） */
  if (paneAt !== null) {
    await realDoubleClick(send, paneAt.x, paneAt.y);
  }
  await sleep(350);
  const refitted = await send("Runtime.evaluate", {
    expression: `(() => {
      const pane = document.querySelector('[data-compare-frame="0"]');
      const canvas = pane?.querySelector("[data-compare-canvas]");
      const paneRect = pane?.getBoundingClientRect();
      const canvasRect = canvas?.getBoundingClientRect();
      const host = document.querySelector('[data-compare="open"]');
      return {
        inside:
          paneRect && canvasRect
            ? canvasRect.width <= paneRect.width + 0.5 &&
              canvasRect.height <= paneRect.height + 0.5
            : null,
        zoom: Number(host?.getAttribute("data-compare-zoom") ?? "NaN"),
      };
    })()`,
    returnByValue: true,
  });
  const refitState = refitted.result?.value ?? {};
  if (refitState.inside !== true || !(Number(refitState.zoom) < 1)) {
    problems.push(
      `再双击一次应当回到适合窗口（画布落回栏区内、倍率 < 1），实测 ${JSON.stringify(refitState)}`,
    );
  }

  /*
   * 拖动：**鼠标走多少，画面就走多少**（人类 2026-09-20 点名的那条）。
   *
   * 真鼠标事件（`Input.dispatchMouseEvent`）而不是合成 DOM 事件 —— 后者没有 pointerId、
   * 也走不了 pointer capture，正好也把「拖动这条路真的通」一起验了。
   * 放大到 100% 之后拖：内容盒比栏区大，所以位移应当原样跟手（不被夹取吃掉）。
   */
  await send("Runtime.evaluate", {
    expression: `(() => {
      const pane = document.querySelector('[data-compare-frame="0"]');
      pane?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, cancelable: true }));
      return Boolean(pane);
    })()`,
    returnByValue: true,
  });
  await sleep(300);
  const dragStart = await send("Runtime.evaluate", {
    expression: `(() => {
      const pane = document.querySelector('[data-compare-frame="0"]');
      const rect = pane?.getBoundingClientRect();
      const canvas = pane?.querySelector("[data-compare-canvas]");
      const canvasRect = canvas?.getBoundingClientRect();
      const matrix = canvas ? new DOMMatrixReadOnly(getComputedStyle(canvas).transform) : null;
      return rect && canvasRect && matrix
        ? {
            x: rect.left + rect.width / 2,
            y: rect.top + rect.height / 2,
            before: { width: canvasRect.width, height: canvasRect.height, x: matrix.e, y: matrix.f },
          }
        : null;
    })()`,
    returnByValue: true,
  });
  const dragFrom = dragStart.result?.value ?? null;
  if (dragFrom === null) {
    problems.push("量不到对比画框的变换，拖动这条没法验");
  } else {
    const dx = 60;
    const dy = -40;
    await send("Input.dispatchMouseEvent", {
      type: "mousePressed",
      button: "left",
      buttons: 1,
      clickCount: 1,
      x: Math.round(dragFrom.x),
      y: Math.round(dragFrom.y),
    });
    await send("Input.dispatchMouseEvent", {
      type: "mouseMoved",
      button: "left",
      buttons: 1,
      x: Math.round(dragFrom.x + dx),
      y: Math.round(dragFrom.y + dy),
    });
    await sleep(120);
    const dragMoved = await send("Runtime.evaluate", {
      expression: `(() => {
        const canvas = document.querySelector('[data-compare-frame="0"] [data-compare-canvas]');
        const matrix = canvas ? new DOMMatrixReadOnly(getComputedStyle(canvas).transform) : null;
        const rect = canvas?.getBoundingClientRect();
        return matrix && rect ? { width: rect.width, height: rect.height, x: matrix.e, y: matrix.f } : null;
      })()`,
      returnByValue: true,
    });
    await send("Input.dispatchMouseEvent", {
      type: "mouseReleased",
      button: "left",
      buttons: 0,
      clickCount: 1,
      x: Math.round(dragFrom.x + dx),
      y: Math.round(dragFrom.y + dy),
    });
    const moved = dragMoved.result?.value ?? null;
    if (moved === null) {
      problems.push("拖动后量不到画框变换");
    } else {
      // 倍数没变（拖动不缩放），位移应当就是鼠标位移（容差 1px 给取整）
      if (Math.abs(moved.width - dragFrom.before.width) > 1) {
        problems.push(
          `拖动不该改变倍率（拖前 ${JSON.stringify(dragFrom.before.width)}，拖后 ${JSON.stringify(moved.width)}）`,
        );
      }
      if (Math.abs(moved.x - dragFrom.before.x - dx) > 1.5) {
        problems.push(
          `拖动位移应当跟手：鼠标走 ${dx}px，实际画面走了 ${JSON.stringify(moved.x - dragFrom.before.x)}px`,
        );
      }
      if (Math.abs(moved.y - dragFrom.before.y - dy) > 1.5) {
        problems.push(
          `拖动位移应当跟手：鼠标走 ${dy}px，实际画面走了 ${JSON.stringify(moved.y - dragFrom.before.y)}px`,
        );
      }
    }
    // 拖完回到适配，后面的控件检查从干净状态开始
    await send("Runtime.evaluate", {
      expression: `(() => {
        const pane = document.querySelector('[data-compare-frame="0"]');
        pane?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, cancelable: true }));
        return Boolean(pane);
      })()`,
      returnByValue: true,
    });
    await sleep(200);
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
        backOpacity: back === null ? null : getComputedStyle(back).opacity,
        zoomOpacity: host?.querySelector('[data-viewer-controls="zoom"]') === null
          ? null
          : getComputedStyle(host.querySelector('[data-viewer-controls="zoom"]')).opacity,
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
  if (controls.backOpacity !== "0" || controls.zoomOpacity !== "0") {
    problems.push(`对比控件默认应当隐藏，靠近角落才显示（实测 ${JSON.stringify(controls)}）`);
  }
  if (controls.zoom === 1) {
    await send("Runtime.evaluate", {
      expression: `(() => {
        const host = document.querySelector('[data-compare="open"]');
        const rect = host?.getBoundingClientRect();
        if (!host || !rect) return false;
        host.dispatchEvent(new PointerEvent("pointermove", {
          bubbles: true,
          clientX: rect.right - 20,
          clientY: rect.bottom - 20,
        }));
        return true;
      })()`,
      returnByValue: true,
    });
    await sleep(220);
    const zoomInteraction = await send("Runtime.evaluate", {
      expression: `(() => {
        const controls = document.querySelector('[data-compare="open"] [data-viewer-controls="zoom"]');
        const buttons = controls ? [...controls.querySelectorAll("button")] : [];
        const before = buttons[1]?.textContent?.trim() ?? null;
        buttons[2]?.click();
        return {
          opacity: controls ? getComputedStyle(controls).opacity : null,
          before,
          after: buttons[1]?.textContent?.trim() ?? null,
        };
      })()`,
      returnByValue: true,
    });
    await sleep(50);
    const zoomAfter = await send("Runtime.evaluate", {
      expression: `document.querySelector('[data-compare="open"] [data-viewer-controls="zoom"] button:nth-child(2)')?.textContent?.trim() ?? null`,
      returnByValue: true,
    });
    const zi = zoomInteraction.result?.value ?? {};
    if (zi.opacity !== "1") {
      problems.push(`鼠标靠近右下角后对比缩放控件应显示（实测 opacity=${JSON.stringify(zi.opacity)}）`);
    }
    if (zoomAfter.result?.value === zi.before) {
      problems.push(`对比放大按钮点击后倍率没有变化（前后都是 ${JSON.stringify(zi.before)}）`);
    }
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
      problems.push(`对比态点返回之后四态应当复位（实测 ${JSON.stringify(backState.chrome)}）`);
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

  /*
   * 人类 2026-09-19 报的 bug：Enter 进看图 → Esc 退出后，照片还是选中的，
   * 但再按 Enter 进不去。真因：焦点在看图件关掉后掉到了 `body`，键事件再也到不了网格。
   *
   * 这一整段是**自足**的：按真实路径走一遍「选中 → 回车进看图 → Esc 退出 → 再回车」，
   * 不依赖前面几步留下的状态（前面那些步子已经动过筛选、删除、对比）。
   */
  const keyOn = (key) => (target) => `(() => {
    const el = ${target} ?? document.body;
    el.dispatchEvent(new KeyboardEvent("keydown", { key: "${key}", bubbles: true, cancelable: true }));
    return true;
  })()`;
  const viewerOpen = async () => {
    const reply = await send("Runtime.evaluate", {
      expression: `Boolean(document.querySelector('[data-viewer="open"]'))`,
      returnByValue: true,
    });
    return reply.result?.value === true;
  };
  const focusInGrid = async () => {
    const reply = await send("Runtime.evaluate", {
      expression: `(() => {
        const scroller = document.querySelector("[data-virtual-scroller]");
        const active = document.activeElement;
        return Boolean(scroller) && Boolean(active) && scroller.contains(active);
      })()`,
      returnByValue: true,
    });
    return reply.result?.value === true;
  };

  // 先把可能的看图态关掉，再从 tiles 里点一张（真实用户动作）
  await send("Runtime.evaluate", {
    expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }))`,
    returnByValue: true,
  });
  await sleep(400);
  await send("Runtime.evaluate", {
    expression: `(() => {
      const tile = document.querySelector('main [data-virtual-scroller] [role="option"]');
      tile?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return Boolean(tile);
    })()`,
    returnByValue: true,
  });
  await sleep(500);
  // 真机点击会顺带把焦点给 tile（浏览器原生行为），合成事件不会 —— 补上它。
  // 注意要**等重渲染落地之后**再聚焦：点选会把行元素整块换掉。
  await send("Runtime.evaluate", {
    expression: `(() => {
      const tile = document.querySelector('main [data-virtual-scroller] [role="option"][aria-selected="true"]')
        ?? document.querySelector('main [data-virtual-scroller] [role="option"]');
      if (tile instanceof HTMLElement) tile.focus();
      return document.activeElement === tile;
    })()`,
    returnByValue: true,
  });
  await sleep(200);

  // ① 回车进看图
  await send("Runtime.evaluate", { expression: keyOn("Enter")("document.activeElement"), returnByValue: true });
  await sleep(900);
  if ((await viewerOpen()) !== true) {
    problems.push("网格里选中一张后按回车应当能进看图");
  } else {
    // ② Esc 退出：看图关掉，**焦点必须回到网格里**
    await send("Runtime.evaluate", {
      expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }))`,
      returnByValue: true,
    });
    await sleep(700);
    if ((await viewerOpen()) !== false) {
      problems.push("Esc 之后看图件没关掉");
    }
    const focused = await focusInGrid();
    const where = await send("Runtime.evaluate", {
      expression: `(() => {
        const a = document.activeElement;
        return a ? a.tagName + "[" + (a.getAttribute("role") ?? "-") + "]" : "null";
      })()`,
      returnByValue: true,
    });
    if (!focused) {
      problems.push(
        `Esc 退出看图后焦点应当回到网格里（实测 ${JSON.stringify(where.result?.value)}）—— 否则回车/方向键就断了`,
      );
    }
    // ③ 再按回车：必须能重新进去（这就是人类报的那条）
    await send("Runtime.evaluate", { expression: keyOn("Enter")("document.activeElement"), returnByValue: true });
    await sleep(900);
    if ((await viewerOpen()) !== true) {
      problems.push("Esc 退出后再按回车应当能重新进看图（人类 2026-09-19 报的 bug）");
    } else {
      await send("Runtime.evaluate", {
        expression: `window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }))`,
        returnByValue: true,
      });
      await sleep(400);
    }
  }

  /*
   * ══ 标题栏菜单（M2-W3：五个菜单，每一项都是可搜索命令）══
   *
   * 菜单**悬停标题栏才出现**（`design/main.md` §2.1 的规矩），所以先用真鼠标
   * （`Input.dispatchMouseEvent`）移到标题栏上，再断言：
   *   ① 五个菜单都在；② 点开「编辑」→ 项在、右侧有键位提示。
   */
  const headerBox = await send("Runtime.evaluate", {
    expression: `(() => {
      const header = document.querySelector("header[data-tauri-drag-region]");
      if (!header) return null;
      const rect = header.getBoundingClientRect();
      return { x: rect.left + 220, y: rect.top + rect.height / 2 };
    })()`,
    returnByValue: true,
  });
  const headerPoint = headerBox.result?.value ?? null;
  if (headerPoint === null) {
    problems.push("找不到标题栏（菜单这条验不了）");
  } else {
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: headerPoint.x, y: headerPoint.y });
    await sleep(350);
    const menus = await send("Runtime.evaluate", {
      expression: `(() => {
        const header = document.querySelector("header[data-tauri-drag-region]");
        const labels = [...(header?.querySelectorAll("button") ?? [])].map((b) => b.textContent?.trim() ?? "");
        return { labels };
      })()`,
      returnByValue: true,
    });
    const labels = menus.result?.value?.labels ?? [];
    for (const expected of ["文件", "编辑", "视图", "窗口", "帮助"]) {
      if (!labels.includes(expected)) {
        problems.push(`标题栏菜单里缺「${expected}」（实测 ${JSON.stringify(labels)}）`);
      }
    }
    // 用真实鼠标移到「编辑」并点击：现在一个菜单打开后，悬浮其它标题会自动切换，
    // 测试不能把鼠标留在另一个菜单标题上再用脚本伪点，否则会被正确的 hover 行为切走。
    const editBox = await send("Runtime.evaluate", {
      expression: `(() => {
        const header = document.querySelector("header[data-tauri-drag-region]");
        const button = [...(header?.querySelectorAll("button") ?? [])].find((b) => b.textContent?.trim() === "编辑");
        if (!button) return null;
        const rect = button.getBoundingClientRect();
        return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
      })()`,
      returnByValue: true,
    });
    const editPoint = editBox.result?.value ?? null;
    if (editPoint !== null) {
      await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: editPoint.x, y: editPoint.y });
      await send("Input.dispatchMouseEvent", { type: "mousePressed", button: "left", buttons: 1, clickCount: 1, x: editPoint.x, y: editPoint.y });
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", button: "left", buttons: 0, clickCount: 1, x: editPoint.x, y: editPoint.y });
    }
    await sleep(350);
    const items = await send("Runtime.evaluate", {
      expression: `(() => {
        const parts = [...document.querySelectorAll('[data-scope="menu"][data-part="item"]')];
        return {
          count: parts.length,
          texts: parts.map((item) => (item.textContent ?? "").replace(/\\s+/g, " ").trim()),
        };
      })()`,
      returnByValue: true,
    });
    const menuContent = items.result?.value ?? {};
    if (editPoint === null) {
      problems.push("点不开「编辑」菜单（菜单这条验不了）");
    } else if ((menuContent.count ?? 0) < 3) {
      problems.push(`「编辑」菜单的项太少（实测 ${JSON.stringify(menuContent)}）`);
    } else {
      const joined = (menuContent.texts ?? []).join(" | ");
      if (!joined.includes("撤销")) problems.push(`「编辑」菜单里应当有「撤销」（实测 ${JSON.stringify(menuContent.texts)}）`);
      if (!joined.includes("Ctrl+Z")) {
        problems.push(`菜单项右侧应当显示当前键位（实测 ${JSON.stringify(menuContent.texts)}）`);
      }
      // 关掉菜单（点空白）
      await send("Input.dispatchMouseEvent", { type: "mousePressed", button: "left", buttons: 1, clickCount: 1, x: 700, y: 400 });
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", button: "left", buttons: 0, clickCount: 1, x: 700, y: 400 });
      await sleep(300);
    }
    // 鼠标移开标题栏（把菜单收起来，别影响后面的断言）
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 700, y: 400 });
    await sleep(250);
  }

  /*
   * ══ 命令面板 / 快捷键体系（M2-W3）══
   *
   * 三条判据（对 `PLAN.md` 的 DoD）：
   *   ① `Ctrl+K` 开面板、搜得到、**每行右侧显示键位**；
   *   ② 回车真的执行了那条命令（用「按时间」开关当观测点）；
   *   ③ 快捷键能改：`Ctrl+,` 开设置 → 把「信息档位」改到 `Ctrl+Alt+I` → 保存 →
   *      新键生效、旧键解绑。
   *
   * 键盘一律走 CDP 真实按键（`Input.dispatchKeyEvent`）：分发器挂在 window 上，
   * 真按键顺带验了「浏览器不抢这个组合」。
   */
  const cdpKey = async (options) => {
    const base = {
      key: options.key,
      code: options.code ?? "",
      windowsVirtualKeyCode: options.vk ?? 0,
      modifiers: options.modifiers ?? 0,
    };
    await send("Input.dispatchKeyEvent", { type: "keyDown", ...base, ...(options.text ? { text: options.text } : {}) });
    await send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
  };
  const MOD_CTRL = 2;
  const MOD_ALT = 1;

  await cdpKey({ key: "k", code: "KeyK", vk: 75, modifiers: MOD_CTRL });
  await sleep(350);
  const paletteOpen = await send("Runtime.evaluate", {
    expression: `(() => {
      const host = document.querySelector('[data-command-palette="open"]');
      if (!host) return { open: false };
      const rows = [...host.querySelectorAll("[data-command-item]")];
      return {
        open: true,
        chords: rows.map((row) => ({
          id: row.getAttribute("data-command-item"),
          chord: row.querySelector("[data-command-chord]")?.textContent?.trim() ?? null,
        })),
      };
    })()`,
    returnByValue: true,
  });
  const palette = paletteOpen.result?.value ?? {};
  if (palette.open !== true) {
    problems.push('按下 Ctrl+K 应当打开命令面板（[data-command-palette="open"] 不在）');
  } else {
    const chords = Array.isArray(palette.chords) ? palette.chords : [];
    if (chords.length === 0) problems.push("命令面板里一条命令都没有（注册表没接上？）");
    if (!chords.some((row) => row.chord && row.chord !== "—")) {
      problems.push(`命令面板每行应当显示快捷键（实测 ${JSON.stringify(chords.slice(0, 3))}）`);
    }
    const infoChord = chords.find((row) => row.id === "view.tiles.info");
    if (infoChord !== undefined && infoChord.chord !== "I") {
      problems.push(`「信息档位」的键位应当显示 I（实测 ${JSON.stringify(infoChord)}）`);
    }
  }

  // 搜「按时间」→ 回车 → 控制条上的「按时间」真的被切换
  const beforeByTime = await send("Runtime.evaluate", {
    expression: `document.querySelector('button[aria-label="按时间"]')?.getAttribute("aria-pressed") ?? null`,
    returnByValue: true,
  });
  await send("Runtime.evaluate", {
    expression: `(() => { const input = document.querySelector('[data-command-palette="open"] input'); input?.focus(); return Boolean(input); })()`,
    returnByValue: true,
  });
  await send("Input.insertText", { text: "按时间" });
  await sleep(300);
  const searched = await send("Runtime.evaluate", {
    expression: `[...document.querySelectorAll('[data-command-palette="open"] [data-command-item]')].map((row) => row.getAttribute("data-command-item"))`,
    returnByValue: true,
  });
  const hits = searched.result?.value ?? [];
  if (!hits.includes("view.tiles.byTime")) {
    problems.push(`搜「按时间」应当命中 view.tiles.byTime（实测 ${JSON.stringify(hits)}）`);
  }
  await cdpKey({ key: "Enter", code: "Enter", vk: 13 });
  await sleep(500);
  const afterByTime = await send("Runtime.evaluate", {
    expression: `(() => ({
      pressed: document.querySelector('button[aria-label="按时间"]')?.getAttribute("aria-pressed") ?? null,
      paletteStillOpen: document.querySelector('[data-command-palette="open"]') !== null,
    }))()`,
    returnByValue: true,
  });
  const byTime = afterByTime.result?.value ?? {};
  if (byTime.pressed === (beforeByTime.result?.value ?? null)) {
    problems.push(`面板里回车执行「按时间」应当切换它（实测 ${JSON.stringify(byTime)}）`);
  }
  if (byTime.paletteStillOpen === true) problems.push("执行一条命令之后面板应当关掉");
  // 收尾：切回去（后面的分组断言不欢迎「按时间」开着）
  await send("Runtime.evaluate", {
    expression: `(() => {
      const button = [...document.querySelectorAll("button")].find((b) => b.getAttribute("aria-label") === "按时间");
      if (button?.getAttribute("aria-pressed") === "true") button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return true;
    })()`,
    returnByValue: true,
  });
  await sleep(300);

  // 快捷键设置：改一条键 → 保存 → 新键生效、旧键解绑
  await cdpKey({ key: ",", code: "Comma", vk: 188, modifiers: MOD_CTRL, text: "," });
  await sleep(450);
  const settingsOpen = await send("Runtime.evaluate", {
    expression: `(() => {
      const dialog = document.querySelector('[data-shortcuts-dialog="open"]');
      return {
        open: dialog !== null,
        rows: dialog ? dialog.querySelectorAll("[data-shortcut-row]").length : 0,
        infoKey: document.querySelector('[data-shortcut-key="view.tiles.info"]')?.textContent?.trim() ?? null,
      };
    })()`,
    returnByValue: true,
  });
  const settings = settingsOpen.result?.value ?? {};
  if (settings.open !== true) {
    problems.push('按下 Ctrl+, 应当打开快捷键设置（[data-shortcuts-dialog="open"] 不在）');
  } else {
    if (settings.rows < 30) problems.push(`快捷键设置应当列出全部命令（实测 ${settings.rows} 行）`);
    if (settings.infoKey === null) problems.push("快捷键设置里没找到「信息档位」那一行");

    const remap = await send("Runtime.evaluate", {
      expression: `(() => {
        const button = document.querySelector('[data-shortcut-key="view.tiles.info"]');
        button?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        return Boolean(button);
      })()`,
      returnByValue: true,
    });
    if (remap.result?.value !== true) {
      problems.push("点不动「信息档位」那一行的键位按钮（改键这条验不了）");
    } else {
      await cdpKey({ key: "i", code: "KeyI", vk: 73, modifiers: MOD_CTRL | MOD_ALT });
      await sleep(300);
      const afterCapture = await send("Runtime.evaluate", {
        expression: `document.querySelector('[data-shortcut-key="view.tiles.info"]')?.textContent?.trim() ?? null`,
        returnByValue: true,
      });
      const captured = afterCapture.result?.value ?? null;
      if (typeof captured !== "string" || !captured.includes("I")) {
        problems.push(`捕获新键之后那一行应当显示 Ctrl+Alt+I（实测 ${JSON.stringify(captured)}）`);
      }
      await send("Runtime.evaluate", {
        expression: `(() => {
          const dialog = document.querySelector('[data-shortcuts-dialog="open"]');
          const save = [...(dialog?.querySelectorAll("button") ?? [])].find((b) => b.textContent?.trim() === "保存");
          save?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
          return Boolean(save);
        })()`,
        returnByValue: true,
      });
      await sleep(450);
      const savedState = await send("Runtime.evaluate", {
        expression: `(() => {
          const raw = localStorage.getItem("raybend.shortcuts.v1");
          let parsed = null;
          try { parsed = raw === null ? null : JSON.parse(raw); } catch { parsed = "unparsable"; }
          return { parsed, dialog: document.querySelector('[data-shortcuts-dialog="open"]') !== null };
        })()`,
        returnByValue: true,
      });
      const saved = savedState.result?.value ?? {};
      if (saved.dialog === true) problems.push("点了保存之后设置弹窗应当关掉");
      if (!saved.parsed || saved.parsed === "unparsable" || saved.parsed.overrides?.["view.tiles.info"] !== "Ctrl+Alt+I") {
        problems.push(`改键必须落盘（实测 ${JSON.stringify(saved)}）`);
      }

      const beforeInfo = await send("Runtime.evaluate", {
        expression: `document.querySelector("[data-tiles-control-bar] [data-tile-info]")?.getAttribute("data-tile-info") ?? null`,
        returnByValue: true,
      });
      await cdpKey({ key: "i", code: "KeyI", vk: 73, modifiers: MOD_CTRL | MOD_ALT });
      await sleep(400);
      const afterInfo = await send("Runtime.evaluate", {
        expression: `document.querySelector("[data-tiles-control-bar] [data-tile-info]")?.getAttribute("data-tile-info") ?? null`,
        returnByValue: true,
      });
      const newLevel = afterInfo.result?.value ?? null;
      if (newLevel === (beforeInfo.result?.value ?? null)) {
        problems.push(`改键后 Ctrl+Alt+I 应当切一档信息档位（前 ${beforeInfo.result?.value} 后 ${newLevel}）`);
      }
      // 旧键 i 应当已经解绑：按一下不再变化
      await cdpKey({ key: "i", code: "KeyI", vk: 73, text: "i" });
      await sleep(400);
      const afterOldKey = await send("Runtime.evaluate", {
        expression: `document.querySelector("[data-tiles-control-bar] [data-tile-info]")?.getAttribute("data-tile-info") ?? null`,
        returnByValue: true,
      });
      if (afterOldKey.result?.value !== newLevel) {
        problems.push(`改键之后旧键 i 不该再触发（实测 ${JSON.stringify(newLevel)} → ${JSON.stringify(afterOldKey.result?.value)}）`);
      }

      // 收尾：全部恢复默认，并把信息档位转回 off
      await cdpKey({ key: ",", code: "Comma", vk: 188, modifiers: MOD_CTRL, text: "," });
      await sleep(450);
      const restored = await send("Runtime.evaluate", {
        expression: `(() => {
          const dialog = document.querySelector('[data-shortcuts-dialog="open"]');
          const buttons = [...(dialog?.querySelectorAll("button") ?? [])];
          buttons.find((b) => b.textContent?.trim() === "全部恢复默认")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
          buttons.find((b) => b.textContent?.trim() === "保存")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
          return true;
        })()`,
        returnByValue: true,
      });
      void restored;
      await sleep(450);
      // 信息档位是三态循环：按到位为止（探针纪律：把结局塞进返回值一次读全）
      const infoReset = await send("Runtime.evaluate", {
        expression: `(() => {
          const level = () => document.querySelector("[data-tiles-control-bar] [data-tile-info]")?.getAttribute("data-tile-info") ?? null;
          const seen = [];
          for (let i = 0; i < 4 && level() !== "off"; i += 1) {
            seen.push(level());
            window.dispatchEvent(new KeyboardEvent("keydown", { key: "i", bubbles: true, cancelable: true }));
          }
          return { seen, final: level() };
        })()`,
        returnByValue: true,
      });
      const infoState = infoReset.result?.value ?? {};
      if (infoState.final !== "off") {
        problems.push(`信息档位没转回 off（实测 ${JSON.stringify(infoState)}）`);
      }
    }
  }

  /*
   * ⚠️ 这一段放在**最后**：它会真的开一次模态（库设置弹窗）。
   * 模态的收尾（Ark 的 inert / 焦点归还）在无头环境里有时会留下痕迹，
   * 放在中间会让后面那些「hover / 键盘」断言莫名失败 —— 那是工装噪音，不是产品问题。
   */
  /*
   * 弹窗层级（人类 2026-09-20 报）：**弹窗要盖住看图的覆盖层，但要在标题栏之下**
   *（阶梯：toast 80 > titlebar 75 > modal 70 > scrim 60 > popover 40）。
   *
   * 复现的正是真机那条路：进看图（view/film）→ 点左列库卡片上的齿轮 →
   * 从前弹窗被 `z-10` 的看图覆盖层压住，只有周围一圈被遮罩压暗。
   *
   * 判据用**命中测试**（`elementFromPoint`）而不是只看 z 值 —— 用户看到的就是「点在谁身上」。
   */
  await send("Runtime.evaluate", {
    expression: `(() => {
      const tile = document.querySelector('main [data-virtual-scroller] [role="option"]');
      tile?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return true;
    })()`,
    returnByValue: true,
  });
  await sleep(300);
  await send("Runtime.evaluate", {
    expression: `(() => {
      const tile = document.querySelector('main [data-virtual-scroller] [role="option"]');
      tile?.focus();
      tile?.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
      return true;
    })()`,
    returnByValue: true,
  });
  await sleep(600);
  const gateOpen = await send("Runtime.evaluate", {
    expression: `(() => {
      const gear = [...document.querySelectorAll("button")].find(
        (b) => b.getAttribute("aria-label") === "库设置",
      );
      gear?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return { gear: Boolean(gear), viewer: Boolean(document.querySelector('[data-viewer="open"]')) };
    })()`,
    returnByValue: true,
  });
  const dialogGate = gateOpen.result?.value ?? {};
  if (dialogGate.gear !== true || dialogGate.viewer !== true) {
    problems.push(
      `弹窗层级这条没复现出来（需要「看图开着 + 库卡片齿轮可见」，实测 ${JSON.stringify(dialogGate)}）`,
    );
  }
  await sleep(600);
  const layering = await send("Runtime.evaluate", {
    expression: `(() => {
      const scope = (part) => document.querySelector('[data-scope="dialog"][data-part="' + part + '"]');
      const positioner = scope("positioner");
      const content = scope("content");
      const zOf = (value) => (value ? Number(getComputedStyle(value).zIndex) : null);
      const contentRect = content?.getBoundingClientRect();
      // 弹窗中心偏上一点：这个点同时落在看图覆盖层与弹窗上
      const hit = contentRect
        ? document.elementFromPoint(contentRect.left + contentRect.width / 2, contentRect.top + 12)
        : null;
      // 标题栏那一条（沉浸式窗口：拖拽区与关窗键都在里面）
      const titlePoint = document.querySelector("[data-tauri-drag-region]");
      const titleRect = titlePoint?.getBoundingClientRect();
      const titleHit = titleRect
        ? document.elementFromPoint(titleRect.left + titleRect.width / 2, titleRect.top + titleRect.height / 2)
        : null;
      const toastHost = document.querySelector("[data-toast-host]");
      return {
        modalZ: zOf(positioner),
        scrimZ: zOf(scope("backdrop")),
        viewerZ: zOf(document.querySelector('[data-viewer="open"]')),
        titlebarZ: zOf(document.querySelector("[data-tauri-drag-region]")?.closest("header") ?? null),
        toastZ: zOf(toastHost),
        hitIsDialog: hit ? hit.closest('[data-scope="dialog"]') !== null : null,
        hitTag: hit ? hit.tagName : null,
        titleHitIsDialog: titleHit ? titleHit.closest('[data-scope="dialog"]') !== null : null,
        titleHitTag: titleHit ? titleHit.tagName : null,
      };
    })()`,
    returnByValue: true,
  });
  const layers = layering.result?.value ?? {};
  if (layers.modalZ === null || layers.viewerZ === null) {
    problems.push(`量不到弹窗/看图层的 z（实测 ${JSON.stringify(layers)}）`);
  } else {
    if (!(layers.modalZ > layers.viewerZ)) {
      problems.push(
        `弹窗必须在看图/胶片覆盖层之上（弹窗 ${layers.modalZ} vs 看图 ${layers.viewerZ}）；` +
          "Zag 的 dialog positioner 不会自己写 z-index，靠的是 index.css 里那条 !important",
      );
    }
    if (layers.hitIsDialog !== true) {
      problems.push(
        `弹窗中心命中的不是弹窗（拿到 ${JSON.stringify(layers.hitTag)}）—— 就是人类报的「被挡住、只有周围一圈变暗」`,
      );
    }
    if (!(layers.scrimZ !== null && layers.scrimZ > layers.viewerZ)) {
      problems.push(`遮罩也要盖住看图覆盖层（实测 scrim=${layers.scrimZ}, viewer=${layers.viewerZ}）`);
    }
    if (layers.modalZ !== null && layers.titlebarZ !== null && !(layers.titlebarZ > layers.modalZ)) {
      problems.push(
        `标题栏要留在弹窗之上（沉浸式窗口要靠它拖拽/关窗，实测 titlebar=${layers.titlebarZ}, modal=${layers.modalZ}）`,
      );
    }
    if (layers.titleHitIsDialog === true) {
      problems.push("标题栏那条被弹窗盖住了（应该还是标题栏自己接事件）");
    }
    if (layers.toastZ !== null && layers.titlebarZ !== null && !(layers.toastZ > layers.titlebarZ)) {
      problems.push(`右上角消息要在标题栏之上（实测 toast=${layers.toastZ}, titlebar=${layers.titlebarZ}）`);
    }
  }
  /*
   * 关弹窗：**点它自己的关闭键**，不是往 window 上发一个 Esc ——
   * Ark 的 Esc 处理挂在 content 上，合成事件从 window 往下发根本到不了它
   *（真机上键盘事件是冒泡上来的，所以那条路在浏览器里是通的）。
   */
  await send("Runtime.evaluate", {
    expression: `(() => {
      const close = document.querySelector('[data-scope="dialog"][data-part="close-trigger"]');
      close?.click();
      return Boolean(close);
    })()`,
    returnByValue: true,
  });
  await sleep(400);
  await send("Runtime.evaluate", {
    expression: `(() => {
      const back = document.querySelector('[data-viewer="open"] button[aria-label="返回"]')
        ?? document.querySelector('[data-compare="open"] button[aria-label="返回"]');
      back?.click();
      const esc = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
      if (!back) window.dispatchEvent(esc);
      return Boolean(back);
    })()`,
    returnByValue: true,
  });
  await sleep(500);
  const layeringCleanup = await send("Runtime.evaluate", {
    expression: `(() => ({
      dialog: Boolean(document.querySelector('[data-scope="dialog"]')),
      viewer: Boolean(document.querySelector('[data-viewer="open"], [data-compare="open"]')),
    }))()`,
    returnByValue: true,
  });
  const cleanState = layeringCleanup.result?.value ?? {};
  if (cleanState.dialog === true || cleanState.viewer === true) {
    problems.push(`弹窗层级这段没把界面收干净（后面的断言会被带偏，实测 ${JSON.stringify(cleanState)}）`);
  }


  const stackOverflow = consoleErrors.find((text) => /Maximum call stack/.test(text));
  if (stackOverflow) {
    problems.push("控制台出现爆栈（很可能是响应式自激）：" + stackOverflow.split("\n")[0]);
  }
  const otherErrors = consoleErrors.filter((text) => !/Maximum call stack/.test(text));
  if (otherErrors.length > 0) {
    problems.push(`控制台有 ${otherErrors.length} 条错误，例如：` + otherErrors[0].split("\n")[0]);
  }
  const runtimeErrors = await send("Runtime.evaluate", {
    expression: `(window.__REJECTIONS || []).slice()`,
    returnByValue: true,
  });
  const runtimeErrorList = runtimeErrors.result?.value ?? [];
  if (runtimeErrorList.length > 0) {
    problems.push(`页面有 ${runtimeErrorList.length} 条未处理异常，例如：` + String(runtimeErrorList[0]).split("\n")[0]);
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
