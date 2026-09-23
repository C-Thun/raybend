/**
 * ScrollBar / ScrollBox（DESIGN.md §10.1 #15）。
 *
 * `ScrollBox` = 「可滚动的内容区」，滚动条样式来自 `src/styles/scrollbar.css`：
 *   默认   滑块用注释色派生的半透明色（几乎不抢视线）
 *   指向   滑块提亮（`fg-2` 派生）
 *   拖拽   同指向态
 *
 * 选型理由写在 `scrollbar.css` 里（一句话：不牺牲系统惯性滚动与触控板手势）。
 * 这个组件只负责把「可滚动」这件事做对：
 *   - `min-h-0`：flex 子项默认 `min-height:auto`，不写这个就会把容器撑破而不是滚动
 *   - `overscroll-contain`：滚到头不要把滚动传递给外层（避免「滚着滚着整页跳了」）
 */

import { splitProps, type JSX } from "solid-js";

export interface ScrollBoxProps extends JSX.HTMLAttributes<HTMLDivElement> {
  /** 只滚纵向 / 只滚横向 / 都滚 */
  axis?: "y" | "x" | "both";
}

export function ScrollBox(props: ScrollBoxProps) {
  const [local, rest] = splitProps(props, ["axis", "class"]);

  const axis = () => local.axis ?? "y";

  return (
    <div
      {...rest}
      class={[
        "min-h-0 min-w-0 flex-1 overscroll-contain",
        /*
         * 纵向滚动一律带 `scroll-y-reserved`：**滚动条落在预留空间里**
         * （人类 2026-09-23 定的全局规则，见 `DESIGN.md` §8.9 与 `scrollbar.css`）。
         * 写进组件而不是每个调用点各写一遍：这是「可滚动」这件事本身的一部分，
         * 漏写就会重新出现「滚动条一出现、内容缩一下」的抽动。
         */
        axis() === "y" ? "scroll-y-reserved overflow-x-hidden" : "",
        axis() === "x" ? "overflow-x-auto overflow-y-hidden" : "",
        axis() === "both" ? "scroll-y-reserved overflow-x-auto" : "",
        local.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    />
  );
}
