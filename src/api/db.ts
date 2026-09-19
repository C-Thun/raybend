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
import { onTauriEvent } from "./events.ts";
import { t } from "../i18n/index.ts";
import type {
  DirEmptyView,
  DirEntry,
  MigrationNotice,
  RepositorySettings,
  TemplatePreview,
  FileExif,
  PhotoCount,
  RecentDir,
  MetaFile,
  PhotoMeta,
  RebuildReport,
  RebuildProgress,
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

/**
 * 一批路径现在还是不是目录（给「最近目录」标灰用）。
 *
 * 浏览器里没有文件系统 → 一律当「可用」（标灰是给真机用的提示，
 * 在浏览器里猜错反而会误导）。
 */
/**
 * 确保一批文件的展示元信息（宽高 + 方向）是新鲜的。
 *
 * 缓存活在 Rust 进程里（会话级、不落盘）：同一目录反复进出不会重复扫盘。
 * 浏览器里没有文件系统 → 一律返回「尺寸未知」（`0×0`），界面按默认比例占位。
 */
export async function dirMetaEnsure(
  dir: string,
  files: readonly MetaFile[],
): Promise<PhotoMeta[]> {
  if (!isTauriRuntime()) {
    return files.map((file) => ({
      relative: file.relative,
      width: 0,
      height: 0,
      orientation: 1,
    }));
  }
  return call<PhotoMeta[]>("dir_meta_ensure", { path: dir, files });
}

export async function pathsStatus(paths: string[]): Promise<boolean[]> {
  if (!isTauriRuntime()) return paths.map(() => true);
  return call<boolean[]>("source_paths_status", { paths });
}

export async function listVolumes(): Promise<Volume[]> {
  if (!isTauriRuntime()) return [];
  return call<Volume[]>("volumes_list");
}

export async function listDirs(path: string): Promise<DirEntry[]> {
  if (!isTauriRuntime()) return [];
  return call<DirEntry[]>("dir_list", { path });
}

/*
 * 库内目录的菜单动作（`BROWSE.md` §4.3 行尾 `⋯`）。
 *
 * 传的是 `root + rel`（**库根 + 库内相对路径**），不是绝对路径：
 * Rust 侧会拿 `rel` 过一遍越界检查（`..`、绝对路径、盘符一律拒）——
 * 界面上一层笔误不该能碰到库外面去。
 */

/** 深度检查这个目录的整棵子树里有没有文件（决定「删除空目录」能不能点）。 */
export async function dirEmptyCheck(root: string, rel: string): Promise<DirEmptyView> {
  // 浏览器里没有真目录：按「不空」处理，菜单项于是是禁用的（而不是假装能删）
  if (!isTauriRuntime()) {
    return { empty: false, fileCount: 0, dirCount: 0, emptyDirCount: 0, hasUnresolvedLink: false };
  }
  return call<DirEmptyView>("dir_empty_check", { root, rel });
}

/** 删除空目录（连同其下所有空子目录），返回删掉的目录个数。 */
export async function dirRemoveEmpty(root: string, rel: string): Promise<number> {
  if (!isTauriRuntime()) return 0;
  return call<number>("dir_remove_empty", { root, rel });
}

/** 建一个子目录，返回它的库内相对路径。 */
export async function dirCreate(root: string, rel: string, name: string): Promise<string> {
  if (!isTauriRuntime()) return rel === "" ? name : `${rel}/${name}`;
  return call<string>("dir_create", { root, rel, name });
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

/** "取一张能显示的图"的用途（`crates/raybend/src/display` 的 `ImagePurpose`）。 */
export type ImagePurpose = "grid" | "strip" | "screen" | "original";

/**
 * **统一取图口**（`plans/M2-W2.md` §2.1）：view 与缩略图共用。
 *
 * 调用方只说「哪张、要多大」，**不需要知道它是 RAW 还是位图** ——
 * 分派（以及「位图 + 原图 + 没编辑 ⇒ 直接给原文件」）都在 Rust 侧（`display` 模块）。
 * 现在 view 还走在 `getViewImage` 之外的旧路上（`getThumbBytes`），1.5 接过来。
 *
 * 浏览器里返回 `null`（没有后端），界面自然走占位。
 */
export async function getViewImage(
  path: string,
  purpose: ImagePurpose = "screen",
): Promise<Uint8Array | null> {
  if (!isTauriRuntime()) return null;
  return toBytes(await call<unknown>("view_image", { path, purpose }));
}

/**
 * 看图态右栏的**直方图**（24 柱 RGB 合成，`plans/M2-W2.md` 1.6）。
 *
 * 统计在 Rust 侧做 —— `AGENTS.md` §6.1 的红线：**前端不碰像素**。
 * 前端只拿回 24 个整数画柱子。取不到（浏览器里、认不出的文件）返回 `null`，
 * 界面画一条空直方图。
 */
export interface Histogram {
  bins: number;
  r: number[];
  g: number[];
  b: number[];
  /** 三通道合并后的峰值（归一化柱高用） */
  max: number;
}

export async function getHistogram(
  path: string,
  /**
   * 采样点数（**桶数**）。默认 52 = 0..255 每 5 级一个点（人类 2026-09-19 定的口径）；
   * 后端按 `value * bins / 256` 分桶，所以这里给几就是几个采样点。
   */
  bins = 52,
): Promise<Histogram | null> {
  if (!isTauriRuntime()) return null;
  const value = await call<Histogram>("image_histogram", { path, bins });
  if (value === null || typeof value !== "object") return null;
  return value;
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

/** 一个库的设置（离线/打不开会抛错 —— 设置必须在线改）。 */
export async function repositorySettings(
  repositoryId: string,
): Promise<RepositorySettings> {
  if (!isTauriRuntime()) {
    return { repositoryId, importTemplate: ":CYEAR-:CMONTH-:CDAY/MY:FILENAME" };
  }
  return call<RepositorySettings>("repository_settings", { repositoryId });
}

/** 改一个库的导入模版（写库内真相源 + app.db 缓存）。 */
export async function setRepositoryTemplate(
  repositoryId: string,
  templateSource: string,
): Promise<RepositorySettings> {
  if (!isTauriRuntime()) {
    return { repositoryId, importTemplate: templateSource };
  }
  return call<RepositorySettings>("repository_set_template", {
    repositoryId,
    templateSource,
  });
}

/** 模版预览（纯函数命令：用户打字时随手调，不碰库）。 */
export async function previewTemplate(
  templateSource: string,
): Promise<TemplatePreview> {
  if (!isTauriRuntime()) {
    return { ok: true, error: null, warnings: [], paths: [] };
  }
  return call<TemplatePreview>("repository_template_preview", { templateSource });
}

/** 建库，或把一个已有库登记进来（分支判定在 Rust 侧，见 `repo.rs`）。 */
export async function createRepository(
  path: string,
  name?: string,
): Promise<RepositoryView> {
  if (!isTauriRuntime()) throw new Error(t("common.desktop_only"));
  return call<RepositoryView>("repository_create", { path, name: name ?? null });
}

/** 重新挂载一个离线库；找不到**不是错误**（返回的 `online` 会是 false）。 */
export async function remountRepository(
  repositoryId: string,
): Promise<RepositoryView> {
  if (!isTauriRuntime()) throw new Error(t("common.desktop_only"));
  return call<RepositoryView>("repository_remount", { repositoryId });
}

/**
 * 一个库现在的两个计数：`[相片数量, 图片数量]`（离线或还没数过 → `null`）。
 *
 * 口径（人类 2026-09-19）：**相片**不含 `_RAW/`，**图片**含 `_RAW/`。
 */
export async function repositoryCounts(
  repositoryId: string,
): Promise<[number, number] | null> {
  if (!isTauriRuntime()) return null;
  return call<[number, number] | null>("repository_counts", { repositoryId });
}

/**
 * **进目录时同步计数**（人类 2026-09-19）：读盘数一次这个目录，与库里那行对比，
 * 不一样就写回去并把差值滚到库级汇总。
 *
 * 返回 `[目录计数, 库级汇总]`，各是 `[相片, 图片]`；离线或没给目录时是 `null`。
 * 本地 `readdir` 是微秒级，所以**每次进目录都调**（`AGENTS.md` §2 #13 的实时性优先）。
 */
export async function syncDirectoryCounts(
  repositoryId: string,
  scopePath: string | null,
): Promise<[[number, number], [number, number]] | null> {
  if (!isTauriRuntime()) return null;
  return call<[[number, number], [number, number]] | null>("repository_sync_dir", {
    repositoryId,
    scopePath,
  });
}

/**
 * **重建数据**：重扫整个库、把 catalog 与计数拉平（耗时，界面上要挡住操作）。
 *
 * 干四件事：扫盘对齐文件、重读老库缺失的元数据（EXIF）、重算目录计数、汇总进库表。
 */
export async function rebuildRepository(repositoryId: string): Promise<RebuildReport> {
  if (!isTauriRuntime()) throw new Error(t("common.desktop_only"));
  return call<RebuildReport>("repository_rebuild", { repositoryId });
}

/** 进度事件名（与 `src-tauri/src/repo.rs::REBUILD_EVENT` 一致）。 */
export const REBUILD_PROGRESS_EVENT = "db://rebuild";

/** 订阅所有库的重建进度；具体库的过滤由发起重建的界面负责。 */
export async function onRebuildProgress(
  handler: (progress: RebuildProgress) => void,
): Promise<() => void> {
  return onTauriEvent<RebuildProgress>(REBUILD_PROGRESS_EVENT, handler);
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

/** 数据库升级事件名（与 `src-tauri/src/migration.rs` 的 `MIGRATION_EVENT` 一致）。 */
export const MIGRATION_EVENT = "db://migration";

/** 缓存的事件模块（浏览器里根本不会加载它）。 */
let eventModule: Promise<typeof import("@tauri-apps/api/event")> | undefined;

/**
 * 订阅数据库升级通知；返回**取消订阅**的函数。
 *
 * 一次升级有头有尾（`running: true` / `false`），失败也会发 `false` ——
 * 界面据此弹/撤阻塞遮罩（`src/features/migration/`），不会卡在里面。
 * 浏览器里返回一个什么都不做的函数（开发预览没有后端）。
 */
export async function onMigrationNotice(
  handler: (notice: MigrationNotice) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  eventModule ??= import("@tauri-apps/api/event");
  const { listen } = await eventModule;
  return listen<MigrationNotice>(MIGRATION_EVENT, (event) =>
    handler(event.payload),
  );
}
