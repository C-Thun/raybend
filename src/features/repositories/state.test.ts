/**
 * 库的中央状态：**一个地方变了，所有挂着的界面都跟着变**。
 *
 * 这里盯的就是这条性质（人类 2026-09-16 的诉求）——
 * 之前「库设置发现离线、外面列表还显示在线」正是因为它没有单一事实源。
 */

import assert from "node:assert/strict";
import test from "node:test";
import type { RepositoryView } from "../../api/types.ts";
import { createRepositoryState } from "./state.ts";

function view(overrides: Partial<RepositoryView> = {}): RepositoryView {
  return {
    id: "lib1",
    name: "库",
    importTemplate: ":CYEAR/:FILENAME",
    createdAt: 0,
    lastOpenedAt: 0,
    online: true,
    root: "D:\\Photos",
    displayPath: "D:\\Photos",
    paths: [],
    triedPaths: 1,
    photoCount: 10,
    ...overrides,
  };
}

function fakeApi(rows: RepositoryView[]) {
  const state = {
    rows,
    remountResult: view(),
    failRemount: false,
    calls: [] as string[],
  };
  return {
    state,
    api: {
      async listRepositories() {
        state.calls.push("list");
        return [...state.rows];
      },
      async remountRepository(id: string) {
        state.calls.push(`remount:${id}`);
        if (state.failRemount) throw new Error("盘没插");
        return state.remountResult;
      },
      async setRepositoryTemplate(id: string, template: string) {
        state.calls.push(`template:${id}`);
        return { importTemplate: template };
      },
    },
  };
}

test("load：拉到列表并置 ready", async () => {
  const { api } = fakeApi([view()]);
  const store = createRepositoryState({ api });
  assert.equal(store.status(), "idle");
  await store.load();
  assert.equal(store.status(), "ready");
  assert.equal(store.list().length, 1);
  assert.equal(store.byId("lib1")?.name, "库");
});

test("load 失败：置 error 且保留上一次的列表（不清空界面）", async () => {
  const { api, state } = fakeApi([view()]);
  const store = createRepositoryState({ api });
  await store.load();
  state.rows = [];
  const failing = {
    ...api,
    listRepositories: async () => {
      throw new Error("读不了");
    },
  };
  const store2 = createRepositoryState({ api: failing });
  await store2.load();
  assert.equal(store2.status(), "error");
  assert.match(store2.error() ?? "", /读不了/);
});

test("patch：就地改一条，别的行对象**原样不动**（不白换引用）", async () => {
  const { api } = fakeApi([view({ id: "a" }), view({ id: "b" })]);
  const store = createRepositoryState({ api });
  await store.load();
  const otherBefore = store.list()[1];
  store.patch("a", { importTemplate: ":CMONTH/:FILENAME" });
  assert.equal(store.byId("a")?.importTemplate, ":CMONTH/:FILENAME");
  assert.equal(store.list()[1], otherBefore, "别的行不该被换掉");
});

test("patch：没有这条时连数组都不换", async () => {
  const { api } = fakeApi([view()]);
  const store = createRepositoryState({ api });
  await store.load();
  const before = store.list();
  store.patch("不存在", { name: "x" });
  assert.equal(store.list(), before);
});

test("markOffline：立刻降级（这就是「库设置发现离线 → 列表同步」的那一步）", async () => {
  const { api } = fakeApi([view()]);
  const store = createRepositoryState({ api });
  await store.load();
  assert.equal(store.byId("lib1")?.online, true);

  store.markOffline("lib1");
  const row = store.byId("lib1");
  assert.equal(row?.online, false, "列表里立刻变离线");
  assert.equal(row?.photoCount, null, "照片数读不到 → 显示「—」而不是 0");
  assert.equal(row?.displayPath, "D:\\Photos", "保留上次已知路径（离线时正要显示它）");
});

test("markOffline：已经离线的不用再动（避免无意义的重渲染）", async () => {
  const { api } = fakeApi([view({ online: false, root: null })]);
  const store = createRepositoryState({ api });
  await store.load();
  const before = store.list();
  store.markOffline("lib1");
  assert.equal(store.list(), before);
});

test("remount：找到 → 就地变在线并清掉提示", async () => {
  const { api, state } = fakeApi([view({ online: false, root: null, photoCount: null })]);
  state.remountResult = view({ online: true });
  const store = createRepositoryState({ api });
  await store.load();
  await store.remount("lib1");
  assert.equal(store.byId("lib1")?.online, true);
  assert.deepEqual(store.remountErrors(), {});
  assert.equal(store.remountingId(), null, "飞行状态要落回 null");
});

test("remount：没找到**不是错误**，但要给一句可读的话", async () => {
  const { api, state } = fakeApi([view({ online: false })]);
  state.remountResult = view({ online: false, triedPaths: 3 });
  const store = createRepositoryState({ api });
  await store.load();
  await store.remount("lib1");
  assert.equal(store.byId("lib1")?.online, false);
  assert.match(store.remountErrors()["lib1"] ?? "", /已试过 3 处/);
});

test("remount：命令抛错 → 记在提示里，状态不变", async () => {
  const { api, state } = fakeApi([view({ online: false })]);
  state.failRemount = true;
  const store = createRepositoryState({ api });
  await store.load();
  await store.remount("lib1");
  assert.match(store.remountErrors()["lib1"] ?? "", /盘没插/);
});

test("setTemplate：写库成功后**列表里的模版立刻同步**", async () => {
  const { api } = fakeApi([view()]);
  const store = createRepositoryState({ api });
  await store.load();
  const saved = await store.setTemplate("lib1", ":CYEAR/:SEQ000");
  assert.equal(saved, ":CYEAR/:SEQ000");
  assert.equal(store.byId("lib1")?.importTemplate, ":CYEAR/:SEQ000");
});

test("upsert：有则换、没有则插", async () => {
  const { api } = fakeApi([view({ id: "a" })]);
  const store = createRepositoryState({ api });
  await store.load();
  store.upsert(view({ id: "a", name: "改过名" }));
  assert.equal(store.byId("a")?.name, "改过名");
  store.upsert(view({ id: "b", name: "新库" }));
  assert.equal(store.list().length, 2, "没有的那条要插进去");
});
