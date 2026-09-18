/**
 * 库内目录显示口径的三个纯函数（`dirs.ts`）。
 *
 * 重点钉住的边界：`_RAW` 的**大小写与空白**（Windows 口径）、
 * 结尾斜杠、反斜杠路径、Unicode 目录名、空输入。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import type { DirEntry } from "../../api/types.ts";
import { dirDisplayName, isRawDirName, PHOTOS_DIR, visibleChildDirs } from "./dirs.ts";

function entry(name: string, path = `C:/lib/photos/${name}`): DirEntry {
  return { name, path };
}

// ─────────────────────────── isRawDirName ───────────────────────────

test("_RAW 判定的边界：大小写、空白、相似名字", () => {
  // 命中的：大小写不敏感（Windows 上 _raw 与 _RAW 是同一个目录）
  for (const name of ["_RAW", "_raw", "_Raw", " _RAW ", "_RAW "]) {
    assert.equal(isRawDirName(name), true, `${JSON.stringify(name)} 应当被认成 _RAW`);
  }
  // 不命中的：别的名字、前缀相似、空
  for (const name of ["_RAWX", "RAW", "_RA", "", "  ", "2026-08-15", "我的照片", "_RAW/子目录"]) {
    assert.equal(isRawDirName(name), false, `${JSON.stringify(name)} 不该被认成 _RAW`);
  }
});

test("_RAW 只说目录名：中英混杂与全角字符不受影响", () => {
  assert.equal(isRawDirName("＿RAW"), false, "全角下划线不是保留名");
  assert.equal(isRawDirName("_ＲＡＷ"), false, "全角字母不是保留名");
});

// ─────────────────────────── visibleChildDirs ───────────────────────────

test("过滤保留目录，其余原样保留（含顺序与 path）", () => {
  const input = [
    entry("2026-08-15"),
    entry("_RAW"),
    entry("日本 京都"),
    entry("_raw"),
  ];
  assert.deepEqual(
    visibleChildDirs(input).map((e) => e.name),
    ["2026-08-15", "日本 京都"],
  );
  // path 不被改写（上层还要拿它换算库内相对路径）
  assert.equal(visibleChildDirs(input)[0]?.path, "C:/lib/photos/2026-08-15");
});

test("空输入 / 全被滤掉 → 空数组（不是 undefined）", () => {
  assert.deepEqual(visibleChildDirs([]), []);
  assert.deepEqual(visibleChildDirs([entry("_RAW")]), []);
});

test("过滤不改动原数组", () => {
  const input = [entry("_RAW"), entry("2026")];
  visibleChildDirs(input);
  assert.equal(input.length, 2, "纯函数不许就地改参数");
});

// ─────────────────────────── dirDisplayName ───────────────────────────

test("行名 = 最后一段（正反斜杠、结尾斜杠都认）", () => {
  assert.equal(dirDisplayName("photos/2026-08-15"), "2026-08-15");
  assert.equal(dirDisplayName("photos/2026-08-15/"), "2026-08-15");
  assert.equal(dirDisplayName("photos\\2026-08-15"), "2026-08-15");
  assert.equal(dirDisplayName("photos/日本/京都"), "京都");
});

test("根那一级显示成 photos；空串也给 photos（不显示空白行）", () => {
  assert.equal(dirDisplayName(PHOTOS_DIR), PHOTOS_DIR);
  assert.equal(dirDisplayName(""), PHOTOS_DIR);
  assert.equal(dirDisplayName("   "), PHOTOS_DIR);
  assert.equal(dirDisplayName("/"), PHOTOS_DIR);
});

test("Unicode 与超长名字原样返回最后一段", () => {
  assert.equal(dirDisplayName("photos/2026-08-15 婚礼 📷"), "2026-08-15 婚礼 📷");
  const long = "x".repeat(300);
  assert.equal(dirDisplayName(`photos/${long}`), long);
});
