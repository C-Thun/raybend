/**
 * 编辑预设的 IPC 封装（`specs/editor-presets.md` §4）。
 *
 * 六个命令统一返回最新整库（Rust 侧 `src-tauri/src/presets.rs`）；
 * 返回值是**原始 JSON**，调用方（工作区）用 `sanitizePresetLibrary` 清洗后进 store ——
 * 与 `api/lut.ts` 的模式一致：`api` 层不做业务清洗。
 */

import type { PresetLibrary } from "../lib/presets.ts";
import { isTauriRuntime } from "./tauri-env.ts";

let coreModule: Promise<typeof import("@tauri-apps/api/core")> | undefined;
function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  coreModule ??= import("@tauri-apps/api/core");
  return coreModule.then((core) => core.invoke<T>(command, args));
}

export async function getPresetLibrary(): Promise<PresetLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<PresetLibrary>("preset_library");
}

export async function createPresetDirectory(
  id: string,
  name: string,
): Promise<PresetLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<PresetLibrary>("preset_create_directory", { id, name });
}

export async function deletePresetDirectory(id: string): Promise<PresetLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<PresetLibrary>("preset_delete_directory", { id });
}

export async function createPreset(
  id: string,
  directoryId: string,
  name: string,
  payload: string,
): Promise<PresetLibrary | null> {
  if (!isTauriRuntime()) return null;
  // Rust 侧收一个结构体参数（`PresetCreateArgs`，camelCase）
  return call<PresetLibrary>("preset_create", { args: { id, directoryId, name, payload } });
}

export async function deletePreset(id: string): Promise<PresetLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<PresetLibrary>("preset_delete", { id });
}

export async function movePreset(
  id: string,
  directoryId: string,
): Promise<PresetLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<PresetLibrary>("preset_move", { id, directoryId });
}
