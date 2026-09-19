/**
 * `ToolsBar` —— 工具行（`design/main.md` §2.3）。
 *
 * 三条规则，全部来自设计稿：
 *   1. **内容居中**（不靠右、不靠左）
 *   2. **跟着工作流走**（装配表在 `shell/flow.ts`）
 *   3. **无内容时整行不渲染** —— 注意是「不存在」，不是「一条空条」：
 *      切到还没实现的工作流时，这一行应当整体消失
 *
 * 「批量排除」是**反转**语义（`DESIGN.md` §12.2）：把选中的未排除项排除、
 * 已排除项恢复。所以按钮文字与外观恒定，**没有**「反排除」按钮；
 * 没有选中项时它是禁用态（无从反转）。
 *
 * 选中状态不属于 shell（那是 `photo-grid` 模块的事，见 `ARCHITECTURE.md` §3），
 * 所以这里只接一个 `hasSelection` 布尔量与点击回调 —— M1-5 接上照片网格时不用改这里。
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
  * 各工作流自己的工具（浏览模式的标记系列在 `features/browse/BrowseToolbar.tsx`）。
  *
  * 为什么用插槽而不是往 `TOOL_CATALOG` 里塞：那张表描述的是「外壳认识的简单按钮」，
  * 而浏览的标记控件带**三态**、要读选中照片的状态 —— 那是模块自己的事
  * （`ARCHITECTURE.md` §3 的状态归属）。外壳只负责**条带与居中**，控件由模块给。
  */
 children?: JSX.Element;
 /*
  * 插槽里**到底有没有东西**。必须由调用方明说，不能靠 `props.children !== undefined` 判：
  * JSX 子节点在 Solid 里是个 getter，里面套着 `<Show>` 时哪怕求值为假，`props.children`
  * 本身仍然是真值 —— 于是「无内容时整行不渲染」这条规则会失效（编辑/导出工作流上
  * 会留一条空条）。
  */
 hasExtraTools?: boolean;
}

export function ToolsBar(props: ToolsBarProps) {
 const tools = () => toolsFor(props.store.workflow());

 return (
  <Show when={tools().length > 0 || props.hasExtraTools}>
   <div
    /*
     * `data-toolsbar`：给冒烟一个**稳定的锚**。
     * 不然断言只能靠「在 document 里瞎找按钮」——色标那种一排同名文案的控件
     * 很容易找错地方（2026-09-19 加色标断言时正踩了这个）。
     */
    data-toolsbar
    class="flex h-bar-tool-h shrink-0 items-center justify-center gap-1 bg-surface-main px-pad-x"
   >
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
   </Show>
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
