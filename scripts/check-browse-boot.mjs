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

/* 假后端：一个在线库。形状与 `RepositoryView` DTO 对齐（camelCase）。 */
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
];

const FIXTURES = {
  repositories_list: REPOSITORIES,
  dir_list: [],
  browse_page: { total: 0, offset: 0, items: [] },
  browse_timeline: { total: 0, entries: [] },
  browse_facets: { ratings: [], colors: [], likes: [], locks: [] },
  browse_flags_get: { total: 0, picks: [], rejects: [] },
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
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
        transformCallback: (cb) => cb,
        convertFileSrc: (p) => p,
        invoke: (cmd) => {
          const fixtures = ${JSON.stringify(FIXTURES)};
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
console.log("✓ 库非空时进浏览正常（左列渲染出库、无爆栈、无控制台错误）");
