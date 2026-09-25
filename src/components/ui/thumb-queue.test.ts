/**
 * 缩略图队列的测试。
 *
 * 这三件事在界面上分别表现为「滚动时卡顿」「重复读盘」「内存一直涨」——
 * 都属于「不像 bug 的 bug」，所以用假 loader 把它们钉死。
 * URL 的创建与回收也是注入的，测试里数得清每一个 `blob:` 有没有被回收。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import { createThumbQueue } from "./thumb-queue.ts";

/** 可挂起、可失败的假加载器；同时统计并发峰值 */
function fakeLoader(options: { hold?: boolean; fail?: boolean } = {}) {
  const calls: string[] = [];
  const pendings: Array<() => void> = [];
  const state = {
    hold: options.hold ?? false,
    fail: options.fail ?? false,
    inflight: 0,
    peak: 0,
  };

  const load = async (path: string): Promise<Uint8Array | null> => {
    calls.push(path);
    state.inflight += 1;
    state.peak = Math.max(state.peak, state.inflight);
    try {
      if (state.hold) {
        await new Promise<void>((resolve) => pendings.push(resolve));
      }
      if (state.fail) throw new Error("读不了");
      return new Uint8Array([1, 2, 3]);
    } finally {
      state.inflight -= 1;
    }
  };

  return {
    load,
    calls,
    state,
    releaseAll() {
      const waiters = pendings.splice(0, pendings.length);
      for (const resolve of waiters) resolve();
    },
  };
}

/** 记录 URL 的创建与回收 */
function fakeUrls() {
  const created: string[] = [];
  const revoked: string[] = [];
  return {
    created,
    revoked,
    toUrl: (bytes: Uint8Array) => {
      const url = `blob:fake/${created.length}-${bytes.length}`;
      created.push(url);
      return url;
    },
    revokeUrl: (url: string) => {
      revoked.push(url);
    },
  };
}

async function flush(times = 4): Promise<void> {
  for (let i = 0; i < times; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

test("请求一张：加载 → ready，带 URL", async () => {
  const loader = fakeLoader();
  const urls = fakeUrls();
  const queue = createThumbQueue({ load: loader.load, ...urls });

  assert.deepEqual(queue.get("/a.jpg"), { status: "idle", url: null });
  queue.request("/a.jpg");
  await flush();

  const entry = queue.get("/a.jpg");
  assert.equal(entry.status, "ready");
  assert.ok(entry.url);
  assert.equal(loader.calls.length, 1);
  assert.deepEqual(queue.stats(), { entries: 1, inflight: 0, queued: 0 });
});

test("同一个文件请求两次只读一次（滚出去又滚回来）", async () => {
  const loader = fakeLoader();
  const urls = fakeUrls();
  const queue = createThumbQueue({ load: loader.load, ...urls });

  queue.request("/a.jpg");
  queue.request("/a.jpg");
  await flush();
  queue.request("/a.jpg"); // 已完成后再次请求也不重读
  await flush();

  assert.equal(loader.calls.length, 1);
  assert.equal(urls.created.length, 1);
});

test("限流：同时在飞的请求不超过上限", async () => {
  const loader = fakeLoader({ hold: true });
  const urls = fakeUrls();
  const queue = createThumbQueue({ load: loader.load, concurrency: 2, ...urls });

  for (let i = 0; i < 6; i += 1) queue.request(`/p${i}.jpg`);
  assert.equal(loader.calls.length, 2, "只有 2 个真的发出去了");
  assert.equal(queue.stats().inflight, 2);
  assert.ok(queue.stats().queued >= 3);

  loader.state.hold = false;
  loader.releaseAll();
  await flush(8);
  assert.equal(loader.state.peak, 2, "峰值并发不能超过 2");
  for (let i = 0; i < 6; i += 1) {
    assert.equal(queue.get(`/p${i}.jpg`).status, "ready");
  }
  assert.equal(queue.stats().inflight, 0);
});

test("加载失败：记 error，再请求可以重试", async () => {
  const loader = fakeLoader({ fail: true });
  const urls = fakeUrls();
  const queue = createThumbQueue({ load: loader.load, ...urls });

  queue.request("/a.jpg");
  await flush();
  assert.equal(queue.get("/a.jpg").status, "error");
  assert.equal(queue.get("/a.jpg").url, null);

  queue.request("/a.jpg");
  await flush();
  assert.equal(loader.calls.length, 2, "失败过的允许重试");
});

test("拿不到字节（浏览器里就是这样）：记 error 而不是一直转圈", async () => {
  const urls = fakeUrls();
  const queue = createThumbQueue({
    load: async () => null,
    ...urls,
  });

  queue.request("/a.jpg");
  await flush();
  assert.equal(queue.get("/a.jpg").status, "error");
  assert.equal(urls.created.length, 0);
});

test("LRU：超过上限时淘汰最久未用并回收 URL", async () => {
  const loader = fakeLoader();
  const urls = fakeUrls();
  const queue = createThumbQueue({
    load: loader.load,
    maxEntries: 2,
    ...urls,
  });

  queue.request("/a.jpg");
  await flush();
  queue.request("/b.jpg");
  await flush();
  queue.request("/c.jpg");
  await flush();

  assert.equal(queue.stats().entries, 2, "表里最多 2 条");
  assert.equal(queue.get("/a.jpg").status, "idle", "最久没用的被淘汰");
  assert.equal(queue.get("/b.jpg").status, "ready");
  assert.equal(queue.get("/c.jpg").status, "ready");
  assert.equal(urls.revoked.length, 1, "被淘汰的 URL 必须回收");
  assert.equal(urls.revoked[0], urls.created[0]);
});

test("clear：回收全部 URL、清空表，并丢掉上一个目录还在飞的结果", async () => {
  const loader = fakeLoader({ hold: true });
  const urls = fakeUrls();
  const queue = createThumbQueue({ load: loader.load, ...urls });

  queue.request("/old.jpg");
  await flush(1);
  assert.equal(queue.stats().inflight, 1);

  queue.clear();
  assert.deepEqual(queue.stats(), { entries: 0, inflight: 0, queued: 0 });

  // 上一个目录的请求结果回来：不能补进新表
  loader.state.hold = false;
  loader.releaseAll();
  await flush();
  assert.equal(queue.get("/old.jpg").status, "idle");
  assert.equal(urls.created.length, 0, "迟到的结果不该创建 URL");
});

test("已完成之后 clear：URL 被回收", async () => {
  const loader = fakeLoader();
  const urls = fakeUrls();
  const queue = createThumbQueue({ load: loader.load, ...urls });

  queue.request("/a.jpg");
  queue.request("/b.jpg");
  await flush();
  assert.equal(urls.created.length, 2);

  queue.clear();
  assert.equal(urls.revoked.length, 2);
  assert.deepEqual(queue.stats().entries, 0);
});

test("refresh：只重取这一张，别的格子不受影响（编辑落库那条路）", async () => {
  const loader = fakeLoader();
  const urls = fakeUrls();
  const queue = createThumbQueue({ load: loader.load, ...urls });

  queue.request("/a.jpg");
  queue.request("/b.jpg");
  await flush();
  const aUrl = queue.get("/a.jpg").url;
  const bUrl = queue.get("/b.jpg").url;
  assert.ok(aUrl && bUrl);

  queue.refresh("/a.jpg");
  await flush();

  assert.equal(loader.calls.filter((path) => path === "/a.jpg").length, 2, "a 重取了一次");
  assert.equal(loader.calls.filter((path) => path === "/b.jpg").length, 1, "b 一次都不多");
  assert.notEqual(queue.get("/a.jpg").url, aUrl, "a 换了新 URL");
  assert.equal(queue.get("/b.jpg").url, bUrl, "b 的 URL 原样（节点身份不丢）");
  assert.deepEqual(urls.revoked, [aUrl], "只回收 a 的旧 URL");
});

test("refresh 保留旧图直到新图解码完成，再替换并回收旧 URL", async () => {
  const loader = fakeLoader();
  const urls = fakeUrls();
  let releaseDecode: (() => void) | null = null;
  let decodeCount = 0;
  const queue = createThumbQueue({
    load: loader.load,
    ...urls,
    prepareUrl: () => {
      decodeCount += 1;
      return decodeCount === 1
        ? Promise.resolve()
        : new Promise<void>((resolve) => { releaseDecode = resolve; });
    },
  });
  queue.request("/a.jpg");
  await flush();
  const oldUrl = queue.get("/a.jpg").url;
  assert.ok(oldUrl);

  queue.refresh("/a.jpg");
  assert.equal(queue.get("/a.jpg").url, oldUrl, "重取期间旧图仍可见");
  await flush();
  assert.equal(queue.get("/a.jpg").url, oldUrl, "新图尚未解码时不能闪白");
  assert.deepEqual(urls.revoked, []);
  assert.ok(releaseDecode);
  (releaseDecode as () => void)();
  await flush();
  assert.notEqual(queue.get("/a.jpg").url, oldUrl);
  assert.deepEqual(urls.revoked, [oldUrl]);
});

test("refresh 时在飞的旧结果被丢掉，不会把旧图盖在新图上", async () => {
  const loader = fakeLoader({ hold: true });
  const urls = fakeUrls();
  const queue = createThumbQueue({ load: loader.load, ...urls });

  queue.request("/a.jpg");
  await flush(1);
  queue.refresh("/a.jpg"); // 旧请求还在飞
  loader.state.hold = false;
  loader.releaseAll();
  await flush(6);

  assert.equal(loader.calls.length, 2, "旧的一次 + 新的一次");
  assert.equal(queue.get("/a.jpg").status, "ready");
  assert.equal(urls.created.length, 1, "只有新结果创建了 URL（旧结果被代号丢掉）");
  assert.equal(queue.get("/a.jpg").url, urls.created[0]);
  assert.deepEqual(queue.stats(), { entries: 1, inflight: 0, queued: 0 });
});

test("refresh 一张没请求过的：当作首次请求", async () => {
  const loader = fakeLoader();
  const urls = fakeUrls();
  const queue = createThumbQueue({ load: loader.load, ...urls });

  queue.refresh("/new.jpg");
  await flush();
  assert.equal(queue.get("/new.jpg").status, "ready");
  assert.equal(loader.calls.length, 1);
});

test("统计里的 queued / inflight 与实际进度一致", async () => {
  const loader = fakeLoader({ hold: true });
  const urls = fakeUrls();
  const queue = createThumbQueue({ load: loader.load, concurrency: 1, ...urls });

  queue.request("/a.jpg");
  queue.request("/b.jpg");
  queue.request("/c.jpg");
  assert.deepEqual(queue.stats(), { entries: 3, inflight: 1, queued: 2 });

  loader.state.hold = false;
  loader.releaseAll();
  await flush(6);
  assert.deepEqual(queue.stats(), { entries: 3, inflight: 0, queued: 0 });
});
/* 关于「`clear()` 不能把 `entries` 读进依赖」这条（2026-09-17 爆栈事故）：
 *
 * 它**在 Node 里测不出来** —— `import "solid-js"` 解析到的是 SSR 构建（没有响应式），
 * 那样的 `createEffect` 只跑一次，成不了环；就算显式引客户端构建，合成场景也不复现
 * （实测三次都是假绿：把修复撤掉，测试照样过）。
 * 真正能抓住它的是一次**端到端的界面对照**：库非空时进浏览。
 * 那个场景的自动化在 `scripts/repro-browse.mjs`（假后端 + 真前端，可重复执行），
 * `thumb-queue.ts` 的 `clear()` 上也留了说明。这条注释是给「想再加测试」的人看的：
 * 别在 Node 里写这一条，写不出真的。 */

test("当前照片排到待加载队列前面，避免切图时总览等整条胶片带", async () => {
  const loader = fakeLoader({ hold: true });
  const queue = createThumbQueue({ load: loader.load, ...fakeUrls(), concurrency: 1 });
  queue.request("/first.jpg");
  queue.request("/neighbor.jpg");
  queue.request("/current.jpg");
  queue.request("/current.jpg", true);
  assert.deepEqual(loader.calls, ["/first.jpg"]);
  loader.state.hold = false;
  loader.releaseAll();
  await flush();
  assert.deepEqual(loader.calls, ["/first.jpg", "/current.jpg", "/neighbor.jpg"]);
});
