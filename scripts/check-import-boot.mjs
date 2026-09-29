#!/usr/bin/env node
/**
 * 导入工作区冒烟：**点「导入」先确认**（崔总 2026-09-28 要求的那道闸）。
 *
 * 走一遍真实操作：左列勾选一个目录 → 右列选中库 → 点「导入」→ 弹出确认窗
 * （上面已选目录 / 中间宽扁的向下箭头 / 下面选中的库卡片）→ 取消（**不能**调
 * `import_start`）→ 再点「导入」+「开始导入」→ 确认窗关掉、`import_start` 收到
 * 正确的库与源、顶上来的是既有的「导入中」进度窗。
 *
 * 为什么单独一个脚本：`check:browse` 的假后端是浏览侧的（没有 `source_scan` /
 * `source_count` / `import_start` 这套导入命令），`smoke:ui` 则完全没有假后端
 * （导入按钮一直是禁用的）—— 导入**开工那条路**此前没有任何脚本走过。
 *
 * 用法：另开一个终端 `pnpm dev`，然后 `pnpm check:import [url]`。
 * 不做视觉判断（色彩、间距、手感归人类真机确认）。
 */

import assert from "node:assert/strict";
import { launchChrome, connectCdp, sleep } from "./lib/cdp.mjs";

const url = process.argv[2] ?? "http://localhost:1420/";
const port = Number(process.env.CDP_PORT ?? 9519);
const response = await fetch(url);
assert(response.ok, "Start pnpm dev before running check:import");

function backend() {
  const repo = {
    id: "repro0000000000",
    name: "测试库",
    importTemplate: ":FILENAME",
    createdAt: 1_700_000_000_000,
    lastOpenedAt: null,
    online: true,
    root: "C:/Library/demo",
    displayPath: "C:/Library/demo",
    paths: [],
    photosCount: 7,
    imagesCount: 7,
    triedPaths: 0,
  };
  repo.connection = {repositoryId:repo.id,state:"online",reason:null,root:repo.root,generation:"1",revision:"1",observedAt:1700000000000};
  const handlers = new Map();
  let next = 0;
  window.__importTest = { calls: [], volumes: [{ path: "C:\\", kind: "local", kindLabel: "本地磁盘" }] };
  window.__importTest.progress = {batchId:"batch-1",state:"running",stage:"scan",total:0,done:0,imported:0,skipped:0,duplicates:0,failed:0,bytes:0,errors:[],errorsTotal:0,freeBytes:null,startedAt:0,finishedAt:null,currentRun:null,runs:[]};
  localStorage.clear();
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  window.__importTest.emit = (progress) => {
    window.__importTest.progress = progress;
    for (const [id, listener] of handlers) if (listener.event === "import://progress") listener.handler({event:"import://progress",id,payload:progress});
  };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    transformCallback: (cb) => cb,
    unregisterCallback: () => {},
    convertFileSrc: (p) => p,
    invoke: async (cmd, args = {}) => {
      window.__importTest.calls.push(cmd);
      if (cmd === "plugin:event|listen") { const id = ++next; handlers.set(id, args); return id; }
      if (cmd === "plugin:event|unlisten") { handlers.delete(args.eventId); return; }
      if (cmd === "repository_remount") return structuredClone(repo);
      if (cmd === "repositories_list") return [structuredClone(repo)];
      if (cmd === "volumes_list") return [...window.__importTest.volumes];
      if (cmd === "recent_dirs_list") return [];
      if (cmd === "source_paths_status") return (args.paths ?? []).map(() => !window.__importTest.offline);
      if (cmd === "dir_list") {
        return String(args.path).replaceAll("\\", "/") === "C:/"
          ? [{ name: "photos", path: "C:\\photos", hasChildren: false }]
          : [];
      }
      if (cmd === "source_count") return { photos: 2, skipped: 0, truncated: false };
      if (cmd === "source_scan") return { root: String(args.path), items: [], skipped: 0, problems: [], elapsedMs: 1 };
      if (cmd === "import_precheck") return { tight: false, neededBytes: 1024, freeBytes: 1_000_000_000 };
      if (cmd === "import_start") { window.__importTest.start = args; return { batchId: "batch-1", runs: [] }; }
      if (cmd === "import_status" || cmd === "import_resume" || cmd === "import_cancel") return window.__importTest.progress ?? null;
      if (cmd === "repository_release") {
        repo.online=false; repo.root=null; repo.connection={repositoryId:repo.id,state:"released",reason:null,root:null,generation:"1",revision:"3",observedAt:1};
        return structuredClone(repo);
      }
      if (cmd === "setting_get") return null;
      if (cmd === "setting_set") return null;
      if (["thumb_get", "view_image", "export_variant_image"].includes(cmd))
        return Uint8Array.from(atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR4nGNgAAIAAAUAAXpeqz8AAAAASUVORK5CYII="), (c) => c.charCodeAt(0)).buffer;
      return null;
    },
  };
}

const chrome = launchChrome({ port });
let cdp;
try {
  cdp = await connectCdp(port);
  await cdp.send("Runtime.enable");
  await cdp.send("Page.enable");
  await cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: `(${backend.toString()})();` });
  await cdp.send("Page.navigate", { url });
  const evaluate = async (expression) => {
    const answer = await cdp.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (answer.exceptionDetails) throw new Error(answer.exceptionDetails.exception?.description ?? answer.exceptionDetails.text);
    return answer.result?.value;
  };
  const until = async (expression, label) => {
    const end = Date.now() + 30000;
    while (Date.now() < end) { if (await evaluate(expression)) return; await sleep(100); }
    throw new Error(label + ": " + JSON.stringify({ text: await evaluate("document.body.innerText.slice(0,1200)"),
      calls: await evaluate("window.__importTest?.calls.slice(-30)"), errors: cdp.consoleErrors, exceptions: cdp.exceptions,
      ready: await evaluate("document.readyState"), url: await evaluate("location.href") }));
  };
  const click = async (expression, label) => {
    assert(await evaluate(`!!(${expression})`), `找不到要点的东西：${label}`);
    await evaluate(`(${expression}).click()`);
  };
  /*
   * 路径带反斜杠，**不能用 CSS 属性选择器**（`[title="C:\"]` 里的 `\"` 在 CSS 里
   * 是「转义引号」，选择器会静默地变成别的意思）—— 一律用 JS 比字符串。
   */
  const byTitle = (title) => `[...document.querySelectorAll('span[title]')].find(s=>s.title===${JSON.stringify(title)})`;
  const byCheckLabel = (label) => `[...document.querySelectorAll('[role=checkbox]')].find(el=>el.getAttribute('aria-label')===${JSON.stringify(label)})`;

  await until("[...document.querySelectorAll('[data-part=item]')].some(b=>b.textContent.trim()==='导入')", "flowbar startup");
  // 库卡片是 `div[role=option]`（不是 button）—— 等它渲染出来再切工作流
  await until("[...document.querySelectorAll('[role=option]')].some(el=>el.textContent.includes('测试库'))", "repository list loaded");
  await click("[...document.querySelectorAll('[data-part=item]')].find(b=>b.textContent.trim()==='导入')", "导入工作流");
  // 来源树的第一层是盘：**展开它**才看得到里面的目录（双击行 = 展开，见 TreeNode）
  await until(byTitle("C:\\"), "来源树里出现 C:\\");
  await evaluate(`${byTitle("C:\\")}.closest('[role=treeitem]').dispatchEvent(new MouseEvent('dblclick',{bubbles:true}))`);
  await until(byCheckLabel("勾选 C:\\photos"), "展开后看到子目录 C:\\photos");

  const importButton = "[...document.querySelectorAll('button:not([data-part=item])')].find(b=>b.textContent.trim()==='导入')";
  assert(await evaluate(`(${importButton}).disabled`), "一个目录都没勾选时「导入」应当是禁用的");

  // ① 勾选一个源目录：它应当出现在左列「已选目录」里
  await click(byCheckLabel("勾选 C:\\photos"), "目录勾选圈");
  await until("document.querySelector('[data-import-confirm]')===null && [...document.querySelectorAll('button')].some(b=>b.textContent.includes('C:\\\\photos'))", "checked dir shows up");

  await click(byTitle("C:\\photos"), "选中来源目录");
  await until("window.__importTest.calls.includes('source_scan')", "来源照片清单初次读取");
  const scansBefore = await evaluate("window.__importTest.calls.filter(c=>c==='source_scan').length");
  const dirsBefore = await evaluate("window.__importTest.calls.filter(c=>c==='dir_list').length");
  await evaluate("window.__importTest.offline=true; window.__importTest.volumes=[]");
  await until(`!(${byTitle("C:\\")})`, "拔盘后驱动器退场");
  assert(await evaluate("[...document.querySelectorAll('button')].some(b=>b.textContent.includes('C:\\\\photos'))"), "拔盘保留已勾选目录");
  await evaluate("window.__importTest.offline=false; window.__importTest.volumes=[{path:'C:\\\\',kind:'local',kindLabel:'本地磁盘'}]");
  await until(byCheckLabel("勾选 C:\\photos"), "重新插盘保留展开并重读子目录");
  await until(`window.__importTest.calls.filter(c=>c==='source_scan').length>${scansBefore}`, "同路径恢复重新读取图片区");
  assert(await evaluate(`window.__importTest.calls.filter(c=>c==='dir_list').length>${dirsBefore}`), "恢复读取展开树");

  // ② 选中目标库（右列库卡片）
  await click("[...document.querySelectorAll('[role=option]')].find(el=>el.textContent.includes('测试库'))", "库卡片");
  await until(`!(${importButton}).disabled`, "勾了目录 + 选了库之后「导入」可点");

  // ③ 点「导入」→ 确认窗（不是直接开工）
  await click(importButton, "导入按钮");
  await until("document.querySelector('[data-import-confirm]')", "确认弹窗出现");
  assert(!await evaluate("window.__importTest.start"), "确认之前不许调 import_start");
  const dialog = await evaluate(`(()=>{
    const box=document.querySelector('[data-import-confirm]');
    const dirs=document.querySelector('[data-import-confirm-dirs]');
    const repo=document.querySelector('[data-import-confirm-repo]');
    const text=box.innerText;
    const arrow=box.querySelector('[data-import-confirm-arrow] svg');
    const buttons=[...document.querySelectorAll('[role=dialog] button')].map(b=>b.textContent.trim()).filter(Boolean);
    return {
      dirs: dirs.innerText.trim(),
      dirRows: dirs.querySelectorAll('div').length,
      repo: repo.innerText.trim(),
      arrow: arrow ? {className:arrow.getAttribute('class'),size:Math.round(arrow.getBoundingClientRect().width)} : null,
      text,
      buttons,
      hasSwitch: !!box.querySelector('[role=switch]'),
      hasRemove: !!box.querySelector('[aria-label^="移除"]'),
    };
  })()`);
  assert.match(dialog.dirs, /C:\\photos/, "确认窗要写出选了哪些目录：" + JSON.stringify(dialog));
  assert.match(dialog.repo, /测试库/, "确认窗要写出进哪个库：" + JSON.stringify(dialog));
  assert.match(dialog.text, /已选目录/, "目录那一段要有标题：" + JSON.stringify(dialog));
  assert.match(dialog.text, /导入到/, "库那一段要有标题：" + JSON.stringify(dialog));
  // 底部动作区：取消在左、往前进类在右（弹窗里还有目录行的路径按钮与右上角的叉，都过滤掉）
  assert.deepEqual(dialog.buttons.slice(-2), ["取消", "开始导入"], "取消在左、确认在右：" + JSON.stringify(dialog.buttons));
  // 中间那个箭头：宽扁的 chevron-down（大号、指向下方），不是细长竖箭头
  assert(dialog.arrow !== null, "中间要有向下箭头：" + JSON.stringify(dialog));
  assert.match(dialog.arrow.className ?? "", /chevron-down/, "箭头要用宽扁的 chevron-down：" + JSON.stringify(dialog.arrow));
  assert(dialog.arrow.size >= 40, "箭头要大号（视觉分隔上下两段）：" + JSON.stringify(dialog.arrow));
  assert(!dialog.hasSwitch && !dialog.hasRemove, "确认窗里是只读展示（没有开关、没有移除）：" + JSON.stringify(dialog));

  // ④ 取消：窗关掉，且一条命令都没往后端发
  await click("[...document.querySelectorAll('[role=dialog] button')].find(b=>b.textContent.trim()==='取消')", "取消按钮");
  await until("document.querySelector('[data-import-confirm]')===null", "取消后确认窗关掉");
  assert(!await evaluate("window.__importTest.start"), "取消之后不许开工");
  assert(!await evaluate("window.__importTest.calls.includes('import_precheck')"), "取消之后连预检都不该跑");

  // ⑤ 再点「导入」+「开始导入」：确认窗关掉、后端收到正确的库与源、顶上「导入中」进度窗
  await click(importButton, "导入按钮（第二次）");
  await until("document.querySelector('[data-import-confirm]')", "确认弹窗再次出现");
  await click("[...document.querySelectorAll('[role=dialog] button')].find(b=>b.textContent.trim()==='开始导入')", "开始导入按钮");
  await until("document.querySelector('[data-import-confirm]')===null", "确认后确认窗关掉");
  await until("window.__importTest.start", "开始导入后后端收到 import_start");
  const started = await evaluate("window.__importTest.start");
  assert.equal(started.repositoryId, "repro0000000000", "要导进选中的那个库：" + JSON.stringify(started));
  assert.deepEqual(started.sources, [{ path: "C:\\photos", includeSubdirs: false }], "源目录与「包含子目录」开关要如实传：" + JSON.stringify(started));
  await until("[...document.querySelectorAll('[role=dialog]')].some(d=>d.innerText.includes('导入中'))", "顶上来的是「导入中」进度窗");

  await evaluate(`window.__importTest.emit({batchId:"batch-1",state:"waiting",stage:"import",total:2,done:1,imported:1,skipped:0,duplicates:0,failed:0,bytes:5,errors:[],errorsTotal:0,freeBytes:null,startedAt:0,finishedAt:null,currentRun:0,runs:[{runId:1,sourceRoot:"C:/photos",state:"waiting",stage:"import",total:2,done:1,imported:1,skipped:0,duplicates:0,failed:0,bytes:5,scanned:2,current:null,note:"storage.wait.source"}]})`);
  await until("[...document.querySelectorAll('[role=dialog]')].some(d=>d.innerText.includes('等待设备连接') && d.innerText.includes('来源暂时不可用'))", "等待以中性说明呈现，已导入计数保留");
  const buttons=await evaluate("[...document.querySelectorAll('[role=dialog] button')].map(b=>b.textContent.trim())");
  assert(buttons.includes("重新检查")); assert(buttons.includes("取消导入"));
  await evaluate(`window.__importTest.emit({...window.__importTest.progress,state:"done",stage:"done",finishedAt:1})`);
  await until("[...document.querySelectorAll('[role=dialog]')].some(d=>d.innerText.includes('导入完成'))", "等待任务完成后进入终态");
  await click("[...document.querySelectorAll('[role=dialog] button')].find(b=>b.textContent.trim()==='关闭')", "关闭导入进度");
  await evaluate("[...document.querySelectorAll('button')].find(b=>b.getAttribute('aria-label')==='库设置')?.click()");
  await until("document.querySelector('[data-repository-release]')", "库设置提供释放入口");
  await click("[...document.querySelectorAll('[role=dialog] button')].find(b=>b.textContent.trim()==='释放此库')", "释放库");
  await until("[...document.querySelectorAll('[role=dialog]')].some(d=>d.innerText.includes('释放此库？'))", "释放需要明确确认");
  await evaluate("[...document.querySelectorAll('[role=dialog] button')].filter(b=>b.textContent.trim()==='释放此库').at(-1)?.click()");
  await until("document.querySelector('[data-repository-release]')?.textContent.includes('已释放')", "释放后停止自动连接，设置入口继续可达");

  assert.deepEqual(cdp.exceptions, []);
  assert.deepEqual(cdp.consoleErrors, []);
  console.log("✓ import: 勾选目录 → 选库 → 确认窗（已选目录 / 向下箭头 / 库卡片）→ 取消不开工 → 确认后开工");
} finally {
  cdp?.close();
  chrome.kill("SIGKILL");
}
