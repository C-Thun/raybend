/**
 * **视口覆盖层**的宿主（M3-W3 定契约，W5 的裁切 / 旋转 / 对比与将来的蒙版都插进这里）。
 *
 * # 一条规矩（`AGENTS.md` §6.1 红线 2）
 *
 * 覆盖层的子元素**一律用「图像像素」坐标书写**，由这一层统一套上 Rust 给的仿射矩阵
 * （`renderState().overlayTransform`，语义与 CSS `matrix()` 一致）。
 *
 * ```text
 * 子元素写： <div style={{ left: "1200px", top: "800px" }}>   ← 图像像素
 * 这一层做： transform: matrix(a, b, c, d, e, f)              ← 缩放/平移/旋转/DPR 全在里面
 * ```
 *
 * **禁止**任何覆盖层自己乘 zoom / 减 pan / 补 DPR —— 那正是「三处各写一份对齐规则、
 * 改一处漏两处」的老路（拖拽把手与锚点规则在本仓都踩过，见 `AGENTS.md` §2.12）。
 * 命中测试同理：指针位置交给 Rust 的 `hitTest` 意图，前端不算坐标。
 *
 * # 现在有谁在用
 *
 * W3 这一波**还没有**覆盖层内容（裁切 / 旋转 / 对比在 W5），这里先把宿主与契约立起来 ——
 * 有了它，W5 的三个工具只需要往里面放「用图像像素写的」元素，不需要碰视口数学。
 */

import { Show, type JSX } from "solid-js";

import type { EditorRenderState } from "../../api/types.ts";

/** 把 `[a, b, c, d, e, f]` 写成 CSS `matrix(...)`。 */
export function overlayMatrixCss(
  matrix: readonly [number, number, number, number, number, number],
): string {
  return `matrix(${matrix.map((value) => Number(value.toFixed(6))).join(", ")})`;
}

export interface ViewportOverlayProps {
  /** 渲染线程的状态（里面有 `overlayTransform`）；`null` = 还没起来 */
  renderState: EditorRenderState | null;
  /** 覆盖层内容（用图像像素坐标书写） */
  children?: JSX.Element;
  class?: string;
}

export function ViewportOverlay(props: ViewportOverlayProps): JSX.Element {
  const transform = (): string | null => {
    const matrix = props.renderState?.overlayTransform ?? null;
    return matrix === null ? null : overlayMatrixCss(matrix);
  };

  return (
    <Show when={transform()}>
      {(value) => (
        <div
          data-viewport-overlay="on"
          aria-hidden="true"
          class={["pointer-events-none absolute inset-0 overflow-hidden", props.class ?? ""]
            .filter(Boolean)
            .join(" ")}
        >
          {/*
            坐标原点在**图像左上角**：子元素用 `left/top` 写图像像素，
            `transform-origin: 0 0` 保证矩阵按同一原点作用。
          */}
          <div
            class="absolute left-0 top-0 origin-top-left"
            style={{ transform: value() }}
            data-viewport-overlay-content
          >
            {props.children}
          </div>
        </div>
      )}
    </Show>
  );
}
