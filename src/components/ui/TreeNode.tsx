/**
 * TreeNode —— 树形行（DESIGN.md §10.1 #12、§12.4）。
 *
 * 一行里同时可能出现**两套独立状态**，这是本组件最容易做错的地方：
 *
 *   勾选（`RadioCircle`，行**左侧**）  「已加入待导入」——可多选
 *   选中（整行**主色底**）            「当前正在浏览」——全应用同时只有一个
 *
 * 两者互相独立：可以「勾了但没选中」，也可以「选中但没勾」。
 *
 * 状态：
 *   默认      无底
 *   指向      辅色底（§5）
 *   选中      主色**低透明度**底（大面积用低浓度，§5.1）；行名提到 `fg-1`
 *   勾选      左侧圆圈转主色实心
 *   展开      箭头朝下；折叠朝右（**只由用户点击箭头改变**）
 *   禁用      前景降次级，不响应指针
 *
 * ⚠️ **不要因为某行被选中就把它展开**（§12.4.1 明确禁止）：
 * 展开状态只跟用户操作有关。本组件只**如实渲染**传进来的 `expanded`，
 * 不做任何「为了显示选中而展开」的动作 —— 摊平逻辑在 `src/lib/tree.ts`，
 * 它只看展开集合，压根看不到选中集合。
 */

import { IconChevronDown, IconChevronRight } from "@tabler/icons-solidjs";
import { Show, splitProps, type JSX } from "solid-js";
import { indentPx } from "../../lib/tree.ts";
import { RadioCircle } from "./Form.tsx";

export interface TreeNodeProps {
  /** 行名（目录名） */
  label: string;
  /**
   * 行内容的替代渲染（省略时用 `label` 画文字）。
   *
   * `Recent` 列表用它换行 `PathText`（缩写路径 + 悬停看完整路径，`DESIGN.md` §12.3）；
   * 目录树则用默认的纯名字。行本身的语义（勾选 / 选中 / 指向）不分家。
   */
  labelNode?: JSX.Element;
  /** 深度，从 0 开始。决定缩进 */
  depth: number;
  /** 有子节点（决定是否画展开箭头） */
  hasChildren?: boolean;
  expanded?: boolean;
  /** 勾选态（与选中态独立） */
  checked?: boolean;
  /** 选中态（整行主色底；同一时间全应用只有一个） */
  selected?: boolean;
  disabled?: boolean;
  /** 左侧图标（目录 / 驱动器 / 来源类型） */
  icon?: JSX.Element;
  /** 行尾的次要信息（如照片数量） */
  trailing?: JSX.Element;
  /** 无障碍名（如「勾选 D:\\Photos」） */
  checkLabel?: string;
  onClick?: JSX.EventHandlerUnion<HTMLDivElement, MouseEvent>;
  /**
   * 双击整行。目前唯一的用法是**双击名字展开/折叠**（与点箭头同效，`DirTree` 接的）。
   * 注意双击会**先**触发两次 `onClick`（选中）再触发它 —— 所以「选中」必须是幂等的。
   */
  onDoubleClick?: JSX.EventHandlerUnion<HTMLDivElement, MouseEvent>;
  onToggleExpand?: () => void;
  onCheckedChange?: (checked: boolean) => void;
  class?: string;
}

export function TreeNode(props: TreeNodeProps) {
  const [local, rest] = splitProps(props, [
    "label",
    "labelNode",
    "depth",
    "hasChildren",
    "expanded",
    "checked",
    "selected",
    "disabled",
    "icon",
    "trailing",
    "checkLabel",
    "onClick",
    "onDoubleClick",
    "onToggleExpand",
    "onCheckedChange",
    "class",
  ]);

  return (
    <div
      {...rest}
      role="treeitem"
      aria-selected={Boolean(local.selected)}
      aria-expanded={local.hasChildren ? Boolean(local.expanded) : undefined}
      aria-level={local.depth + 1}
      aria-disabled={local.disabled || undefined}
      tabindex={local.disabled ? -1 : 0}
      // 缩进是**间距类**尺寸，随密度档变（§8.1）；level 0 也有一个基准缩进
      style={{ "padding-left": `${indentPx(local.depth, 1)}em` }}
      class={[
        "group/row flex h-row-h min-w-0 items-center gap-1 rounded-ui ps-row-indent pe-1",
        // ⚠️ **刻意不加 `transition-colors`**（2026-09-16 人类反馈「目录滚动有 0.2 秒延迟感」）：
        // 行是**连续排列**的，鼠标停在列表上滚动时每一行都会经过指针 → 每行都跑一次 150ms
        // 颜色过渡 = 持续重绘，手感就是「粘」。行的高亮要**立刻**（`motion.css` 的纪律：
        // 超过 150ms 就开始像卡顿）。按钮那类孤立元素保留过渡，行与列表行一律不要。
        "outline-none",
        local.selected
          ? "bg-state-selected text-fg-1"
          : "text-fg-2 hover:bg-state-hover hover:text-fg-1",
        local.disabled
          ? "pointer-events-none text-fg-3 opacity-60"
          : "cursor-pointer",
        local.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      onClick={local.onClick}
      // Solid 的 DOM 事件名是 `onDblClick`（不是 React 的 onDoubleClick）——
      // 对外的 prop 仍叫 `onDoubleClick`，更好念
      onDblClick={local.onDoubleClick}
    >
      {/* ── 展开箭头：点它只展开，不选中 ─────────────────── */}
      <Show
        when={local.hasChildren}
        fallback={<span class="size-4 shrink-0" aria-hidden="true" />}
      >
        <button
          type="button"
          tabindex={-1}
          aria-label={local.label}
          aria-hidden="true"
          onClick={(event) => {
            // 不 stopPropagation 的话点箭头会顺带选中该行 —— 那是两个不同的意图
            event.stopPropagation();
            local.onToggleExpand?.();
          }}
          class="flex size-4 shrink-0 cursor-pointer items-center justify-center rounded-[2px] text-fg-3 hover:text-fg-1"
        >
          {local.expanded ? (
            <IconChevronDown size={12} aria-hidden="true" />
          ) : (
            <IconChevronRight size={12} aria-hidden="true" />
          )}
        </button>
      </Show>

      {/* ── 勾选圈：与选中态独立，点它不应顺带选中（RadioCircle 内部已 stopPropagation）── */}
      <Show when={local.onCheckedChange}>
        <RadioCircle
          checked={Boolean(local.checked)}
          onCheckedChange={(next) => local.onCheckedChange?.(next)}
          label={local.checkLabel ?? local.label}
          disabled={local.disabled}
        />
      </Show>

      <Show when={local.icon}>
        <span
          class="flex size-4 shrink-0 items-center justify-center"
          aria-hidden="true"
        >
          {local.icon}
        </span>
      </Show>

      <Show when={local.labelNode} fallback={
        <span
          class={[
            "min-w-0 flex-1 truncate text-fs-2",
            local.selected ? "text-fg-1" : "",
          ]
            .filter(Boolean)
            .join(" ")}
          title={local.label}
        >
          {local.label}
        </span>
      }>
        <span class="min-w-0 flex-1">{local.labelNode}</span>
      </Show>

      <Show when={local.trailing}>
        <span class="shrink-0 text-fs-1 text-fg-3 tnum">
          {local.trailing}
        </span>
      </Show>
    </div>
  );
}
