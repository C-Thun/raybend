/**
 * 窗口控制的前端封装（`ARCHITECTURE.md` §1.1 的 `src/api/` 层）。
 *
 * 覆盖两件事：
 *   1. **沉浸式外框的判定**：窗口是不是由我们接管（`decorations: false`）。
 *      判定用运行时的 `isDecorated()`，而不是把平台配置抄成前端常量 ——
 *      `src-tauri/tauri.linux.conf.json` 会在 Linux/WSL 下保留系统标题栏，
 *      抄常量就必然漂移，而 `isDecorated()` 永远等于真相。
 *   2. **窗口三键**：最小化 / 最大化-还原 / 关闭，以及最大化状态的同步。
 *
 * 全部依赖以参数注入（`WindowHandle`），所以单元测试不需要启动 Tauri、
 * 也不需要 DOM —— 这是刻意的：窗口控制出错的代价是「关不掉窗口」。
 */

import { createSignal } from "solid-js";
import { isTauriRuntime } from "./tauri-env.ts";

/** 窗口控制的最小接口。真实的 Tauri 窗口对象由 `tauriWindowHandle()` 适配 */
export interface WindowHandle {
  minimize: () => Promise<void>;
  toggleMaximize: () => Promise<void>;
  close: () => Promise<void>;
  isMaximized: () => Promise<boolean>;
  isDecorated: () => Promise<boolean>;
  /** 订阅尺寸变化（最大化/还原也会触发）；返回取消订阅函数 */
  onResized: (handler: () => void) => Promise<() => void>;
}

export interface WindowChromeState {
  /** 能不能用窗口 API（不在 Tauri 里就是 false —— 例如 `pnpm dev` 的浏览器） */
  available: boolean;
  /** 窗口外框是否由我们接管。决定要不要画拖拽区与窗口三键 */
  undecorated: boolean;
  maximized: boolean;
}

export const INITIAL_WINDOW_CHROME: WindowChromeState = {
  available: false,
  undecorated: false,
  maximized: false,
};

export interface WindowControlView {
  /** 三键是否该显示 */
  visible: boolean;
  /** 最大化键此刻是「最大化」还是「还原」 */
  action: "maximize" | "restore";
}

/**
 * 纯函数：由窗口状态推出三键的表现。
 *
 * **只在沉浸式且能力可用时显示**：浏览器里显示它们就是摆设，
 * 而有系统标题栏时再显示一套重复按钮只会让人误点。
 */
export function windowControlView(state: WindowChromeState): WindowControlView {
  return {
    visible: state.available && state.undecorated,
    action: state.maximized ? "restore" : "maximize",
  };
}

export interface WindowChromeDeps {
  /**
   * 测试路径：直接注入句柄（`null` 表示「在 Tauri 里但拿不到句柄」）。
   * 省略时什么都不做，等 `attach()` 被叫。
   */
  handle?: WindowHandle | null;
  /** 环境判定（测试用），默认走 `isTauriRuntime()` */
  runtime?: () => boolean;
}

export interface WindowChrome {
  state: () => WindowChromeState;
  view: () => WindowControlView;
  /**
   * 接上真正的窗口句柄（或接上 `null` 表示降级）。
   *
   * 为什么是**后接**而不是构造时传：真实句柄要等 `@tauri-apps/api` 动态 import 完成，
   * 而组件必须能同步渲染第一帧。
   */
  attach: (handle: WindowHandle | null) => Promise<void>;
  minimize: () => Promise<boolean>;
  toggleMaximize: () => Promise<boolean>;
  close: () => Promise<boolean>;
  /** 取消订阅（组件卸载时调用） */
  dispose: () => void;
}

export function createWindowChrome(deps: WindowChromeDeps = {}): WindowChrome {
  const runtime = deps.runtime ?? isTauriRuntime;
  const [state, setState] = createSignal<WindowChromeState>(
    INITIAL_WINDOW_CHROME,
  );

  let handle: WindowHandle | null = null;
  let unlisten: (() => void) | undefined;
  let disposed = false;

  const patch = (next: Partial<WindowChromeState>): void => {
    if (disposed) return;
    setState((prev) => ({ ...prev, ...next }));
  };

  const readMaximized = async (): Promise<void> => {
    if (!handle) return;
    try {
      const maximized = await handle.isMaximized();
      patch({ maximized });
    } catch (error) {
      console.error("[window] 读取最大化状态失败", { error });
    }
  };

  const readDecorated = async (): Promise<void> => {
    if (!handle) return;
    try {
      const decorated = await handle.isDecorated();
      patch({ undecorated: !decorated });
    } catch (error) {
      /*
       * 读失败时**假定我们要自己画三键**：更坏的失败是「窗口既没有系统标题栏、
       * 我们也没画三键」——那样窗口就关不掉了。多画一组按钮最多是难看。
       */
      console.error("[window] 读取外框状态失败，按沉浸式处理", { error });
      patch({ undecorated: true });
    }
  };

  const subscribeResize = async (): Promise<void> => {
    if (!handle) return;
    try {
      unlisten = await handle.onResized(() => {
        void readMaximized();
      });
    } catch (error) {
      console.error("[window] 订阅尺寸变化失败（最大化图标可能不刷新）", {
        error,
      });
    }
  };

  const attach = async (next: WindowHandle | null): Promise<void> => {
    // 组件已卸载：别再把句柄接回来（否则卸载后还会收到回调）
    if (disposed) return;
    unlisten?.();
    unlisten = undefined;
    handle = next && runtime() ? next : null;
    patch({ available: handle !== null });
    if (!handle) return;

    await readDecorated();
    await readMaximized();
    await subscribeResize();
  };

  const run = async (
    action: (target: WindowHandle) => Promise<void>,
    name: string,
  ): Promise<boolean> => {
    const target = handle;
    if (!target) return false;
    try {
      await action(target);
      return true;
    } catch (error) {
      console.error("[window] 窗口操作失败", { action: name, error });
      return false;
    }
  };

  // 测试路径：构造时就给了句柄（包括显式给的 `null`）
  if (deps.handle !== undefined) void attach(deps.handle);

  return {
    state,
    view: () => windowControlView(state()),
    attach,
    minimize: () => run((win) => win.minimize(), "minimize"),
    toggleMaximize: async () => {
      const ok = await run((win) => win.toggleMaximize(), "toggleMaximize");
      // 不等尺寸事件：`toggleMaximize` 返回后状态就已经变了，主动读一次更跟手
      if (ok) await readMaximized();
      return ok;
    },
    close: () => run((win) => win.close(), "close"),
    dispose: () => {
      disposed = true;
      unlisten?.();
      unlisten = undefined;
    },
  };
}

/**
 * 取真实的 Tauri 窗口句柄。不在 Tauri 里时返回 `null`（浏览器降级）。
 *
 * 用**动态 import**：`@tauri-apps/api` 只在真有窗口时才需要，
 * 静态 import 会让浏览器构建也把它拉进来。
 */
export async function tauriWindowHandle(): Promise<WindowHandle | null> {
  if (!isTauriRuntime()) return null;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const win = getCurrentWindow();
  return {
    minimize: () => win.minimize(),
    toggleMaximize: () => win.toggleMaximize(),
    close: () => win.close(),
    isMaximized: () => win.isMaximized(),
    isDecorated: () => win.isDecorated(),
    onResized: async (handler) => win.onResized(() => handler()),
  };
}
