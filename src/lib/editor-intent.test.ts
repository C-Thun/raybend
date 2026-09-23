/**
 * 编辑器视口意图构造的冒烟（`lib/editor-intent.ts`）。
 *
 * 盯三件事：**合并成每帧一次**、**尾样本不丢**、**点击 / 拖动的判据**。
 * 前两条是 `AGENTS.md` §7.9「诊断红旗」里点名的坑（节流丢尾样本 → Rust 手里的 pan 停在半路）。
 */

import assert from "node:assert/strict";
import test from "node:test";

import {
  CLICK_SLOP_PX,
  createDragSession,
  createPanAccumulator,
  isClickGesture,
  type PanIntent,
} from "./editor-intent.ts";
import type { FrameScheduler, FrameTicket } from "./editor-viewport.ts";

/** 确定的假帧调度：`run()` 手动推进一帧。 */
function fakeScheduler() {
  let nextId = 1;
  const queued = new Map<number, () => void>();
  let cancelled = 0;
  const scheduler: FrameScheduler = {
    request: (run) => {
      const id = nextId;
      nextId += 1;
      queued.set(id, run);
      return { id } satisfies FrameTicket;
    },
    cancel: (ticket) => {
      if (queued.delete(ticket.id)) cancelled += 1;
    },
  };
  return {
    scheduler,
    /** 推进一帧（把挂起的回调跑掉） */
    run: () => {
      const pending = [...queued.entries()];
      queued.clear();
      for (const [, run] of pending) run();
    },
    pending: () => queued.size,
    cancelled: () => cancelled,
  };
}

function collect() {
  const out: PanIntent[] = [];
  return { out, send: (intent: PanIntent) => out.push(intent) };
}

test("一次拖动里的多段位移合并成「一帧一条」，位移相加", () => {
  const frames = fakeScheduler();
  const sink = collect();
  const accumulator = createPanAccumulator({ send: sink.send, scheduler: frames.scheduler });

  accumulator.move(3, 0);
  accumulator.move(4, -2);
  accumulator.move(1, 1);
  assert.equal(sink.out.length, 0, "同一帧内一次都不该发");
  assert.equal(frames.pending(), 1, "只挂一次调度（不是三次）");

  frames.run();
  assert.deepEqual(sink.out, [{ kind: "pan", dx: 8, dy: -1 }]);
  assert.equal(accumulator.sentCount(), 1);

  // 第二帧再来一段：又是一条，不是把上一条改掉
  accumulator.move(2, 2);
  frames.run();
  assert.deepEqual(sink.out, [
    { kind: "pan", dx: 8, dy: -1 },
    { kind: "pan", dx: 2, dy: 2 },
  ]);
});

test("尾样本：松手时挂起的那一帧一定发得出去", () => {
  const frames = fakeScheduler();
  const sink = collect();
  const accumulator = createPanAccumulator({ send: sink.send, scheduler: frames.scheduler });

  accumulator.move(5, 5);
  accumulator.flush(); // 还没到帧点就松手
  assert.deepEqual(sink.out, [{ kind: "pan", dx: 5, dy: 5 }]);
  assert.equal(frames.cancelled(), 1, "挂起的那张票要取消掉（不然会再发一次空的）");

  frames.run();
  assert.equal(sink.out.length, 1, "取消之后不该有第二条");
});

test("零位移不发；非法值直接丢掉（不给 Rust 送要拒绝的东西）", () => {
  const frames = fakeScheduler();
  const sink = collect();
  const accumulator = createPanAccumulator({ send: sink.send, scheduler: frames.scheduler });

  accumulator.move(0, 0);
  accumulator.move(Number.NaN, 3);
  accumulator.move(3, Number.POSITIVE_INFINITY);
  accumulator.flush();
  assert.deepEqual(sink.out, []);

  // 合法位移里的那部分照发
  accumulator.move(1, 2);
  accumulator.flush();
  assert.deepEqual(sink.out, [{ kind: "pan", dx: 1, dy: 2 }]);
});

test("dispose 之后不再有挂起的调度，也不发", () => {
  const frames = fakeScheduler();
  const sink = collect();
  const accumulator = createPanAccumulator({ send: sink.send, scheduler: frames.scheduler });

  accumulator.move(9, 9);
  accumulator.dispose();
  frames.run();
  assert.deepEqual(sink.out, []);
  assert.equal(accumulator.sentCount(), 0);
});

test("点击判据用总位移（慢慢拖出去再拖回来算点击，中途超阈值不算）", () => {
  assert.equal(isClickGesture(0, 0), true, "没动就是点击");
  assert.equal(isClickGesture(CLICK_SLOP_PX, 0), true, "正好到阈值仍算点击");
  assert.equal(isClickGesture(CLICK_SLOP_PX + 1, 0), false);
  assert.equal(isClickGesture(3, 4), false, "斜着走：3-4-5 的斜边超过阈值");
  assert.equal(isClickGesture(Number.NaN, 0), false, "非法值不认作点击");
});

test("拖动会话：总位移与逐次位移分开记", () => {
  const session = createDragSession();
  assert.equal(session.active(), false);
  assert.equal(session.move(10, 10), null, "没按下时不产生位移");

  session.start(100, 100);
  assert.equal(session.active(), true);
  assert.deepEqual(session.move(100, 100), { dx: 0, dy: 0 }, "第一次采样没有位移");
  assert.deepEqual(session.move(104, 98), { dx: 4, dy: -2 });
  assert.deepEqual(session.move(105, 98), { dx: 1, dy: 0 });

  const ended = session.end();
  assert.equal(ended.click, false, "总共走了 5+ 像素，是拖动");
  assert.deepEqual([ended.totalDx, ended.totalDy], [5, -2]);
  assert.equal(session.active(), false);

  // 原地按一下再松手 = 点击
  session.start(50, 50);
  session.move(50, 51);
  assert.equal(session.end().click, true);

  // 拖着走再拖回来：总位移小，仍算点击（这正是「用总位移判」的意义）
  session.start(0, 0);
  session.move(40, 0);
  session.move(0, 0);
  assert.equal(session.end().click, true);
});
