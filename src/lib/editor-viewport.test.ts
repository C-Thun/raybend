/**
 * 洞口上报的纯逻辑测试（`AGENTS.md` §7.9 的坐标契约）。
 *
 * 盯三件事：**非法值不报**、**相同值不重发**、**节流保留尾样本**。
 */
import assert from "node:assert/strict";
import test from "node:test";

import {
  createViewportReporter,
  editorViewportPayload,
  sameViewportPayload,
  type EditorViewportPayload,
  type FrameScheduler,
  type FrameTicket,
} from "./editor-viewport.ts";

/** 确定的假调度：手动 `run()` 推进（不依赖 rAF / 定时器）。 */
function fakeScheduler(): FrameScheduler & { run: () => void; pending: number } {
  const queue = new Map<number, () => void>();
  let nextId = 1;
  const state = {
    pending: 0,
    request: (run: () => void): FrameTicket => {
      const id = nextId;
      nextId += 1;
      queue.set(id, run);
      state.pending = queue.size;
      return { id };
    },
    cancel: (ticket: FrameTicket): void => {
      queue.delete(ticket.id);
      state.pending = queue.size;
    },
    run: (): void => {
      const runs = [...queue.values()];
      queue.clear();
      state.pending = 0;
      for (const run of runs) run();
    },
  };
  return state;
}

function observeInput(width: number, dpr = 1) {
  return {
    rect: { x: 10, y: 20, width, height: 400 },
    dpr,
    viewport: { width: 1600, height: 900 },
  };
}

test("载荷：非法值一律拒绝（不静默回退）", () => {
  const base = observeInput(800);
  assert.ok(editorViewportPayload(base) !== null);
  assert.equal(editorViewportPayload({ ...base, dpr: 0 }), null, "dpr 是换算分母，不能是 0");
  assert.equal(editorViewportPayload({ ...base, dpr: Number.NaN }), null);
  // 1.375（系统 125% × 文字 110%）是**合法**输入，必须原样收下
  assert.equal(editorViewportPayload({ ...base, dpr: 1.375 })?.dpr, 1.375);
  assert.equal(
    editorViewportPayload({ ...base, rect: { ...base.rect, width: -1 } }),
    null,
    "负宽度非法",
  );
  assert.equal(
    editorViewportPayload({ ...base, rect: { ...base.rect, x: Number.POSITIVE_INFINITY } }),
    null,
  );
  assert.equal(
    editorViewportPayload({ ...base, viewport: { width: 0, height: Number.NaN } }),
    null,
  );
});

test("1.375（系统 125% × 文字 110%）原样上行，前端不做任何换算", () => {
  const payload = editorViewportPayload(observeInput(800, 1.375));
  assert.equal(payload?.dpr, 1.375);
  assert.deepEqual(payload?.hole, { x: 10, y: 20, width: 800, height: 400 });
});

test("相同载荷判定逐字段（差一个字段就不算相同）", () => {
  const a = editorViewportPayload(observeInput(800)) as EditorViewportPayload;
  const b = editorViewportPayload(observeInput(800)) as EditorViewportPayload;
  const c = editorViewportPayload(observeInput(801)) as EditorViewportPayload;
  const d = editorViewportPayload(observeInput(800, 1.5)) as EditorViewportPayload;
  assert.equal(sameViewportPayload(a, b), true);
  assert.equal(sameViewportPayload(a, c), false);
  assert.equal(sameViewportPayload(a, d), false);
});

test("合并成每帧一次：同一帧内多次观察只发最后一次", () => {
  const scheduler = fakeScheduler();
  const sent: EditorViewportPayload[] = [];
  const reporter = createViewportReporter({
    send: (payload) => sent.push(payload),
    scheduler,
  });

  reporter.observe(observeInput(800));
  reporter.observe(observeInput(900));
  reporter.observe(observeInput(1000));
  assert.equal(sent.length, 0, "还没到帧边界，一次都不该发");
  scheduler.run();
  assert.equal(sent.length, 1);
  assert.equal(sent[0]?.hole.width, 1000, "发的是**最新值**");
  assert.equal(reporter.sentCount(), 1);
});

test("节流保留尾样本：拖完之后那一次一定发得出去", () => {
  const scheduler = fakeScheduler();
  const sent: EditorViewportPayload[] = [];
  const reporter = createViewportReporter({ send: (p) => sent.push(p), scheduler });

  reporter.observe(observeInput(800));
  scheduler.run();
  reporter.observe(observeInput(900)); // 拖拽中：还在帧里
  reporter.flush(); // 用户松手 → 卸载 / 收尾
  assert.equal(sent.length, 2);
  assert.equal(sent[1]?.hole.width, 900, "尾样本不能丢");
  assert.equal(scheduler.pending, 0, "flush 之后不该还挂着调度");
});

test("相同值不重发（ResizeObserver 会为空挂载也回调一次）", () => {
  const scheduler = fakeScheduler();
  const sent: EditorViewportPayload[] = [];
  const reporter = createViewportReporter({ send: (p) => sent.push(p), scheduler });
  reporter.observe(observeInput(800));
  scheduler.run();
  reporter.observe(observeInput(800));
  scheduler.run();
  assert.equal(sent.length, 1);
});

test("布局中间态（0 宽）是合法值，但非法值不会把好值擦掉", () => {
  const scheduler = fakeScheduler();
  const sent: EditorViewportPayload[] = [];
  const reporter = createViewportReporter({ send: (p) => sent.push(p), scheduler });
  reporter.observe(observeInput(800));
  scheduler.run();
  // 一次非法上报（比如中间帧算出 NaN）——不该覆盖上一次的好值
  reporter.observe({ ...observeInput(800), dpr: Number.NaN });
  scheduler.run();
  assert.equal(sent.length, 1, "非法值不产生上报");
  reporter.observe(observeInput(0));
  scheduler.run();
  assert.equal(sent.length, 2, "0 宽是合法输入（最小化 / 中间态），照报");
  assert.equal(sent[1]?.hole.width, 0);
});

test("dispose 之后不再有挂起的调度", () => {
  const scheduler = fakeScheduler();
  const sent: EditorViewportPayload[] = [];
  const reporter = createViewportReporter({ send: (p) => sent.push(p), scheduler });
  reporter.observe(observeInput(800));
  reporter.dispose();
  assert.equal(scheduler.pending, 0);
  scheduler.run();
  assert.equal(sent.length, 0);
});


test("覆盖层主题颜色原样上报，颜色变化也触发视口更新", () => {
  const colors: [string,string,string,string] = ["var(--overlay-line)", "var(--overlay-halo)", "var(--brand)", "var(--brand-2)"];
  const input = { rect: {x:0,y:0,width:600,height:400}, dpr:1.375,
    viewport: {width:800,height:600}, overlayColors: colors };
  const a = editorViewportPayload(input)!;
  assert.deepEqual(a.overlayColors, colors);
  assert.equal(sameViewportPayload(a, editorViewportPayload(input)!), true);
  colors[0] = "var(--fg-1)";
  assert.equal(sameViewportPayload(a, editorViewportPayload(input)!), false);
  assert.equal(a.overlayColors?.[0], "var(--overlay-line)", "已发送快照不跟着可变输入改变");
});
