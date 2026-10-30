import { isTauriRuntime } from "./tauri-env.ts";
import { onTauriEvent } from "./events.ts";

/** LUT 导入进度事件名（Rust 侧常量同名，改一处就得改两处） */
const LUT_IMPORT_PROGRESS_EVENT = "lut://import-progress";

let coreModule: Promise<typeof import("@tauri-apps/api/core")> | undefined;
function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  coreModule ??= import("@tauri-apps/api/core");
  return coreModule.then((core) => core.invoke<T>(command, args));
}

export interface LutRecord {
  id: string;
  categoryId: string;
  name: string;
  originalFilename: string;
  originalPath: string;
  fileRelPath: string;
  coverRelPath: string;
  format: "cube" | "hald";
  hidden: boolean;
  fileHash: string | null;
  createdAt: number;
  available: boolean;
  coverAvailable: boolean;
}
export interface LutLibrary {
  categories: { id: string; name: string }[];
  entries: LutRecord[];
}
export interface LutImportResult { library: LutLibrary; imported: number; duplicates: number; restored: number; skipped: string[]; cancelled: boolean }

/**
 * LUT 导入的进度快照（Rust 侧事件 `lut://import-progress`）。
 *
 * `phase` 三档：`scan` 还不知道总数（界面走**不确定态**）/ `copy` 总数已知、逐张处理 / `done` 收尾。
 */
export interface LutImportProgress {
  phase: "scan" | "copy" | "done";
  done: number;
  total: number;
  /** 正在处理的文件名（不含目录） */
  current: string | null;
  imported: number;
  duplicates: number;
  skipped: number;
  cancelled: boolean;
}

export async function getLutLibrary(legacyCategories: { id: string; name: string }[] = []): Promise<LutLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<LutLibrary>("lut_library", { legacyCategories });
}
export async function createLutCategory(id: string, name: string): Promise<LutLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<LutLibrary>("lut_create_category", { id, name });
}
export async function importLutDirectory(path: string, categoryId: string): Promise<LutImportResult | null> {
  if (!isTauriRuntime()) return null;
  return call<LutImportResult>("lut_import_directory", { path, categoryId });
}
export async function hideLut(id: string): Promise<LutLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<LutLibrary>("lut_hide", { id });
}
export async function getLutCover(id: string): Promise<Uint8Array | null> {
  if (!isTauriRuntime()) return null;
  const bytes = await call<number[] | Uint8Array>("lut_cover", { id });
  return bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
}

/** 订阅 LUT 导入进度；返回**取消订阅**的函数（浏览器里是个空订阅）。 */
export async function onLutImportProgress(
  handler: (progress: LutImportProgress) => void,
): Promise<() => void> {
  return onTauriEvent<LutImportProgress>(LUT_IMPORT_PROGRESS_EVENT, handler);
}

/** 中止正在跑的 LUT 导入：处理完当前文件后停下，已导入的保留。 */
export async function cancelLutImport(): Promise<void> {
  if (!isTauriRuntime()) return;
  await call<void>("lut_import_cancel");
}
