/**
 * 键盘映射的逐条测试。
 *
 * 键盘表的价值全在「每一条都对」：`U` 到底是清旗标还是清全部标记、
 * 带 `Ctrl` 的组合该不该管、看图与网格里 `←/→` 的含义不同 ——
 * 这些都在这里钉死，视图那边只剩一个 switch。
 */

import assert from "node:assert/strict";
import test from "node:test";
import { browseKeyIntent, shouldHandleKey, type KeyContext } from "./viewer-keys.ts";

const tiles: KeyContext = { viewing: false, hasSelection: true };
const viewing: KeyContext = { viewing: true, hasSelection: true };
const nothingSelected: KeyContext = { viewing: false, hasSelection: false };

test("方向键：在看图里切图，在网格里移动「当前那张」", () => {
  assert.deepEqual(browseKeyIntent({ key: "ArrowLeft" }, viewing), { kind: "viewer-prev" });
  assert.deepEqual(browseKeyIntent({ key: "ArrowRight" }, viewing), { kind: "viewer-next" });
  assert.deepEqual(browseKeyIntent({ key: "ArrowLeft" }, tiles), { kind: "move", delta: -1 });
  assert.deepEqual(browseKeyIntent({ key: "ArrowRight" }, tiles), { kind: "move", delta: 1 });
});

test("数字键 0–5 打星；其它数字不管", () => {
  for (const value of [0, 1, 2, 3, 4, 5]) {
    assert.deepEqual(browseKeyIntent({ key: String(value) }, tiles), {
      kind: "rating",
      value,
    });
  }
  assert.equal(browseKeyIntent({ key: "6" }, tiles), null);
  assert.equal(browseKeyIntent({ key: "9" }, tiles), null);
});

test("旗标：P 留下、X 弃掉（大小写都认）", () => {
  assert.deepEqual(browseKeyIntent({ key: "p" }, tiles), { kind: "flag", value: "pick" });
  assert.deepEqual(browseKeyIntent({ key: "P" }, tiles), { kind: "flag", value: "pick" });
  assert.deepEqual(browseKeyIntent({ key: "x" }, tiles), { kind: "flag", value: "reject" });
  assert.deepEqual(browseKeyIntent({ key: "X" }, tiles), { kind: "flag", value: "reject" });
});

test("U 只清旗标（不是把星级色标一起抹掉）", () => {
  assert.deepEqual(browseKeyIntent({ key: "u" }, tiles), { kind: "flag", value: null });
  assert.deepEqual(browseKeyIntent({ key: "U" }, tiles), { kind: "flag", value: null });
});

test("回车：网格里进看图；看图里不接（那是看图件自己的事）", () => {
  assert.deepEqual(browseKeyIntent({ key: "Enter" }, tiles), { kind: "open-viewer" });
  assert.equal(browseKeyIntent({ key: "Enter" }, viewing), null);
});

test("Esc：看图里退出；网格里取消选择", () => {
  assert.deepEqual(browseKeyIntent({ key: "Escape" }, viewing), { kind: "close-viewer" });
  assert.deepEqual(browseKeyIntent({ key: "Escape" }, tiles), { kind: "clear-selection" });
});

test("删除：Delete 与 Backspace 都认（macOS 上 Delete 键发的是 Backspace）", () => {
  assert.deepEqual(browseKeyIntent({ key: "Delete" }, tiles), { kind: "delete" });
  assert.deepEqual(browseKeyIntent({ key: "Backspace" }, tiles), { kind: "delete" });
});

test("删除：没选中就没有可删的（按键不接）", () => {
  assert.equal(browseKeyIntent({ key: "Delete" }, nothingSelected), null);
  assert.equal(browseKeyIntent({ key: "Backspace" }, nothingSelected), null);
});

test("Ctrl/Cmd + A：全选（唯一允许带修饰键的一条）", () => {
  // 人类 2026-09-20：「tiles 里要支持 ctrl+a 全选，即使未显示的部分也要设置选中状态」
  const at = { viewing: false, hasSelection: false };
  assert.deepEqual(browseKeyIntent({ key: "a", ctrlKey: true }, at), { kind: "select-all" });
  assert.deepEqual(browseKeyIntent({ key: "A", ctrlKey: true }, at), { kind: "select-all" });
  assert.deepEqual(browseKeyIntent({ key: "a", metaKey: true }, at), { kind: "select-all" });
  // 不带修饰键的 a 不是全选（留给将来的「喜欢」之类），带 Alt 的也不接
  assert.equal(browseKeyIntent({ key: "a" }, at), null);
  assert.equal(browseKeyIntent({ key: "a", altKey: true }, at), null);
});

test("带 Ctrl / Cmd / Alt 的组合一律不管（留给快捷键体系）", () => {
  for (const modifier of ["ctrlKey", "metaKey", "altKey"] as const) {
    assert.equal(browseKeyIntent({ key: "3", [modifier]: true }, tiles), null);
    assert.equal(browseKeyIntent({ key: "Delete", [modifier]: true }, tiles), null);
    assert.equal(browseKeyIntent({ key: "ArrowLeft", [modifier]: true }, tiles), null);
    assert.equal(browseKeyIntent({ key: "p", [modifier]: true }, tiles), null);
  }
});

test("Shift 不影响单键语义（Shift+P 仍然是留下旗标）", () => {
  assert.deepEqual(browseKeyIntent({ key: "P", shiftKey: true }, tiles), {
    kind: "flag",
    value: "pick",
  });
  assert.deepEqual(browseKeyIntent({ key: "3", shiftKey: true }, tiles), {
    kind: "rating",
    value: 3,
  });
});

test("不认识的键返回 null（不抢别人的键）", () => {
  assert.equal(browseKeyIntent({ key: "a" }, tiles), null);
  assert.equal(browseKeyIntent({ key: "F2" }, tiles), null);
  assert.equal(browseKeyIntent({ key: " " }, tiles), null);
});

// ─────────────────── 该不该接这个事件 ───────────────────

test("shouldHandleKey：在输入框里打字时不接（p 不该变成旗标、数字不该变成打星）", () => {
  const input = { tagName: "INPUT", isContentEditable: false } as unknown as EventTarget;
  const textarea = { tagName: "TEXTAREA", isContentEditable: false } as unknown as EventTarget;
  const select = { tagName: "SELECT", isContentEditable: false } as unknown as EventTarget;
  const editable = { tagName: "DIV", isContentEditable: true } as unknown as EventTarget;
  for (const target of [input, textarea, select, editable]) {
    assert.equal(shouldHandleKey(target, false), false);
  }
});

test("shouldHandleKey：有模态开着时不接（弹窗里的键归弹窗）", () => {
  const button = { tagName: "BUTTON", isContentEditable: false } as unknown as EventTarget;
  assert.equal(shouldHandleKey(button, true), false);
  assert.equal(shouldHandleKey(button, false), true);
});

test("shouldHandleKey：目标为空（比如 body）时接", () => {
  assert.equal(shouldHandleKey(null, false), true);
});
