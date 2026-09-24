/**
 * 编辑 store 的载荷口径：`interactive`（拖动中）与「换照片必须清掉它」。
 *
 * 为什么值得单测：`interactive` 决定 Rust 侧算哪一档 —— 拖动中只算预览档、
 * 松手才按缩放补全尺寸（人类 2026-09-24）。它一旦**卡在 `true`** 上，
 * 画面会永远偏软，而这条只有真机拖一下才看得出来（`AGENTS.md` §2.10）。
 */
import assert from "node:assert/strict";
import test from "node:test";

import { DEFAULT_EDITOR_PREFS } from "../../lib/editor-prefs.ts";
import { createEditorStore } from "./store.ts";

function makeStore(): ReturnType<typeof createEditorStore> {
  return createEditorStore({
    readPrefs: () => ({ ...DEFAULT_EDITOR_PREFS }),
    writePrefs: () => undefined,
  });
}

test("载荷默认不在拖动中；按下 / 松手跟着走", () => {
  const store = makeStore();
  assert.equal(store.developPayload().interactive, false, "默认不在拖");
  store.beginParamDrag();
  assert.equal(store.paramDragging(), true);
  assert.equal(store.developPayload().interactive, true, "按下之后是拖动中");
  store.endParamDrag();
  assert.equal(store.developPayload().interactive, false, "松手之后不再是");
});

test("换照片要把「拖动中」清掉（否则画面永远偏软）", () => {
  const store = makeStore();
  store.beginParamDrag();
  store.loadDevelop({ exposure: 0.5 }, {});
  assert.equal(store.paramDragging(), false, "换照片后不许还挂在拖动中");
  assert.equal(store.developPayload().interactive, false);
});

test("载荷只装与基线不同的项（`interactive` 不掺进 values）", () => {
  const store = makeStore();
  assert.deepEqual(store.developPayload().values, {}, "什么都没动 → 空对象");
  store.setParam("exposure", 0.5);
  const payload = store.developPayload();
  assert.deepEqual(payload.values, { exposure: 0.5 });
  assert.equal("interactive" in payload.values, false, "标志是载荷的字段，不是参数");
});
