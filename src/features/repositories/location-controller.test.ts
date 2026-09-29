import assert from "node:assert/strict";
import test from "node:test";
import type { RepositoryView } from "../../api/types.ts";
import { createLocationController } from "./location-controller.ts";

const row = { id: "库", online: true } as RepositoryView;
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(r => { resolve = r; }); return { promise, resolve }; }
test("取消、关闭或切库后原生选择器的迟到结果不登记位置", async () => {
  for (const cancel of [true, false]) {
    const picked = deferred<string | null>(); const calls: string[] = []; let id: string | null = "库";
    const controller = createLocationController({ target: () => id, pick: () => picked.promise,
      add: async (_, path) => { calls.push(path); return row; }, remove: async () => row, busy: () => {}, error: () => {} });
    const pending = controller.choose();
    if (!cancel) { id = "另一个库"; controller.reset(); }
    picked.resolve(cancel ? null : "G:/中文库"); await pending;
    assert.deepEqual(calls, []);
  }
});
test("重复选择合并；空输入拒绝；错误与忙碌状态只写当前弹窗", async () => {
  const picked = deferred<string | null>(); const errors: unknown[] = []; const busy: boolean[] = []; let picks = 0;
  const controller = createLocationController({ target: () => "库", pick: () => { picks++; return picked.promise; },
    add: async () => row, remove: async () => { throw { code: "active_location" }; }, busy: value => busy.push(value), error: value => errors.push(value) });
  const pending = controller.choose(); await controller.choose(); assert.equal(picks, 1);
  picked.resolve("   "); await pending; assert.deepEqual(errors[errors.length - 1], { code: "invalid_path" });
  await controller.remove("D:/照片库"); assert.deepEqual(errors[errors.length - 1], { code: "active_location" }); assert.equal(busy[busy.length - 1], false);
});
test("登记等待中关闭后不把错误写进重新打开的弹窗", async () => {
  const reply = deferred<RepositoryView>(); const errors: unknown[] = [];
  const controller = createLocationController({ target: () => "库", pick: async () => "G:/照片库",
    add: () => reply.promise, remove: async () => row, busy: () => {}, error: value => errors.push(value) });
  const pending = controller.choose(); await Promise.resolve(); controller.reset();
  const count = errors.length;
  reply.resolve({ ...row, online: false, connection: { state: "unavailable", reason: "catalog_invalid" } as RepositoryView["connection"] });
  await pending; assert.equal(errors.length, count);
});
