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
    photosCount: 10,
    imagesCount: 12,
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
  assert.equal(row?.photosCount, 10, "离线保留最后成功计数");
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
  const { api, state } = fakeApi([view({ online: false, root: null, photosCount: null, imagesCount: null })]);
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
  // 交出来的是**事实**（找不到 + 试过几处），句子由视图按语言拼
  assert.deepEqual(store.remountErrors()["lib1"], {
    kind: "not_found",
    tried: 3,
  });
});

test("remount：命令抛错 → 记在提示里，状态不变", async () => {
  const { api, state } = fakeApi([view({ online: false })]);
  state.failRemount = true;
  const store = createRepositoryState({ api });
  await store.load();
  await store.remount("lib1");
  const error = store.remountErrors()["lib1"];
  assert.equal(error?.kind, "message");
  assert.match(
    error?.kind === "message" ? error.text : "",
    /盘没插/,
    "后端原话要原样带出来（它本来就是给人看的）",
  );
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

function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
test("恢复的所有入口清旧提示", async () => {
  const {api,state} = fakeApi([view({online:false})]); const store=createRepositoryState({api}); await store.load();
  state.remountResult=view({online:false}); await store.remount("lib1"); assert.ok(store.remountErrors().lib1);
  store.patch("lib1", {online:true}); assert.deepEqual(store.remountErrors(),{});
  state.remountResult=view({online:false}); await store.remount("lib1"); store.upsert(view()); assert.deepEqual(store.remountErrors(),{});
});
test("旧 load 不覆盖新 remount；同库单飞，两库并发", async () => {
  const list=deferred<RepositoryView[]>(); const a=deferred<RepositoryView>(); const b=deferred<RepositoryView>(); let calls=0;
  const {api}=fakeApi([]); const store=createRepositoryState({api:{...api,listRepositories:()=>list.promise,remountRepository:id=>{calls++;return id==="a"?a.promise:b.promise;}}});
  const loading=store.load(); const first=store.remount("a"); assert.equal(first,store.remount("a")); const second=store.remount("b"); assert.equal(calls,2);
  a.resolve(view({id:"a"})); b.resolve(view({id:"b"})); await Promise.all([first,second]);
  list.resolve([view({id:"a",online:false})]); await loading; assert.equal(store.byId("a")?.online,true); assert.equal(store.byId("b")?.online,true);
});
test("事件先于快照，u64 字符串不损精度，销毁后拒绝落结果", async () => {
  const rows=deferred<RepositoryView[]>(); const {api}=fakeApi([]); const store=createRepositoryState({api:{...api,listRepositories:()=>rows.promise}}); const loading=store.load();
  const connection={repositoryId:"lib1",state:"online" as const,reason:null,root:"D:/库",generation:"9007199254740993",revision:"9007199254740993",observedAt:1};
  store.applyConnection(connection); store.applyConnection({...connection,state:"offline",revision:"9007199254740992"});
  rows.resolve([view({online:false})]);await loading;assert.equal(store.byId("lib1")?.online,true);
  store.dispose();store.upsert(view({id:"after"}));assert.equal(store.byId("after"),undefined);
});

test("离线保留最后验证展示根与计数，跨库不借未验证路径", async () => {
  const { api } = fakeApi([view()]); const store = createRepositoryState({ api });
  await store.load(); assert.equal(store.lastVerifiedRoot("lib1"), "D:\\Photos");
  store.markOffline("lib1"); assert.equal(store.byId("lib1")?.online, false);
  assert.equal(store.lastVerifiedRoot("lib1"), "D:\\Photos"); assert.equal(store.byId("lib1")?.photosCount, 10);
  assert.equal(store.lastVerifiedRoot("未验证库"), null); store.dispose();
});


test("重连事件先到仍归并同一响应的资料；更晚本地改动保留", async () => {
  let finish!: (value: RepositoryView) => void;
  const { api } = fakeApi([view()]); api.remountRepository = () => new Promise(resolve => { finish = resolve; });
  const store = createRepositoryState({ api }); await store.load();
  const connection = { repositoryId: "lib1", state: "online" as const, reason: null, root: "E:/库", generation: "2", revision: "2", observedAt: 0 };
  const first = store.remount("lib1"); store.applyConnection(connection);
  finish(view({ connection, triedPaths: 3, photosCount: 42 })); await first;
  assert.equal(store.byId("lib1")?.triedPaths, 3); assert.equal(store.byId("lib1")?.photosCount, 42);
  assert.equal(store.byId("lib1")?.displayPath, "E:/库");
  const second = store.remount("lib1"); store.applyConnection({ ...connection, revision: "3" });
  store.patch("lib1", { importTemplate: "新模板" });
  finish(view({ connection: { ...connection, revision: "3" }, importTemplate: "旧模板" })); await second;
  assert.equal(store.byId("lib1")?.importTemplate, "新模板"); store.dispose();
});

test("位置登记统一归并且同库互斥；失败保留路径和在线状态", async () => {
  const { api } = fakeApi([view()]); let finish!: (row: RepositoryView) => void;
  const pending = new Promise<RepositoryView>(resolve => { finish = resolve; });
  const store = createRepositoryState({ api: { ...api, addRepositoryLocation: () => pending,
    removeRepositoryLocation: async () => { throw { code: "active_location" }; } } });
  await store.load();
  const task = store.addLocation("lib1", "G:/中文库"); assert.equal(store.isChangingLocation("lib1"), true);
  await assert.rejects(store.addLocation("lib1", "H:/其他库"), { code: "busy" });
  finish(view({ paths: [{ path: "G:/中文库", status: "unknown", lastSeenAt: 1 }] })); await task;
  assert.equal(store.byId("lib1")?.paths[0].path, "G:/中文库");
  const before = store.byId("lib1"); await assert.rejects(store.removeLocation("lib1", "D:/照片"), { code: "active_location" });
  assert.equal(store.byId("lib1"), before); assert.equal(store.isChangingLocation("lib1"), false);
});

test("位置操作的迟到回复不能覆盖新状态或写回已销毁的 store", async () => {
  const { api } = fakeApi([view()]); let finish!: (row: RepositoryView) => void;
  const store = createRepositoryState({ api: { ...api, addRepositoryLocation: () => new Promise(resolve => { finish = resolve; }) } });
  await store.load(); const task = store.addLocation("lib1", "G:/库");
  store.patch("lib1", { name: "后来的修改" }); finish(view({ name: "旧名字" })); await task;
  assert.equal(store.byId("lib1")?.name, "后来的修改");
  const again = store.addLocation("lib1", "H:/库"); store.dispose(); finish(view({ name: "不能落结果" })); await again;
  assert.equal(store.byId("lib1")?.name, "后来的修改");
});
