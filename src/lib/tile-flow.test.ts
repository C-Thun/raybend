/**
 * Tile 流换行与尺寸档位的单元测试（DESIGN.md §12.6）。
 *
 * 运行：`pnpm test`（node --test，零依赖）
 *
 * 覆盖重点按 AGENTS.md §2.10：
 *   空输入 / 单元素 / 上下限 / 非法值 / 溢出 / 不变式
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  computeTileFlow,
  DEFAULT_TILE_STEP_INDEX,
  TILE_IMAGE_ASPECT,
  TILE_SIZE_STEPS,
  clampTileStepIndex,
  tileImageHeight,
  tileRowCount,
  tileSizeAt,
  tileTotalHeight,
  nextIndexForArrow,
} from "./tile-flow.ts";

// ─── computeTileFlow：正常路径 ────────────────────────────

test("设计稿实例：紧凑档中列（960 宽 / 150 单元 / 8 间距）→ 6 列 余 20", () => {
  const flow = computeTileFlow({ containerWidth: 960, cellWidth: 150, gap: 8 });
  assert.equal(flow.columns, 6);
  assert.equal(flow.remainder, 20);
  assert.equal(flow.overflows, false);
});

test("设计稿实例：宽松档中列（868 宽）→ 5 列 余 86", () => {
  // 这条正是「面板加宽后中列变窄、照片行必须减一张」的由来
  const flow = computeTileFlow({ containerWidth: 868, cellWidth: 150, gap: 8 });
  assert.equal(flow.columns, 5);
  assert.equal(flow.remainder, 86);
  assert.equal(flow.overflows, false);
});

test("正好整除时余量为 0", () => {
  // 4 列 × 100 + 3 × 0 = 400
  const flow = computeTileFlow({ containerWidth: 400, cellWidth: 100, gap: 0 });
  assert.equal(flow.columns, 4);
  assert.equal(flow.remainder, 0);
});

test("间距为 0 也能算", () => {
  const flow = computeTileFlow({
    containerWidth: 1000,
    cellWidth: 100,
    gap: 0,
  });
  assert.equal(flow.columns, 10);
  assert.equal(flow.remainder, 0);
});

// ─── computeTileFlow：边界与退化 ──────────────────────────

test("容器恰好等于一个单元格 → 1 列，余 0", () => {
  const flow = computeTileFlow({ containerWidth: 200, cellWidth: 200, gap: 8 });
  assert.equal(flow.columns, 1);
  assert.equal(flow.remainder, 0);
});

test("容器比一个单元格略窄 → 仍给 1 列并标记溢出（不返回 0 列）", () => {
  const flow = computeTileFlow({ containerWidth: 180, cellWidth: 200, gap: 8 });
  assert.equal(flow.columns, 1);
  assert.equal(flow.remainder, -20);
  assert.equal(flow.overflows, true);
});

test("容器宽为 0 或负数 → 1 列且标记溢出", () => {
  for (const w of [0, -50]) {
    const flow = computeTileFlow({ containerWidth: w, cellWidth: 150, gap: 8 });
    assert.equal(flow.columns, 1);
    assert.equal(flow.overflows, true);
  }
});

test("cellWidth 非法（0 / 负 / NaN）→ 1 列、不除零、不产生 NaN", () => {
  for (const cw of [0, -100, Number.NaN]) {
    const flow = computeTileFlow({
      containerWidth: 900,
      cellWidth: cw,
      gap: 8,
    });
    assert.equal(flow.columns, 1);
    assert.ok(
      Number.isFinite(flow.remainder),
      `remainder 不应为 NaN：${flow.remainder}`,
    );
  }
});

test("负间距按 0 处理（不让公式产出诡异列数）", () => {
  const a = computeTileFlow({ containerWidth: 900, cellWidth: 150, gap: -8 });
  const b = computeTileFlow({ containerWidth: 900, cellWidth: 150, gap: 0 });
  assert.deepEqual(a, b);
});

test("NaN / Infinity 输入不传播成 NaN 结果", () => {
  for (const bad of [Number.NaN, Number.POSITIVE_INFINITY]) {
    const flow = computeTileFlow({
      containerWidth: bad,
      cellWidth: 150,
      gap: 8,
    });
    assert.ok(Number.isFinite(flow.columns));
    assert.ok(Number.isFinite(flow.remainder));
  }
});

// ─── computeTileFlow：不变式 ──────────────────────────────

test("不变式：列数恒 ≥ 1，且余量 < 一个「单元格+间距」", () => {
  for (const w of [1, 50, 100, 199, 200, 201, 433, 960, 1920, 3840]) {
    for (const cw of [96, 150, 208, 512]) {
      for (const gap of [0, 4, 8, 16]) {
        const { columns, remainder } = computeTileFlow({
          containerWidth: w,
          cellWidth: cw,
          gap,
        });
        assert.ok(
          columns >= 1,
          `w=${w} cw=${cw} gap=${gap} → columns=${columns}`,
        );
        // 若能再塞一列，说明列数算少了
        if (!(w < cw)) {
          assert.ok(
            remainder < cw + gap,
            `w=${w} cw=${cw} gap=${gap} → remainder=${remainder} 本可再放一列`,
          );
        }
      }
    }
  }
});

test("不变式：used + remainder 恒等于容器宽（无四舍五入丢像素）", () => {
  for (const w of [320, 640, 868, 960, 1000, 1920]) {
    for (const cw of [96, 150, 208, 512]) {
      for (const gap of [4, 8]) {
        const { columns, remainder } = computeTileFlow({
          containerWidth: w,
          cellWidth: cw,
          gap,
        });
        const used = columns * cw + (columns - 1) * gap;
        assert.equal(used + remainder, w, `w=${w} cw=${cw} gap=${gap}`);
      }
    }
  }
});

// ─── 尺寸档位 ─────────────────────────────────────────────

test("档位是 9 档、奇数个、存在中间档、最大 512", () => {
  assert.equal(TILE_SIZE_STEPS.length, 9);
  assert.equal(TILE_SIZE_STEPS.length % 2, 1, "奇数档才有可选的中间档");
  assert.equal(TILE_SIZE_STEPS[TILE_SIZE_STEPS.length - 1], 512);
});

test("默认档位就是正中间那档", () => {
  assert.equal(DEFAULT_TILE_STEP_INDEX, 4);
  assert.equal(tileSizeAt(DEFAULT_TILE_STEP_INDEX), 208);
});

test("档位严格单调递增（滑块拖起来不能有平台或回落）", () => {
  for (let i = 1; i < TILE_SIZE_STEPS.length; i++) {
    assert.ok(
      TILE_SIZE_STEPS[i] > TILE_SIZE_STEPS[i - 1],
      `第 ${i} 档 ${TILE_SIZE_STEPS[i]} 未大于前一档 ${TILE_SIZE_STEPS[i - 1]}`,
    );
  }
});

test("越界下标被夹到合法范围", () => {
  assert.equal(clampTileStepIndex(-1), 0);
  assert.equal(clampTileStepIndex(-999), 0);
  assert.equal(clampTileStepIndex(9), 8);
  assert.equal(clampTileStepIndex(999), 8);
  assert.equal(clampTileStepIndex(2.6), 3, "小数四舍五入");
  assert.equal(
    clampTileStepIndex(Number.NaN),
    DEFAULT_TILE_STEP_INDEX,
    "NaN 回落到默认档",
  );
});

test("tileSizeAt 对越界与非法下标都返回合法档位", () => {
  assert.equal(tileSizeAt(-5), TILE_SIZE_STEPS[0]);
  assert.equal(tileSizeAt(100), TILE_SIZE_STEPS[8]);
  assert.equal(tileSizeAt(Number.NaN), tileSizeAt(DEFAULT_TILE_STEP_INDEX));
});

// ─── 高度计算 ─────────────────────────────────────────────

test("图片区按 3:2 取高并对齐到整像素", () => {
  assert.equal(TILE_IMAGE_ASPECT, 1.5);
  assert.equal(tileImageHeight(150), 100);
  assert.equal(tileImageHeight(208), 139); // 138.67 → 139
  assert.equal(tileImageHeight(96), 64);
  assert.ok(Number.isInteger(tileImageHeight(120)));
});

test("图片区高度对非法输入返回 0（不发散）", () => {
  for (const bad of [0, -150, Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.equal(tileImageHeight(bad), 0);
  }
});

test("整块高度 = 图片区 + 字幕条", () => {
  assert.equal(tileTotalHeight(150, 22), 122);
  assert.equal(tileTotalHeight(150, 28), 128);
  // 字幕高非法时按 0 处理，不产生 NaN
  assert.equal(tileTotalHeight(150, Number.NaN), 100);
  assert.equal(tileTotalHeight(150, -10), 100);
});

// ─── 行数 ─────────────────────────────────────────────────

test("行数向上取整，且最后一行可以不满", () => {
  assert.equal(tileRowCount(12, 6), 2);
  assert.equal(tileRowCount(13, 6), 3, "13 张 6 列 → 2 满行 + 1 张的末行");
  assert.equal(tileRowCount(6, 6), 1);
  assert.equal(tileRowCount(1, 6), 1);
});

test("行数：空集与非正列数不产生 Infinity / NaN", () => {
  assert.equal(tileRowCount(0, 6), 0);
  assert.equal(tileRowCount(-3, 6), 0);
  assert.equal(tileRowCount(Number.NaN, 6), 0);
  assert.equal(tileRowCount(10, 0), 10, "列数非法时退化为每行一张");
  assert.ok(Number.isFinite(tileRowCount(10, Number.NaN)));
});

test("不变式：行数 × 列数 ≥ 总数，且 (行数−1) × 列数 < 总数", () => {
  for (const count of [1, 5, 6, 7, 12, 13, 59, 60, 61, 1248, 100000]) {
    for (const cols of [1, 2, 5, 6, 8]) {
      const rows = tileRowCount(count, cols);
      assert.ok(
        rows * cols >= count,
        `count=${count} cols=${cols} rows=${rows}`,
      );
      if (rows > 0) {
        assert.ok(
          (rows - 1) * cols < count,
          `count=${count} cols=${cols} rows=${rows} 多算了一行`,
        );
      }
    }
  }
});

test("方向键：左右一格、上下整行；到头停在原地", () => {
  const base = { count: 10, columns: 4 } as const;
  assert.equal(nextIndexForArrow({ ...base, from: 5, key: "ArrowRight" }), 6);
  assert.equal(nextIndexForArrow({ ...base, from: 5, key: "ArrowLeft" }), 4);
  assert.equal(nextIndexForArrow({ ...base, from: 5, key: "ArrowDown" }), 9);
  assert.equal(nextIndexForArrow({ ...base, from: 5, key: "ArrowUp" }), 1);
  // 越界不动（不绕到另一端：绕过去像是选中莫名跳走）
  assert.equal(nextIndexForArrow({ ...base, from: 0, key: "ArrowLeft" }), null);
  assert.equal(nextIndexForArrow({ ...base, from: 0, key: "ArrowUp" }), null);
  assert.equal(nextIndexForArrow({ ...base, from: 9, key: "ArrowRight" }), null);
  assert.equal(nextIndexForArrow({ ...base, from: 9, key: "ArrowDown" }), null);
});

test("方向键：空列表 / 无效起点 / 列数为 0 都不炸", () => {
  assert.equal(nextIndexForArrow({ from: 0, count: 0, columns: 4, key: "ArrowRight" }), null);
  assert.equal(nextIndexForArrow({ from: -1, count: 5, columns: 4, key: "ArrowRight" }), null);
  assert.equal(nextIndexForArrow({ from: 9, count: 5, columns: 4, key: "ArrowRight" }), null);
  // 列数 0（还没算出来）时上下按 1 步走，至少不会跳飞
  assert.equal(nextIndexForArrow({ from: 2, count: 5, columns: 0, key: "ArrowDown" }), 3);
});
