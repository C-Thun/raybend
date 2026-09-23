/**
 * `ToolsBar` —— 工具行（`design/main.md` §2.3；三段式见 `design/editor.md` §2.1）。
 *
 * 三条规则，全部来自设计稿：
 *   1. **三段式**：`left` 贴左、中段吃掉剩余宽度、`right` 贴右（人类 2026-09-23 口述的 editor 形态，
 *      术语见 `AGENTS.md` §11.1）；
 *   2. **跟着工作流走**（装配表在 `shell/flow.ts`）；
 *   3. **无内容时整行不渲染** —— 注意是「不存在」，不是「一条空条」。
 *
 * ```text
 * ┌────┬────────────────────────────────┬────┐
 * │左  │      中段（flex-1，居中）        │右  │
 * └────┴────────────────────────────────┴────┘
 *  ↑ 自然宽 + 朝中段的 padding          ↑ 同上，方向相反
 * ```
 *
 * # 中段：`flex-1 + overflow-hidden + 两侧 20px 渐变淡出`（人类 2026-09-23 口述）
 *
 * * 左 / 右段**没有内容时宽度为 0**（`<Show>` 直接不渲染那一块）；
 * * 中段**始终存在**，自动扩张、**撑满扣除左右两段后的剩余宽度**，并 `overflow-hidden`；
 * * 中段内左右各一条**方向相反**的渐变遮罩（左：不透明 → 透明；右：透明 → 不透明，各 20px）：
 *   内容超宽时先被裁在中段内，再从中段两侧**柔和地淡出**，不是硬切；
 * * 左 / 右段与中段相接的那条边各留一份 padding（`pr-pad-x` / `pl-pad-x`，
 *   走密度令牌，随紧凑 / 宽松自然撑开）—— 那是「左右段内容」与「中段内容」之间的分隔隙。
 *
 * ⚠️ 一个**有意**的取舍：中段是在**剩余宽度**里居中，不是整条 bar 的几何中心
 * （左右段宽度不等时中段内容会偏一点）。人类 2026-09-23 明确要这个形态
 * （「中段自动扩张撑满剩余宽度」），所以 W1 那版「绝对定位铺满整条、按几何中心对齐」
 * 已经被替换掉了 —— 别按旧注释改回去。
 *
 * # 整条 bar 退出 `Tab` 序列（人类 2026-09-23 要求）
 *
 * 桌面应用里 `Tab` 是**切面板档位**的功能键（editor / import / browse 都这么用），
 * 而浏览器的默认行为会顺手把焦点移到工具条上的按钮 —— 现象是「按一下 Tab，面板切了，
 * 按钮上还套了一个被裁掉一半的焦点框」（真机反馈）。所以挂载后把所有**本来在 Tab 序列里**
 * 的后代标成 `tabindex="-1"`：仍然可点、仍能被快捷键触发，只是不再参与 Tab 遍历
 * （输入框之间用 Tab 跳转那种有用场景不在这一条 bar 上）。工具随工作流变化，
 * 所以用 `MutationObserver` 跟着补。
 *
 * 「批量排除」是**反转**语义（`DESIGN.md` §12.2）：把选中的未排除项排除、
 * 已排除项恢复。所以按钮文字与外观恒定，**没有**「反排除」按钮；
 * 没有选中项时它是禁用态（无从反转）。
 *
 * 选中状态不属于 shell（那是 `photo-grid` 模块的事，见 `ARCHITECTURE.md` §3），
 * 所以这里只接一个 `hasSelection` 布尔量与点击回调。
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
  /** 右段：与 workspace **右列**内容相关的面板开关（editor 暂时没有） */
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
        class="relative flex h-bar-tool-h shrink-0 items-center bg-surface-main px-pad-x"
      >
        {/* 左段：贴左；宽度由内容自然撑开；朝中段那一侧留一份密度相关的 padding */}
        <Show when={props.hasLeftTools === true}>
          <div data-toolsbar-left class="flex shrink-0 items-center gap-1 pr-pad-x">
            {props.left}
          </div>
        </Show>

        {/*
          中段：吃掉剩余宽度（`flex-1` + `min-w-0` 才能在窄窗口下真正收缩），
          `overflow-hidden` 保证内容不会跑到 bar 外面，两侧的渐变把溢出柔化掉。
        */}
        <div class="relative min-w-0 flex-1 self-stretch overflow-hidden">
          <div class="flex h-full items-center justify-center gap-1">
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
          <FadeEdge side="left" />
          <FadeEdge side="right" />
        </div>

        {/* 右段：贴右（中段是 `flex-1`，它自然被顶到最右）；朝中段那一侧同样留 padding */}
        <Show when={props.hasRightTools === true}>
          <div data-toolsbar-right class="flex shrink-0 items-center gap-1 pl-pad-x">
            {props.right}
          </div>
        </Show>
      </div>
    </Show>
  );
}

/**
 * 中段一侧的渐变淡出（宽 20px = `w-5`，人类：「过渡设个 20 像素左右就行」）。
 *
 * 压在内容上面（`z-10`）并且 `pointer-events-none` —— 它只是「视觉上把溢出吃掉」，
 * 不参与命中（否则中段边缘的按钮会点不到）。
 */
function FadeEdge(props: { side: "left" | "right" }): JSX.Element {
  return (
    <span
      aria-hidden="true"
      data-toolsbar-fade={props.side}
      class={[
        "pointer-events-none absolute inset-y-0 z-10 w-5",
        props.side === "left"
          ? "left-0 bg-linear-to-r from-surface-main to-transparent"
          : "right-0 bg-linear-to-l from-surface-main to-transparent",
      ].join(" ")}
    />
  );
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
