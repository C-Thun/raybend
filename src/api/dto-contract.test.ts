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
  DirEntry,
  FileExif,
  PhotoCount,
  RecentDir,
  RepositoryPath,
  RepositoryProbe,
  RepositoryView,
  SourceItem,
  SourceScan,
  ThumbCacheStats,
  TimeEntry,
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
const DIR_ENTRY_KEYS = ["name", "path"] as const satisfies readonly (keyof DirEntry)[];
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
  "photoCount",
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

/** 手写的键表 → 覆盖检查（跑一遍，顺便让 `noUnusedLocals` 满意） */
const KEY_TABLES = {
  RecentDir: checkKeys<RecentDir, typeof RECENT_DIR_KEYS>(RECENT_DIR_KEYS),
  Volume: checkKeys<Volume, typeof VOLUME_KEYS>(VOLUME_KEYS),
  DirEntry: checkKeys<DirEntry, typeof DIR_ENTRY_KEYS>(DIR_ENTRY_KEYS),
  SourceItem: checkKeys<SourceItem, typeof SOURCE_ITEM_KEYS>(SOURCE_ITEM_KEYS),
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
