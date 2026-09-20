/**
 * 无头 Chrome + CDP 的**公共工装**（`scripts/` 下几个脚本共用）。
 *
 * ## 为什么现在抽
 *
 * `ui-smoke.mjs` / `check-browse-boot.mjs` / `shot.mjs` 各自抄了一份「找 Chrome →
 * 起无头 → 连 WebSocket → send()」。2026-09-20 写第四个脚本（`perf-browse.mjs`）时
 * 按 `AGENTS.md` §2.12「同一个能力只允许有一套实现」把它抽出来 ——
 * **新脚本一律用它**；另外三个脚本的迁移排在后面（它们各自带着大量与本文无关的断言，
 * 一次性重写风险不值得）。
 *
 * ## 用法
 *
 * ```js
 * import { findChrome, launchChrome, connectCdp, sleep } from "./lib/cdp.mjs";
 * const chrome = launchChrome({ port: 9500, windowSize: [1440, 900] });
 * const cdp = await connectCdp(9500);
 * await cdp.send("Page.navigate", { url: "http://localhost:1420/" });
 * ...
 * cdp.close(); chrome.kill("SIGKILL");
 * ```
 */

import { spawn } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** 找一份可用的 Chromium（Playwright 缓存优先，其次系统路径） */
export function findChrome() {
  if (process.env.CHROME_BIN) return process.env.CHROME_BIN;
  const cache = join(homedir(), ".cache", "ms-playwright");
  if (existsSync(cache)) {
    for (const dir of readdirSync(cache)) {
      for (const candidate of [
        join(cache, dir, "chrome-linux64", "chrome"),
        join(cache, dir, "chrome-linux", "chrome"),
        join(cache, dir, "chrome-headless-shell-linux64", "chrome-headless-shell"),
      ]) {
        if (existsSync(candidate)) return candidate;
      }
    }
  }
  for (const candidate of ["/usr/bin/google-chrome", "/usr/bin/chromium", "/usr/bin/chromium-browser"]) {
    if (existsSync(candidate)) return candidate;
  }
  return undefined;
}

/**
 * 起一个无头 Chrome。
 *
 * ⚠️ **无头默认没有鼠标**（`(hover: hover)` / `(pointer: fine)` 都是 false）——
 * Tailwind 的 `hover:*` 变体整条失效。所以默认就把 blink 的悬停/指针能力声明出来
 * （2026-09-20 真踩过：怎么移鼠标信息条都不浮出，一度误判成 CSS 写错）。
 */
export function launchChrome(options = {}) {
  const executable = options.executable ?? findChrome();
  if (!executable) throw new Error("没找到 Chromium：设置 CHROME_BIN，或用 Playwright 装一个");
  const port = options.port ?? 9500;
  const [width, height] = options.windowSize ?? [1440, 900];
  const chrome = spawn(
    executable,
    [
      "--headless=new",
      "--no-sandbox",
      "--disable-gpu",
      "--disable-dev-shm-usage",
      "--blink-settings=primaryHoverType=2,availableHoverTypes=2,primaryPointerType=4,availablePointerTypes=4",
      `--remote-debugging-port=${port}`,
      `--window-size=${width},${height}`,
      ...(options.args ?? []),
      options.url ?? "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  // 无头 Chrome 在 stderr 上很啰嗦（GPU / dbus 警告），吞掉
  chrome.stderr.on("data", () => {});
  return chrome;
}

/** 连上调试端口，返回 `{ send, close, on }`。
 *
 * `options.host`（默认 `127.0.0.1`）：无头本机就用默认；连 **Windows 宿主上的
 * WebView2**（`perf-win`）要传宿主 IP —— WSL NAT 下 `localhost` 不通 Windows 侧服务。
 * 返回的 `webSocketDebuggerUrl` 里写的是 `127.0.0.1`，连接前会改写成请求的那个 host。
 */
export async function connectCdp(port, options = {}) {
  const host = options.host ?? "127.0.0.1";
  const deadline = Date.now() + (options.timeoutMs ?? 20000);
  let target;
  while (Date.now() < deadline) {
    try {
      const list = await (await fetch(`http://${host}:${port}/json/list`)).json();
      target = list.find((item) => item.type === "page" && item.webSocketDebuggerUrl);
      if (target) break;
    } catch {
      /* 还没起来 */
    }
    await sleep(200);
  }
  if (!target) throw new Error(`Chrome 调试端口没起来（${host}:${port}）`);

  // DevTools 报的 ws 地址永远是 127.0.0.1 —— 按我们实际连的 host 改写，否则跨机连不上
  let wsUrl;
  try {
    wsUrl = new URL(target.webSocketDebuggerUrl);
    wsUrl.host = `${host}:${port}`;
  } catch {
    throw new Error(`DevTools 报的 ws 地址不合法：${target.webSocketDebuggerUrl}`);
  }
  const ws = new WebSocket(wsUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve, { once: true });
    ws.addEventListener("error", reject, { once: true });
  });

  let nextId = 1;
  const pending = new Map();
  const listeners = new Map();
  const consoleErrors = [];
  const exceptions = [];
  ws.addEventListener("message", (event) => {
    // 帧解不开就当它不存在（工装脚本不该因为一条奇怪的消息整个死掉）
    let message;
    try {
      message = JSON.parse(event.data);
    } catch {
      return;
    }
    if (message.id && pending.has(message.id)) {
      pending.get(message.id)(message);
      pending.delete(message.id);
      return;
    }
    if (message.method === "Runtime.exceptionThrown") {
      const details = message.params?.exceptionDetails;
      exceptions.push(details?.exception?.description ?? details?.text ?? "unknown exception");
    }
    if (message.method === "Runtime.consoleAPICalled" && message.params.type === "error") {
      consoleErrors.push(
        (message.params.args ?? []).map((arg) => arg.value ?? arg.description ?? "").join(" "),
      );
    }
    for (const handler of listeners.get(message.method) ?? []) handler(message.params);
  });

  const send = (method, params) =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`CDP 超时：${method}`));
      }, options.callTimeoutMs ?? 30000);
      pending.set(id, (message) => {
        clearTimeout(timer);
        if (message.error) reject(new Error(`${method}: ${message.error.message}`));
        else resolve(message.result);
      });
      ws.send(JSON.stringify({ id, method, params }));
    });

  await send("Runtime.enable");
  await send("Page.enable");

  return {
    send,
    consoleErrors,
    exceptions,
    on: (method, handler) => {
      const list = listeners.get(method) ?? [];
      list.push(handler);
      listeners.set(method, list);
    },
    close: () => ws.close(),
  };
}

/** 探一下 dev server 在不在 */
export async function requireServer(url) {
  try {
    const probe = await fetch(url, { method: "HEAD" });
    if (!probe.ok && probe.status !== 405) throw new Error(String(probe.status));
  } catch {
    throw new Error(`${url} 打不开 —— 先另开一个终端跑 \`pnpm dev\``);
  }
}
