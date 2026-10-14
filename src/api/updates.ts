/**
 * 更新命令的 IPC 薄层（Rust 侧：`src-tauri/src/updates/mod.rs`）。
 *
 * 状态只有一个事实源：Rust 的 `updates_status`，变化通过 `updates://state` 广播。
 * 前端不自己推导「该不该提示」，只按 status 摆 UI。
 */
import { isTauriRuntime } from "./tauri-env.ts";
import type { UpdateSource } from "../lib/update-source.ts";

export interface AvailableUpdate {
  version: string;
  notes: string | null;
}

/** 与 Rust `UpdateStatus` 一一对应。 */
export interface UpdateStatus {
  phase: "idle" | "downloading" | "paused" | "ready";
  version: string | null;
  doneBytes: number;
  totalBytes: number | null;
  /** 最近一次失败原因（只在更新对话框里显示，不弹窗） */
  error: string | null;
  skippedVersion: string | null;
  installPromptedVersion: string | null;
  currentVersion: string;
  /** 上次失败且这个版本还没静默重试过 ⇒ 启动时该接着重试一次 */
  autoRetry: boolean;
  downloading: boolean;
}

/** 状态广播事件名（与 Rust `STATE_EVENT` 一致） */
export const UPDATE_STATE_EVENT = "updates://state";

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) throw new Error("UPDATE_NATIVE_REQUIRED");
  const { invoke: call } = await import("@tauri-apps/api/core");
  return call<T>(command, args);
}

export function updateStatus(): Promise<UpdateStatus> {
  return invoke("updates_status");
}

/** `manual = false` 时 Rust 会压住「已跳过 / 已在办」的版本；手动查永远如实返回。 */
export function updateCheck(source: UpdateSource, manual: boolean): Promise<AvailableUpdate | null> {
  return invoke("updates_check", { source, manual });
}

/** 开始或继续下载（后台跑，立刻返回）。`automatic` 只用于「启动时静默重试一次」的记账。 */
export function updateBegin(source: UpdateSource, automatic = false): Promise<AvailableUpdate> {
  return invoke("updates_begin", { source, automatic });
}

export function updateSkip(version: string): Promise<void> {
  return invoke("updates_skip", { version });
}

export function updateMarkInstallPrompted(): Promise<void> {
  return invoke("updates_mark_install_prompted");
}

export function updateInstall(source: UpdateSource): Promise<void> {
  return invoke("updates_install", { source });
}

/** 订阅状态广播；返回取消订阅的句柄（非 Tauri 运行时是空操作）。 */
export async function onUpdateState(
  handler: (status: UpdateStatus) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<UpdateStatus>(UPDATE_STATE_EVENT, (event) => handler(event.payload));
}
