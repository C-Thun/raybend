/**
 * 曲线求值的测试（`lib/curve.ts`）。
 *
 * 最重要的一条是**跨语言一致性**：前端画曲线那份实现必须与 Rust 侧
 * （`crates/raybend/src/develop/curve.rs`）算出同一组值 ——
 * 基准是 `src/lib/curve-vectors.json`（外部给定，Rust 侧有同一条断言）。
 */

import assert from "node:assert/strict";
import test from "node:test";

import vectors from "./curve-vectors.json" with { type: "json" };
import {
  IDENTITY_CURVE,
  MIN_X_GAP,
  addPoint,
  canPlaceAt,
  curveFunction,
  curvePath,
  isIdentityCurve,
  movePoint,
  nearestPoint,
  removePoint,
  sampleCurve,
  type CurvePoint,
} from "./curve.ts";

test("恒等曲线就是对对角线", () => {
  assert.equal(isIdentityCurve(IDENTITY_CURVE), true);
  const evaluate = curveFunction(IDENTITY_CURVE);
  for (let step = 0; step <= 10; step += 1) {
    const x = step / 10;
    assert.ok(Math.abs(evaluate(x) - x) < 1e-6, `x=${x}`);
  }
  assert.equal(isIdentityCurve([[0, 0.1], [1, 1]]), false);
  assert.equal(isIdentityCurve([[0, 0], [0.5, 0.5], [1, 1]]), false);
});

test("曲线与 Rust 侧算出同一组值（外部给定向量）", () => {
  assert.ok(vectors.cases.length > 0, "向量文件不能是空的");
  for (const item of vectors.cases) {
    // JSON 里是 `number[][]`（TS 推不出「长度为 2」），显式收窄一次
    const points: CurvePoint[] = item.points.map(
      ([x, y]) => [x, y] as CurvePoint,
    );
    const samples = sampleCurve(points, item.steps);
    assert.equal(samples.length, item.steps + 1);
    item.samples.forEach((expected, index) => {
      const got = samples[index];
      assert.ok(
        Math.abs(got - expected) < 5e-4,
        `控制点 ${JSON.stringify(points)} 在 x=${index / item.steps}：向量 ${expected} / 本实现 ${got}`,
      );
    });
  }
});

test("单调不过冲：平台 + 陡坡也不出现反转", () => {
  const points: CurvePoint[] = [
    [0, 0],
    [0.25, 0.05],
    [0.5, 0.5],
    [0.75, 0.95],
    [1, 1],
  ];
  const evaluate = curveFunction(points);
  let previous = -1;
  for (let step = 0; step <= 500; step += 1) {
    const x = step / 500;
    const y = evaluate(x);
    assert.ok(y >= 0 && y <= 1, `越界：${x} → ${y}`);
    assert.ok(y >= previous - 1e-6, `不单调：${x} → ${y}（前一个 ${previous}）`);
    previous = y;
  }
});

test("端点是黑场 / 白场：曲线之外取端点值", () => {
  const points: CurvePoint[] = [
    [0.1, 0.0],
    [0.5, 0.55],
    [0.9, 1.0],
  ];
  const evaluate = curveFunction(points);
  assert.equal(evaluate(0), 0, "黑场以下全丢");
  assert.equal(evaluate(0.05), 0);
  assert.equal(evaluate(1), 1, "白场以上取端点值");
  assert.equal(evaluate(0.95), 1);
  assert.ok(evaluate(0.5) > 0.5 && evaluate(0.5) < 0.6);
});

test("拖点：不许越过邻居，越界就停在边界上", () => {
  const points: CurvePoint[] = [
    [0, 0],
    [0.5, 0.5],
    [1, 1],
  ];
  assert.equal(canPlaceAt(points, 1, 0.3), true);
  assert.equal(canPlaceAt(points, 1, 0.001), false, "会撞上左邻居");
  assert.equal(canPlaceAt(points, 1, 0.999), false, "会撞上右邻居");

  const moved = movePoint(points, 1, 0.001, 0.8);
  assert.ok(moved[1][0] >= MIN_X_GAP - 1e-9, `停在边界上，实际 ${moved[1][0]}`);
  assert.equal(moved[1][1], 0.8);
  assert.deepEqual(points[1], [0.5, 0.5], "原数组不许被改（响应式要它不变）");
});

test("加点 / 删点：太近不加，首尾不许删", () => {
  const base: CurvePoint[] = [
    [0, 0],
    [1, 1],
  ];
  const added = addPoint(base, 0.5, 0.7);
  assert.equal(added.length, 3);
  assert.deepEqual(added[1], [0.5, 0.7]);
  assert.equal(addPoint(base, 0.0001, 0.2).length, 2, "与已有点太近：不加");

  assert.equal(removePoint(added, 1).length, 2);
  assert.equal(removePoint(added, 0).length, 3, "首点（黑场）不能删");
  assert.equal(removePoint(added, 2).length, 3, "末点（白场）不能删");
});

test("命中测试：找最近的点，超出容差返回 -1", () => {
  const points: CurvePoint[] = [
    [0, 0],
    [0.5, 0.5],
    [1, 1],
  ];
  assert.equal(nearestPoint(points, 0.51, 0.49, 0.05), 1);
  assert.equal(nearestPoint(points, 0.02, 0.02, 0.05), 0);
  assert.equal(nearestPoint(points, 0.3, 0.8, 0.05), -1, "离谁都远：没有命中");
});

test("SVG 路径：起于左端、止于右端、y 轴翻转", () => {
  const path = curvePath(IDENTITY_CURVE, 100, 100, 4);
  // 路径是 "M<x> <y>L<x> <y>…"：按空格切开是「命令 + 坐标」交替
  const tokens = path.split(" ");
  assert.equal(tokens[0], "M0.00", "起点命令");
  assert.equal(tokens[1], "100.00", "起点在左下角（y 轴翻转）");
  assert.equal(tokens[tokens.length - 2], "L100.00", "终点命令");
  assert.equal(tokens[tokens.length - 1], "0.00", "终点在右上角");
  assert.equal(tokens.length, 10, "4 段 = 1 个 M + 4 个 L，每个带一对坐标");
});

test("非法输入不炸：NaN / 越界坐标被夹回来", () => {
  const evaluate = curveFunction([
    [0, 0],
    [1, 1],
  ]);
  assert.equal(evaluate(Number.NaN), 0);
  assert.equal(evaluate(-5), 0);
  assert.equal(evaluate(5), 1);
  // 坐标越界的控制点会被夹进 [0,1]
  const clamped = curveFunction([
    [-1, -1],
    [2, 2],
  ]);
  assert.ok(clamped(0) >= 0 && clamped(1) <= 1);
  // 点数不足：退化成恒等，不抛
  assert.equal(curveFunction([[0.5, 0.5]])(0.5), 0.5);
});
