#!/usr/bin/env node
/**
 * 截图脚本（开发期视觉自查，零新增依赖）。
 *
 * 用途：把界面按指定的**主题 / 密度 / URL / 尺寸**截成 PNG，供人与 Agent 快速比对。
 *
 * 为什么要它：
 *   - 改 CSS 后每次都等 `pnpm tauri dev` 编译 Rust 是不可接受的；
 *   - **Tauri 自己没有截图 API** —— 已在 `tauri 2.11.5` 与 `wry 0.55.1` 源码里 grep 过
 *     `screenshot`，一个字都没有。真机 Windows 上可以给 WebView2 传
 *     `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=…` 用同样的 CDP 抓，
 *     但那是发布前的排障手段，日常不需要。
 *   - 前端是同一份代码，视觉判断够用；真机手感仍归人目视（`AGENTS.md` §2.8）。
 *
 * 用法：
 *   pnpm shot                                  # 深色·紧凑 首页
 *   pnpm shot --theme light --density loose
 *   pnpm shot --url http://localhost:1420/dev/kitchen-sink --name sink
 *   pnpm shot --width 1600 --height 1000
 *
 * 两个实现要点（都踩过）：
 *   1. 主题/密度走**开发期 URL 覆盖**（`src/lib/appearance.ts` 的 `readAppearanceOverride`），
 *      不用去折腾浏览器 profile。
 *   2. 截图走 CDP（`Emulation.setDeviceMetricsOverride` + `Page.captureScreenshot`）：
 *      视口尺寸精确、无多余黑边。`--headless` 命令行截图那条路会渲染出比窗口小一圈的视口，
 *      底部留一条纯黑（**注意**：只有 `chrome-headless-shell` 那个二进制的 CDP 截图是真的全黑，
 *      完整 chromium 的 CDP 截图正常）。
 */

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const OUT_DIR = process.env.RB_SHOT_DIR ?? "/mnt/c/src/tmp";
const PORT = Number(process.env.RB_SHOT_PORT ?? 9349);

const args = process.argv.slice(2);
const flag = (name, fallback) => {
  const index = args.indexOf(`--${name}`);
  return index >= 0 && args[index + 1] ? args[index + 1] : fallback;
};

const baseUrl = flag("url", "http://localhost:1420/");
const theme = flag("theme", "dark");
const density = flag("density", "compact");
const width = Number(flag("width", "1400"));
const height = Number(flag("height", "900"));

/** 主题/密度拼进 query（应用侧只在 DEV 读它） */
const separator = baseUrl.includes("?") ? "&" : "?";
const url = `${baseUrl}${separator}theme=${theme}&density=${density}`;

function findChrome() {
  if (process.env.CHROME_BIN) return process.env.CHROME_BIN;
  const cache = join(homedir(), ".cache", "ms-playwright");
  if (!existsSync(cache)) return undefined;
  const candidates = readdirSync(cache)
    .filter((dir) => dir.startsWith("chromium-"))
    .map((dir) => join(cache, dir, "chrome-linux64", "chrome"));
  return candidates.find(existsSync);
}

const chromePath = findChrome();
if (!chromePath) {
  console.error("✗ 找不到 Chromium：设置 CHROME_BIN，或用 Playwright 装一个浏览器");
  process.exit(2);
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const chrome = spawn(
  chromePath,
  [
    "--headless=new",
    "--no-sandbox",
    "--disable-gpu",
    "--disable-dev-shm-usage",
    `--remote-debugging-port=${PORT}`,
    "about:blank",
  ],
  { stdio: "ignore" },
);

let wsUrl;
for (let i = 0; i < 60 && !wsUrl; i += 1) {
  try {
    const response = await fetch(`http://127.0.0.1:${PORT}/json/list`);
    const list = await response.json();
    wsUrl = list.find((target) => target.type === "page")?.webSocketDebuggerUrl;
  } catch {
    /* 还没起来 */
  }
  if (!wsUrl) await sleep(250);
}
if (!wsUrl) {
  chrome.kill("SIGKILL");
  console.error("✗ CDP 目标没出现");
  process.exit(1);
}

const ws = new WebSocket(wsUrl);
let nextId = 1;
const pending = new Map();
ws.addEventListener("message", (event) => {
  let message;
  try {
    message = JSON.parse(event.data);
  } catch {
    return; // 进程被杀时会收到残帧，不该因此崩掉
  }
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
});
await new Promise((resolve, reject) => {
  ws.addEventListener("open", resolve);
  ws.addEventListener("error", reject);
});

const send = (method, params = {}) => {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    pending.set(id, (message) =>
      message.error
        ? reject(new Error(`${method}: ${message.error.message}`))
        : resolve(message.result),
    );
    ws.send(JSON.stringify({ id, method, params }));
  });
};
const evaluate = async (expression) =>
  (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result
    .value;

await send("Page.enable");
await send("Runtime.enable");
await send("Emulation.setDeviceMetricsOverride", {
  width,
  height,
  deviceScaleFactor: 1,
  mobile: false,
});
await send("Page.navigate", { url });

// 等真的渲染出来（开发服务器冷缓存时首屏可达 15s）
let mounted = false;
for (let i = 0; i < 60; i += 1) {
  mounted = await evaluate(
    '(document.getElementById("root")?.childElementCount ?? 0) > 0',
  ).catch(() => false);
  if (mounted) break;
  await sleep(500);
}
if (!mounted) console.error("⚠ 页面 30 秒内没有渲染出内容，截出来的可能是空白");
await sleep(800); // 字体与布局收尾

const { data } = await send("Page.captureScreenshot", { format: "png" });
mkdirSync(OUT_DIR, { recursive: true });
const name = flag("name", baseUrl.includes("kitchen") ? "sink" : "app");
const out = join(OUT_DIR, `rb-${name}-${theme}-${density}.png`);
writeFileSync(out, Buffer.from(data, "base64"));

ws.close();
chrome.kill("SIGKILL");
console.log(out);
