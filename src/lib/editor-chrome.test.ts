/**
 * 编辑工作区「Tab 档位 × LUT 面板」状态机的测试（`lib/editor-chrome.ts`）。
 *
 * 四条规则逐个钉住（`prompts/editor.pd` 2026-09-23 口述）：
 *   1. 只有切到 ③（仅 view）才动面板，①② 档不碰；
 *   2. ③ 档把**开着**的面板关掉并记住，离开时开回来；
 *   3. ③ 档遇到**本来就关着**的面板：无动作，也不留记忆（离开时不会莫名开出来）；
 *   4. 用户显式点开关永远优先（哪怕在 ③ 档里点）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  editorChromeName,
  editorLutVisible,
  editorShowsFilm,
  editorShowsRight,
  initialEditorChrome,
  resetEditorChrome,
  setEditorLutOpen,
  stepEditorTab,
  type EditorChromeState,
} from "./editor-chrome.ts";

/** 按 n 下 Tab */
function tab(state: EditorChromeState, times = 1): EditorChromeState {
  let next = state;
  for (let i = 0; i < times; i += 1) next = stepEditorTab(next);
  return next;
}

test("档位：三档循环，按三下回到第一档", () => {
  const start = initialEditorChrome(true);
  assert.equal(editorChromeName(start), "default");
  assert.equal(editorChromeName(tab(start, 1)), "view");
  assert.equal(editorChromeName(tab(start, 2)), "view-only");
  assert.deepEqual(tab(start, 3), start, "按满三下必须回到原样");
});

test("第二档只藏胶片带，左右两列都留着", () => {
  const state = tab(initialEditorChrome(true), 1);
  assert.equal(editorShowsFilm(state), false);
  assert.equal(editorShowsRight(state), true);
  assert.equal(editorLutVisible(state), true);
});

test("面板开着时进 ③：关掉 + 记住；离开 ③：开回来", () => {
  const first = initialEditorChrome(true);
  const viewOnly = tab(first, 2);
  assert.equal(viewOnly.lutOpen, false, "③ 档里面板要关掉");
  assert.equal(viewOnly.lutHiddenForViewOnly, true, "要记住是我关的");
  assert.equal(editorLutVisible(viewOnly), false);

  const back = tab(viewOnly, 1); // ③ → ①
  assert.equal(back.step, 0);
  assert.equal(back.lutOpen, true, "离开 ③ 要恢复成开着");
  assert.equal(back.lutHiddenForViewOnly, false);
});

test("面板本来就关着：进 ③ 无动作、离开 ③ 也不会莫名开出来", () => {
  const first = initialEditorChrome(false);
  const viewOnly = tab(first, 2);
  assert.equal(viewOnly.lutOpen, false);
  assert.equal(viewOnly.lutHiddenForViewOnly, false, "本来关着就不该留「我关的」记忆");

  const back = tab(viewOnly, 1);
  assert.equal(back.lutOpen, false, "本来关着，回来还得是关着");
});

test("①② 档不碰面板：中间档来回切不改变开 / 关", () => {
  const open = initialEditorChrome(true);
  assert.equal(tab(open, 1).lutOpen, true);
  assert.equal(tab(open, 1).lutHiddenForViewOnly, false);

  const closed = initialEditorChrome(false);
  assert.equal(tab(closed, 1).lutOpen, false);
});

test("在 ③ 档里手动把面板开回来：用户操作优先，且离开时状态不漂", () => {
  const viewOnly = tab(initialEditorChrome(true), 2);
  const reopened = setEditorLutOpen(viewOnly, true);
  assert.equal(reopened.lutOpen, true);
  assert.equal(
    reopened.lutHiddenForViewOnly,
    false,
    "显式开关要清掉「我关的」记忆",
  );
  assert.equal(
    editorLutVisible(reopened),
    true,
    "③ 里显式开启要**立刻看得见**（人类 2026-09-23：「按了没反应」是 bug）",
  );
  assert.equal(
    reopened.leftForcedInViewOnly,
    true,
    "③ 里的显式开启要亮一枚临时通行证",
  );

  // 离开 ③ 之后按面板自己的状态显示（开着），通行证要收回去
  const back = tab(reopened, 1);
  assert.equal(editorLutVisible(back), true);
  assert.equal(back.leftForcedInViewOnly, false, "临时通行证只在本轮 ③ 里有效");
});

test("在 ③ 档里手动关掉再切回来：保持用户的最后一次意图", () => {
  const viewOnly = tab(initialEditorChrome(true), 2);
  const closedAgain = setEditorLutOpen(viewOnly, false);
  const back = tab(closedAgain, 1);
  assert.equal(back.lutOpen, false, "用户最后一次是关掉，回来就该是关着");
});

test("来回切两轮：状态不漂（幂等的不变式）", () => {
  const start = initialEditorChrome(true);
  const twice = tab(tab(start, 3), 3);
  assert.deepEqual(twice, start);
});

test("复位回第一档：只动档位，不动面板偏好", () => {
  const state = tab(initialEditorChrome(true), 1);
  const reset = resetEditorChrome(state);
  assert.equal(reset.step, 0);
  assert.equal(reset.lutOpen, true);
  assert.equal(reset.lutHiddenForViewOnly, false);
});

test("③ 里显式开启：通行证只在本轮有效，再进 ③ 不会留着", () => {
  const viewOnly = tab(initialEditorChrome(true), 2);
  const reopened = setEditorLutOpen(viewOnly, true);
  assert.equal(editorLutVisible(reopened), true);

  // 出 ③ 再进 ③：进的时候照旧关掉（并清掉通行证）
  const roundTrip = tab(tab(reopened, 1), 2);
  assert.equal(roundTrip.step, 2);
  assert.equal(roundTrip.lutOpen, false, "第二次进 ③ 照样先把面板关掉");
  assert.equal(roundTrip.leftForcedInViewOnly, false, "上一次的通行证不许跨轮");
  assert.equal(editorLutVisible(roundTrip), false);
});

test("③ 里再点一次关掉：通行证跟着收回去", () => {
  const reopened = setEditorLutOpen(tab(initialEditorChrome(true), 2), true);
  const closed = setEditorLutOpen(reopened, false);
  assert.equal(closed.lutOpen, false);
  assert.equal(closed.leftForcedInViewOnly, false);
  assert.equal(editorLutVisible(closed), false);
});

test("①② 档里的显式开启不需要通行证（档位本来就给左列）", () => {
  const second = tab(initialEditorChrome(true), 1);
  const reopened = setEditorLutOpen(second, true);
  assert.equal(reopened.leftForcedInViewOnly, false);
  assert.equal(editorLutVisible(reopened), true);
});

test("复位：③ 里开出来的面板随复位一起收掉通行证", () => {
  const forced = setEditorLutOpen(tab(initialEditorChrome(true), 2), true);
  const reset = resetEditorChrome(forced);
  assert.equal(reset.step, 0);
  assert.equal(reset.leftForcedInViewOnly, false);
  assert.equal(reset.lutHiddenForViewOnly, false);
  assert.equal(reset.lutOpen, true);
});
