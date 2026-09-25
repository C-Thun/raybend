/**
 * `ToolsBar`：mid 始终横跨工具栏全宽，按钮作为一组按整窗中心对齐。
 * left/right 叠在 mid 上方，靠各自内侧的渐隐遮住挤到边缘的 mid 内容。
 * 无内容时整行不渲染；工具随工作流切换。设计见 `design/editor.md` §2.1。
 */

import { For, onCleanup, onMount, Show, createEffect, type Component, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import { IconBan } from "@tabler/icons-solidjs";
import { Button } from "../components/ui/Button.tsx";
import { t } from "../i18n";
import { isToolDisabled, toolsFor, type ToolId } from "./flow.ts";
import type { ShellStore } from "./store.ts";

export interface ToolsBarProps {
  store: ShellStore;
  /** 当前有没有选中照片（决定「批量排除」能不能点） */
  hasSelection?: boolean;
  /** 点「批量排除」 */
  onBatchExclude?: () => void;
  /*
   * 各工作流自己的工具（浏览模式的标记系列在 `features/browse/BrowseToolbar.tsx`，
   * 编辑模式的裁切/旋转/对比在 `features/editor/EditorToolbar.tsx`）。
   *
   * 为什么用插槽而不是往 `TOOL_CATALOG` 里塞：那张表描述的是「外壳认识的简单按钮」，
   * 而浏览的标记控件带**三态**、编辑的三个工具带**互斥状态**，都要读各自模块的状态 ——
   * 那是模块自己的事（`ARCHITECTURE.md` §3 的状态归属）。外壳只负责**条带与三段布局**。
   */
  children?: JSX.Element;
  /*
   * 插槽里**到底有没有东西**。必须由调用方明说，不能靠 `props.children !== undefined` 判：
   * JSX 子节点在 Solid 里是个 getter，里面套着 `<Show>` 时哪怕求值为假，`props.children`
   * 本身仍然是真值 —— 于是「无内容时整行不渲染」这条规则会失效（编辑/导出工作流上
   * 会留一条空条）。
   */
  hasExtraTools?: boolean;

  /* ── 三段式的左右两段（`AGENTS.md` §11.1）──────────────────── */
  /** 左段：与 workspace **左列**内容相关的面板开关（editor 的 LUT 开关放这里） */
  left?: JSX.Element;
  /** 右段：与 workspace **右列**相关的开关或全局动作 */
  right?: JSX.Element;
  hasLeftTools?: boolean;
  hasRightTools?: boolean;
}

export function ToolsBar(props: ToolsBarProps) {
  const tools = () => toolsFor(props.store.workflow());
  const hasCenter = (): boolean => tools().length > 0 || props.hasExtraTools === true;

  let bar: HTMLDivElement | undefined;

  /*
   * 让整条 bar 退出 Tab 序列（见文件头）。只标「本来 tabIndex >= 0」的元素：
   * 那是「会参与 Tab 遍历」的判据（普通 div 的 tabIndex 是 -1，不用管）。
   */
  const stripTabStops = (root: HTMLElement): void => {
    for (const element of root.querySelectorAll<HTMLElement>("*")) {
      if (element.tabIndex >= 0) element.setAttribute("tabindex", "-1");
    }
  };

  onMount(() => {
    if (bar === undefined) return;
    const root = bar;
    stripTabStops(root);
    const observer = new MutationObserver(() => stripTabStops(root));
    // 只看结构变化：**别监听 attributes** —— 我们自己写的 tabindex 会把它变成自激循环
    observer.observe(root, { childList: true, subtree: true });
    onCleanup(() => observer.disconnect());
  });

  // 工作流 / 工具集变化会换掉一批按钮：MutationObserver 会跟着补，这里再兜一次同步调用
  createEffect(() => {
    void props.store.workflow();
    void props.hasSelection;
    if (bar !== undefined) stripTabStops(bar);
  });

  return (
    <Show
      when={
        hasCenter() || props.hasLeftTools === true || props.hasRightTools === true
      }
    >
      <div
        /*
         * `data-toolsbar`：给冒烟一个**稳定的锚**。
         * 不然断言只能靠「在 document 里瞎找按钮」——色标那种一排同名文案的控件
         * 很容易找错地方（2026-09-19 加色标断言时正踩了这个）。
         */
        data-toolsbar
        ref={bar}
        class="relative flex h-bar-tool-h shrink-0 items-center justify-between bg-surface-main px-pad-x"
      >
        {/* mid 始终以整条 toolsbar 为参照居中，宽度与左右内容无关。 */}
        <div data-toolsbar-mid class="absolute inset-0 z-0 flex items-center justify-center overflow-hidden">
          <div class="flex h-full w-max shrink-0 items-center justify-center gap-1">
            <For each={tools()}>
              {(tool) => (
                <Button
                  variant="ghost"
                  disabled={isToolDisabled(tool, Boolean(props.hasSelection))}
                  icon={<Dynamic component={TOOL_ICONS[tool.id]} size={14} />}
                  onClick={tool.id === "batch-exclude" ? props.onBatchExclude : undefined}
                >
                  {t(tool.labelKey)}
                </Button>
              )}
            </For>
            {props.children}
          </div>
        </div>

        <Show when={props.hasLeftTools === true}>
          <div data-toolsbar-left class="relative z-10 flex h-full shrink-0 items-center gap-1 bg-surface-main">
            {props.left}
            <FadeEdge side="left" />
          </div>
        </Show>
        <Show when={props.hasRightTools === true}>
          <div data-toolsbar-right class="relative z-10 ml-auto flex h-full shrink-0 items-center gap-1 bg-surface-main">
            <FadeEdge side="right" />
            {props.right}
          </div>
        </Show>
      </div>
    </Show>
  );
}

/** left/right 覆盖层的内侧渐隐，不参与布局，也不放任何操作控件。 */
function FadeEdge(props: { side: "left" | "right" }): JSX.Element {
  return <span
    aria-hidden="true"
    data-toolsbar-fade={props.side}
    class={[
      "absolute inset-y-0 w-5",
      props.side === "left"
        ? "left-full bg-linear-to-r from-surface-main to-transparent"
        : "right-full bg-linear-to-l from-surface-main to-transparent",
    ].join(" ")}
  />;
}

/**
 * 工具的图标。用**禁行图标**（填充圆 + 横杠）而不是叉号 ——
 * 排除不是删除到磁盘（`AGENTS.md` §11.3），图标一换语义就变了。
 *
 * 存组件引用而不是 JSX 元素的原因同 `FlowBar.tsx`（模块级 JSX 会在 render 外创建计算）。
 */
const TOOL_ICONS: Record<ToolId, Component<{ size?: number }>> = {
  "batch-exclude": IconBan,
};
