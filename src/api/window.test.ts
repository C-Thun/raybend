/**
 * 窗口控制的单元测试。
 *
 * 用假句柄跑，不启动 Tauri、不需要 DOM。重点覆盖三类真会咬人的情况：
 *   1. **浏览器降级**：不在 Tauri 里时一个窗口方法都不能被调用
 *   2. **状态同步**：最大化/还原（含用 Win 快捷键触发的）要让图标跟着变
 *   3. **失败路径**：API 抛错不能把界面带崩，也不能让窗口变得关不掉
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  createWindowChrome,
  INITIAL_WINDOW_CHROME,
  uiReady,
  windowControlView,
  type WindowChromeState,
  type WindowHandle,
} from "./window.ts";

/** 可控的假句柄：记录调用、可注入错误、可手动触发尺寸事件 */
function fakeHandle(
  options: {
    maximized?: boolean;
    decorated?: boolean;
    failOn?: (keyof WindowHandle)[];
  } = {},
) {
  const calls: string[] = [];
  let maximized = options.maximized ?? false;
  const resizeHandlers: Array<() => void> = [];
  let unlistened = false;

  const guard = (method: keyof WindowHandle) => {
    calls.push(method);
    if (options.failOn?.includes(method)) throw new Error(`${method} 炸了`);
  };

  const handle: WindowHandle = {
    async minimize() {
      guard("minimize");
    },
    async toggleMaximize() {
      guard("toggleMaximize");
      maximized = !maximized;
    },
    async close() {
      guard("close");
    },
    async isMaximized() {
      guard("isMaximized");
      return maximized;
    },
    async isDecorated() {
      guard("isDecorated");
      return options.decorated ?? false;
    },
    async onResized(handler: () => void) {
      guard("onResized");
      resizeHandlers.push(handler);
      return () => {
        unlistened = true;
        const index = resizeHandlers.indexOf(handler);
        if (index >= 0) resizeHandlers.splice(index, 1);
      };
    },
  };

  return {
    handle,
    calls,
    get unlistened() {
      return unlistened;
    },
    setMaximized(next: boolean) {
      maximized = next;
    },
    emitResize() {
      for (const handler of [...resizeHandlers]) handler();
    },
    handlerCount: () => resizeHandlers.length,
  };
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

/* ─── 纯函数 ───────────────────────────────────────────── */

test("windowControlView：浏览器里（available=false）永远不显示三键", () => {
  const state: WindowChromeState = {
    available: false,
    undecorated: true,
    maximized: false,
  };
  assert.equal(windowControlView(state).visible, false);
});

test("windowControlView：有系统标题栏时不显示三键（避免两套按钮）", () => {
  const state: WindowChromeState = {
    available: true,
    undecorated: false,
    maximized: false,
  };
  assert.equal(windowControlView(state).visible, false);
});

test("windowControlView：沉浸式 + 可用 → 显示；图标按最大化状态切换", () => {
  const base: WindowChromeState = {
    available: true,
    undecorated: true,
    maximized: false,
  };
  assert.deepEqual(windowControlView(base), {
    visible: true,
    action: "maximize",
  });
  assert.deepEqual(windowControlView({ ...base, maximized: true }), {
    visible: true,
    action: "restore",
  });
});

test("初始状态：一切都关着（免得 SSR/首帧闪出一排按钮）", () => {
  assert.deepEqual(INITIAL_WINDOW_CHROME, {
    available: false,
    undecorated: false,
    maximized: false,
  });
});

/* ─── 浏览器降级 ────────────────────────────────────────── */

test("不在 Tauri 里：不取句柄、不订阅、动作一律不调用", async () => {
  const fake = fakeHandle();
  const chrome = createWindowChrome({
    handle: fake.handle,
    runtime: () => false,
  });
  await tick();

  assert.deepEqual(fake.calls, [], "浏览器里不该碰任何窗口 API");
  assert.equal(chrome.view().visible, false);

  assert.equal(await chrome.minimize(), false);
  assert.equal(await chrome.toggleMaximize(), false);
  assert.equal(await chrome.close(), false);
  assert.deepEqual(fake.calls, []);
  chrome.dispose();
});

test("在 Tauri 里但拿不到句柄（IPC 未就绪）：同样安全降级", async () => {
  const chrome = createWindowChrome({ handle: null, runtime: () => true });
  await tick();
  assert.equal(chrome.view().visible, false);
  assert.equal(await chrome.close(), false);
  chrome.dispose();
});

test("后接句柄（真实路径）：attach 之后才显示三键", async () => {
  const fake = fakeHandle({ decorated: false });
  const chrome = createWindowChrome({ runtime: () => true });
  await tick();
  assert.equal(
    chrome.state().available,
    false,
    "attach 之前必须保持降级——首帧渲染是同步的",
  );
  assert.equal(chrome.view().visible, false);

  await chrome.attach(fake.handle);
  assert.equal(chrome.state().available, true);
  assert.equal(chrome.view().visible, true);
  assert.equal(await chrome.minimize(), true);
  chrome.dispose();
});

test("attach(null)：从句柄退回到降级，并取消旧订阅", async () => {
  const fake = fakeHandle();
  const chrome = createWindowChrome({
    handle: fake.handle,
    runtime: () => true,
  });
  await tick();
  await tick();
  assert.equal(chrome.view().visible, true);

  await chrome.attach(null);
  assert.equal(chrome.state().available, false);
  assert.equal(chrome.view().visible, false);
  assert.equal(fake.unlistened, true, "换句柄时必须先退掉旧的订阅");
  assert.equal(await chrome.close(), false);
  chrome.dispose();
});

test("attach 两次：只保留最新句柄，旧句柄不再被调用", async () => {
  const first = fakeHandle();
  const second = fakeHandle();
  const chrome = createWindowChrome({ runtime: () => true });

  await chrome.attach(first.handle);
  await chrome.attach(second.handle);
  first.calls.length = 0;

  assert.equal(await chrome.minimize(), true);
  assert.deepEqual(first.calls, [], "旧句柄不能再收到任何调用");
  assert.ok(second.calls.includes("minimize"));
  chrome.dispose();
});

test("dispose 之后再 attach 不生效（组件已卸载）", async () => {
  const fake = fakeHandle();
  const chrome = createWindowChrome({ runtime: () => true });
  chrome.dispose();
  await chrome.attach(fake.handle);
  await tick();
  assert.equal(chrome.state().available, false);
  assert.deepEqual(fake.calls, []);
});

/* ─── 正常路径 ─────────────────────────────────────────── */

test("沉浸式窗口：读外框状态 → 显示三键；并订阅尺寸变化", async () => {
  const fake = fakeHandle({ decorated: false });
  const chrome = createWindowChrome({
    handle: fake.handle,
    runtime: () => true,
  });
  await tick();
  await tick();

  assert.equal(chrome.state().available, true);
  assert.equal(chrome.state().undecorated, true);
  assert.equal(chrome.view().visible, true);
  assert.ok(fake.calls.includes("isDecorated"), "必须问窗口而不是靠常量猜");
  assert.ok(fake.calls.includes("onResized"));
  chrome.dispose();
});

test("有系统标题栏的窗口（WSL/Linux 开发配置）：不显示三键", async () => {
  const fake = fakeHandle({ decorated: true });
  const chrome = createWindowChrome({
    handle: fake.handle,
    runtime: () => true,
  });
  await tick();
  await tick();

  assert.equal(chrome.state().undecorated, false);
  assert.equal(chrome.view().visible, false);
  chrome.dispose();
});

test("三个动作分别转发到对应方法，且返回 true", async () => {
  const fake = fakeHandle();
  const chrome = createWindowChrome({
    handle: fake.handle,
    runtime: () => true,
  });
  await tick();

  assert.equal(await chrome.minimize(), true);
  assert.equal(await chrome.toggleMaximize(), true);
  assert.equal(await chrome.close(), true);
  assert.deepEqual(
    fake.calls.filter(
      (name) =>
        name !== "isDecorated" &&
        name !== "isMaximized" &&
        name !== "onResized",
    ),
    ["minimize", "toggleMaximize", "close"],
  );
  chrome.dispose();
});

test("最大化状态：toggle 之后图标变成「还原」", async () => {
  const fake = fakeHandle({ maximized: false });
  const chrome = createWindowChrome({
    handle: fake.handle,
    runtime: () => true,
  });
  await tick();

  assert.equal(chrome.view().action, "maximize");
  await chrome.toggleMaximize();
  assert.equal(chrome.state().maximized, true);
  assert.equal(chrome.view().action, "restore");

  await chrome.toggleMaximize();
  assert.equal(chrome.view().action, "maximize");
  chrome.dispose();
});

test("尺寸事件（含 Win 快捷键最大化）会让状态跟上", async () => {
  const fake = fakeHandle({ maximized: false });
  const chrome = createWindowChrome({
    handle: fake.handle,
    runtime: () => true,
  });
  await tick();
  await tick();

  fake.setMaximized(true);
  fake.emitResize();
  await tick();
  assert.equal(
    chrome.state().maximized,
    true,
    "窗口被外部最大化后，我们的图标必须跟着变",
  );

  fake.setMaximized(false);
  fake.emitResize();
  await tick();
  assert.equal(chrome.state().maximized, false);
  chrome.dispose();
});

/* ─── 失败路径 ─────────────────────────────────────────── */

test("读外框状态失败 → 按沉浸式处理（宁可多画三键，也不能让窗口关不掉）", async () => {
  const fake = fakeHandle({ failOn: ["isDecorated"] });
  const chrome = createWindowChrome({
    handle: fake.handle,
    runtime: () => true,
  });
  await tick();
  await tick();

  assert.equal(chrome.state().undecorated, true);
  assert.equal(chrome.view().visible, true);
  chrome.dispose();
});

test("动作抛错 → 返回 false，不把异常丢给界面", async () => {
  const fake = fakeHandle({ failOn: ["minimize", "close"] });
  const chrome = createWindowChrome({
    handle: fake.handle,
    runtime: () => true,
  });
  await tick();

  assert.equal(await chrome.minimize(), false);
  assert.equal(await chrome.close(), false);
  chrome.dispose();
});

test("订阅失败不致命：三键仍然可用（只是最大化图标可能不刷新）", async () => {
  const fake = fakeHandle({ failOn: ["onResized"] });
  const chrome = createWindowChrome({
    handle: fake.handle,
    runtime: () => true,
  });
  await tick();
  await tick();

  assert.equal(chrome.view().visible, true);
  assert.equal(await chrome.minimize(), true);
  chrome.dispose();
});

test("dispose 之后取消订阅，事件不再改状态", async () => {
  const fake = fakeHandle({ maximized: false });
  const chrome = createWindowChrome({
    handle: fake.handle,
    runtime: () => true,
  });
  await tick();
  await tick();

  chrome.dispose();
  assert.equal(
    fake.unlistened,
    true,
    "必须真的取消订阅，否则组件卸载后还在被回调",
  );

  fake.setMaximized(true);
  fake.emitResize();
  await tick();
  assert.equal(chrome.state().maximized, false, "dispose 后不该再更新状态");
});

test("重复 dispose 与从未初始化就 dispose 都不抛错", async () => {
  const fake = fakeHandle();
  const chrome = createWindowChrome({
    handle: fake.handle,
    runtime: () => true,
  });
  chrome.dispose();
  chrome.dispose();
  await tick();
  assert.ok(true);
});

/*
 * 启动闪屏的收尾（`uiReady`）。
 *
 * 它跑在**启动路径**上：一旦抛错，整个外壳的挂载都会被带崩 ——
 * 而这条路径在正常开发里几乎不会被走到（浏览器里它直接返回）。
 * 所以这里只钉两件事：**浏览器里静默返回**、**Tauri 里拿不到 API 也不抛**。
 */
test("uiReady：浏览器环境里静默返回，不做任何事", async () => {
  // Node 里没有 Tauri 标记（`detectRuntime` 会判成 browser）
  await uiReady(); // 不抛即为通过
  assert.ok(true);
});

test("uiReady：真的在 Tauri 里但命令失败时也不抛（闪屏收尾不该带崩启动）", async () => {
  const scope = globalThis as Record<string, unknown>;
  scope["__TAURI_INTERNALS__"] = { invoke: () => Promise.reject(new Error("命令挂了")) };
  try {
    await uiReady();
  } finally {
    delete scope["__TAURI_INTERNALS__"];
  }
  assert.ok(true, "命令失败被吞掉 —— 主窗口已经在那儿了，只是多一张图挡着");
});
