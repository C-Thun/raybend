/**
 * 全屏看图的 IPC 封装（`ARCHITECTURE.md` §1.1：只有 `src/api/` 能 `invoke`）。
 *
 * 三条命令 + 一个事件：
 *
 * | 谁 | 用途 |
 * | --- | --- |
 * | `openFullscreen(items, index)` | 主窗口调：存清单 + 开窗（已开着就换图 + 聚焦） |
 * | `getFullscreenPayload()` | 全屏页挂载时取一次清单 |
 * | `closeFullscreen()` | 全屏页按 `Esc` / `Enter` 时调 |
 * | `onFullscreenPayload(cb)` | 窗口已开着时换图（不重建窗口） |
 *
 * 浏览器里全部是空操作 / `null`（那里没有窗口可开），调用方据此不做事。
 */

import type { FullscreenItem, FullscreenPayload } from "./types.ts";
import { onTauriEvent } from "./events.ts";
import { isTauriRuntime } from "./tauri-env.ts";

/** 清单更新事件名（与 `src-tauri/src/fullscreen.rs` 的 `FULLSCREEN_EVENT` 一致）。 */
export const FULLSCREEN_EVENT = "fullscreen://payload";

let coreModule: Promise<typeof import("@tauri-apps/api/core")> | undefined;

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  coreModule ??= import("@tauri-apps/api/core");
  return coreModule.then((core) => core.invoke<T>(cmd, args));
}

/**
 * 打开全屏看图（清单 + 当前下标 + 画布底色）。
 *
 * `background` 是 CSS 颜色串（前端用 `getComputedStyle` 拿的 `--surface-track`）：
 * Rust 拿它设窗口背景色，消除 WebView 首帧的**白闪**。给不出就不给（不致命）。
 * Rust 侧会校验：空清单 / 越界直接报错。
 */
export async function openFullscreen(
  items: readonly FullscreenItem[],
  index: number,
  background?: string,
): Promise<void> {
  if (!isTauriRuntime()) return;
  await call<void>("fullscreen_open", {
    items: [...items],
    index,
    background: background ?? null,
  });
}

/** 关闭全屏看图（幂等：窗口不在也算成功）。 */
export async function closeFullscreen(): Promise<void> {
  if (!isTauriRuntime()) return;
  await call<void>("fullscreen_close");
}

/** 取当前清单（全屏页挂载时用；没开过就是 `null`）。 */
export async function getFullscreenPayload(): Promise<FullscreenPayload | null> {
  if (!isTauriRuntime()) return null;
  return call<FullscreenPayload | null>("fullscreen_payload");
}

/** 订阅清单更新（窗口已开着时换图）。返回取消订阅的函数。 */
export async function onFullscreenPayload(
  handler: (payload: FullscreenPayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  return onTauriEvent<FullscreenPayload>(FULLSCREEN_EVENT, handler);
}
