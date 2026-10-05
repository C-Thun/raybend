/** System display facts stay behind the desktop adapter; browser previews are explicit unavailable. */
import { isTauriRuntime } from "./tauri-env.ts";
import { onTauriEvent } from "./events.ts";

import type { ColorDefaults } from "../lib/color-model.ts";
export type { ColorDefaults, OutputColor, PhotoColorState, SourceColor } from "../lib/color-model.ts";

export async function getColorDefaults(): Promise<ColorDefaults | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ColorDefaults>("color_get_defaults");
}
export async function setColorDefaults(defaults: ColorDefaults): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("color_set_defaults", { defaults });
}

/** Preparation runs in Rust; selecting a file only changes portable interpretation. */
export async function preparePhotoColor(path: string, profileId: string | null): Promise<import("../lib/color-model.ts").PhotoColorState> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke("color_prepare_photo", {path,profileId});
}

export interface ColorBatchReview {token:string;selected:number;applicable:number;existing:number;skipped:string[];profileName:string|null}
export async function reviewPhotoColors(repositoryId:string,assetIds:number[],profileId:string|null):Promise<ColorBatchReview> {
  const {invoke}=await import("@tauri-apps/api/core");return invoke("color_batch_review",{repositoryId,assetIds,profileId});
}
export async function commitPhotoColors(token:string):Promise<number> {
  const {invoke}=await import("@tauri-apps/api/core");return invoke("color_batch_commit",{token});
}

export async function setEditorProof(target: import("../lib/color-model.ts").OutputColor | null, warning: boolean): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("editor_set_proof", {target,warning});
}

import type { DisplaySnapshot } from "../lib/display-color.ts";
export type { DisplayColorState, DisplayPresentation, DisplaySnapshot } from "../lib/display-color.ts";

export interface VersionedDisplaySnapshot {
  generation: number;
  snapshot: DisplaySnapshot;
}

export interface ColorProfileEntry {
  key: string;
  profileId: string;
  name: string;
  originalFilename: string;
  profileClass: "input" | "display" | "output" | "color_space";
  builtIn: boolean;
  hidden: boolean;
  available: boolean;
}

export interface ColorProfileLibrary {
  entries: ColorProfileEntry[];
}

export interface ColorProfileImport {
  library: ColorProfileLibrary;
  imported: number;
  duplicates: number;
  restored: number;
  skipped: string[];
}

export async function getColorProfileLibrary(): Promise<ColorProfileLibrary | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ColorProfileLibrary>("color_profile_library");
}

export async function importColorProfileFiles(paths: string[]): Promise<ColorProfileImport> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ColorProfileImport>("color_profile_import_files", { paths });
}

export async function hideColorProfile(profileId: string): Promise<ColorProfileLibrary> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ColorProfileLibrary>("color_profile_hide", { profileId });
}

export async function getDisplaySnapshot(): Promise<VersionedDisplaySnapshot | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<VersionedDisplaySnapshot>("color_display_snapshot");
}

export async function openSystemDisplaySettings(): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("color_open_system_settings");
}

/** A hint to refresh, not a cached display profile. The adapter always reads the current state. */
export async function onDisplayEnvironmentChange(handler: () => void): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const window = getCurrentWindow();
  const unsystem = await onTauriEvent("color://environment-changed", handler);
  let unmove: (() => void) | undefined;
  try {
    unmove = await window.onMoved(handler);
    const unfocus = await window.onFocusChanged(({ payload }) => {
      if (payload) handler();
    });
    return () => {
      unmove?.();
      unfocus();
      unsystem();
    };
  } catch (error) {
    unmove?.();
    unsystem();
    throw error;
  }
}
