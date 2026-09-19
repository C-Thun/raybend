/**
 * IPC 契约的形状冒烟（`plans/M1-5.md` §3.1 的「手写镜像 + 形状冒烟测试」）。
 *
 * 要防的是**一类静默故障**：Rust 侧改了个字段名（或忘了 `rename_all`），
 * 前端拿到 `undefined`，界面上表现为「某块永远空着」——不报错、不崩、就是没数据。
 * 所以这里做两件事：
 *
 *   1. **编译期覆盖**：每个键表都必须覆盖接口的**全部字段**（漏一个就编译不过），
 *      且键表里的每个名字都必须是接口的字段（写错一个也编译不过）。
 *   2. **运行期对齐**：键表与 `dto-contract.json` 逐字一致；Rust 侧有另一条测试
 *      对着**同一份** JSON 断言真实序列化的键名。两侧都过，才说明名字没漂。
 *
 * 于是「改字段名」这件事会同时惊动三处（Rust 结构 / JSON / TS 接口），
 * 想漏都漏不掉。
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import type {
  AssetItem,
  BrowseFacets,
  BrowseTimeline,
  BrowseWindow,
  DeleteFailure,
  DeleteResult,
  DirEmptyView,
  DirEntry,
  FacetCount,
  FileExif,
  FlagsView,
  ImportBatchProgress,
  ImportCurrentItem,
  ImportError,
  ImportPrecheck,
  ImportRunProgress,
  ImportStart,
  MarkingItem,
  MarkResult,
  MigrationNotice,
  MetaFile,
  ImportPlannedRun,
  InterruptedRun,
  PhotoCount,
  PhotoMeta,
  RecentDir,
  RebuildProgress,
  RebuildReport,
  RepositoryPath,
  RepositoryProbe,
  RepositorySettings,
  RepositoryView,
  TemplatePreview,
  SourceItem,
  SourceScan,
  ThumbCacheStats,
  TimeEntry,
  TimelineEntry,
  Volume,
} from "./types.ts";

/* ══════════════════════════════════════════════════════════════
 * 键表 + 编译期覆盖检查
 * ══════════════════════════════════════════════════════════════ */

/**
 * 覆盖判定：`T` 里没有出现在 `K` 里的字段为空 → `true`；否则给出缺的名字。
 * 漏字段时类型不是 `true`，下面的调用就会编译不过。
 */
type Covered<T, K extends readonly string[]> = Exclude<
  keyof T,
  K[number]
> extends never
  ? true
  : { missing: Exclude<keyof T, K[number]> };

/** 编译期 + 运行期都过一遍：键名合法（satisfies）且覆盖完整（Covered） */
function checkKeys<T, const K extends readonly string[]>(
  keys: K & (Covered<T, K> extends true ? unknown : ["字段漏了", Covered<T, K>]),
): K {
  return keys;
}

const RECENT_DIR_KEYS = [
  "includeSubdirs",
  "path",
  "useCount",
  "usedAt",
] as const satisfies readonly (keyof RecentDir)[];
const VOLUME_KEYS = ["kind", "kindLabel", "path"] as const satisfies readonly (keyof Volume)[];
const META_FILE_KEYS = [
  "fileSize",
  "mtimeMs",
  "relative",
] as const satisfies readonly (keyof MetaFile)[];
const PHOTO_META_KEYS = [
  "height",
  "orientation",
  "relative",
  "width",
] as const satisfies readonly (keyof PhotoMeta)[];
const DIR_ENTRY_KEYS = ["hasChildren", "name", "path"] as const satisfies readonly (keyof DirEntry)[];
const SOURCE_ITEM_KEYS = [
  "ext",
  "fileName",
  "kind",
  "mtimeMs",
  "path",
  "sizeBytes",
  "takenAtMs",
  "takenAtOffsetMin",
  "takenAtSource",
] as const satisfies readonly (keyof SourceItem)[];
const SOURCE_SCAN_KEYS = [
  "elapsedMs",
  "items",
  "problems",
  "root",
  "skipped",
] as const satisfies readonly (keyof SourceScan)[];
const TIME_ENTRY_KEYS = [
  "path",
  "takenAtMs",
  "takenAtOffsetMin",
  "takenAtSource",
] as const satisfies readonly (keyof TimeEntry)[];
const PHOTO_COUNT_KEYS = [
  "photos",
  "skipped",
  "truncated",
] as const satisfies readonly (keyof PhotoCount)[];
const FILE_EXIF_KEYS = [
  "cameraMake",
  "cameraModel",
  "exposureMs",
  "ext",
  "fNumber",
  "focalMm",
  "height",
  "iso",
  "kind",
  "lens",
  "orientation",
  "takenAtMs",
  "takenAtOffsetMin",
  "takenAtSource",
  "width",
] as const satisfies readonly (keyof FileExif)[];
const REPOSITORY_VIEW_KEYS = [
  "createdAt",
  "displayPath",
  "id",
  "importTemplate",
  "lastOpenedAt",
  "name",
  "online",
  "paths",
  "imagesCount",
  "photosCount",
  "root",
  "triedPaths",
] as const satisfies readonly (keyof RepositoryView)[];
const REPOSITORY_PATH_KEYS = [
  "lastSeenAt",
  "path",
  "status",
] as const satisfies readonly (keyof RepositoryPath)[];
const REPOSITORY_PROBE_KEYS = [
  "kind",
  "message",
  "name",
  "registered",
  "repositoryId",
] as const satisfies readonly (keyof RepositoryProbe)[];
const THUMB_CACHE_STATS_KEYS = [
  "bySize",
  "bytes",
  "entries",
  "pinned",
] as const satisfies readonly (keyof ThumbCacheStats)[];

/* ══════════════════════════════════════════════════════════════
 * 测试
 * ══════════════════════════════════════════════════════════════ */

/* 导入（M1-6）：进度事件的载荷是最大的一份 DTO，键名漂了界面就整块空着 */
const IMPORT_BATCH_KEYS = [
  "batchId",
  "bytes",
  "currentRun",
  "done",
  "duplicates",
  "errors",
  "errorsTotal",
  "failed",
  "finishedAt",
  "freeBytes",
  "imported",
  "runs",
  "skipped",
  "stage",
  "startedAt",
  "state",
  "total",
] as const satisfies readonly (keyof ImportBatchProgress)[];
const IMPORT_RUN_KEYS = [
  "bytes",
  "current",
  "done",
  "duplicates",
  "failed",
  "imported",
  "note",
  "runId",
  "scanned",
  "skipped",
  "sourceRoot",
  "stage",
  "state",
  "total",
] as const satisfies readonly (keyof ImportRunProgress)[];
const IMPORT_CURRENT_KEYS = ["source", "target"] as const satisfies readonly (keyof ImportCurrentItem)[];
const IMPORT_ERROR_KEYS = [
  "reason",
  "source",
  "status",
  "target",
] as const satisfies readonly (keyof ImportError)[];
const IMPORT_PRECHECK_KEYS = [
  "freeBytes",
  "neededBytes",
  "tight",
  "totalBytes",
] as const satisfies readonly (keyof ImportPrecheck)[];
const IMPORT_START_KEYS = ["batchId", "runs"] as const satisfies readonly (keyof ImportStart)[];
const IMPORT_PLANNED_RUN_KEYS = [
  "index",
  "sourceRoot",
] as const satisfies readonly (keyof ImportPlannedRun)[];
const INTERRUPTED_RUN_KEYS = [
  "failed",
  "imported",
  "runId",
  "skipped",
  "sourceRoot",
  "startedAt",
  "template",
] as const satisfies readonly (keyof InterruptedRun)[];

const REPOSITORY_SETTINGS_KEYS = [
  "importTemplate",
  "repositoryId",
] as const satisfies readonly (keyof RepositorySettings)[];
const TEMPLATE_PREVIEW_KEYS = [
  "error",
  "ok",
  "paths",
  "warnings",
] as const satisfies readonly (keyof TemplatePreview)[];

/* ── 浏览（M2-W1）── */

const ASSET_ITEM_KEYS = [
  "hasRaw",
  "author",
  "description",
  "gpsLat",
  "gpsLon",
  "country",
  "provinceState",
  "city",
  "sublocation",
  "createdMs",
  "cameraMake",
  "cameraModel",
  "colorLabel",
  "exposureMs",
  "ext",
  "fNumber",
  "fileName",
  "focalMm",
  "height",
  "id",
  "isRaw",
  "iso",
  "lens",
  "likeState",
  "lockLevel",
  "missing",
  "orientation",
  "rating",
  "relPath",
  "sizeBytes",
  "takenAt",
  "takenAtOffsetMin",
  "width",
] as const satisfies readonly (keyof AssetItem)[];
const BROWSE_WINDOW_KEYS = ["items", "offset", "total"] as const satisfies readonly (keyof BrowseWindow)[];
const TIMELINE_ENTRY_KEYS = ["id", "relPath", "takenAt"] as const satisfies readonly (keyof TimelineEntry)[];
const BROWSE_TIMELINE_KEYS = ["entries", "total"] as const satisfies readonly (keyof BrowseTimeline)[];
const FACET_COUNT_KEYS = ["count", "value"] as const satisfies readonly (keyof FacetCount)[];
const BROWSE_FACETS_KEYS = ["colors", "likes", "locks", "ratings"] as const satisfies readonly (keyof BrowseFacets)[];
const MARKING_ITEM_KEYS = [
  "colorLabel",
  "id",
  "likeState",
  "lockLevel",
  "rating",
  "tagIds",
] as const satisfies readonly (keyof MarkingItem)[];
const MARK_RESULT_KEYS = [
  "canRedo",
  "canUndo",
  "changed",
  "redoLabel",
  "skippedLocked",
  "undoLabel",
] as const satisfies readonly (keyof MarkResult)[];
const DELETE_FAILURE_KEYS = ["path", "reason"] as const satisfies readonly (keyof DeleteFailure)[];
const DELETE_RESULT_KEYS = [
  "alreadyGone",
  "blockedLocked",
  "deleted",
  "failed",
] as const satisfies readonly (keyof DeleteResult)[];
const REBUILD_REPORT_KEYS = [
  "imagesCount",
  "metadataFilled",
  "missing",
  "photosCount",
  "registered",
  "renamed",
  "returned",
  "scanned",
] as const satisfies readonly (keyof RebuildReport)[];
const REBUILD_PROGRESS_KEYS = [
  "done",
  "phase",
  "repositoryId",
  "total",
] as const satisfies readonly (keyof RebuildProgress)[];
const MIGRATION_NOTICE_KEYS = [
  "from",
  "kind",
  "label",
  "running",
  "to",
] as const satisfies readonly (keyof MigrationNotice)[];
const FLAGS_VIEW_KEYS = ["picks", "rejects", "total"] as const satisfies readonly (keyof FlagsView)[];
const DIR_EMPTY_VIEW_KEYS = [
  "empty",
  "fileCount",
  "dirCount",
  "emptyDirCount",
  "hasUnresolvedLink",
] as const satisfies readonly (keyof DirEmptyView)[];

/** 手写的键表 → 覆盖检查（跑一遍，顺便让 `noUnusedLocals` 满意） */
const KEY_TABLES = {
  RecentDir: checkKeys<RecentDir, typeof RECENT_DIR_KEYS>(RECENT_DIR_KEYS),
  Volume: checkKeys<Volume, typeof VOLUME_KEYS>(VOLUME_KEYS),
  DirEntry: checkKeys<DirEntry, typeof DIR_ENTRY_KEYS>(DIR_ENTRY_KEYS),
  SourceItem: checkKeys<SourceItem, typeof SOURCE_ITEM_KEYS>(SOURCE_ITEM_KEYS),
  MetaFile: checkKeys<MetaFile, typeof META_FILE_KEYS>(META_FILE_KEYS),
  PhotoMeta: checkKeys<PhotoMeta, typeof PHOTO_META_KEYS>(PHOTO_META_KEYS),
  SourceScan: checkKeys<SourceScan, typeof SOURCE_SCAN_KEYS>(SOURCE_SCAN_KEYS),
  TimeEntry: checkKeys<TimeEntry, typeof TIME_ENTRY_KEYS>(TIME_ENTRY_KEYS),
  PhotoCount: checkKeys<PhotoCount, typeof PHOTO_COUNT_KEYS>(PHOTO_COUNT_KEYS),
  FileExif: checkKeys<FileExif, typeof FILE_EXIF_KEYS>(FILE_EXIF_KEYS),
  RepositoryView: checkKeys<RepositoryView, typeof REPOSITORY_VIEW_KEYS>(
    REPOSITORY_VIEW_KEYS,
  ),
  RepositoryPath: checkKeys<RepositoryPath, typeof REPOSITORY_PATH_KEYS>(
    REPOSITORY_PATH_KEYS,
  ),
  RepositoryProbe: checkKeys<RepositoryProbe, typeof REPOSITORY_PROBE_KEYS>(
    REPOSITORY_PROBE_KEYS,
  ),
  ThumbCacheStats: checkKeys<ThumbCacheStats, typeof THUMB_CACHE_STATS_KEYS>(
    THUMB_CACHE_STATS_KEYS,
  ),
  ImportBatchProgress: checkKeys<ImportBatchProgress, typeof IMPORT_BATCH_KEYS>(
    IMPORT_BATCH_KEYS,
  ),
  ImportRunProgress: checkKeys<ImportRunProgress, typeof IMPORT_RUN_KEYS>(
    IMPORT_RUN_KEYS,
  ),
  ImportCurrentItem: checkKeys<ImportCurrentItem, typeof IMPORT_CURRENT_KEYS>(
    IMPORT_CURRENT_KEYS,
  ),
  ImportError: checkKeys<ImportError, typeof IMPORT_ERROR_KEYS>(IMPORT_ERROR_KEYS),
  ImportPrecheck: checkKeys<ImportPrecheck, typeof IMPORT_PRECHECK_KEYS>(
    IMPORT_PRECHECK_KEYS,
  ),
  ImportStart: checkKeys<ImportStart, typeof IMPORT_START_KEYS>(IMPORT_START_KEYS),
  ImportPlannedRun: checkKeys<ImportPlannedRun, typeof IMPORT_PLANNED_RUN_KEYS>(
    IMPORT_PLANNED_RUN_KEYS,
  ),
  InterruptedRun: checkKeys<InterruptedRun, typeof INTERRUPTED_RUN_KEYS>(
    INTERRUPTED_RUN_KEYS,
  ),
  RepositorySettings: checkKeys<RepositorySettings, typeof REPOSITORY_SETTINGS_KEYS>(
    REPOSITORY_SETTINGS_KEYS,
  ),
  TemplatePreview: checkKeys<TemplatePreview, typeof TEMPLATE_PREVIEW_KEYS>(
    TEMPLATE_PREVIEW_KEYS,
  ),
  AssetItem: checkKeys<AssetItem, typeof ASSET_ITEM_KEYS>(ASSET_ITEM_KEYS),
  BrowseWindow: checkKeys<BrowseWindow, typeof BROWSE_WINDOW_KEYS>(BROWSE_WINDOW_KEYS),
  TimelineEntry: checkKeys<TimelineEntry, typeof TIMELINE_ENTRY_KEYS>(TIMELINE_ENTRY_KEYS),
  BrowseTimeline: checkKeys<BrowseTimeline, typeof BROWSE_TIMELINE_KEYS>(BROWSE_TIMELINE_KEYS),
  FacetCount: checkKeys<FacetCount, typeof FACET_COUNT_KEYS>(FACET_COUNT_KEYS),
  BrowseFacets: checkKeys<BrowseFacets, typeof BROWSE_FACETS_KEYS>(BROWSE_FACETS_KEYS),
  MarkingItem: checkKeys<MarkingItem, typeof MARKING_ITEM_KEYS>(MARKING_ITEM_KEYS),
  MarkResult: checkKeys<MarkResult, typeof MARK_RESULT_KEYS>(MARK_RESULT_KEYS),
  DeleteFailure: checkKeys<DeleteFailure, typeof DELETE_FAILURE_KEYS>(DELETE_FAILURE_KEYS),
  DeleteResult: checkKeys<DeleteResult, typeof DELETE_RESULT_KEYS>(DELETE_RESULT_KEYS),
  FlagsView: checkKeys<FlagsView, typeof FLAGS_VIEW_KEYS>(FLAGS_VIEW_KEYS),
  RebuildProgress: checkKeys<RebuildProgress, typeof REBUILD_PROGRESS_KEYS>(
    REBUILD_PROGRESS_KEYS,
  ),
  RebuildReport: checkKeys<RebuildReport, typeof REBUILD_REPORT_KEYS>(REBUILD_REPORT_KEYS),
  MigrationNotice: checkKeys<MigrationNotice, typeof MIGRATION_NOTICE_KEYS>(
    MIGRATION_NOTICE_KEYS,
  ),
  DirEmptyView: checkKeys<DirEmptyView, typeof DIR_EMPTY_VIEW_KEYS>(DIR_EMPTY_VIEW_KEYS),
} as const;

const HERE = dirname(fileURLToPath(import.meta.url));

function readFixture(): Record<string, unknown> {
  const raw = readFileSync(join(HERE, "dto-contract.json"), "utf8");
  return JSON.parse(raw) as Record<string, unknown>;
}

test("键表与 dto-contract.json 逐字一致（Rust 侧也对着它断言）", () => {
  const fixture = readFixture();
  for (const [name, keys] of Object.entries(KEY_TABLES)) {
    const expected = fixture[name];
    assert.ok(
      Array.isArray(expected),
      `dto-contract.json 里缺少 ${name} 的键表`,
    );
    assert.deepEqual(
      [...keys].sort(),
      [...(expected as string[])].sort(),
      `${name} 的键与契约文件不一致：Rust 或 TS 有一侧改了字段名`,
    );
  }
});

test("契约文件里的每一项都有对应的键表（没有漏测的 DTO）", () => {
  const fixture = readFixture();
  const coveredNames = new Set(Object.keys(KEY_TABLES));
  const fixtureNames = Object.keys(fixture).filter((k) => !k.startsWith("_"));
  for (const name of fixtureNames) {
    assert.ok(
      coveredNames.has(name),
      `dto-contract.json 里的 ${name} 在测试里没有对应键表`,
    );
  }
});

test("键表本身不重复、也不为空", () => {
  for (const [name, keys] of Object.entries(KEY_TABLES)) {
    assert.ok(keys.length > 0, `${name} 的键表不能为空`);
    assert.equal(
      new Set(keys).size,
      keys.length,
      `${name} 的键表里有重复项`,
    );
  }
});
