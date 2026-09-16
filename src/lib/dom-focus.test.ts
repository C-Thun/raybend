/**
 * `dom-focus` 的单测。
 *
 * 这个仓库的测试是零依赖的 `node --test`（没有 jsdom），所以用**最小替身**：
 * 只验「有没有调 `blur()`」「有没有挂上/摘掉监听」这两件真正要保证的事。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { blurActive, installEscapeBlur } from "./dom-focus.ts";

interface FakeElement {
  blurred: number;
  blur: () => void;
}

function fakeElement(): FakeElement {
  const element: FakeElement = {
    blurred: 0,
    blur() {
      element.blurred += 1;
    },
  };
  return element;
}

function fakeDoc(active: unknown, body: unknown = {}): Document {
  return { activeElement: active, body } as unknown as Document;
}

/** 只实现用得到的那几个成员：够验「捕获期挂上、卸载摘掉」。 */
function fakeWindow(document: Document): {
  win: Window;
  dispatch: (type: string, event: { key: string }, capture: boolean) => void;
} {
  const listeners = new Set<{ type: string; capture: boolean; fn: (event: unknown) => void }>();
  const win = {
    document,
    addEventListener(type: string, fn: (event: unknown) => void, capture?: boolean) {
      listeners.add({ type, capture: capture === true, fn });
    },
    removeEventListener(type: string, fn: (event: unknown) => void, capture?: boolean) {
      for (const entry of listeners) {
        if (entry.type === type && entry.fn === fn && entry.capture === (capture === true)) {
          listeners.delete(entry);
        }
      }
    },
  } as unknown as Window;
  const dispatch = (type: string, event: { key: string }, capture: boolean): void => {
    for (const entry of [...listeners]) {
      if (entry.type === type && entry.capture === capture) entry.fn(event);
    }
  };
  return { win, dispatch };
}

test("blurActive 会把焦点从当前元素上摘掉", () => {
  const element = fakeElement();
  blurActive(fakeDoc(element));
  assert.equal(element.blurred, 1);
});

test("没有焦点、或焦点已在 body 上时什么都不做（也不报错）", () => {
  assert.doesNotThrow(() => blurActive(fakeDoc(null)));
  const body = fakeElement();
  blurActive(fakeDoc(body, body));
  assert.equal(body.blurred, 0, "body 不需要 blur");
});

test("blur() 抛错时被吞掉 —— 清焦点不该把流程搞崩", () => {
  const angry = {
    blur() {
      throw new Error("元素已卸载");
    },
  };
  assert.doesNotThrow(() => blurActive(fakeDoc(angry)));
});

test("installEscapeBlur：Esc 清焦点，其它键不动；卸载后不再响应", () => {
  const element = fakeElement();
  const { win, dispatch } = fakeWindow(fakeDoc(element));
  const uninstall = installEscapeBlur(win);

  dispatch("keydown", { key: "Tab" }, true);
  assert.equal(element.blurred, 0, "Tab 不该清焦点");

  dispatch("keydown", { key: "Escape" }, true);
  assert.equal(element.blurred, 1, "Esc 要先清焦点");

  // 必须是**捕获期**监听：抢在组件的 Esc 关闭逻辑之前跑
  dispatch("keydown", { key: "Escape" }, false);
  assert.equal(element.blurred, 1, "冒泡期不该再触发一次");

  uninstall();
  dispatch("keydown", { key: "Escape" }, true);
  assert.equal(element.blurred, 1, "卸载之后不再响应");
});
