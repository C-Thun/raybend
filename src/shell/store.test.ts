/**
 * 外壳状态机的单元测试。
 *
 * 这里最值得钉住的是**菜单钉住**那条：菜单挂在 portal 里，
 * 鼠标移进菜单会触发标题行的 pointerleave —— 少了钉住逻辑，
 * 菜单会在用户点它的一瞬间自己关掉，而这种 bug 在手动点击时很难复现清楚。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import { DEFAULT_WORKFLOW } from "./flow.ts";
import { createShellStore } from "./store.ts";

/* ─── 工作流 ───────────────────────────────────────────── */

test("默认工作流是「导入」", () => {
 assert.equal(createShellStore().workflow(), DEFAULT_WORKFLOW);
});

test("可以指定初始工作流；非法初值回落到默认", () => {
 assert.equal(createShellStore("edit").workflow(), "edit");
 assert.equal(createShellStore("nonsense").workflow(), DEFAULT_WORKFLOW);
 assert.equal(createShellStore(null).workflow(), DEFAULT_WORKFLOW);
});

test("setWorkflow：合法值生效，非法值被忽略（不把界面带到不存在的状态）", () => {
 const store = createShellStore();
 store.setWorkflow("browse");
 assert.equal(store.workflow(), "browse");

 store.setWorkflow("shutdown");
 assert.equal(store.workflow(), "browse", "非法值不该改变现状");

 store.setWorkflow(undefined);
 assert.equal(store.workflow(), "browse");

 store.setWorkflow("export");
 assert.equal(store.workflow(), "export");
});

/* ─── 菜单可见性 ───────────────────────────────────────── */

test("默认不显示菜单（设计稿：只在指向标题行时出现）", () => {
 const store = createShellStore();
 assert.equal(store.menuVisible(), false);
});

test("指针进入标题行 → 显示；离开且无其它条件 → 隐藏", () => {
 const store = createShellStore();
 store.setMenuHover(true);
 assert.equal(store.menuVisible(), true);
 store.setMenuHover(false);
 assert.equal(store.menuVisible(), false);
});

test("键盘焦点也能显出来（否则键盘用户永远打不开菜单）", () => {
 const store = createShellStore();
 store.setMenuFocus(true);
 assert.equal(store.menuVisible(), true, "Tab 进标题行就该能看到菜单");

 store.setMenuHover(true);
 store.setMenuFocus(false);
 assert.equal(store.menuVisible(), true, "焦点走了但指针还在，仍应显示");

 store.setMenuHover(false);
 assert.equal(store.menuVisible(), false);
});

test("钉住：菜单展开期间即使指针离开标题行也必须保持可见", () => {
 const store = createShellStore();
 store.setMenuHover(true);
 store.pinMenus(true); // 用户点开了菜单
 store.setMenuHover(false); // 指针移到 portal 里的菜单上 → 标题行收到 leave

 assert.equal(
  store.menuVisible(),
  true,
  "不钉住的话菜单会在点它的瞬间自己关掉",
 );

 store.pinMenus(false);
 assert.equal(store.menuVisible(), false, "关掉菜单后回到「指针决定」");
});

test("钉住不会把指针状态也改掉（解除钉住后要如实反映现状）", () => {
 const store = createShellStore();
 store.setMenuHover(true);
 store.pinMenus(true);
 store.pinMenus(false);
 assert.equal(
  store.menuVisible(),
  true,
  "指针仍在标题行内，解除钉住后依然可见",
 );

 store.setMenuHover(false);
 assert.equal(store.menuVisible(), false);
});

test("重复设置同一状态是幂等的（指针可能连续触发 enter）", () => {
 const store = createShellStore();
 store.setMenuHover(true);
 store.setMenuHover(true);
 assert.equal(store.menuVisible(), true);
 store.pinMenus(true);
 store.pinMenus(true);
 store.setMenuHover(false);
 assert.equal(store.menuVisible(), true);
});
