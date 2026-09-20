/**
 * 「信息」档位开关的纯逻辑（`components/ui/tile-info.ts`）。
 *
 * 档位本身的循环很直白，真正容易错的是**范围**：`i` 键只在 tiles / film 下接
 * （人类 2026-09-19 定的），纯看图态不该有反应。两个工作区共用这一条判据，
 * 所以钉在这里 —— 它在浏览侧曾经整条缺失（2026-09-20 人类报「信息级别无效」）。
 */

import assert from "node:assert/strict";
import test from "node:test";

import {
  importInfoMode,
  infoKeyApplies,
  nextTileInfoMode,
  TILE_INFO_CYCLE,
  toggleImportTileInfo,
} from "./tile-info.ts";
import { resetDisplayPrefsForTests } from "../../lib/display-prefs.ts";

test("档位循环：off → marks → marks-name → off（三态）", () => {
  assert.deepEqual([...TILE_INFO_CYCLE], ["off", "marks", "marks-name"]);
  assert.equal(nextTileInfoMode("off"), "marks");
  assert.equal(nextTileInfoMode("marks"), "marks-name");
  assert.equal(nextTileInfoMode("marks-name"), "off");
});

test("档位循环：脏值退回 off（不卡在非法档位）", () => {
  assert.equal(nextTileInfoMode("nonsense" as never), "off");
});

test("import 信息只有关 / 文件名两态", () => {
  resetDisplayPrefsForTests();
  assert.equal(importInfoMode(), "off");
  toggleImportTileInfo();
  assert.equal(importInfoMode(), "marks-name");
  toggleImportTileInfo();
  assert.equal(importInfoMode(), "off");
  resetDisplayPrefsForTests();
});

test("infoKeyApplies：tiles 下接，纯看图态不接，film 下接", () => {
  // 没在看图 = tiles
  assert.equal(infoKeyApplies({ viewing: false, filmVisible: false }), true);
  // 看图 + 胶片带可见（film 三态里的前两态）
  assert.equal(infoKeyApplies({ viewing: true, filmVisible: true }), true);
  // 纯看图（view only）：`i` 不该有任何作用
  assert.equal(infoKeyApplies({ viewing: true, filmVisible: false }), false);
});
