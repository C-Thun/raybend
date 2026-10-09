/**
 * 取图探针（`probe.ts`）的行为。
 *
 * 探针是**诊断工具**，它本身错了比没有更糟 —— 会让人顺着假现场查错方向。
 * 所以这三件事必须有测试钉住：开关语义（关着时彻底不动）、事件流去重与容量、
 * 「谁还活着」的账（URL 建了/回收了要对应得上）。
 */

import assert from "node:assert/strict";
import test from "node:test";

import { createViewerUrlProbe, VIEWER_PROBE_STORAGE_KEY } from "./probe.ts";

test("探针：默认关着时 record 什么都不做（生产构建里它就该是死的）", () => {
  const probe = createViewerUrlProbe();
  assert.equal(probe.enabled(), false);
  probe.record({ kind: "new", key: "a", url: "blob:1" });
  assert.deepEqual(probe.events(), []);
  assert.equal(probe.liveFor("a"), 0);
  assert.equal(probe.liveTotal(), 0);
});

test("探针：开关语义（存储标志、toggle 返回值、注入 enabled）", () => {
  const fromStorage = createViewerUrlProbe({
    storage: { getItem: (key) => (key === VIEWER_PROBE_STORAGE_KEY ? "1" : null) },
  });
  assert.equal(fromStorage.enabled(), true, "存储里写 1 就该开");

  const probe = createViewerUrlProbe({ enabled: false });
  assert.equal(probe.toggle(), true, "toggle 返回的是切换后的状态");
  assert.equal(probe.enabled(), true);
  assert.equal(probe.toggle(), false);
  // 关掉之后再来事件也不记
  probe.record({ kind: "new", key: "a", url: "blob:1" });
  assert.deepEqual(probe.events(), []);
});

test("探针：URL 的账 —— 建了算活、回收才算死；同一个 URL 只属于一张照片", () => {
  const probe = createViewerUrlProbe({ enabled: true });
  probe.record({ kind: "new", key: "a", url: "blob:1" });
  probe.record({ kind: "new", key: "a", url: "blob:2" });
  probe.record({ kind: "new", key: "b", url: "blob:3" });
  assert.equal(probe.liveFor("a"), 2);
  assert.equal(probe.liveFor("b"), 1);
  assert.equal(probe.liveTotal(), 3);
  assert.equal(probe.keyOf("blob:3"), "b", "URL 反查得到主人（回收事件要靠它标 key）");

  probe.record({ kind: "revoke", key: "a", url: "blob:1" });
  assert.equal(probe.liveFor("a"), 1, "回收一个还剩一个");
  assert.equal(probe.liveTotal(), 2);
  probe.record({ kind: "revoke", key: "a", url: "blob:2" });
  assert.equal(probe.liveFor("a"), 0, "都回收了就该是 0 —— 「空白而 live=0」的判据");
  assert.equal(probe.keyOf("blob:2"), "", "回收后不再认这个 URL");
});

test("探针：同 key 同动作的连续重复只留第一条（渲染期会被问很多次）", () => {
  const probe = createViewerUrlProbe({ enabled: true });
  for (let i = 0; i < 10; i += 1) {
    probe.record({ kind: "missing", key: "a", url: null, detail: "a.jpg" });
  }
  assert.equal(probe.events().length, 1, "「这一格还是没 URL」连记十条只会冲掉前因");

  // 中间夹了别的动作之后，再来一条同样的 still 要记（那是新一轮现场）
  probe.record({ kind: "ensure", key: "a", url: null });
  probe.record({ kind: "missing", key: "a", url: null });
  assert.deepEqual(
    probe.events().map((event) => event.kind),
    ["missing", "ensure", "missing"],
  );
});

test("探针：容量固定 —— 只留最近 160 条，序号继续往前排", () => {
  const probe = createViewerUrlProbe({ enabled: true });
  for (let i = 0; i < 200; i += 1) {
    probe.record({ kind: "ensure", key: `k${i}`, url: null });
  }
  const events = probe.events();
  assert.equal(events.length, 160);
  assert.equal(events[0]?.seq, 41, "最老的 40 条被挤掉");
  assert.equal(events[events.length - 1]?.seq, 200);

  probe.clear();
  assert.deepEqual(probe.events(), []);
  assert.equal(probe.dump().includes("live urls"), true, "清掉之后 dump 还能用");
});

test("探针：dump 是人读的现场（开关状态 + 事件行）", () => {
  const probe = createViewerUrlProbe({ enabled: true });
  probe.record({ kind: "new", key: "a", url: "blob:1", detail: "a.jpg" });
  probe.record({ kind: "release", key: "a", url: "blob:1", detail: "single-replace · kept by cache:a" });
  const text = probe.dump();
  assert.ok(text.includes("viewer probe: ON"));
  assert.ok(text.includes("live urls 1"));
  assert.ok(text.includes("new a blob:1 · a.jpg"));
  assert.ok(text.includes("kept by cache:a"), "「放手但没回收」的现场要能读出来");
});
