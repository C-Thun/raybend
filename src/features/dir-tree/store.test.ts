/**
 * 目录树内部状态的测试。
 *
 * 两条最要紧的性质：
 *   1. **展开 = 加载一次**（并发的两次展开不会读两遍；折叠再展开是秒开）；
 *   2. **读失败要说清楚**，而且不能把「失败」表现成「这个目录是空的」。
 *
 * 另外这里**没有**「选中」这个输入 —— 「选中不触发展开」这条规则
 * 靠结构成立，不靠自觉（`DESIGN.md` §12.4.1）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import type { DirEntry } from "../../api/types.ts";
import { createDirTreeStore } from "./store.ts";

function dir(path: string): DirEntry {
  return { path, name: path };
}

/** 可挂起、可失败的假加载器 */
function fakeLoader(entries: Record<string, DirEntry[]> = {}) {
  const calls: string[] = [];
  const pendings: Array<() => void> = [];
  const state = { hold: false, fail: false };

  const loadDirs = async (path: string): Promise<DirEntry[]> => {
    calls.push(path);
    if (state.hold) {
      await new Promise<void>((resolve) => pendings.push(resolve));
    }
    if (state.fail) throw new Error(`读不了 ${path}`);
    return entries[path] ?? [];
  };

  return {
    loadDirs,
    calls,
    state,
    release() {
      const waiters = pendings.splice(0, pendings.length);
      for (const resolve of waiters) resolve();
    },
  };
}

test("展开会读一次子目录，并记在状态里", async () => {
  const loader = fakeLoader({ "/a": [dir("/a/b"), dir("/a/c")] });
  const store = createDirTreeStore({ loadDirs: loader.loadDirs });

  assert.equal(store.isExpanded("/a"), false);
  assert.equal(store.childrenOf("/a"), undefined, "没展开过就没读过");

  await store.expand("/a");
  assert.equal(store.isExpanded("/a"), true);
  assert.deepEqual(
    store.childrenOf("/a")?.map((entry) => entry.name),
    ["/a/b", "/a/c"],
  );
  assert.equal(loader.calls.length, 1);
});

test("折叠再展开是秒开（不重复读）", async () => {
  const loader = fakeLoader({ "/a": [dir("/a/b")] });
  const store = createDirTreeStore({ loadDirs: loader.loadDirs });

  await store.expand("/a");
  store.collapse("/a");
  assert.equal(store.isExpanded("/a"), false);
  assert.ok(store.childrenOf("/a"), "折叠不该把读到的东西丢掉");

  await store.expand("/a");
  assert.equal(loader.calls.length, 1, "第二次展开不该再读一遍");
});

test("并发的两次展开只读一次", async () => {
  const loader = fakeLoader({ "/a": [dir("/a/b")] });
  loader.state.hold = true;
  const store = createDirTreeStore({ loadDirs: loader.loadDirs });

  const first = store.expand("/a");
  const second = store.expand("/a");
  assert.equal(store.isLoading("/a"), true);
  loader.release();
  await Promise.all([first, second]);

  assert.equal(loader.calls.length, 1, "两个请求要合并");
  assert.equal(store.isLoading("/a"), false);
  assert.ok(store.childrenOf("/a"));
});

test("toggle：展开与折叠交替", async () => {
  const loader = fakeLoader({ "/a": [] });
  const store = createDirTreeStore({ loadDirs: loader.loadDirs });

  await store.toggle("/a");
  assert.equal(store.isExpanded("/a"), true);
  await store.toggle("/a");
  assert.equal(store.isExpanded("/a"), false);
});

test("读失败：记下原因、不当成「空目录」", async () => {
  const loader = fakeLoader();
  loader.state.fail = true;
  const store = createDirTreeStore({ loadDirs: loader.loadDirs });

  await store.expand("/a");
  assert.match(store.errorOf("/a") ?? "", /读不了 \/a/);
  assert.equal(
    store.childrenOf("/a"),
    undefined,
    "失败不等于「这个目录里什么都没有」",
  );
  assert.equal(store.isLoading("/a"), false, "别把加载态永远挂着");
  assert.equal(store.isExpanded("/a"), true, "仍然处于展开态，让用户看到错误行");
});

test("refresh 会重新读并清掉上一次的错误", async () => {
  const loader = fakeLoader({ "/a": [dir("/a/b")] });
  loader.state.fail = true;
  const store = createDirTreeStore({ loadDirs: loader.loadDirs });

  await store.expand("/a");
  assert.ok(store.errorOf("/a"));

  loader.state.fail = false;
  await store.refresh("/a");
  assert.equal(store.errorOf("/a"), undefined);
  assert.deepEqual(store.childrenOf("/a")?.length, 1);
  assert.equal(loader.calls.length, 2);
});

test("多个目录各自独立（状态不串）", async () => {
  const loader = fakeLoader({ "/a": [dir("/a/1")], "/b": [dir("/b/1"), dir("/b/2")] });
  const store = createDirTreeStore({ loadDirs: loader.loadDirs });

  await store.expand("/a");
  await store.expand("/b");
  assert.equal(store.childrenOf("/a")?.length, 1);
  assert.equal(store.childrenOf("/b")?.length, 2);
  store.collapse("/a");
  assert.equal(store.isExpanded("/b"), true, "折叠一个不该影响另一个");
});

test("空目录：读回来是空数组（而不是 undefined）", async () => {
  const loader = fakeLoader({ "/empty": [] });
  const store = createDirTreeStore({ loadDirs: loader.loadDirs });

  await store.expand("/empty");
  assert.deepEqual(store.childrenOf("/empty"), []);
  assert.notEqual(store.childrenOf("/empty"), undefined);
});

test("refreshAll：只重读展开着的目录，且保留展开状态", async () => {
  const calls: string[] = [];
  const listings: Record<string, DirEntry[]> = {
    "/a": [{ name: "b", path: "/a/b" }],
    "/b": [{ name: "c", path: "/b/c" }],
  };
  const store = createDirTreeStore({
    loadDirs: async (path) => {
      calls.push(path);
      return listings[path] ?? [];
    },
  });

  await store.expand("/a");
  await store.expand("/b");
  calls.length = 0;

  await store.refreshAll();
  assert.deepEqual(calls.sort(), ["/a", "/b"], "展开着的两个都要重读");
  assert.equal(store.isExpanded("/a"), true, "刷新不该把展开状态弄丢");
  assert.equal(store.isExpanded("/b"), true);
});

test("refreshAll：外部新增的目录，刷新之后能看到（这就是「运行期刷新」的意义）", async () => {
  let extra: DirEntry | null = null;
  const store = createDirTreeStore({
    loadDirs: async () => [
      { name: "原有", path: "/a/原有" },
      ...(extra === null ? [] : [extra]),
    ],
  });

  await store.expand("/a");
  assert.equal(store.childrenOf("/a")?.length, 1);

  // 程序外面新建了一个目录：我们不监听，所以**不刷新就看不到**（这是明说的取舍）
  extra = { name: "外部新建", path: "/a/外部新建" };
  assert.equal(store.childrenOf("/a")?.length, 1, "没刷新时仍是缓存的旧结果");

  await store.refreshAll();
  assert.equal(store.childrenOf("/a")?.length, 2, "刷新之后应当看到新目录");
});
