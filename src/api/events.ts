/**
 * Tauri 事件订阅的唯一入口。
 *
 * 命令 API 会按领域拆文件，但「浏览器里返回空订阅、桌面端懒加载 event 模块」
 * 这套运行时规则只有一份，避免每加一种进度事件就复制一次。
 */

import { isTauriRuntime } from "./tauri-env.ts";

let eventModule: Promise<typeof import("@tauri-apps/api/event")> | undefined;

export async function onTauriEvent<T>(
  name: string,
  handler: (payload: T) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  eventModule ??= import("@tauri-apps/api/event");
  const { listen } = await eventModule;
  return listen<T>(name, (event) => handler(event.payload));
}
