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
 *   5. 分段（Ark `Splitter`）的面板高度不为 0
 *   6. **导入工作区**（总会导航到应用外壳跑一遍）：右列库区有宽度、
 *      没勾选目录时「导入」禁用且给出原因、建库弹窗能打开且空路径时不可提交
 *   7. **导入进度弹窗**（画廊里的真 store + 假后端演示）：阶段条 / 计数 / 当前项 /
 *      错误清单 / 取消的二次确认 / 结束摘要 / 关得掉
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
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { existsSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

/*
 * 自检：本脚本大量用 `evaluate(\`...\`)` 把代码送到页面里跑，而那些模板串里
 * **不能出现反引号** —— 一个写在注释里的反引号就会把模板串截断，后面的内容跑到
 * Node 这边来，报出一个跟真正原因毫不相干的错（比如「dialog is not defined」）。
 *
 * 这个坑踩过三次，而 `node --check` **抓不到**（截断后剩下的往往还是合法 JS）。
 * 所以自己扫一遍源码，早失败、说清楚。
 */
{
  const source = readFileSync(fileURLToPath(import.meta.url), "utf8");
  const offenders = source
    .split("\n")
    .map((line, index) => ({ line: line.trim(), number: index + 1 }))
    .filter((entry) => entry.line.startsWith("//") && entry.line.includes("\u0060"));
  if (offenders.length > 0) {
    console.error(
      `✗ 冒烟脚本里有 ${offenders.length} 处注释带反引号（会把 evaluate 的模板串截断）：\n` +
        offenders.map((entry) => `  L${entry.number}: ${entry.line}`).join("\n") +
        "\n（判据只扫 // 注释；/* */ 形式的注释在模板串里同样会截断，见实现记录）",
    );
    process.exit(2);
  }
}

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

  /*
   * CDP 调用加时限：页面**卡死**（渲染器忙循环）时，CDP 也会一起不应答 ——
   * 没有时限的话整个冒烟脚本就永远挂在那里，看不出出了什么事。
   * 20 秒对任何一条 CDP 调用都绰绰有余。
   */
  const send = (method, params = {}) => {
    const id = nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`${method}: 20 秒没有回应（页面可能卡死了）`));
      }, 20_000);
      pending.set(id, (msg) => {
        clearTimeout(timer);
        if (msg.error) reject(new Error(`${method}: ${msg.error.message}`));
        else resolve(msg.result);
      });
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
  if (!mounted) problems.push("等待 60 秒后 #root 仍为空（白屏）");

  /**
   * 把浏览器日志里的「未捕获异常 / console.error / console.warning」收进 problems。
   *
   * 抽成函数是因为脚本要访问**两个**页面（厨房水槽 + 应用外壳），
   * 每个页面加载后都得收一次 —— 只收第一页会漏掉外壳页的真实报错。
   */
  const collectConsoleProblems = (list) => {
    for (const event of list) {
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
  };

  collectConsoleProblems(events);

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


  /*
   * 导入工作区（M1-5）：三列里的**右列库区** + 建库弹窗。
   *
   * 为什么值得断言：右列是这一轮新加的，而它有两种「看起来没事」的坏法 ——
   * 面板宽度塌成 0（Ark 只给变量，尺寸要自己接），或者选中统计/按钮可用性算错
   * （左边没勾选目录时「导入」**必须**是禁用的）。
   * 页面上没有导入工作区时返回 null（陈列室等其它路由）。
   */

  /*
   * 导入进度弹窗（M1-6）：只在「真有库 + 真勾了目录」时才打得开，
   * 所以画廊里放了一个**真 store + 假后端**的演示（`src/dev/import-progress-demo.tsx`），
   * 这里对它的结构做断言 —— 阶段条、计数、当前项、取消的二次确认、结束摘要。
   */
  const importDialog = await evaluate(`(async () => {
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    const button = (label) =>
      [...document.querySelectorAll("button")].find(
        (node) => node.textContent.trim() === label,
      );
    const pageText = () => document.body.innerText.replace(/\\s+/g, " ");

    // ① 演示区块在不在（真 store + 假后端；见 src/dev/import-progress-demo.tsx）
    if (!document.body.innerText.includes("导入进度（M1-6）")) return null;
    // 两种标签任取：演示挂载时是收起的（「打开…」），被点开后就变成「关闭…」
    const openButton = button("打开导入弹窗") ?? button("关闭导入弹窗");
    if (!openButton) return null;

    // ② **回归守卫**：点开之后按钮标签必须翻面 ——
    //    store 的 open 若不是响应式，这里永远翻不过来（M1-6 真的踩过：弹窗弹不出来）
    if (button("打开导入弹窗")) {
      openButton.click();
      await sleep(250);
    }
    const reactive = button("关闭导入弹窗") !== undefined;

    button("推一条「导入中」")?.click();
    await sleep(250);
    const text = pageText();
    const result = {
      reactive,
      arkRendered: document.querySelectorAll('[data-scope="dialog"]').length > 0,
      pushButtons: [...document.querySelectorAll("button")].filter((n) =>
        n.textContent.trim().startsWith("推"),
      ).length,
      stageChips: ["扫描", "规划", "导入", "缩略图"].filter((name) => text.includes(name)).length,
      counts: /已导入\\s*82/.test(text) && /跳过\\s*3/.test(text) && /失败\\s*1/.test(text),
      currentItem: text.includes("P1040733.ORF") && text.includes("_RAW/MYP0733.ORF"),
      errorRow: text.includes("磁盘写满"),
      keepPartial: text.includes("已经导入的照片会保留在库里"),
      cancelConfirm: false,
      doneSummary: false,
      revealButton: false,
      closed: false,
    };

    // ③ 弹窗内容只在 Ark 真的把它渲染出来时才严查（画廊里目前渲染不出来，见 ASSISTANCE.md §二）
    if (result.arkRendered) {
      button("取消导入")?.click();
      await sleep(250);
      result.cancelConfirm = pageText().includes("取消这次导入");
      button("继续导入")?.click();
      await sleep(250);

      button("推到「结束」")?.click();
      await sleep(300);
      const doneText = pageText();
      result.doneSummary = doneText.includes("导入完成") && /已导入\\s*117/.test(doneText);
      result.revealButton = doneText.includes("在库中查看这些照片");

      result.afterDoneButtons = [...document.querySelectorAll("button")]
        .map((n) => n.textContent.trim())
        .filter((label) => label !== "");

      button("关闭")?.click();
      await sleep(300);
      result.closed = button("打开导入弹窗") !== undefined;
    }
    return result;
  })()`);

  /*
   * 后端挂掉时的**逃生路径**（2026-09-16 真机 bug 的永久断言）：
   * Rust 命令 panic → 前端 promise 永远不 settle → 弹窗卡在忙碌态、连「取消」都发不出去。
   * 现在：命令有时限（演示里 300ms）→ 落到错误态 → 底部按钮变成「关闭」→ 点它能关掉。
   */
  const deadBackend = await evaluate(`(async () => {
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
    const byText = (label) =>
      [...document.querySelectorAll("button")].find((el) => el.textContent.trim() === label);
    if (!byText("打开导入弹窗")) return null;
    byText("打开导入弹窗").click();
    await sleep(200);
    // 先重新开一批：上一段断言点过「推到结束」，终态之后 store 会退订，
    // 不退订的话后面的「推一条」没人听，一条断言就白测了（踩过）
    document.querySelector('[data-demo-action="restart"]')?.click();
    await sleep(300);

    // 打开「后端挂掉」，再点暂停 —— 这次命令会挂住，等着被时限打断
    document.querySelector('[data-demo-action="break-backend"]')?.click();
    const pauseButton = byText("暂停");
    pauseButton?.click();
    await sleep(800); // 时限 300ms + 一点余量

    // 诊断：Ark 的弹窗内容在 data-part="content" 上（[role=dialog] 命中的是外层，innerText 常为空）
    const content = document.querySelector('[data-scope="dialog"][data-part="content"]');

    // 查**整页**文本：[role=dialog] 命中的可能是 Portal 外壳（它的 innerText 是空的），
    // 这个坑在导入弹窗那组断言里踩过一次
    const text = document.body.innerText.replace(/\\s+/g, " ");
    const contentText = content ? content.innerText.replace(/\\s+/g, " ") : "";
    const result = {
      showsError: /秒内回应|挂了/.test(text) || /秒内回应|挂了/.test(contentText),
      closeOffered: byText("关闭") !== undefined,
      cancelHidden: byText("取消导入") === undefined,
      // 诊断：点了没有、内容是什么，红了直接看得出卡在哪一步
      pauseFound: pauseButton !== undefined,
      contentFound: content !== null,
      contentText: contentText.slice(0, 200),
    };
    byText("关闭")?.click();
    await sleep(250);
    result.closed = document.querySelector('[role="dialog"]') === null;
    return result;
  })()`);

  if (deadBackend !== null) {
    if (!deadBackend.showsError) {
      problems.push("后端挂掉时弹窗没报错（命令时限没生效？）");
    }
    if (!deadBackend.closeOffered || !deadBackend.cancelHidden) {
      problems.push(
        "后端挂掉时底部应当只给「关闭」（不然用户会去点一个同样发不出去的「取消导入」）",
      );
    }
    if (!deadBackend.closed) {
      problems.push("后端挂掉时点「关闭」关不掉弹窗 —— 逃生路径断了");
    }
  }

  if (importDialog === null) {
    problems.push("画廊里没有「导入进度」演示（src/dev/import-progress-demo.tsx 没挂上？）");
  } else {
    if (importDialog.pushButtons !== 3) {
      problems.push(`演示的推进按钮不齐（应有 3 个，看到 ${importDialog.pushButtons} 个）`);
    }
    if (!importDialog.reactive) {
      problems.push(
        "store 的 open 不响应界面：点了「打开导入弹窗」按钮标签没翻面（弹窗也就永远弹不出来）",
      );
    }
    if (importDialog.arkRendered) {
      if (importDialog.stageChips !== 4) {
        problems.push(`阶段条不完整（只看到 ${importDialog.stageChips}/4）`);
      }
      if (!importDialog.counts) problems.push("计数不对（已导入 82 / 跳过 3 / 失败 1）");
      if (!importDialog.currentItem) problems.push("没显示当前项（源文件名 → 库内目标路径）");
      if (!importDialog.errorRow) problems.push("错误清单里没有失败原因");
      if (!importDialog.keepPartial) problems.push("缺「已导入的会保留」那句");
      if (!importDialog.cancelConfirm) problems.push("点「取消导入」没有二次确认");
      if (!importDialog.doneSummary) problems.push("结束态摘要不对（导入完成 + 已导入 117）");
      if (!importDialog.revealButton) problems.push("结束态缺「在库中查看这些照片」");
      if (!importDialog.closed) problems.push("点「关闭」弹窗没关上");
    }
  }

  /*
   * 导入工作区在**应用外壳**页上（厨房水槽里没有它），所以这里自己导航过去 ——
   * 脚本无论被传入哪个 URL，都会把两页都过一遍。
   */
  /*
   * 通用目录树（`DirTree`）：两种配置 + 双击展开 + 滚动开销。
   *
   * 浏览器里没有真文件系统，所以画廊用**假目录树**驱动（`src/dev/dir-tree-demo.tsx`）。
   * 这里验三件事：勾选圈的有无确实由「传没传回调」决定；双击行名能展开/折叠；
   * 滚动时的样式/布局开销（盯住「滚动发粘」这类回归 —— 起因通常是行上的颜色过渡）。
   */
  const dirTree = await evaluate(`(async () => {
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
    const demo = document.querySelector('[data-demo="dir-tree"]');
    if (!demo) return null;
    const panel = (variant) => demo.querySelector('[data-variant="' + variant + '"]');
    const rowsIn = (el) => [...el.querySelectorAll('[role="treeitem"]')];
    const boxesIn = (el) => el.querySelectorAll('[role="checkbox"]').length;

    const plain = panel("plain");
    const checkable = panel("checkable");
    if (!plain || !checkable) return null;

    const result = {
      checkableBoxes: boxesIn(checkable),
      plainBoxes: boxesIn(plain),
      rowsAtStart: rowsIn(plain).length,
    };

    // ① 双击第一行（卷）→ 展开；再双击 → 折叠
    // 注意：展开会让行列表重建，**节点引用会失效** —— 每次都重新取第一个节点
    const firstRow = () => rowsIn(plain)[0];
    if (firstRow()) {
      firstRow()?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
      await sleep(500);
      result.rowsAfterExpand = rowsIn(plain).length;
      firstRow()?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
      await sleep(250);
      result.rowsAfterCollapse = rowsIn(plain).length;
    }

    // ② 展开到几十行，量滚动开销
    for (let round = 0; round < 3; round++) {
      const collapsed = rowsIn(plain).filter(
        (row) => row.getAttribute("aria-expanded") === "false",
      );
      for (const row of collapsed.slice(0, 3)) {
        row.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
        await sleep(150);
      }
    }
    result.rowsExpanded = rowsIn(plain).length;

    const scroller = [...plain.querySelectorAll("div")].find(
      (el) => getComputedStyle(el).overflowY === "auto",
    );
    if (scroller) {
      const before = performance.now();
      for (let step = 0; step < 20; step++) {
        scroller.scrollTop = step * 24;
        // 读一次 scrollHeight：逼出这次滚动的样式/布局工作，否则量到的是空转
        void scroller.scrollHeight;
      }
      result.scrollMs = Math.round(performance.now() - before);
      result.scrolledTo = Math.round(scroller.scrollTop);
    }

    /*
     * 注：这里**不**再断言「点勾选圈不选中这一行」。
     * 试过两版都很脆（演示里起始只有一行、且那一行本来就是选中的，测不出因果），
     * 而真正的判据已经落在**原语**里：RemoveButton 自己 stopPropagation
     * （见 components/ui/RemoveButton.tsx），所有调用点自动继承。
     * 注意：本段在 evaluate 的模板串**里面**，所以任何注释都不许出现反引号。
     */

    return result;
  })()`);

  if (dirTree === null) {
    problems.push("画廊里没有「目录树」演示（src/dev/dir-tree-demo.tsx 没挂上？）");
  } else {
    if (!(dirTree.checkableBoxes > 0)) {
      problems.push("导入变体的目录树应当有勾选圈（传了 onToggleCheck）");
    }
    if (dirTree.plainBoxes !== 0) {
      problems.push(
        `浏览变体的目录树不该有勾选圈（未传 onToggleCheck），实际 ${dirTree.plainBoxes} 个`,
      );
    }
    if (!(dirTree.rowsAfterExpand > dirTree.rowsAtStart)) {
      problems.push(
        `双击目录名没有展开：行数 ${dirTree.rowsAtStart} → ${dirTree.rowsAfterExpand}`,
      );
    }
    if (dirTree.rowsAfterCollapse !== dirTree.rowsAtStart) {
      problems.push(
        `再次双击没有折叠回来：行数 ${dirTree.rowsAfterCollapse}（期望 ${dirTree.rowsAtStart}）`,
      );
    }
    /*
     * **操作图标不该顺带选中这一行**（人类 2026-09-16：Recent 的移除图标会顺带选中，
     * 后来又重申「移除是移除，选中是选中，选中要点没有操作图标的部分」）。
     * 勾选圈与移除按钮都在这一条规则下 —— 这里用目录树的勾选圈验它：
     * 点圈 → 不选中；点行的其它部分 → 才选中。
     */
    /*
     * 注：**不在此处**断言「点勾选圈不选中这一行」。试过两版都很脆
     * （演示起始只有一行，且那一行本来就是选中的，测不出因果），
     * 判据落在原语里：RemoveButton 自己 stopPropagation（components/ui/RemoveButton.tsx）。
     */

    if (typeof dirTree.scrollMs === "number" && dirTree.scrollMs > 500) {
      problems.push(
        `滚动 ${dirTree.rowsExpanded} 行用了 ${dirTree.scrollMs}ms —— 行上可能有过渡/重绘（历史上就是它让滚动发粘）`,
      );
    }
  }

  /*
   * 开关的**可见性**（2026-09-16 人类截图报回来的原样：关闭态的轨道与卡片同色，
   * 整个开关只剩一个灰点；「包含子目录」那行文字也没渲染出来）。
   *
   * 这类「同色不可见」的 bug 单元测试抓不到 —— 颜色是令牌算出来的，只有真的渲染出来
   * 再量计算样式才看得见。判据刻意很松（分得出来就算过），目标只是钉死「完全同色」。
   */
  const switchBar = await evaluate(`(() => {
    const demo = document.querySelector('[data-demo="selected-bar"]');
    if (!demo) return null;
    const control = demo.querySelector('[data-part="control"]');
    const label = demo.querySelector('span[aria-hidden="true"]');
    const css = (el) => (el ? getComputedStyle(el) : null);
    const parse = (value) => {
      const m = /rgb\\((\\d+)[,\\s]+(\\d+)[,\\s]+(\\d+)/.exec(value ?? "");
      return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
    };
    const cardEl = css(demo);
    const trackEl = css(control);
    const labelEl = css(label);
    const bg = parse(cardEl ? cardEl.backgroundColor : null);
    const distance = (other) =>
      bg && other
        ? Math.abs(bg[0] - other[0]) + Math.abs(bg[1] - other[1]) + Math.abs(bg[2] - other[2])
        : -1;

    return {
      cardBg: cardEl ? cardEl.backgroundColor : null,
      trackBg: trackEl ? trackEl.backgroundColor : null,
      trackDistance: distance(parse(trackEl ? trackEl.backgroundColor : null)),
      labelText: label ? label.textContent.trim() : null,
      labelDistance: distance(parse(labelEl ? labelEl.color : null)),
    };
  })()`);

  if (switchBar === null) {
    problems.push("画廊里没有「已选目录」条的样例（data-demo=selected-bar）");
  } else {
    if (!(switchBar.trackDistance > 18)) {
      problems.push(
        "开关关闭态的轨道与卡片几乎同色（" +
          switchBar.trackBg +
          " vs " +
          switchBar.cardBg +
          "）—— 开关会看不见",
      );
    }
    if (switchBar.labelText !== "包含子目录") {
      problems.push(
        `开关的标签文字没渲染出来（读到 ${JSON.stringify(switchBar.labelText)}）`,
      );
    }
    if (!(switchBar.labelDistance > 18)) {
      problems.push("开关标签文字与卡片几乎同色 —— 文字会看不见");
    }
  }

  /*
   * 库卡片（M1-9）：**在线 = 齿轮**（开库设置）、**离线 = 离线图标**（点它重新查找），
   * 而且**两者都不带可见文字** —— 人类 2026-09-16 的原话：
   * 「离线不要做成现在这样带文字的……点击离线图标检查是否上线」。
   * 顺带在这里证明「库设置」真的接线了：点齿轮必须弹出弹窗、里面有模版输入框。
   */
  const repoCards = await evaluate(`(async () => {
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    const demo = document.querySelector('[data-demo="repo-cards"]');
    if (!demo) return null;
    const byLabel = (label) =>
      [...demo.querySelectorAll("button")].filter(
        (el) => (el.getAttribute("aria-label") ?? "") === label,
      );
    const gear = byLabel("库设置");
    const offline = byLabel("重新查找");

    const out = {
      gearCount: gear.length,
      offlineCount: offline.length,
      offlineText: offline.length > 0 ? offline[0].textContent.trim() : null,
      visibleText: demo.innerText.replace(/\\s+/g, " ").trim().slice(0, 120),
      remountsBefore: demo.getAttribute("data-remounts"),
    };

    // 点离线图标 → 重新查找（真机上就是「插上盘再点它」）
    offline[0]?.click();
    await sleep(200);
    out.remountsAfter = demo.getAttribute("data-remounts");

    // 点齿轮 → 库设置弹窗必须真的开
    gear[0]?.click();
    await sleep(500);
    const content = document.querySelector('[data-scope="dialog"][data-part="content"]');
    out.settingsOpen = content !== null;
    out.settingsText = content ? content.innerText.replace(/\\s+/g, " ").slice(0, 80) : null;
    const input = content ? content.querySelector("input") : null;
    out.hasTemplateInput = Boolean(input);
    out.templateValue = input ? input.value : null;

    document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    await sleep(300);
    out.settingsClosed = document.querySelector('[data-scope="dialog"][data-part="content"]') === null;
    return out;
  })()`);

  if (repoCards === null) {
    problems.push("画廊里没有库卡片样例（data-demo=repo-cards）");
  } else {
    if (repoCards.gearCount < 1) {
      problems.push("在线的库卡片上没有齿轮（库设置入口）——「配置功能没实装」就是这个症状");
    }
    if (repoCards.offlineCount < 1) {
      problems.push("离线的库卡片上没有离线图标");
    }
    if (repoCards.offlineText !== "") {
      problems.push(
        `离线图标上带了可见文字（${JSON.stringify(repoCards.offlineText)}）—— 人类要求只留图标`,
      );
    }
    if (repoCards.visibleText.includes("离线")) {
      problems.push("库卡片上出现了「离线」二字 —— 该只进无障碍名与悬停提示");
    }
    if (repoCards.remountsAfter === repoCards.remountsBefore) {
      problems.push("点离线图标没有触发重新查找");
    }
    if (!repoCards.settingsOpen) {
      problems.push("点齿轮没有打开库设置弹窗");
    } else if (!repoCards.hasTemplateInput || !repoCards.templateValue) {
      problems.push("库设置弹窗里没有导入模版输入框（或没读到当前模版）");
    }
  }

  const appUrl = new URL("/", url).href;
  await send("Page.navigate", { url: appUrl });
  if (!(await waitForContent(send))) {
    problems.push("导航到应用外壳后 60 秒仍没渲染出内容（白屏）");
  }
  const workspaceEventsFrom = events.length;

  const workspace = await evaluate(`(async () => {
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
    const aside = [...document.querySelectorAll("aside")].find((el) =>
      el.textContent.includes("新建库"),
    );
    if (!aside) return null;

    const buttons = [...aside.querySelectorAll("button")];
    const importButton = buttons.find((el) => el.textContent.trim() === "导入");
    const createButton = buttons.find((el) => el.textContent.includes("新建库"));
    const text = aside.textContent.replace(/\\s+/g, " ");

    const result = {
      width: Math.round(aside.getBoundingClientRect().width),
      emptyState: text.includes("还没有库"),
      summary: /已选择\\s*0\\s*个目录/.test(text),
      avoidDuplicates: text.includes("避免重复导入"),
      hint: text.includes("先在左边勾选目录"),
      importDisabled: importButton ? importButton.disabled : null,
      createFound: Boolean(createButton),
      dialog: null,
    };

    // 点「+ 新建库」——弹窗必须真的开（Portal 出去，role=dialog）
    if (createButton) {
      createButton.click();
      await sleep(400);
      const dialog = document.querySelector('[role="dialog"]');
      if (dialog) {
        const dialogText = dialog.innerText.replace(/\\s+/g, " ").trim();
        const submit = [...dialog.querySelectorAll("button")].find(
          (el) => el.textContent.trim() === "新建库",
        );
        result.dialog = {
          text: dialogText.slice(0, 120),
          hasNameField: dialogText.includes("名称"),
          hasPathField: dialogText.includes("库根目录"),
          hasIntro: dialogText.includes("catalog.db"),
          submitDisabled: submit ? submit.disabled : null,
          // 空路径时**没有**判定行（画布上就是这个口径）
          noHintYet: !dialogText.includes("将登记为已有库"),
        };
        // 关掉，别影响后面的断言
        const cancel = [...dialog.querySelectorAll("button")].find(
          (el) => el.textContent.trim() === "取消",
        );
        if (cancel) cancel.click();
        await sleep(250);
        result.dialogClosed = document.querySelector('[role="dialog"]') === null;
      }
    }
    return result;
  })()`);

  /*
   * 左列结构（M1-8 的新形态）：**两段可拖 + 底部自适应「已选目录」**。
   *
   * 浏览器里没有后端（卷与最近都是空的），所以这里量的是**几何不变量** ——
   * 恰好也是人类报回来的那几个症状：
   *   * 只剩**一根**分隔条（第三根已随「已选目录」自动高度一起撤掉）；
   *   * 两段各自 ≥ 自己的像素下限（`minSize` 必须写成 `"120px"` —— 写成数字是百分比，
   *     上一版就是那样畸变的）；
   *   * 两段 + 分隔条 = 容器高（不许溢出）；
   *   * 树面板（pane）**底部不超过容器底部**（「滚到底也只看到 mnt」就是这个溢出的症状）；
   *   * 没勾任何来源时「已选目录」整块不存在。
   *
   * 选择器用 **Ark 自己的 DOM 契约**（`data-scope="splitter"` + `data-part="panel"/"resize-trigger"`），
   * 不靠「往上找祖先」那种脆招 —— 上一版就是那么找错的（找到 pane 上去了）。
   */
  const leftColumn = await evaluate(`(async () => {
    const rect = (el) => el.getBoundingClientRect();
    const titleOf = (pane) => {
      const heading = pane.querySelector("h2");
      return heading ? heading.textContent.trim() : "";
    };

    // 工作区里可能不止一个 splitter（左右分栏也是一个）——按 pane 标题认出左列那个
    // 工作区现在有**两个** splitter：外层横向（左列宽度）与左列内部竖直（最近/来源）。
    // 判据必须只看**直接子元素**：querySelectorAll 会把嵌套 splitter 的 pane 一起捞进来
    // （那样两个都会命中）。也别用 aria-orientation —— 分隔条的 ARIA 方向与 splitter 的
    // 方向是**反的**（左右分栏的分隔条是 vertical），按它挑会挑到外层那个。
    const directPanels = (el) =>
      [...el.children].filter((child) => child.getAttribute("data-part") === "panel");
    const splitters = [...document.querySelectorAll('[data-scope="splitter"]')];
    const root = splitters.find((el) => {
      const titles = directPanels(el).map(titleOf);
      return titles.includes("最近") && titles.includes("来源");
    });
    if (!root) return null;

    const panes = directPanels(root).map((pane) => ({
      title: titleOf(pane),
      h: Math.round(rect(pane).height),
      bottom: Math.round(rect(pane).bottom),
    }));
    // 只数直接子元素里的手柄：嵌套在里面的别的 splitter（外层横向那个）不算
    const triggers = [...root.children].filter(
      (el) => el.getAttribute("data-part") === "resize-trigger",
    );
    const selectedPanel = [...document.querySelectorAll("section")].find((el) => {
      const heading = el.querySelector("h2");
      return heading && heading.textContent.trim() === "已选目录";
    });

    return {
      viewportH: window.innerHeight,
      rootH: Math.round(rect(root).height),
      rootBottom: Math.round(rect(root).bottom),
      panes,
      triggerCount: triggers.length,
      recentH: panes.find((pane) => pane.title === "最近")?.h ?? -1,
      sourceH: panes.find((pane) => pane.title === "来源")?.h ?? -1,
      sourceOverflow: (panes.find((pane) => pane.title === "来源")?.bottom ?? 0) - Math.round(rect(root).bottom),
      // 注意 find() 没找到时返回的是 undefined —— 必须按 undefined 判（写 !== null 会恒真）
      selectedPanel: selectedPanel !== undefined,
      h2s: [...document.querySelectorAll("h2")].map((el) => el.textContent.trim()),
    };
  })()`);

  /*
   * 弹窗骨架（2026-09-16 人类反馈：内边距太小、标题与右上角的叉没对齐）。
   * 这里是**同一份** Dialog 组件，所以量一次就够；数值取自设计稿
   * （padding 16 / 标题 17 / 底部按钮 32）。改成小数会立刻红。
   */
  const dialogFrame = await evaluate(`(async () => {
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    const trigger = [...document.querySelectorAll("button")].find((el) =>
      el.textContent.includes("新建库"),
    );
    if (!trigger) return null;
    trigger.click();
    await sleep(400);
    const content = document.querySelector('[data-scope="dialog"][data-part="content"]');
    if (!content) return null;
    const title = content.querySelector('[data-part="title"]');
    const close = content.querySelector('button[aria-label="关闭"]');
    const cs = getComputedStyle(content);
    const center = (el) => {
      const rect = el.getBoundingClientRect();
      return Math.round(rect.top + rect.height / 2);
    };
    const out = {
      padTop: Math.round(Number.parseFloat(cs.paddingTop)),
      padLeft: Math.round(Number.parseFloat(cs.paddingLeft)),
      gap: Math.round(Number.parseFloat(cs.rowGap || cs.gap)),
      titleSize: title ? Math.round(Number.parseFloat(getComputedStyle(title).fontSize)) : null,
      titleCenter: title ? center(title) : null,
      closeCenter: close ? center(close) : null,
    };
    document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    return out;
  })()`);

  if (dialogFrame !== null) {
    if (dialogFrame.padTop < 16 || dialogFrame.padLeft < 16) {
      problems.push(
        `弹窗内边距太小（top ${dialogFrame.padTop} / left ${dialogFrame.padLeft}，设计稿是 16）`,
      );
    }
    if (dialogFrame.titleSize !== null && dialogFrame.titleSize < 16) {
      problems.push(`弹窗标题字号太小（${dialogFrame.titleSize}，设计稿是 17）`);
    }
    if (
      dialogFrame.titleCenter !== null &&
      dialogFrame.closeCenter !== null &&
      Math.abs(dialogFrame.titleCenter - dialogFrame.closeCenter) > 2
    ) {
      problems.push(
        "弹窗标题与右上角的叉没对齐（中心相差 " +
          Math.abs(dialogFrame.titleCenter - dialogFrame.closeCenter) +
          "px）",
      );
    }
  }

  if (leftColumn === null) {
    problems.push("应用外壳里找不到左列的 splitter（最近 / 来源 两个 pane）");
  } else {
    if (leftColumn.triggerCount !== 1) {
      problems.push(
        `左列应当只有一根分隔条（已选目录改成自动高度、不再有把手），实际 ${leftColumn.triggerCount} 根`,
      );
    }
    if (leftColumn.recentH < 119) {
      problems.push(`「最近」高 ${leftColumn.recentH}px，低于 120px 的像素下限`);
    }
    if (leftColumn.sourceH < 159) {
      problems.push(`「来源」高 ${leftColumn.sourceH}px，低于 160px 的像素下限`);
    }
    if (leftColumn.recentH + leftColumn.sourceH > leftColumn.rootH + 2) {
      problems.push(
        `两段加起来（${leftColumn.recentH}+${leftColumn.sourceH}）超过了左列可视高 ${leftColumn.rootH}px`,
      );
    }
    if (leftColumn.sourceOverflow > 1) {
      problems.push(
        `树面板底部超出容器 ${leftColumn.sourceOverflow}px —— 滚到底也会看不全`,
      );
    }
    if (leftColumn.selectedPanel) {
      problems.push("没勾任何来源时不该出现「已选目录」面板");
    }
  }

  /*
   * 浮层层级（M1-8 第 9 条反馈）：**菜单弹出来时不许被下面的元素压住**。
   *
   * 人类报的是「帮助菜单弹出来时，工作流切换按钮的文字浮在菜单面板之上」。
   * 静态看 `z-50`（菜单门户）> `z-10`（chip 文字）本该没事 —— 所以这里**实测**：
   * 在「菜单面板」与「工作流条」的重叠区中心调 `elementFromPoint`，命中的必须落在菜单内部。
   * 顺带把双方的 `z-index / position` 与各自的层叠上下文祖先报回来 —— 一旦红了，直接能看出是哪一层。
   */
  /*
   * 左列宽度把手（**自写，不用 Ark** —— 2026-09-16 真机事故后的替代方案）：
   * 拖它能改宽度、松手写进 localStorage、**刷新之后还原**。
   * 这条断言的另一半价值是「拖的时候不能卡」：页面真卡住的话下面的 evaluate 会超时报错。
   */
  const widthHandle = await evaluate(`(async () => {
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    const handle = document.querySelector('[role="separator"][aria-label="调整左列宽度"]');
    if (!handle) return null;
    const aside = handle.previousElementSibling;
    if (!aside) return null;

    const before = Math.round(aside.getBoundingClientRect().width);
    const rect = handle.getBoundingClientRect();
    const startX = Math.round(rect.left + rect.width / 2);
    const y = Math.round(rect.top + 20);
    const fire = (type, clientX) =>
      handle.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          clientX,
          clientY: y,
          pointerId: 1,
          isPrimary: true,
        }),
      );

    fire("pointerdown", startX);
    for (let step = 1; step <= 5; step += 1) fire("pointermove", startX + step * 24);
    fire("pointerup", startX + 120);
    await sleep(200);

    const after = Math.round(aside.getBoundingClientRect().width);
    let stored = null;
    try {
      stored = JSON.parse(localStorage.getItem("raybend.layout.v1") ?? "null");
    } catch {
      stored = null;
    }
    return { before, after, storedLeftRatio: stored ? stored.leftRatio : null };
  })()`);

  if (widthHandle === null) {
    problems.push("找不到左列宽度把手（role=separator + aria-label=调整左列宽度）");
  } else {
    if (!(widthHandle.after > widthHandle.before)) {
      problems.push(
        `拖左列把手没有改宽度：${widthHandle.before} → ${widthHandle.after}`,
      );
    }
    if (!(typeof widthHandle.storedLeftRatio === "number")) {
      problems.push("拖完没有把比例写进 localStorage（重启还原就无从谈起）");
    }
  }

  const layers = await evaluate(`(async () => {
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
    const header = document.querySelector("header");
    if (!header) return null;

    // 菜单只在鼠标指向标题行时出现（设计如此）—— 直接派发 pointerenter 把它请出来
    header.dispatchEvent(new PointerEvent("pointerenter", { bubbles: false }));
    await sleep(200);
    const trigger = [...header.querySelectorAll("button")].find(
      (el) => el.textContent.trim() === "帮助",
    );
    if (!trigger) return { triggerFound: false };

    trigger.click();
    await sleep(300);

    const menu = [...document.querySelectorAll('[data-scope="menu"][data-part="content"]')].at(-1);
    const menuItem = [...document.querySelectorAll('[data-scope="menu"][data-part="item"]')].at(-1);
    // 菜单面板所在的那一层（Positioner）：它就是决定 z 的那个元素
    const positioner = menu ? menu.closest('[data-scope="menu"][data-part="positioner"]') : null;
    const flowBar = [...document.querySelectorAll("div")].find((el) =>
      el.textContent.includes("导入") && el.textContent.includes("浏览") && el.clientHeight > 0,
    );

    const describe = (el) => {
      if (!el) return null;
      const cs = getComputedStyle(el);
      return {
        tag: el.tagName.toLowerCase(),
        cls: (el.getAttribute("class") ?? "").slice(0, 80),
        z: cs.zIndex,
        position: cs.position,
      };
    };
    // 往上收集「造出层叠上下文」的祖先：z-index 非 auto / transform / filter / isolation
    const contextChain = (el) => {
      const out = [];
      let node = el;
      while (node && node !== document.documentElement) {
        const cs = getComputedStyle(node);
        if (
          cs.zIndex !== "auto" ||
          cs.transform !== "none" ||
          cs.filter !== "none" ||
          (cs.isolation !== undefined && cs.isolation !== "auto")
        ) {
          out.push({ tag: node.tagName.toLowerCase(), z: cs.zIndex, transform: cs.transform, isolation: cs.isolation });
        }
        node = node.parentElement;
      }
      return out;
    };

    // 诊断：内联样式（Ark 会写 style）
    const inlineStyle = positioner ? (positioner.getAttribute("style") ?? "") : "";

    const result = {
      triggerFound: true,
      menuFound: Boolean(menu),
      positioner: describe(positioner),
      inlineStyle: inlineStyle.slice(0, 240),
      // 变量解析值：--z-index 到底是多少、--z-popover 认不认得、我们的规则匹配上了吗
      zVar: positioner ? getComputedStyle(positioner).getPropertyValue("--z-index").trim() : null,
      zPopoverVar: positioner
        ? getComputedStyle(positioner).getPropertyValue("--z-popover").trim()
        : null,
      matchesRule: positioner
        ? positioner.matches('[data-scope="menu"][data-part="positioner"]')
        : null,
    };
    if (!menu) return result;

    const menuRect = menu.getBoundingClientRect();
    result.menuRect = { top: Math.round(menuRect.top), bottom: Math.round(menuRect.bottom), left: Math.round(menuRect.left), height: Math.round(menuRect.height) };

    if (flowBar) {
      const flowRect = flowBar.getBoundingClientRect();
      const top = Math.max(menuRect.top, flowRect.top);
      const bottom = Math.min(menuRect.bottom, flowRect.bottom);
      result.overlapH = Math.round(bottom - top);
      if (bottom - top > 1) {
        const x = Math.round(menuRect.left + Math.min(20, menuRect.width / 2));
        const y = Math.round((top + bottom) / 2);
        const hit = document.elementFromPoint(x, y);
        result.probe = { x, y };
        result.hit = describe(hit);
        result.hitInsideMenu = Boolean(hit && menu.contains(hit));
        result.hitInsideFlow = Boolean(hit && flowBar.contains(hit));
        result.hitChain = contextChain(hit ?? document.body);
        result.menuChain = contextChain(menu);
      }
    }

    // 关掉菜单，别影响后面的断言
    document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    await sleep(200);
    return result;
  })()`);

  if (layers === null) {
    problems.push("应用外壳里找不到标题行（header）");
  } else if (layers.triggerFound === false) {
    problems.push("标题行里找不到「帮助」菜单入口");
  } else if (!layers.menuFound) {
    problems.push("点了「帮助」但菜单没出来");
  } else if (layers.overlapH !== undefined && layers.overlapH > 1) {
    if (layers.hitInsideMenu !== true) {
      problems.push(
        "帮助菜单被下面的元素压住了：重叠区中心命中的是 " +
          JSON.stringify(layers.hit) +
          "（应当是菜单内部）",
      );
    }
  }

  /*
   * **改窗口尺寸不许卡死**（2026-09-16 真机回归的断言）。
   *
   * 真机症状：一最大化就卡死、界面乱缩。根因是「resize → 写状态 → 重渲染 →
   * splitter 收到新 props → 又一次 resize」的回路。这里把窗口从 1440×900 改到
   * 1100×700 再改回来：只要页面还答得上话、面板尺寸还正常，就说明没有回路。
   * 页面真卡死的话，上面的 CDP 时限会先报出来（不会静默挂住）。
   */
  let resizeProbe = null;
  try {
    await send("Emulation.setDeviceMetricsOverride", {
      width: 1100,
      height: 700,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await sleep(400);
    resizeProbe = await evaluate(`(() => {
      const panes = [...document.querySelectorAll('[data-scope="splitter"] [data-part="panel"]')];
      const triggers = document.querySelectorAll('[data-part="resize-trigger"]');
      return {
        responsive: true,
        paneCount: panes.length,
        // 面板被压成 0 高/0 宽就是「异常缩放」的样子
        minSize: panes.length === 0
          ? -1
          : Math.min(...panes.map((el) => {
              const rect = el.getBoundingClientRect();
              return Math.round(Math.min(rect.width, rect.height));
            })),
        triggerCount: triggers.length,
      };
    })()`);
    await send("Emulation.clearDeviceMetricsOverride");
  } catch (error) {
    problems.push(`改窗口尺寸后页面没有回应：${error.message} —— 可能是 resize 回路（真机症状：最大化卡死）`);
  }

  if (resizeProbe) {
    if (resizeProbe.paneCount === 0 || resizeProbe.triggerCount === 0) {
      problems.push("改尺寸之后 splitter 不见了");
    }
    if (resizeProbe.minSize <= 0) {
      problems.push(
        `改尺寸之后有面板的宽/高塌成 0（最小 ${resizeProbe.minSize}px）—— 异常缩放`,
      );
    }
  }

  if (workspace) {
    if (workspace.width <= 0) {
      problems.push("右列（库）宽度为 0 —— 面板尺寸没接上");
    }
    if (!workspace.createFound) {
      problems.push("右列里找不到「+ 新建库」入口");
    }
    if (!workspace.avoidDuplicates) {
      problems.push("右列里没有「避免重复导入」复选框");
    }
    if (workspace.importDisabled !== true) {
      problems.push(
        `左边一个目录都没勾选时，「导入」按钮必须禁用（实际 disabled=${workspace.importDisabled}）`,
      );
    }
    if (!workspace.hint) {
      problems.push("「导入」禁用时没有给出一句原因（先在左边勾选目录、再选一个库）");
    }
    if (workspace.emptyState !== true && !workspace.summary) {
      problems.push("右列既没有库列表的形态，也没有空态文案");
    }
    if (workspace.createFound) {
      if (workspace.dialog) {
        if (!workspace.dialog.hasNameField) problems.push("建库弹窗里没有「名称」字段");
        if (!workspace.dialog.hasPathField) problems.push("建库弹窗里没有「库根目录」字段");
        if (workspace.dialog.submitDisabled !== true) {
          problems.push("建库弹窗：路径为空时「新建库」必须禁用");
        }
        if (!workspace.dialog.noHintYet) {
          problems.push("建库弹窗：路径为空时不该出现判定行");
        }
        if (workspace.dialogClosed === false) {
          problems.push("建库弹窗点了「取消」没关上");
        }
      } else {
        problems.push("点了「+ 新建库」但弹窗没出现");
      }
    }
  }

  // 外壳页自己加载出来的报错也要算上
  /*
   * 刷新之后宽度比例要还原：这是「重启还原」在浏览器里能做到的最接近的验证
   * （真机上是关掉程序再开）。
   */
  if (widthHandle && typeof widthHandle.storedLeftRatio === "number") {
    await send("Page.reload", { ignoreCache: false });
    if ((await waitForContent(send))) {
      // 轮询等左列挂上：#root 有内容只说明外壳渲染了，工作区可能还差一拍
      let handleReady = false;
      for (let attempt = 0; attempt < 100 && !handleReady; attempt += 1) {
        handleReady = await evaluate(
          'Boolean(document.querySelector(\'[role="separator"][aria-label="调整左列宽度"]\'))',
        );
        if (!handleReady) await sleep(150);
      }
      const restored = await evaluate(`(() => {
        const handle = document.querySelector('[role="separator"][aria-label="调整左列宽度"]');
        if (!handle) return null;
        const aside = handle.previousElementSibling;
        const container = handle.parentElement;
        if (!aside || !container) return null;
        return {
          ratio:
            Math.round(
              (aside.getBoundingClientRect().width /
                container.getBoundingClientRect().width) *
                1000,
            ) / 1000,
          expected: ${JSON.stringify(1)},
        };
      })()`);
      if (restored === null) {
        problems.push("刷新之后找不到左列把手");
      } else {
        const expected = widthHandle.storedLeftRatio;
        if (Math.abs(restored.ratio - expected) > 0.03) {
          problems.push(
            `刷新之后左列宽度没有还原：实际 ${restored.ratio}，存的 ${expected}`,
          );
        }
      }
    } else {
      problems.push("刷新之后页面没渲染出来（白屏）");
    }
  }

  collectConsoleProblems(events.slice(workspaceEventsFrom));

  console.log(
    JSON.stringify(
      {
        url,
        chrome: chromePath,
        snapshot,
        interact,
        shell,
        splitter,
        importDialog,
        deadBackend,
        leftColumn,
        widthHandle,
        dialogFrame,
        repoCards,
        switchBar,
        layers,
        resizeProbe,
        dirTree,
        workspace,
        problems,
      },
      null,
      2,
    ),
  );
} catch (error) {
  problems.push(`冒烟脚本自身失败：${error.message}`);
  // 带上堆栈：脚本自身的错（比如某段 evaluate 里的变量名写错）光看消息定不了位
  console.error(error.stack ?? String(error));
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
/*
 * 等页面渲染出内容。**给到 60 秒**：改完代码后 dev server 要重新编译并重新预打包依赖，
 * 冷启动实测能到十几秒 —— 阈值太紧会把「慢」误报成「白屏」，而误报比不报更消耗信任。
 */
async function waitForContent(send, timeoutMs = 60_000) {
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
