import test from "node:test";
import assert from "node:assert/strict";
import { createCatalogRefresh } from "./catalog-refresh.ts";

test("目录事件风暴：同范围合并、执行期间的变更不会丢失", async () => {
  const calls: [string, readonly string[]][] = [];
  let release!: () => void;
  const blocker = new Promise<void>((resolve) => { release = resolve; });
  const refresh = createCatalogRefresh(async (id, scopes) => { calls.push([id, scopes]); if (calls.length === 1) await blocker; });
  const first = refresh.request("A", ["photos/a"]);
  await Promise.resolve();
  for (let i = 0; i < 1000; i++) assert.equal(refresh.request("A", ["photos/b"]), first);
  release(); await first;
  assert.deepEqual(calls, [["A", ["photos/a"]], ["A", ["photos/b"]]]);
});
test("换库清掉旧范围，卸载后不再刷新", async () => {
  const calls: [string, readonly string[]][] = [];
  const refresh = createCatalogRefresh(async (id, scopes) => { calls.push([id, scopes]); });
  const first = refresh.request("旧库", ["photos/旧"]);
  refresh.request("新库", ["photos/新"]);
  await first;
  assert.deepEqual(calls, [["新库", ["photos/新"]]]);
  refresh.dispose(); await refresh.request("新库", ["photos/a"]);
  assert.equal(calls.length, 1);
});
test("事件范围过多时合并为重扫所有活跃目录，内存与扫描都有界", async () => {
  const batches: number[] = [];
  const refresh = createCatalogRefresh(async (_id, scopes) => { batches.push(scopes.length); });
  await refresh.request("A", Array.from({length: 200}, (_, i) => `photos/${i}`));
  assert.deepEqual(batches, [0]);
  assert.ok(batches.every((size) => size <= 66));
});
