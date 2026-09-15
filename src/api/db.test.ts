/**
 * 数据层的单元测试。重点覆盖**浏览器降级**（`pnpm dev` / `pnpm smoke:ui` 都跑在 Chromium 里）
 * 与 `toBytes` 的三种入参形状 —— 这两处在真机上出问题时表现为「界面空白/破图」，
 * 是最难从错误日志里看出来的那一类。
 *
 * 刻意不测「在 Tauri 里」的路径：那需要真跑一个 Tauri 进程，
 * 属于人类 E2E（`AGENTS.md` §2.8）。这里只钉住「没有 Tauri 时必须优雅」。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  EMPTY_FILE_EXIF,
  getBooleanSetting,
  getNumberSetting,
  getSetting,
  getThumbBytes,
  listDirs,
  listRecentDirs,
  listRepositories,
  listVolumes,
  probeRepository,
  readFileExif,
  readSourceTimes,
  scanSourceDir,
  setSetting,
  toBytes,
} from "./db.ts";

/* ══════════════════════════════════════════════════════════════
 * 浏览器降级：返回空值而不是抛错
 * ══════════════════════════════════════════════════════════════ */

test("浏览器里：列表类命令返回空数组，不是抛错", async () => {
  assert.deepEqual(await listRecentDirs(), []);
  assert.deepEqual(await listVolumes(), []);
  assert.deepEqual(await listDirs("/photos"), []);
  assert.deepEqual(await listRepositories(), []);
  assert.deepEqual(await readSourceTimes(["/a.jpg"]), []);
});

test("浏览器里：目录扫描返回空清单但保留 root（界面能显示「这个目录没有照片」）", async () => {
  const scan = await scanSourceDir("/photos");
  assert.equal(scan.root, "/photos");
  assert.deepEqual(scan.items, []);
  assert.deepEqual(scan.problems, []);
  assert.equal(scan.elapsedMs, 0);
});

test("浏览器里：缩略图返回 null（调用方显示占位图）", async () => {
  assert.equal(await getThumbBytes("/photos/a.jpg"), null);
  assert.equal(await getThumbBytes("/photos/a.jpg", "strip"), null);
});

test("浏览器里：EXIF 返回空值而不是抛错", async () => {
  assert.deepEqual(await readFileExif("/photos/a.jpg"), EMPTY_FILE_EXIF);
});

test("浏览器里：探测目录时按「可以新建库」处理（不假装已有库）", async () => {
  const probe = await probeRepository("/some/dir");
  assert.equal(probe.kind, "empty");
  assert.equal(probe.registered, false);
});

test("浏览器里：设置读写不炸（没有 localStorage 就静默放弃）", async () => {
  assert.equal(await getSetting("test.key"), null);
  await setSetting("test.key", "1"); // 不抛错即通过
  assert.equal(await getNumberSetting("test.n", 42), 42);
  assert.equal(await getBooleanSetting("test.b", true), true);
});

/* ══════════════════════════════════════════════════════════════
 * toBytes：IPC 字节的三种形状
 * ══════════════════════════════════════════════════════════════ */

test("toBytes 认识 ArrayBuffer", () => {
  const buffer = new Uint8Array([1, 2, 3]).buffer;
  assert.deepEqual(toBytes(buffer), new Uint8Array([1, 2, 3]));
});

test("toBytes 认识 TypedArray 视图（含偏移，不能整块 buffer 都拿走）", () => {
  const backing = new Uint8Array([9, 1, 2, 3, 9]);
  const view = new Uint8Array(backing.buffer, 1, 3);
  assert.deepEqual(toBytes(view), new Uint8Array([1, 2, 3]));
});

test("toBytes 认识数字数组（JSON 回退路径）", () => {
  assert.deepEqual(toBytes([4, 5, 6]), new Uint8Array([4, 5, 6]));
});

test("toBytes 对认不出的东西返回 null（界面显示占位图，不崩）", () => {
  assert.equal(toBytes(null), null);
  assert.equal(toBytes(undefined), null);
  assert.equal(toBytes("不是字节"), null);
  assert.equal(toBytes({ data: [1, 2] }), null);
  assert.equal(toBytes(123), null);
});

test("toBytes 处理空字节（合法：一张空图不代表出错）", () => {
  assert.deepEqual(toBytes(new ArrayBuffer(0)), new Uint8Array(0));
  assert.deepEqual(toBytes([]), new Uint8Array(0));
});
