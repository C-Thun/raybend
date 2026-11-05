/** 右栏文字字段的就地回填：口径必须与后端 `marking::set_text` 一致。 */

import assert from "node:assert/strict";
import test from "node:test";

import type { AssetItem } from "../../api/types.ts";
import { fillGeoFields, normalizeTextValue, patchTextFields } from "./text-edit.ts";

function item(id: number, overrides: Partial<AssetItem> = {}): AssetItem {
  return {
    id,
    relPath: `photos/${id}.jpg`,
    fileName: `${id}.jpg`,
    ext: "jpg",
    isRaw: false,
    hasRaw: false,
    author: null,
    description: null,
    gpsLat: null,
    gpsLon: null,
    country: null,
    provinceState: null,
    city: null,
    sublocation: null,
    createdMs: null,
    takenAt: null,
    takenAtOffsetMin: null,
    rating: 0,
    colorLabel: null,
    likeState: null,
    lockLevel: 0,
    cameraMake: null,
    cameraModel: null,
    lens: null,
    focalMm: null,
    fNumber: null,
    exposureMs: null,
    iso: null,
    width: null,
    height: null,
    rawWidth: null,
    rawHeight: null,
    latestWidth: null,
    latestHeight: null,
    latestSourceBase: null,
    orientation: null,
    sizeBytes: 1,
    missing: false,
    issueCount: 0,
    edited: false,
    recycledFrom: null,
    ...overrides,
  };
}

test("与后端同口径：trim；空串/纯空白 = 清空（NULL）", () => {
  assert.equal(normalizeTextValue("  崔总 "), "崔总");
  assert.equal(normalizeTextValue(""), null);
  assert.equal(normalizeTextValue("  \t\n "), null);
  assert.equal(normalizeTextValue("第一行\n第二行"), "第一行\n第二行", "描述里的换行要保留");
});

test("只改命中的 id，其余条目保持原引用", () => {
  const untouched = item(2, { author: "张三" });
  const items = [item(1, { author: "旧作者" }), untouched, null];
  const next = patchTextFields(items, new Set([1]), "author", " 崔总 ");
  assert.equal(next[0]?.author, "崔总");
  assert.equal(next[1], untouched, "没命中的条目不该被复制");
  assert.equal(next[2], null);
});

test("清空字段写回 null（不是空串）", () => {
  const next = patchTextFields([item(1, { description: "旧的" })], new Set([1]), "description", "   ");
  assert.equal(next[0]?.description, null);
});

test("值没变化时返回原数组（不触发一次无意义的渲染）", () => {
  const items = [item(1, { city: "杭州" })];
  assert.equal(patchTextFields(items, new Set([1]), "city", " 杭州 "), items);
  assert.equal(patchTextFields(items, new Set(), "city", "北京"), items);
});

test("多个 id 一起改（批量应用走同一条路），空数组里的洞不炸", () => {
  const items = [item(1), null, item(3)];
  const next = patchTextFields(items, new Set([1, 3]), "country", "中国");
  assert.equal(next[0]?.country, "中国");
  assert.equal(next[1], null);
  assert.equal(next[2]?.country, "中国");
});

test("provinceState 这类驼峰键也能写（与 AssetItem 字段同名）", () => {
  const next = patchTextFields([item(1)], new Set([1]), "provinceState", "浙江省");
  assert.equal(next[0]?.provinceState, "浙江省");
});

test("按 GPS 填地点：只填空字段，人写过的值一律不碰", () => {
  const filled = item(1, { country: "中国", city: "苏州" });
  const empty = item(2);
  const next = fillGeoFields([filled, empty], new Set([1, 2]), {
    country: "中国",
    provinceState: "浙江省",
    city: "杭州",
  });
  assert.equal(next[0]?.country, "中国", "已有国家保持不变");
  assert.equal(next[0]?.city, "苏州", "已有城市保持不变");
  assert.equal(next[0]?.provinceState, "浙江省", "空着的省补上");
  assert.equal(next[1]?.country, "中国");
  assert.equal(next[1]?.provinceState, "浙江省");
  assert.equal(next[1]?.city, "杭州");
});

test("按 GPS 填地点：三项都已有值 / 没有建议时返回原数组", () => {
  const complete = item(1, { country: "中国", provinceState: "浙江省", city: "杭州" });
  const items = [complete];
  assert.equal(
    fillGeoFields(items, new Set([1]), { country: "中国", provinceState: "浙江省", city: "杭州" }),
    items,
    "没有可填的就不改引用（不触发渲染）",
  );
  assert.equal(fillGeoFields(items, new Set([1]), {}), items);
  assert.equal(
    fillGeoFields(items, new Set([1]), { country: "  ", provinceState: null, city: null }),
    items,
    "空白建议等于没有建议",
  );
});
