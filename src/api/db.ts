/**
 * 数据层的前端封装（`ARCHITECTURE.md` §1.1：**只有 `src/api/` 里 `import invoke`**）。
 *
 * 三件事：
 *   1. **命令名与参数在一处收口** —— 界面层看不到字符串形式的命令名；
 *   2. **浏览器降级**：不在 Tauri 里（`pnpm dev`、`pnpm smoke:ui` 都跑在 Chromium 里）
 *      时，读操作返回空值、写操作静默忽略。界面于是显示**空态**而不是一片报错 ——
 *      这是刻意的：开发期看的是布局，不是数据。
 *   3. **错误照原样抛** —— 这一层不吞异常。谁调用谁决定「显示错误」还是「退回空态」，
 *      因为只有调用方知道出错的代价（读不到最近目录 vs 扫描目录失败，处理方式不同）。
 *
 * 类型是手写镜像（见 `types.ts` 的说明），漂移由 `dto-contract.test.ts` 与
 * `src-tauri/src/contract.rs` 两侧对着 `dto-contract.json` 断言来兜住。
 */

import { isTauriRuntime } from "./tauri-env.ts";
import type {
  DirEntry,
  FileExif,
  PhotoCount,
  RecentDir,
  RepositoryProbe,
  RepositoryView,
  SourceScan,
  ThumbCacheStats,
  ThumbSize,
  TimeEntry,
  Volume,
} from "./types.ts";

/**
 * 设置的 key 常量（**唯一来源**：前端用到的那些）。
 *
 * Rust 侧只用到一个（`source.recent_limit`，见 `src-tauri/src/source.rs`）；
 * 其余是界面自己的偏好，读写都走通用的 `setting_get` / `setting_set`。
 */
export const SETTING_KEYS = {
  /** 最近目录保留条数（Rust 侧也读它） */
  recentLimit: "source.recent_limit",
  /** 按时间分组的时间片阈值（分钟） */
  timeGapMinutes: "grid.time_gap_minutes",
  /** 照片网格的档位下标（0..8） */
  tileStep: "grid.tile_step",
  /** 「避免重复导入」是否勾选 */
  avoidDuplicates: "import.avoid_duplicates",
  /** 是否处于「按时间」模式 */
  byTime: "grid.by_time",
} as const;

/** 缓存的 `@tauri-apps/api/core` 模块（浏览器里根本不会加载它）。 */
let coreModule: Promise<typeof import("@tauri-apps/api/core")> | undefined;

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  // 动态 import：模块加载失败也不会影响不碰数据层的页面（陈列室）
  coreModule ??= import("@tauri-apps/api/core");
  return coreModule.then((core) => core.invoke<T>(cmd, args));
}

/* ══════════════════════════════════════════════════════════════
 * 最近目录
 * ══════════════════════════════════════════════════════════════ */

export async function listRecentDirs(): Promise<RecentDir[]> {
  if (!isTauriRuntime()) return [];
  return call<RecentDir[]>("recent_dirs_list");
}

/** 记一条最近目录（勾选目录时调）。浏览器里无操作。 */
export async function rememberRecentDir(
  path: string,
  includeSubdirs: boolean,
): Promise<void> {
  if (!isTauriRuntime()) return;
  await call<void>("recent_dir_remember", { path, includeSubdirs });
}

/** 从最近列表移除一条（**不动磁盘**）。返回是否真的删掉了。 */
export async function forgetRecentDir(path: string): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  return call<boolean>("recent_dir_forget", { path });
}

/* ══════════════════════════════════════════════════════════════
 * 来源（驱动器 / 目录 / 照片）
 * ══════════════════════════════════════════════════════════════ */

export async function listVolumes(): Promise<Volume[]> {
  if (!isTauriRuntime()) return [];
  return call<Volume[]>("volumes_list");
}

export async function listDirs(path: string): Promise<DirEntry[]> {
  if (!isTauriRuntime()) return [];
  return call<DirEntry[]>("dir_list", { path });
}

/** 列一个目录里的照片（**不下钻子目录**）。 */
export async function scanSourceDir(path: string): Promise<SourceScan> {
  if (!isTauriRuntime()) {
    return { root: path, items: [], skipped: 0, problems: [], elapsedMs: 0 };
  }
  return call<SourceScan>("source_scan", { path });
}

/** 数一个目录里有多少张照片（`recursive` 对应「包含子目录」）。 */
export async function countSourcePhotos(
  path: string,
  recursive: boolean,
): Promise<PhotoCount> {
  if (!isTauriRuntime()) return { photos: 0, skipped: 0, truncated: false };
  return call<PhotoCount>("source_count", { path, recursive });
}

/** 并行补真实 EXIF 拍摄时间（用户按下「按时间」时才调）。 */
export async function readSourceTimes(paths: string[]): Promise<TimeEntry[]> {
  if (!isTauriRuntime()) return [];
  return call<TimeEntry[]>("source_times", { paths });
}

/** 一个文件的 EXIF（喂 `flowbar`）。 */
export async function readFileExif(path: string): Promise<FileExif> {
  if (!isTauriRuntime()) return EMPTY_FILE_EXIF;
  return call<FileExif>("file_exif", { path });
}

/**
 * 浏览器降级时返回的空 EXIF（**这是「没有数据」，不是「读失败」**）。
 * 字段与 `FileExif` 一一对应：Rust 侧加了字段，这里也要跟着加。
 */
export const EMPTY_FILE_EXIF: FileExif = {
  cameraMake: null,
  cameraModel: null,
  lens: null,
  focalMm: null,
  fNumber: null,
  exposureMs: null,
  iso: null,
  width: null,
  height: null,
  orientation: null,
  takenAtMs: null,
  takenAtSource: null,
  takenAtOffsetMin: null,
  ext: null,
  kind: "other",
};

/* ══════════════════════════════════════════════════════════════
 * 缩略图
 * ══════════════════════════════════════════════════════════════ */

/**
 * 取一张缩略图的字节。
 *
 * 返回 `Uint8Array`（由 `ArrayBuffer` 包一层），调用方自己做 `Blob` +
 * `URL.createObjectURL` 并按需回收。**浏览器里返回 `null`**（没有数据源）。
 */
export async function getThumbBytes(
  path: string,
  size: ThumbSize = "grid",
): Promise<Uint8Array | null> {
  if (!isTauriRuntime()) return null;
  return toBytes(await call<unknown>("thumb_get", { path, size }));
}

/**
 * 把 IPC 回来的原始字节统一成 `Uint8Array`。
 *
 * 三种形状都要认 —— 这不是「防御性编程」，是**真的会变**：
 * Tauri 的 `tauri::ipc::Response` 在正常路径下给 `ArrayBuffer`，
 * 但在 JSON 序列化路径（例如某些平台的回退实现）下会变成数字数组。
 * 认不出来就返回 `null`，由调用方显示占位图而不是让界面崩掉。
 */
export function toBytes(value: unknown): Uint8Array | null {
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (ArrayBuffer.isView(value)) {
    const view = value as ArrayBufferView;
    return new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
  }
  if (Array.isArray(value)) {
    return Uint8Array.from(value as number[]);
  }
  return null;
}

/** 源文件缩略图缓存的统计（排错与设置面板用）。 */
export async function thumbSourcesStats(): Promise<ThumbCacheStats | null> {
  if (!isTauriRuntime()) return null;
  return call<ThumbCacheStats>("thumb_sources_stats");
}

/* ══════════════════════════════════════════════════════════════
 * 库（相片仓）
 * ══════════════════════════════════════════════════════════════ */

export async function listRepositories(): Promise<RepositoryView[]> {
  if (!isTauriRuntime()) return [];
  return call<RepositoryView[]>("repositories_list");
}

export async function probeRepository(path: string): Promise<RepositoryProbe> {
  if (!isTauriRuntime()) {
    return {
      kind: "empty",
      name: null,
      repositoryId: null,
      registered: false,
      message: null,
    };
  }
  return call<RepositoryProbe>("repository_probe", { path });
}

/** 建库，或把一个已有库登记进来（分支判定在 Rust 侧，见 `repo.rs`）。 */
export async function createRepository(
  path: string,
  name?: string,
): Promise<RepositoryView> {
  if (!isTauriRuntime()) throw new Error("浏览器里不能建库");
  return call<RepositoryView>("repository_create", { path, name: name ?? null });
}

/** 重新挂载一个离线库；找不到**不是错误**（返回的 `online` 会是 false）。 */
export async function remountRepository(
  repositoryId: string,
): Promise<RepositoryView> {
  if (!isTauriRuntime()) throw new Error("浏览器里不能重挂载库");
  return call<RepositoryView>("repository_remount", { repositoryId });
}

/** 一个库现在有多少张照片（离线 → `null`）。 */
export async function repositoryCounts(
  repositoryId: string,
): Promise<number | null> {
  if (!isTauriRuntime()) return null;
  return call<number | null>("repository_counts", { repositoryId });
}

/* ══════════════════════════════════════════════════════════════
 * 设置
 * ══════════════════════════════════════════════════════════════ */

export async function getSetting(key: string): Promise<string | null> {
  if (!isTauriRuntime()) {
    // 浏览器里退回 localStorage：开发期切主题/密度/档位要能记住
    return readLocalSetting(key);
  }
  return call<string | null>("setting_get", { key });
}

export async function setSetting(key: string, value: string): Promise<void> {
  if (!isTauriRuntime()) {
    writeLocalSetting(key, value);
    return;
  }
  await call<void>("setting_set", { key, value });
}

/** 浏览器降级用的本地存储前缀（**不要**与 Tauri 侧的数据混淆）。 */
export const LOCAL_SETTING_PREFIX = "raybend.setting.";

function readLocalSetting(key: string): string | null {
  try {
    return globalThis.localStorage?.getItem(LOCAL_SETTING_PREFIX + key) ?? null;
  } catch {
    return null;
  }
}

function writeLocalSetting(key: string, value: string): void {
  try {
    globalThis.localStorage?.setItem(LOCAL_SETTING_PREFIX + key, value);
  } catch {
    // 隐私模式/禁用存储：静默放弃，不是错误
  }
}

/** 读一个数值设置（读不到或不是数字就用 `fallback`）。 */
export async function getNumberSetting(
  key: string,
  fallback: number,
): Promise<number> {
  const raw = await getSetting(key);
  if (raw === null) return fallback;
  const value = Number(raw);
  return Number.isFinite(value) ? value : fallback;
}

/** 读一个布尔设置（`"1"` / `"true"` 都算真）。 */
export async function getBooleanSetting(
  key: string,
  fallback: boolean,
): Promise<boolean> {
  const raw = await getSetting(key);
  if (raw === null) return fallback;
  return raw === "1" || raw.toLowerCase() === "true";
}
