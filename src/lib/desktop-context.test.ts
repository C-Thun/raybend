import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";

const script = readFileSync(new URL("../../src-tauri/src/desktop_context.js", import.meta.url), "utf8");

test("桌面初始化取消默认菜单但保留应用右键事件，重复执行只安装一次", () => {
  const target = new EventTarget();
  let installed = 0;
  const originalAdd = target.addEventListener.bind(target);
  target.addEventListener = (...args: Parameters<EventTarget["addEventListener"]>) => {
    installed++;
    originalAdd(...args);
  };
  runInNewContext(script, { window: target });
  runInNewContext(script, { window: target });
  assert.equal(installed, 1);
  let applicationEvents = 0;
  target.addEventListener("contextmenu", () => applicationEvents++);
  const rightClick = new Event("contextmenu", { cancelable: true });
  assert.equal(target.dispatchEvent(rightClick), false);
  assert.equal(rightClick.defaultPrevented, true);
  assert.equal(applicationEvents, 1);

  const typing = new Event("keydown", { cancelable: true });
  assert.equal(target.dispatchEvent(typing), true);
  assert.equal(typing.defaultPrevented, false);
});
