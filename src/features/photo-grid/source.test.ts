/**
 * 导入侧数据源适配器的测试。
 *
 * 这里钉住的是**跨层的那个接口**：排除状态必须出现在 `GridItem.excluded` 上。
 * 只把 `isExcluded` 挂在 `TilesSource` 边上是不够的 —— 网格（`PhotoGrid`）只读
 * `GridItem`，漏了这一格就是「批量排除点了没有任何效果」（2026-10-27 的真 bug）。
 *
 * 画面到底变不变（Solid 的响应式绑定）在 Node 的 SSR 构建里测不了，
 * 那一段由 `pnpm check:import` 在真浏览器里盯（选一张 → 点「批量排除」→ 看排除态）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import type { SourceItem } from "../../api/types.ts";
import { importSource } from "./source.ts";
import type { PhotoGridStore } from "./store.ts";

function item(name: string, kind: SourceItem["kind"] = "image"): SourceItem {
  return {
    path: `/src/${name}`,
    fileName: name,
    ext: "jpg",
    kind,
    sizeBytes: 10,
    mtimeMs: null,
    takenAtMs: null,
    takenAtSource: null,
    takenAtOffsetMin: null,
  };
}

/**
 * 适配器只用到 `displayItems` 与 `aspectOf` 两处；
 * 用最小桩即可，不必造一个真 store（那会把缩略图队列也拖进来）。
 */
function fakeStore(items: readonly SourceItem[]): PhotoGridStore {
  return {
    displayItems: () => items,
    aspectOf: () => 1.5,
  } as unknown as PhotoGridStore;
}

test("排除状态带到 GridItem 上（itemAt / itemById 都带）", () => {
  const store = fakeStore([item("甲.jpg"), item("乙.jpg")]);
  const excluded = new Set(["/src/甲.jpg"]);
  const source = importSource(store, { isExcluded: (id) => excluded.has(id) });

  assert.equal(source.itemAt(0)?.excluded, true);
  assert.equal(source.itemAt(1)?.excluded, false);
  assert.equal(source.itemById("/src/甲.jpg")?.excluded, true);
  assert.equal(source.itemById("/src/乙.jpg")?.excluded, false);
});

test("没给 isExcluded 时是 false（不是 undefined 才能让 Tile 的 === true 判定稳定）", () => {
  const source = importSource(fakeStore([item("甲.jpg")]));
  assert.equal(source.itemAt(0)?.excluded, false);
});

test("排除集合变化后重新取值会跟着变（信号由适配器在取值时读取）", () => {
  const excluded = new Set<string>();
  const source = importSource(fakeStore([item("甲.jpg")]), {
    isExcluded: (id) => excluded.has(id),
  });
  assert.equal(source.itemAt(0)?.excluded, false);
  excluded.add("/src/甲.jpg");
  assert.equal(source.itemAt(0)?.excluded, true);
});

test("RAW 角标与排除互不影响", () => {
  const source = importSource(fakeStore([item("甲.CR3", "raw")]), {
    isExcluded: () => true,
  });
  const entry = source.itemAt(0);
  assert.equal(entry?.isRaw, true);
  assert.equal(entry?.excluded, true);
});
