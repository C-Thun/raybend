/**
 * 「排除」纯函数的单测（`src/lib/excluded.ts`）。
 *
 * 重点覆盖边界：不含子目录时的**层级判据**、Windows 大小写与反斜杠、
 * 中文/超长路径、以及「目录被取消勾选后排除不该再算进计数」。
 */

import assert from "node:assert/strict";
import test from "node:test";
import type { CheckedDir } from "./checked-dir.ts";
import {
  countExcludedInDirs,
  importPhotoCount,
  invertExcluded,
  isUnderDir,
} from "./excluded.ts";

function dir(path: string, includeSubdirs = false): CheckedDir {
  return { path, includeSubdirs, photoCount: null, counting: false };
}

test("isUnderDir：含子目录时任意层级都算，不含时只有直属文件算", () => {
  // 直属
  assert.equal(isUnderDir("/src/2026/a.jpg", "/src/2026", true), true);
  assert.equal(isUnderDir("/src/2026/a.jpg", "/src/2026", false), true);
  // 一级子目录
  assert.equal(isUnderDir("/src/2026/_RAW/a.orf", "/src/2026", true), true);
  assert.equal(isUnderDir("/src/2026/_RAW/a.orf", "/src/2026", false), false);
  // 更深
  assert.equal(isUnderDir("/src/2026/a/b/c.jpg", "/src/2026", true), true);
  assert.equal(isUnderDir("/src/2026/a/b/c.jpg", "/src/2026", false), false);
});

test("isUnderDir：**前缀相近但不是子目录**的路径不算（分隔符边界）", () => {
  // `/src/2026-08` 不是 `/src/2026` 的子目录 —— 少了这个判据就会漏排/误排
  assert.equal(isUnderDir("/src/2026-08/a.jpg", "/src/2026", true), false);
  assert.equal(isUnderDir("/src/2026-08/a.jpg", "/src/2026", false), false);
  // 同级、上级
  assert.equal(isUnderDir("/src/2027/a.jpg", "/src/2026", true), false);
  assert.equal(isUnderDir("/src/a.jpg", "/src/2026", true), false);
});

test("isUnderDir：Windows 路径、反斜杠与大小写差异都要认得", () => {
  assert.equal(isUnderDir("D:\\Photos\\2026\\a.JPG", "d:/photos/2026", true), true);
  assert.equal(isUnderDir("D:\\Photos\\2026\\a.JPG", "d:/photos/2026", false), true);
  assert.equal(isUnderDir("D:\\Photos\\2026\\_RAW\\a.ORF", "d:/Photos/2026", false), false);
  assert.equal(isUnderDir("D:\\Photos\\2026\\_RAW\\a.ORF", "d:/Photos/2026", true), true);
  // 尾部斜杠 / 重复斜杠 / 驱动器根
  assert.equal(isUnderDir("D:/Photos/a.jpg", "D:/Photos/", false), true);
  assert.equal(isUnderDir("D://Photos//a.jpg", "D:/Photos", false), true);
  assert.equal(isUnderDir("D:/a.jpg", "D:/", false), true);
});

test("isUnderDir：中文与超长路径、空路径", () => {
  const deep = `/照片/2026 年度/婚礼/${"子目录/".repeat(60)}照片.jpg`;
  assert.equal(isUnderDir(deep, "/照片/2026 年度", true), true);
  assert.equal(isUnderDir(deep, "/照片/2026 年度", false), false);
  assert.equal(isUnderDir("", "/照片", true), false);
  assert.equal(isUnderDir("/照片/a.jpg", "", true), false);
});

test("countExcludedInDirs：只数落在已勾选目录里的那些", () => {
  const excluded = [
    "/src/a/1.jpg",
    "/src/a/_RAW/2.orf",
    "/src/b/3.jpg",
    "/other/4.jpg",
  ];
  // 只勾了 a 且不含子目录 → 只有 1.jpg
  assert.equal(countExcludedInDirs(excluded, [dir("/src/a")]), 1);
  // 含子目录 → 1.jpg + 2.orf
  assert.equal(countExcludedInDirs(excluded, [dir("/src/a", true)]), 2);
  // 两条目录：a（含子目录）+ b → 3 条
  assert.equal(
    countExcludedInDirs(excluded, [dir("/src/a", true), dir("/src/b")]),
    3,
  );
  // 一条都没勾 → 0（排除集合还在，只是与这次导入无关）
  assert.equal(countExcludedInDirs(excluded, []), 0);
  // 排除集合为空
  assert.equal(countExcludedInDirs([], [dir("/src/a", true)]), 0);
});

test("importPhotoCount：总数未知时保持未知，不会算出负数", () => {
  assert.equal(importPhotoCount(100, 3), 97);
  assert.equal(importPhotoCount(3, 3), 0);
  // 排除比总数还多（目录刚重扫、数目还没刷新）时不该出现负号
  assert.equal(importPhotoCount(2, 5), 0);
  assert.equal(importPhotoCount(null, 5), null);
});

test("invertExcluded：未排除的排除、已排除的恢复；不改原集合", () => {
  const before: ReadonlySet<string> = new Set(["/a", "/b"]);
  const after = invertExcluded(before, ["/b", "/c"]);
  assert.deepEqual([...after].sort((a, b) => a.localeCompare(b)), ["/a", "/c"]);
  assert.deepEqual(
    [...before].sort((a, b) => a.localeCompare(b)),
    ["/a", "/b"],
    "原集合不能被改",
  );
  // 空输入：原样返回（同一个引用也无妨，调用方按不可变用）
  assert.equal(invertExcluded(before, []), before);
});
