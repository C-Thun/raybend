/**
 * 目录选择器的浏览器降级测试。
 *
 * 在 Node/浏览器里没有 Tauri，所以这条路径必须**安静地返回 null** ——
 * 抛错的话，建库弹窗在开发预览下会直接崩（而它本该退回手打路径）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import { pickDirectory } from "./dialog.ts";

test("不在 Tauri 里：返回 null 而不是抛错", async () => {
  assert.equal(await pickDirectory(), null);
  assert.equal(await pickDirectory({ title: "选择库目录" }), null);
});
