/**
 * 「总览视野框」测试（`features/editor/overview-rect.ts`）。
 *
 * 事实源全在 Rust（`AGENTS.md` §6.1），这里钉住的是**单位换算**：
 * `zoom` 是「图像像素 → 物理像素」、洞口与 pan 的口径、以及旋转草稿不画框。
 * 数学本体在 `components/ui/viewer/store.ts::visibleRect()`（与浏览右栏共用一份，不重测）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { overviewVisibleRect } from "./overview-rect.ts";
import type { EditorRenderState } from "../../api/types.ts";

function state(overrides: Partial<EditorRenderState>): EditorRenderState {
  return {
    dpr: 1,
    holeCss: { x: 0, y: 0, width: 800, height: 600 },
    image: { width: 4000, height: 3000 },
    zoom: 1,
    panX: 0,
    panY: 0,
    rotation: 0,
    ...overrides,
  } as EditorRenderState;
}

test("没有渲染状态 / 尺寸没齐：不画框（不是画一个乱跳的矩形）", () => {
  assert.equal(overviewVisibleRect(null), null);
  assert.equal(overviewVisibleRect(state({ image: null })), null);
  assert.equal(overviewVisibleRect(state({ holeCss: null })), null);
  assert.equal(overviewVisibleRect(state({ zoom: 0 })), null);
  assert.equal(overviewVisibleRect(state({ zoom: Number.NaN })), null);
  assert.equal(overviewVisibleRect(state({ dpr: 0 })), null);
});

test("适配窗口（缩到比洞口还小）：框就是整张", () => {
  const rect = overviewVisibleRect(state({ zoom: 0.1 }));
  assert.deepEqual(rect, { x: 0, y: 0, width: 4000, height: 3000 });
});

test("100% 且 DPR=1：框 = 洞口那么大的一块，居中", () => {
  const rect = overviewVisibleRect(state({ zoom: 1 }));
  assert.deepEqual(rect, { x: 1600, y: 1200, width: 800, height: 600 });
});

test("DPR=2：zoom 1 是**物理**像素的 1:1，框在图像坐标里要除回来", () => {
  // 洞口 800 CSS px = 1600 物理 px；zoom=1 → 1600 图像像素宽可见
  const rect = overviewVisibleRect(state({ dpr: 2, zoom: 1 }));
  assert.deepEqual(rect, { x: 1200, y: 900, width: 1600, height: 1200 });
});

test("平移：pan 是物理像素，方向与看图一致（往右下拖看到更靠左上的块）", () => {
  const rect = overviewVisibleRect(state({ zoom: 2, panX: 400, panY: 200 }));
  // 洞口 800x600 CSS，zoom=2 → 可见 400x300 图像像素；pan 正 → 看到更靠左上
  assert.deepEqual(rect, { x: 1600, y: 1250, width: 400, height: 300 });
});

test("旋转草稿里不画框：可见区域在图像空间里是斜的，轴对齐框会撒谎", () => {
  assert.equal(overviewVisibleRect(state({ rotation: 12 })), null);
});
