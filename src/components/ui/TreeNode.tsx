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
  onToggleExpand?: () => void;
  onCheckedChange?: (checked: boolean) => void;
  class?: string;
}

export function TreeNode(props: TreeNodeProps) {
  const [local, rest] = splitProps(props, [
    "label",
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
        "transition-colors outline-none",
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

      <Show when={local.trailing}>
        <span class="shrink-0 text-fs-1 text-fg-3 tnum">
          {local.trailing}
        </span>
      </Show>
    </div>
  );
}
