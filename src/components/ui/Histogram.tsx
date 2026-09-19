/**
 * `Histogram` —— RGB 直方图（三通道填充曲线 + 加色区域），看图与**将来的编辑模块共用**。
 *
 * 为什么提到 `components/ui/`（而不是留在浏览的右栏里）：人类 2026-09-19 明确
 * 「这个组件需要优化成方便后续编辑时实时调整的状态」—— 编辑模块要拿它当
 * **实时反馈件**（拖曝光/对比时每帧重画），所以它必须：
 *
 *   * **只吃数据**：`bars` 是已经归一化好的采样（`lib` 算的），组件不碰 IPC、不碰 store；
 *   * **重画只是一个纯函数调用**：调用方把新的 `bars` 传进来就换一帧，
 *     不需要重新挂载、不需要重新请求后端；
 *   * **没有内部状态**（没有信号、没有 effect）—— 这既让它便宜，也让它可以被任意父层驱动。
 *
 * 画法（人类 2026-09-19 的三条要求）：
 *
 * 1. **52 个采样点**（0..255 每 5 级一个点）—— 点数由数据决定，组件不关心；
 * 2. **点与点之间是平滑曲线**（单调三次插值，见 `lib`；不会过冲）；
 * 3. **颜色就是色标那六色**：单通道 = 红/绿/蓝，两两重叠 = 黄/青/紫（绿+红=黄、
 *    蓝+绿=青、红+蓝=紫），三色重叠 = 一个偏亮的**中间灰**（不是纯白）。
 *    实现上**不靠混合模式取色**，而是把 7 个区域显式切开各填各色 —— 颜色因此是
 *    令牌里那个色，改色标只改令牌。
 *
 * 背景（人类要求）：**很浅的细密虚线**，纵向 4 等分、横向上下 2 等分 ——
 * 只给亮度轴一个参照，不能跟曲线抢注意力。
 */

import { For } from "solid-js";
import { t } from "../../i18n/index.ts";
import {
  histogramBandPath,
  histogramBands,
  histogramPath,
  type HistogramBars,
} from "../../lib/histogram.ts";
import { COLOR_FILL_CLASS } from "../../lib/color-labels.ts";

/** 画布坐标系（`preserveAspectRatio="none"` 拉伸到实际尺寸；这样与像素宽度无关） */
const VIEW_W = 256;
const VIEW_H = 100;

export interface HistogramProps {
  /** 归一化后的采样（`histogramBarHeights` 的产物）；`null` = 还没取到，画空态 */
  bars: HistogramBars | null;
  class?: string;
}

export function Histogram(props: HistogramProps) {
  /** 7 个区域（每列按高度排序切开），只在数据变化时重算 */
  const bands = () => {
    const current = props.bars;
    return current === null ? null : histogramBands(current.r, current.g, current.b);
  };

  return (
    <div
      class={["relative h-[72px] w-full overflow-hidden rounded-ui bg-surface-bar", props.class ?? ""]
        .filter(Boolean)
        .join(" ")}
      data-histogram={props.bars === null ? "empty" : "curves"}
    >
      {/*
        背景等分虚线（人类 2026-09-19）：
        * 纵向 4 等分 → 3 根；横向上下 2 等分 → 1 根；
        * 用「2px 线 + 3px 空」的细密虚线，颜色走 `--hist-grid`（很浅）；
        * 用 DOM 而不是 SVG `stroke-dasharray`：SVG 被 `preserveAspectRatio="none"` 拉伸，
          虚线的「段长」会跟着变形（竖线被拉长、横线被压扁），DOM 的 CSS 虚线始终是像素级的。
      */}
      <div class="pointer-events-none absolute inset-0" aria-hidden="true">
        <For each={[25, 50, 75]}>
          {(percent) => (
            <span
              class="absolute inset-y-0 w-px bg-[linear-gradient(to_bottom,var(--hist-grid)_0_2px,transparent_2px_5px)] bg-repeat-y"
              style={{ left: `${percent}%` }}
            />
          )}
        </For>
        <span class="absolute inset-x-0 top-1/2 h-px bg-[linear-gradient(to_right,var(--hist-grid)_0_2px,transparent_2px_5px)] bg-repeat-x" />
      </div>

      <svg
        class="relative h-full w-full"
        viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
        preserveAspectRatio="none"
        role="img"
        aria-label={t("browse.histogramHint")}
      >
        {/* ① 三色重叠（最低那条以下）：中间灰 —— 画在最底层 */}
        <path
          d={histogramPath(bands()?.rgb.top ?? [], VIEW_W, VIEW_H)}
          class="fill-(--hist-triple)"
        />
        {/* ② 两两重叠：绿+红=黄、蓝+绿=青、红+蓝=紫 */}
        <path
          d={histogramBandPath(bands()?.rg.top ?? [], bands()?.rg.bottom ?? [], VIEW_W, VIEW_H)}
          class={COLOR_FILL_CLASS.yellow}
        />
        <path
          d={histogramBandPath(bands()?.gb.top ?? [], bands()?.gb.bottom ?? [], VIEW_W, VIEW_H)}
          class={COLOR_FILL_CLASS.cyan}
        />
        <path
          d={histogramBandPath(bands()?.rb.top ?? [], bands()?.rb.bottom ?? [], VIEW_W, VIEW_H)}
          class={COLOR_FILL_CLASS.purple}
        />
        {/* ③ 单通道：红 / 绿 / 蓝（各自从中位数画到自己的高度） */}
        <path
          d={histogramBandPath(bands()?.r.top ?? [], bands()?.r.bottom ?? [], VIEW_W, VIEW_H)}
          class={COLOR_FILL_CLASS.red}
        />
        <path
          d={histogramBandPath(bands()?.g.top ?? [], bands()?.g.bottom ?? [], VIEW_W, VIEW_H)}
          class={COLOR_FILL_CLASS.green}
        />
        <path
          d={histogramBandPath(bands()?.b.top ?? [], bands()?.b.bottom ?? [], VIEW_W, VIEW_H)}
          class={COLOR_FILL_CLASS.blue}
        />
      </svg>
    </div>
  );
}
