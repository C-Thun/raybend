/**
 * SplitHandle —— 面板之间的拖拽分隔条。**全项目唯一实现**（`ARCHITECTURE.md` 的复用纪律）。
 *
 * 形态：**很细的一整条空白区**，正中画一排 `...` 三点。
 * 它之所以是「空白」而不是「一条线」，是因为本项目**无边线设计**（`DESIGN.md` §6）——
 * 分隔靠留白与面色差，不靠描边。
 *
 * 状态（自己管，调用方不用再传 `active`）：
 *   默认     三点用注释色，几乎看不见
 *   指向     整条叠**辅色底**（`DESIGN.md` §5），三点转为次级色
 *   拖拽中   叠**主色底**，光标保持 resize
 *   禁用     不响应，也不显示三点
 *
 * ## 用法（都在 `onDrag` 里把 dx/dy 换成自己的尺寸数学）
 *
 * ```tsx
 * <SplitHandle
 *   orientation="vertical"                       // 竖着站：拖拽改**宽度**
 *   onDragStart={() => (start = width())}        // 记住起点值
 *   onDrag={(dx) => setWidth(clamp(start + dx))} // 每帧最多来一次
 *   onDragEnd={() => persist(width())}           // 松手才落盘
 * />
 * ```
 *
 * 键盘（方向键）由调用方自己接 `onKeyDown` —— 它知道 +/- 的语义与落盘时机，
 * 组件不猜。`role`/`aria-orientation`/`tabindex` 透传即可。
 *
 * ## 为什么手写而不用 Ark UI 的 Splitter（2026-09-16 真机事故，别再改回去）
 *
 * Ark 的 Splitter 用 ResizeObserver **量了又写、写了又量**：外层横向 splitter 套内层竖向时，
 * 拖外层会让内层不断收到回调、量到过渡中的尺寸 —— 症状是「一拖就卡，松手过一会儿才跳到位」。
 * 这里只做一件事：**pointermove → 一帧一次写尺寸**。没有 observer，没有嵌套，行为完全可预期。
 *
 * 三个细节缺一不可（都是真机上「拖不动」的成因）：
 *   1. `touch-action: none` —— 否则触摸/笔会把拖动当成滚动手势抢走；
 *   2. 拖动期间禁掉文本选择 —— 否则拖着拖着选中一片字；
 *   3. **rAF 节流** —— 一次拖动会来十几个 pointermove，逐条写布局就是自找卡顿。
 */

import { onCleanup, createSignal, Show, splitProps, type JSX } from "solid-js";

import { trackPointerDrag } from "../../lib/pointer-drag.ts";

export type SplitAxisAlignment = "horizontal" | "vertical";

export interface SplitHandleDotsProps {
  /**
   * 分隔条的走向：
   *   `horizontal` = 横着躺（分割上下两段，拖拽改高度）
   *   `vertical`   = 竖着站（分割左右两列，拖拽改宽度）
   */
  orientation?: SplitAxisAlignment;
  /** 拖拽中 */
  active?: boolean;
  class?: string;
}

/** 只有三个点，没有任何交互 —— 需要单独摆点时用它（例如自己画一整条把手的外壳） */
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

export interface SplitHandleProps
  extends Omit<JSX.HTMLAttributes<HTMLDivElement>, "onDrag" | "onDragStart" | "onDragEnd"> {
  orientation?: SplitAxisAlignment;
  /** 外部强制显示为拖拽中（一般不用给：拖动时组件自己知道） */
  active?: boolean;
  disabled?: boolean;
  /** 开始拖动（在这里记住起始尺寸） */
  onDragStart?: () => void;
  /** 正在拖动：参数是**相对起点的位移**（横向看 dx、纵向看 dy），已按帧节流 */
  onDrag?: (delta: number) => void;
  /** 松手/取消：参数同 `onDrag`。落盘写这里，别写在 `onDrag` 里 */
  onDragEnd?: (delta: number) => void;
}

export function SplitHandle(props: SplitHandleProps) {
  const [local, rest] = splitProps(props, [
    "orientation",
    "active",
    "disabled",
    "onDragStart",
    "onDrag",
    "onDragEnd",
    "class",
  ]);
  const horizontal = () => (local.orientation ?? "horizontal") === "horizontal";
  const [internalDragging, setInternalDragging] = createSignal(false);
  const dragging = () => local.active || internalDragging();

  /** 位移取拖动方向上的那一维 */
  const axis = (event: PointerEvent): number =>
    horizontal() ? event.clientY : event.clientX;

  let cancelDrag: (() => void) | undefined;
  onCleanup(() => cancelDrag?.());

  function onPointerDown(event: PointerEvent): void {
    if (local.disabled || event.button !== 0) return;
    // 别让浏览器从这一刻开始「拖选文本」或做原生拖放
    event.preventDefault();

    cancelDrag?.();
    const start = axis(event);
    const coordinate = (point: {x:number;y:number}) => horizontal()?point.y:point.x;
    cancelDrag = trackPointerDrag(event, {
      capture: true,
      start: () => { setInternalDragging(true); local.onDragStart?.(); },
      move: point => local.onDrag?.(coordinate(point)-start),
      end: point => { setInternalDragging(false); local.onDragEnd?.(coordinate(point)-start); },
    });
  }

  return (
    <div
      {...rest}
      role={rest.role ?? "separator"}
      aria-orientation={horizontal() ? "horizontal" : "vertical"}
      aria-disabled={local.disabled || undefined}
      data-dragging={dragging() ? "" : undefined}
      onPointerDown={onPointerDown}
      class={[
        "group/split relative flex shrink-0 touch-none items-center justify-center transition-colors select-none",
        // 细：横条 8px 高、竖条 8px 宽（够按住，又不占版面）
        horizontal() ? "h-2 w-full" : "w-2 self-stretch",
        local.disabled
          ? "cursor-default"
          : horizontal()
            ? "cursor-row-resize"
            : "cursor-col-resize",
        // 指向 = 辅色底；拖拽中 = 主色底（§5）
        dragging()
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
        <SplitHandleDots orientation={local.orientation} active={dragging()} />
      </Show>
    </div>
  );
}
