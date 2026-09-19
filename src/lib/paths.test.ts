/**
 * `joinPath` 的单测：把库内相对路径接到库根上。
 *
 * 重点在**两种分隔符**与**重复分隔符**这两类边界 —— 拼错不会报错，
 * 只会在很远的地方表现为「元信息读不到 / 路径比对不上」。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { joinPath } from "./paths.ts";

test("POSIX 根：用斜杠拼", () => {
  assert.equal(joinPath("/mnt/photos/lib", "photos/2026-08-15/MY0001.JPG"), "/mnt/photos/lib/photos/2026-08-15/MY0001.JPG");
});

test("Windows 根：分隔符跟着根走（不出现混合写法）", () => {
  assert.equal(joinPath("C:\\photos\\lib", "photos/MY0001.JPG"), "C:\\photos\\lib\\photos\\MY0001.JPG");
});

test("根末尾已经有分隔符时不重复加", () => {
  assert.equal(joinPath("/lib/", "photos/a.jpg"), "/lib/photos/a.jpg");
  assert.equal(joinPath("C:\\lib\\", "photos/a.jpg"), "C:\\lib\\photos\\a.jpg");
});

test("相对路径以分隔符开头时先去掉（不出现 lib//photos）", () => {
  assert.equal(joinPath("/lib", "/photos/a.jpg"), "/lib/photos/a.jpg");
  assert.equal(joinPath("C:\\lib", "\\\\photos\\\\a.jpg"), "C:\\lib\\photos\\a.jpg");
});

test("空根原样返回相对路径（调用方负责判断根有没有拿到）", () => {
  assert.equal(joinPath("", "photos/a.jpg"), "photos/a.jpg");
});

test("中文与空格照常（不转义、不改写）", () => {
  assert.equal(joinPath("/lib", "照片/我的 相片.jpg"), "/lib/照片/我的 相片.jpg");
});
