/**
 * 命令面板模糊匹配的测试（`lib/command-match.ts`）。
 *
 * 重点：中文子序列、英文缩写/词首、大小写、空查询（最近使用优先）、
 * 以及「最近用过不能盖过明显更准的匹配」。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { rankCommands, scoreCommand, subsequenceScore, type CommandSearchItem } from "./command-match.ts";

const ITEMS: CommandSearchItem[] = [
  { id: "view.filter.toggle", title: "筛选开关", group: "视图" },
  { id: "view.tiles.byTime", title: "按时间", group: "视图" },
  { id: "view.tiles.info", title: "信息档位", group: "视图" },
  { id: "edit.undo", title: "撤销", group: "编辑" },
  { id: "help.palette", title: "命令面板", group: "帮助" },
  { id: "viewer.zoomIn", title: "放大", group: "看图" },
];

const byId = (id: string): CommandSearchItem => {
  const found = ITEMS.find((item) => item.id === id);
  assert.ok(found, `fixture 里没有 ${id}`);
  return found;
};

test("空查询：人人都匹配（顺序交给调用方）", () => {
  assert.equal(scoreCommand("", byId("edit.undo")), 0);
  assert.equal(scoreCommand("   ", byId("edit.undo")), 0);
});

test("完全相等 > 前缀 > 子序列", () => {
  const exact = scoreCommand("撤销", byId("edit.undo"));
  const prefix = scoreCommand("信", byId("view.tiles.info"));
  assert.ok(exact !== null && prefix !== null);
  assert.ok(exact > prefix, `完全相等应当更高：${exact} vs ${prefix}`);
});

test("中文子序列：搜「时间」命中「按时间」，搜「筛」命中「筛选开关」", () => {
  assert.ok(scoreCommand("时间", byId("view.tiles.byTime")) !== null);
  assert.ok(scoreCommand("筛", byId("view.filter.toggle")) !== null);
  assert.ok(scoreCommand("面板", byId("help.palette")) !== null);
  // 顺序不能反（「间时」不是子序列）
  assert.equal(scoreCommand("间时", byId("view.tiles.byTime")), null);
});

test("id 也能匹配：搜「zoom」命中 viewer.zoomIn", () => {
  assert.ok(scoreCommand("zoom", byId("viewer.zoomIn")) !== null);
  assert.ok(scoreCommand("byTime", byId("view.tiles.byTime")) !== null, "驼峰词首也要能中");
});

test("大小写不敏感；中文与英文混排都能搜", () => {
  assert.ok(scoreCommand("UNDO", byId("edit.undo")) !== null);
  assert.ok(scoreCommand("undo", byId("edit.undo")) !== null);
});

test("不匹配返回 null（面板里直接不显示）", () => {
  assert.equal(scoreCommand("xyzzy", byId("edit.undo")), null);
  assert.equal(scoreCommand("导入", byId("edit.undo")), null);
});

test("subsequenceScore：连续命中比跳着命中分高", () => {
  const consecutive = subsequenceScore("undo", "undo");
  const scattered = subsequenceScore("undo", "u-n-d-o");
  assert.ok(consecutive > scattered, `${consecutive} vs ${scattered}`);
});

test("rankCommands：有查询时按分数排，同分保持注册表顺序", () => {
  const ranked = rankCommands("view", ITEMS);
  assert.ok(ranked.length >= 2);
  // 全部来自 view.* 的那几条，顺序应当与 ITEMS 一致
  const viewIds = ranked.filter((item) => item.id.startsWith("view.")).map((item) => item.id);
  const expected = ITEMS.filter((item) => item.id.startsWith("view.")).map((item) => item.id);
  assert.deepEqual(viewIds, expected);
});

test("rankCommands：空查询把最近用过的排到最前（按最近顺序）", () => {
  const ranked = rankCommands("", ITEMS, ["help.palette", "edit.undo"]);
  assert.equal(ranked[0].id, "help.palette");
  assert.equal(ranked[1].id, "edit.undo");
  // 其余的保持注册表顺序
  assert.deepEqual(
    ranked.slice(2).map((item) => item.id),
    ITEMS.filter((item) => item.id !== "help.palette" && item.id !== "edit.undo").map((item) => item.id),
  );
});

test("rankCommands：最近加分只打破接近的平局，不盖过明显更准的匹配", () => {
  // 「撤销」完全相等（1000）对上「筛选开关」的最近加分（≤20）—— 顺序不该被翻转
  const ranked = rankCommands("撤销", ITEMS, ["view.filter.toggle"]);
  assert.equal(ranked[0].id, "edit.undo");
});

test("关键词：搜键位能命中绑了它的命令（F11）", () => {
  const item: CommandSearchItem = {
    id: "viewer.fullscreen",
    title: "全屏看图",
    group: "视图",
    keywords: ["F11"],
  };
  assert.ok(scoreCommand("F11", item) !== null, "搜 F11 必须命中（面板上就写着这个键）");
  assert.ok(scoreCommand("f11", item) !== null, "大小写不敏感");
  assert.ok(scoreCommand("F1", item) !== null, "前缀也能搜到");
  assert.equal(scoreCommand("F12", item), null, "不是自己的键不该命中");
});

test("关键词：带空格的英文查询会试连写形式（full screen → fullscreen）", () => {
  const item: CommandSearchItem = {
    id: "viewer.fullscreen",
    title: "全屏看图",
    group: "视图",
    keywords: ["F11"],
  };
  assert.ok(scoreCommand("full screen", item) !== null, "按词组写也要能中 id 的连写形式");
  assert.ok(scoreCommand("fullscreen", item) !== null);
});

test("关键词：Mod 写法也收（搜 Mod+K 命中绑 Mod+K 的命令）", () => {
  const item: CommandSearchItem = {
    id: "help.palette",
    title: "命令面板",
    group: "帮助",
    keywords: ["Ctrl+K", "Mod+K"],
  };
  assert.ok(scoreCommand("Mod+K", item) !== null);
  assert.ok(scoreCommand("ctrl+k", item) !== null);
});

test("rankCommands：没有匹配时返回空数组（面板显示「没找到」）", () => {
  assert.deepEqual(rankCommands("zzzzz", ITEMS), []);
});
