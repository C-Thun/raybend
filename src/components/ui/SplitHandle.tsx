/**
 * SplitHandle —— 三段式面板的拖拽分隔条（DESIGN.md §10.1 #14、§12.5）。
 *
 * 形态：**很细的一整条空白区**，正中画一排 `...` 三点。
 * 它之所以是「空白」而不是「一条线」，是因为本项目**无边线设计**（§6）——
 * 分隔靠留白与面色差，不靠描边。
 *
 * 状态：
 *   默认     三点用注释色，几乎看不见
 *   指向     整条叠**辅色底**（§5），三点转为次级色
 *   拖拽中   叠**主色底**（已经「切实按下」了，§5），光标保持 resize
 *   禁用     不响应，也不显示三点
 *
 * 用法有两种：
 *   1. 独立用：`<SplitHandle />`（自己接管 `onPointerDown`）
 *   2. 配 Ark UI `Splitter`：把 `SplitHandleDots` 放进 `Splitter.ResizeTrigger` 里 ——
 *      Ark 负责命中、键盘与尺寸约束，我们只提供长相。
 */

import { Show, splitProps, type JSX } from "solid-js";

export type SplitAxisAlignment = "horizontal" | "vertical";

export interface SplitHandleDotsProps {
  /**
   * 分隔条的走向：
   *   `horizontal` = 横着躺（分割上下两段，拖拽改高度）← 左列三段用这个
   *   `vertical`   = 竖着站（分割左右两列，拖拽改宽度）
   */
  orientation?: SplitAxisAlignment;
  /** 拖拽中 */
  active?: boolean;
  class?: string;
}

/** 只有三个点，没有任何交互 —— 供 Ark `Splitter.ResizeTrigger` 内部复用 */
export function SplitHandleDots(props: SplitHandleDotsProps) {
  const [local, rest] = splitProps(props, ["orientation", "active", "class"]);
  const horizontal = () => (local.orientation ?? "horizontal") === "horizontal";

  return (
    <span
      {...rest}
      aria-hidden="true"
      class={[
        "pointer-events-none flex items-center justify-center gap-[3px]",
        horizontal() ? "flex-row" : "flex-col",
        local.active ? "text-fg-1" : "text-fg-3",
        local.class ?? "",
      ].join(" ")}
    >
      {/* 三个点用元素而不是字体里的 `...` —— 字体的省略号会随字号变，也会被字距影响 */}
      <span class="size-[3px] rounded-full bg-current" />
      <span class="size-[3px] rounded-full bg-current" />
      <span class="size-[3px] rounded-full bg-current" />
    </span>
  );
}

export interface SplitHandleProps extends JSX.HTMLAttributes<HTMLDivElement> {
  orientation?: SplitAxisAlignment;
  active?: boolean;
  disabled?: boolean;
}

export function SplitHandle(props: SplitHandleProps) {
  const [local, rest] = splitProps(props, [
    "orientation",
    "active",
    "disabled",
    "class",
  ]);
  const horizontal = () => (local.orientation ?? "horizontal") === "horizontal";

  return (
    <div
      {...rest}
      role="separator"
      aria-orientation={horizontal() ? "horizontal" : "vertical"}
      aria-disabled={local.disabled || undefined}
      class={[
        "group/split relative flex shrink-0 items-center justify-center transition-colors",
        // 细：横条 8px 高、竖条 8px 宽（够按住，又不占版面）
        horizontal() ? "h-2 w-full" : "w-2 self-stretch",
        local.disabled
          ? "cursor-default"
          : horizontal()
            ? "cursor-row-resize"
            : "cursor-col-resize",
        // 指向 = 辅色底；拖拽中 = 主色底（§5）
        local.active
          ? "bg-state-selected"
          : local.disabled
            ? ""
            : "hover:bg-state-hover",
        local.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <Show when={!local.disabled}>
        <SplitHandleDots orientation={local.orientation} active={local.active} />
      </Show>
    </div>
  );
}
