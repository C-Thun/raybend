/**
 * `ToolsBar` —— 工具行（`design/main.md` §2.3；三段式见 `design/editor.md` §2.1）。
 *
 * 三条规则，全部来自设计稿：
 *   1. **三段式**：`left` 贴左、中段居中、`right` 贴右（人类 2026-09-23 口述的 editor 形态，
 *      术语见 `AGENTS.md` §11.1）；
 *   2. **跟着工作流走**（装配表在 `shell/flow.ts`）；
 *   3. **无内容时整行不渲染** —— 注意是「不存在」，不是「一条空条」。
 *
 * ```text
 * ┌───┬───────────────────────┬───┐
 * │左 │          中           │右 │     左/右：与 workspace 左/右列相关的面板开关
 * └───┴───────────────────────┴───┘     中：该工作流的具体功能按钮（**不指定时的默认段**）
 * ```
 *
 * # 为什么中段是「绝对定位铺满整条」而不是普通 flex 子项
 *
 * 设计稿要求中段**在整条的正中央**（不是「左右两段之间的剩余空间里居中」）——
 * 后者在左右段宽度不等时会让中段肉眼可见地偏一边。所以中段的容器是
 * `absolute inset-0 + justify-center`：它按**整条的几何中心**对齐，与左右段多宽无关。
 *
 * # 挤压行为（人类明确）
 *
 * 窗口变窄、中段按钮变长时，**中段被左右两段的底纹「盖住」** —— 用的是
 * **渐变淡出**（左右段底纹向中间渐隐），不是硬边 + 阴影，也不是按钮互相叠错位。
 * 实现就是左右段各挂一条 `pointer-events-none` 的渐变（`from-surface-main to-transparent`）：
 * 它压在**中段上面**（`z-10`），把被挤压的部分柔和地吃掉；`overflow-hidden` 保证
 * 中段再长也不会把页面撑出横向滚动。
 *
 * 「批量排除」是**反转**语义（`DESIGN.md` §12.2）：把选中的未排除项排除、
 * 已排除项恢复。所以按钮文字与外观恒定，**没有**「反排除」按钮；
 * 没有选中项时它是禁用态（无从反转）。
 *
 * 选中状态不属于 shell（那是 `photo-grid` 模块的事，见 `ARCHITECTURE.md` §3），
 * 所以这里只接一个 `hasSelection` 布尔量与点击回调。
 */

import { For, Show, type Component, type JSX } from "solid-js";
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
        class="relative flex h-bar-tool-h shrink-0 items-center overflow-hidden bg-surface-main px-pad-x"
      >
        {/* 中段：铺满整条的绝对层 → 内容落在**这条带的正中央**（见文件头） */}
        <div class="pointer-events-none absolute inset-0 flex items-center justify-center">
          <div class="pointer-events-auto flex items-center gap-1">
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

        {/* 左段：贴左；右侧那条渐变把挤压过来的中段柔化掉 */}
        <Show when={props.hasLeftTools === true}>
          <div
            data-toolsbar-left
            class="relative z-10 flex shrink-0 items-center gap-1"
          >
            {props.left}
            <FadeToRight />
          </div>
        </Show>

        {/* 右段：贴右（`ml-auto` 把它顶到最右），渐变朝左 */}
        <Show when={props.hasRightTools === true}>
          <div
            data-toolsbar-right
            class="relative z-10 ml-auto flex shrink-0 items-center gap-1"
          >
            <FadeToLeft />
            {props.right}
          </div>
        </Show>
      </div>
    </Show>
  );
}

/** 挤压时的柔和收边（向右渐隐）。`pointer-events-none` 是必须的 —— 它盖在中段上面。 */
function FadeToRight(): JSX.Element {
  return (
    <span
      aria-hidden="true"
      class="pointer-events-none absolute top-0 left-full h-full w-10 bg-linear-to-r from-surface-main to-transparent"
    />
  );
}

/** 同上，朝左（右段用）。 */
function FadeToLeft(): JSX.Element {
  return (
    <span
      aria-hidden="true"
      class="pointer-events-none absolute top-0 right-full h-full w-10 bg-linear-to-l from-surface-main to-transparent"
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
