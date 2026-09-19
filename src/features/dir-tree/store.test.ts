/**
 * 目录树内部状态的测试。
 *
 * 两条最要紧的性质：
 *   1. **展开总会重读那一级**（缓存只为秒开，不为省读磁盘 —— 没有哪个文件管理器
 *      要用户按「刷新」才看得到真实内容）；并发的两次展开不会读两遍；
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
  // 夹具默认「还有下一层」：与「未读过就乐观画箭头」的旧口径一致，测试意图不变
  return { path, name: path, hasChildren: true };
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

test("折叠再展开：旧内容先秒现（不闪空），同时照常重读那一级", async () => {
  const loader = fakeLoader({ "/a": [dir("/a/b")] });
  const store = createDirTreeStore({ loadDirs: loader.loadDirs });

  await store.expand("/a");
  store.collapse("/a");
  assert.equal(store.isExpanded("/a"), false);
  assert.ok(store.childrenOf("/a"), "折叠不该把读到的东西丢掉");

  const again = store.expand("/a");
  // 重建读还没回来，缓存里的内容**已经在**了 —— 这就是「秒开」的机制
  assert.ok(store.childrenOf("/a"), "重新读的过程中也要有内容可看");
  await again;
  assert.equal(loader.calls.length, 2, "重新展开要重读那一级（缓存不是用来省读磁盘的）");
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
    "/a": [{ name: "b", path: "/a/b", hasChildren: true }],
    "/b": [{ name: "c", path: "/b/c", hasChildren: false }],
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

test("展开总是重读那一级：缓存是为秒开，不是为省读磁盘", async () => {
  let calls = 0;
  let listing: DirEntry[] = [{ name: "a", path: "/root/a", hasChildren: true }];
  const store = createDirTreeStore({
    loadDirs: async () => {
      calls += 1;
      return [...listing];
    },
  });

  await store.expand("/root");
  assert.equal(calls, 1);

  store.collapse("/root");
  // 程序外面新建了一个目录：我们不监听，但**展开**必须自己重读，不该等谁按「刷新」
  listing = [...listing, { name: "外部新建", path: "/root/外部新建", hasChildren: false }];
  await store.expand("/root");

  assert.equal(calls, 2, "再次展开必须重新读，即使缓存里已经有内容");
  assert.equal(store.childrenOf("/root")?.length, 2, "展开之后就看到外部新增的目录");
});

test("重读失败不清空已看到的内容（失败不能表现成「这个目录是空的」）", async () => {
  let failing = false;
  const store = createDirTreeStore({
    loadDirs: async () => {
      if (failing) throw new Error("盘掉了");
      return [{ name: "a", path: "/root/a", hasChildren: false }];
    },
  });

  await store.expand("/root");
  assert.equal(store.childrenOf("/root")?.length, 1);

  failing = true;
  await store.expand("/root");
  assert.equal(store.childrenOf("/root")?.length, 1, "重读失败不该清空已经看到的内容");
  assert.ok(store.errorOf("/root"), "失败要有说明");
});

test("refreshAll：重读展开着的目录，能拿到程序外面新增的内容", async () => {
  let extra: DirEntry | null = null;
  const store = createDirTreeStore({
    loadDirs: async () => [
      { name: "原有", path: "/a/原有", hasChildren: false },
      ...(extra === null ? [] : [extra]),
    ],
  });

  await store.expand("/a");
  assert.equal(store.childrenOf("/a")?.length, 1);

  extra = { name: "外部新建", path: "/a/外部新建", hasChildren: false };
  await store.refreshAll();
  assert.equal(store.childrenOf("/a")?.length, 2, "重读之后应当看到新目录");
});
