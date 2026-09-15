#!/usr/bin/env node
/**
 * UI 冒烟（开发机、零依赖）—— 用 CDP 驱动本机已有的 Chromium 打开页面，
 * 收集控制台错误并做几项 DOM / 交互断言。
 *
 * **为什么需要它**：`AGENTS.md` §2.8 说 Agent 只做冒烟、E2E 归人类。
 * 而「白屏」这件事既不是编译错误也不是单元测试能覆盖的 —— M0 期间就吃过一次
 * （`custom-protocol` 缺失导致 Windows 版页面打不开，但窗口正常出现）。
 * 这个脚本就是那类回归的自动化闸门：**页面必须真的渲染出东西**。
 *
 * 它已经抓到过一次真问题：`@solidjs/router` 1.0 下把 `RouteDefinition[]` 数组
 * 直接喂给 `<Router>` 会让所有路由静默不匹配 —— 页面空白、控制台一句报错都没有。
 *
 * 目前断言的内容：
 *   1. 页面**真的画出了东西**（不是白屏）
 *   2. 切主题 / 切密度真的换了 `<html>` 上的 data 属性与令牌值
 *   3. 控制台无 error / warning
 *   4. 外壳规则（若页面上有工作流控件）：工具行**跟着工作流显隐**、
 *      无选中时「批量排除」是禁用态（`DESIGN.md` §12.2、`design/main.md` §2.3）
 *
 * 用法：
 *   pnpm dev                       # 另开一个终端起开发服务器
 *   pnpm smoke:ui                  # 默认打 http://localhost:1420/dev/kitchen-sink
 *   pnpm smoke:ui http://localhost:1420/
 *   CHROME_BIN=/path/to/chrome pnpm smoke:ui
 *
 * 退出码：发现问题（控制台错误 / 页面空白 / 断言不成立）→ 1。
 *
 * ⚠️ 它**不做视觉判断**：WSL 下 headless Chromium 的截图是纯黑（无 GPU），
 * 色彩、间距、对齐一律由人类在真机目视确认（同 `ASSISTANCE.md` A2）。
 *
 * 关于等待：脚本**轮询等 `#root` 长出内容**（最多 30 秒），不睡固定时长。
 * 固定时长会把「开发服务器首次预打包依赖」误判成白屏 —— 实测冷缓存首屏要 ~15 秒。
 */

import { spawn } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const url = process.argv[2] ?? "http://localhost:1420/dev/kitchen-sink";
const PORT = Number(process.env.CDP_PORT ?? 9333);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** 找浏览器：优先环境变量，其次 Playwright 的缓存目录（本机已有，不额外下载） */
function findChrome() {
  if (process.env.CHROME_BIN) return process.env.CHROME_BIN;
  const cache = join(homedir(), ".cache", "ms-playwright");
  if (!existsSync(cache)) return undefined;

  const candidates = [];
  for (const dir of readdirSync(cache)) {
    if (dir.startsWith("chromium_headless_shell-")) {
      candidates.push(
        join(
          cache,
          dir,
          "chrome-headless-shell-linux64",
          "chrome-headless-shell",
        ),
      );
    }
    if (dir.startsWith("chromium-")) {
      candidates.push(join(cache, dir, "chrome-linux64", "chrome"));
    }
  }
  return candidates.find((path) => existsSync(path));
}

const chromePath = findChrome();
if (!chromePath) {
  console.error(
    "✗ 没找到 Chromium。设置 CHROME_BIN 指向可执行文件，或先用 Playwright 装一个浏览器。",
  );
  process.exit(2);
}

const chrome = spawn(
  chromePath,
  [
    "--headless",
    "--no-sandbox",
    "--disable-gpu",
    "--disable-dev-shm-usage",
    `--remote-debugging-port=${PORT}`,
    "--window-size=1440,900",
    "about:blank",
  ],
  { stdio: ["ignore", "ignore", "pipe"] },
);
chrome.stderr.on("data", () => {}); // Chromium 会喷 UPower / GCM 之类的噪声，不要它

let ws;
const events = [];
const problems = [];

try {
  const wsUrl = await findTarget();
  ws = new WebSocket(wsUrl);
  let nextId = 1;
  const pending = new Map();

  ws.addEventListener("message", (event) => {
    const msg = JSON.parse(event.data);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    } else {
      events.push(msg);
    }
  });

  await new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve);
    ws.addEventListener("error", reject);
  });

  const send = (method, params = {}) => {
    const id = nextId++;
    return new Promise((resolve, reject) => {
      pending.set(id, (msg) =>
        msg.error
          ? reject(new Error(`${method}: ${msg.error.message}`))
          : resolve(msg.result),
      );
      ws.send(JSON.stringify({ id, method, params }));
    });
  };

  const evaluate = async (expression) => {
    const result = await send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (result.exceptionDetails) {
      throw new Error(
        `页面里的表达式抛错：${result.exceptionDetails.exception?.description ?? result.exceptionDetails.text}`,
      );
    }
    return result.result.value;
  };

  await send("Runtime.enable");
  await send("Log.enable");
  await send("Page.enable");
  await send("Page.navigate", { url });

  /*
   * **轮询等内容，而不是睡固定时长**。
   * 固定等待会变成间歇性失败：开发服务器首次会预打包依赖，冷缓存时
   * 首屏可能要十几秒（实测 15s）。固定 5s 时页面还是空的，于是
   * 「白屏」这个诊断就成了误报 —— 而误报比不报更消耗信任。
   */
  const mounted = await waitForContent(send);
  if (!mounted) problems.push("等待 30 秒后 #root 仍为空（白屏）");

  for (const event of events) {
    if (event.method === "Runtime.exceptionThrown") {
      const details = event.params.exceptionDetails;
      problems.push(
        `未捕获异常：${details.exception?.description ?? details.text}`,
      );
    }
    if (
      event.method === "Runtime.consoleAPICalled" &&
      ["error", "warning"].includes(event.params.type)
    ) {
      problems.push(
        `console.${event.params.type}：${event.params.args
          .map((arg) => arg.description ?? JSON.stringify(arg.value))
          .join(" ")}`,
      );
    }
    if (
      event.method === "Log.entryAdded" &&
      event.params.entry.level === "error" &&
      /*
       * 浏览器自己会去要 `favicon.ico`，404 与页面无关（本项目目前真没放 favicon），
       * 而这条日志里**不带 URL**，所以只能按状态码放行 —— 真正的模块加载失败
       * 会在下面以「未捕获异常：Failed to fetch dynamically imported module」的形式出现，
       * 不会因为这条放行而被掩盖。
       */
      !String(event.params.entry.text).includes("404")
    ) {
      problems.push(`浏览器日志：${event.params.entry.text}`);
    }
  }

  const snapshot = await evaluate(`(() => {
    const root = document.documentElement;
    return {
      theme: root.dataset.theme,
      density: root.dataset.density,
      lang: root.lang,
      textLength: document.body.innerText.trim().length,
      headings: document.querySelectorAll("h1, h2").length,
      arkParts: document.querySelectorAll("[data-scope]").length,
      bodyBg: getComputedStyle(document.body).backgroundColor,
      barTitleHeight: getComputedStyle(root).getPropertyValue("--bar-title-h").trim(),
    };
  })()`);

  if (snapshot.textLength === 0) {
    problems.push("页面渲染为空（白屏）—— 检查路由是否匹配、入口是否抛错");
  }

  // 主题 / 密度：只有 <html> 上的 data 属性真的换了才算通过
  const interact = await evaluate(`(async () => {
    const clickByText = (label) => {
      const el = [...document.querySelectorAll("label, button")].find(
        (node) => node.textContent.trim() === label,
      );
      if (!el) return false;
      el.click();
      return true;
    };
    const pick = async (label) => {
      const clicked = clickByText(label);
      await new Promise((r) => setTimeout(r, 250));
      return clicked;
    };
    const root = document.documentElement;

    // 主题开关是 titlebar 上的图标按钮（无文字），按 aria-label 找
    const themeButton = document.querySelector(
      'button[aria-label="切换主题"], button[aria-label="Toggle theme"]',
    );
    const switchedTheme = themeButton
      ? (themeButton.click(), await new Promise((r) => setTimeout(r, 250)), true)
      : false;
    const theme = root.dataset.theme;
    const switchedDensity = await pick("宽松");
    const density = root.dataset.density;
    // 浏览器里没有窗口 API：三键必须整组不存在（否则点了就报错）
    const windowControlCount = [
      "最小化",
      "Maximize",
      "最大化",
      "还原",
      "关闭",
      "Close",
    ].filter((label) =>
      document.querySelector('button[aria-label="' + label + '"]'),
    ).length;

    return {
      switchedTheme,
      theme,
      switchedDensity,
      density,
      barTitleHeight: getComputedStyle(root).getPropertyValue("--bar-title-h").trim(),
      windowControlCount,
    };
  })()`);

  if (interact.switchedTheme && interact.theme !== "light") {
    problems.push(`切主题没生效：data-theme=${interact.theme}`);
  }
  if (interact.switchedDensity && interact.density !== "loose") {
    problems.push(`切密度没生效：data-density=${interact.density}`);
  }
  if (interact.windowControlCount > 0) {
    problems.push(
      `浏览器里出现了 ${interact.windowControlCount} 个窗口三键按钮 —— 同环境降级失效（src/api/window.ts）`,
    );
  }
  if (
    interact.switchedDensity &&
    interact.density === "loose" &&
    !interact.barTitleHeight.startsWith("4")
  ) {
    problems.push(
      `密度令牌没跟着换：宽松档 --bar-title-h=${interact.barTitleHeight}（期望 40px）`,
    );
  }

  /*
   * 外壳规则冒烟（M1-4）：切到「没有工具」的工作流时，工具行必须**整行消失**。
   * 这条规则肉眼很容易漏（不看就不知道它是不是还占着一条空条），所以用 DOM 断言钉住。
   * 页面上没有工作流控件（例如陈列室自己）时返回 null，跳过这部分。
   */
  const shell = await evaluate(`(async () => {
    const labelFor = (text) =>
      [...document.querySelectorAll("label")].find(
        (node) => node.textContent.trim() === text,
      );
    if (!labelFor("导入") && !labelFor("Import")) return null;

    // 只看按钮元素：页面上的解释性文字也可能含这两个词（陈列室里就有一句），
    // 用文本匹配会误判成「工具行还在」
    const hasTools = () =>
      [...document.querySelectorAll("button")].some((node) =>
        /批量排除|Exclude selected/.test(node.textContent),
      );

    const before = hasTools();
    (labelFor("浏览") ?? labelFor("Browse"))?.click();
    await new Promise((resolve) => setTimeout(resolve, 250));
    const onBrowse = hasTools();

    (labelFor("导入") ?? labelFor("Import"))?.click();
    await new Promise((resolve) => setTimeout(resolve, 250));
    const restored = hasTools();

    const exclude = [...document.querySelectorAll("button")].find((node) =>
      /批量排除|Exclude selected/.test(node.textContent),
    );

    return {
      hasToolsOnImport: before,
      hasToolsOnBrowse: onBrowse,
      restoredOnImport: restored,
      excludeDisabled: exclude ? exclude.disabled : null,
    };
  })()`);

  if (shell) {
    if (!shell.hasToolsOnImport) {
      problems.push("「导入」工作流下没看到工具行（批量排除）");
    }    if (shell.hasToolsOnBrowse) {
      problems.push(
        "切到「浏览」后工具行还在 —— 设计稿要求整行消失（design/main.md §2.3）",
      );
    }
    if (!shell.restoredOnImport) {
      problems.push("切回「导入」后工具行没回来");
    }
    if (shell.excludeDisabled === false) {
      problems.push(
        "没有选中照片时「批量排除」竟然可点（DESIGN.md §12.2 要求禁用）",
      );
    }
  }

  /*
   * 分段拖拽冒烟（M1-5）：Ark 的 `Splitter` 只给变量，消费方必须自己接上；
   * 接错了**不报错、面板高度是 0** —— 看起来「渲染了」其实什么也看不到。
   * 所以这里真量一遍几何：分隔条存在、每个分隔条旁边有非零高度的面板。
   * 页面上没有分隔条时返回 null（例如陈列室的其它页面）。
   */
  const splitter = await evaluate(`(() => {
    // Ark 的分隔条是 role=separator（Splitter.ResizeTrigger 自己带的语义）
    const triggers = [...document.querySelectorAll('[role="separator"]')];
    if (triggers.length === 0) return null;
    const triggerBoxes = triggers.map((el) => {
      const rect = el.getBoundingClientRect();
      return { role: el.getAttribute("aria-orientation"), h: Math.round(rect.height), w: Math.round(rect.width) };
    });
    // 分隔条两侧的面板：用**相邻兄弟**量高度，不依赖 Ark 内部的属性命名
    const paneHeights = triggers.flatMap((el) =>
      [el.previousElementSibling, el.nextElementSibling]
        .filter(Boolean)
        .map((node) => Math.round(node.getBoundingClientRect().height)),
    );
    return { triggerCount: triggers.length, triggerBoxes, paneHeights };
  })()`);

  if (splitter) {
    if (splitter.triggerCount < 1) {
      problems.push("分段布局里一个分隔条都没有");
    }
    if (splitter.paneHeights.length > 0 && splitter.paneHeights.some((h) => h <= 0)) {
      problems.push(
        `分段面板高度为 0（${splitter.paneHeights.join("/")}）—— 分隔条的尺寸没接上（Ark 只给变量）`,
      );
    }
    if (
      splitter.triggerBoxes.every((box) => box.h === 0 || box.w === 0) &&
      splitter.triggerBoxes.length > 0
    ) {
      problems.push("分隔条没有可点击的尺寸（拖不动）");
    }
  }

  console.log(
    JSON.stringify(
      { url, chrome: chromePath, snapshot, interact, shell, splitter, problems },
      null,
      2,
    ),
  );
} catch (error) {
  problems.push(`冒烟脚本自身失败：${error.message}`);
  console.error(JSON.stringify({ url, problems }, null, 2));
} finally {
  ws?.close();
  chrome.kill("SIGKILL");
}

process.exit(problems.length > 0 ? 1 : 0);

async function findTarget() {
  for (let attempt = 0; attempt < 40; attempt += 1) {
    try {
      const list = await (
        await fetch(`http://127.0.0.1:${PORT}/json/list`)
      ).json();
      const page = list.find((target) => target.type === "page");
      if (page?.webSocketDebuggerUrl) return page.webSocketDebuggerUrl;
    } catch {
      // 浏览器还没起来
    }
    await sleep(250);
  }
  throw new Error(`CDP 目标没出现（端口 ${PORT}）`);
}

/** 轮询等 `#root` 真的长出内容；返回是否等到 */
async function waitForContent(send, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const result = await send("Runtime.evaluate", {
        expression:
          '(document.getElementById("root")?.childElementCount ?? 0) > 0',
        returnByValue: true,
      });
      if (result.result.value === true) return true;
    } catch {
      // 导航过程中 evaluate 可能短暂失败
    }
    await sleep(500);
  }
  return false;
}
