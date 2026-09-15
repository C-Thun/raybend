/**
 * 外壳自己的状态（`ARCHITECTURE.md` §3 的状态归属表）。
 *
 * 目前只有两件事：
 *   1. **当前工作流**（导入/浏览/编辑/导出）—— 它决定 flowbar 选中哪一项、
 *      toolsbar 有没有内容。它不属于任何模块，是外壳自己的状态。
 *   2. **菜单可见性** —— 设计稿要求「菜单只在鼠标指向标题行时才出现」。
 *
 * 为什么不放组件内部 state：`TitleBar` / `FlowBar` / `ToolsBar` 三处都读它，
 * 放组件里就得靠 props 一层层传，而且没法独立测试。放这里 + 由 `App` 下发，
 * 是这一层最省事的做法（外壳只有三行，不值得再上一层 Context）。
 *
 * **刻意不放**：照片选择状态（那是 `photo-grid` 模块的事，M1-5 接）。
 */

import { createSignal } from "solid-js";
import { DEFAULT_WORKFLOW, isWorkflow, type WorkflowId } from "./flow.ts";

export interface ShellStore {
 /* ── 工作流 ─────────────────────────────── */
 workflow: () => WorkflowId;
 /** 非法值会被忽略（存储/外部输入不该把界面带到不存在的状态） */
 setWorkflow: (next: unknown) => void;

 /* ── 菜单可见性 ─────────────────────────── */
 /**
  * 标题行上的菜单此刻是否可见。
  * = 指针在标题行内 **或** 键盘焦点在其中 **或** 菜单正被展开（钉住）。
  */
 menuVisible: () => boolean;
 setMenuHover: (hovering: boolean) => void;
 setMenuFocus: (focused: boolean) => void;
 /**
  * 菜单展开期间把它钉住。
  *
  * 必需，不是锦上添花：菜单弹层挂在 portal 里（DOM 上不在标题行内），
  * 鼠标一旦移进菜单就会触发标题行的 `pointerleave`；不钉住的话
  * **菜单会在用户点它的一瞬间自己关掉**。
  */
 pinMenus: (pinned: boolean) => void;
}

export function createShellStore(
 initialWorkflow: unknown = DEFAULT_WORKFLOW,
): ShellStore {
 const [workflow, setWorkflowSignal] = createSignal<WorkflowId>(
  isWorkflow(initialWorkflow) ? initialWorkflow : DEFAULT_WORKFLOW,
 );
 const [hovering, setHovering] = createSignal(false);
 const [focused, setFocused] = createSignal(false);
 const [pinned, setPinned] = createSignal(false);

 return {
  workflow,
  setWorkflow: (next) => {
   if (isWorkflow(next)) setWorkflowSignal(next);
  },
  menuVisible: () => hovering() || focused() || pinned(),
  setMenuHover: setHovering,
  setMenuFocus: setFocused,
  pinMenus: setPinned,
 };
}
