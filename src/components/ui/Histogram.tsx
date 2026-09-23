/**
 * RGB 直方图：三条完整填充曲线 + 内置通道选择器与亮度指示。
 *
 * 不再把交叠区切成七块硬填色。那套算法在通道交叉的亚像素边界会留下细缝，而且
 * 每次输入变化都要构造 14 条上下边界。现在始终只有三条闭合 path，由浏览器合成：
 * 没有几何缝隙，数据源实时变化时也只更新三条路径。
 */

import { For, Show, createMemo, createSignal } from "solid-js";
import { t } from "../../i18n/index.ts";
import {
  HISTOGRAM_LAYER_ORDER,
  histogramLayers,
  histogramPath,
  type HistogramBars,
  type HistogramLayerKey,
} from "../../lib/histogram.ts";

const VIEW_W = 256;
const VIEW_H = 100;
type Channel = "r" | "g" | "b";

const CHANNELS: readonly Channel[] = ["r", "g", "b"];
const CHANNEL_FILL: Record<Channel, string> = {
  r: "fill-(--label-red)",
  g: "fill-(--label-green)",
  b: "fill-(--label-blue)",
};
/**
 * 7 个区域各自的**固定色**（人类 2026-09-19 定、2026-09-23 重申「是固定色不是自然叠加」）。
 *
 * 两两重叠用的是色标里那一对「加色」的色（红+绿=黄、绿+蓝=青、红+蓝=紫）——
 * 不再另立一套，所以**改色只改令牌**；三色重叠用单独的中间灰 `--hist-triple`。
 */
const LAYER_FILL: Record<HistogramLayerKey, string> = {
  r: "fill-(--label-red)",
  g: "fill-(--label-green)",
  b: "fill-(--label-blue)",
  rg: "fill-(--label-yellow)",
  gb: "fill-(--label-cyan)",
  rb: "fill-(--label-purple)",
  rgb: "fill-(--hist-triple)",
};
const CHANNEL_NAME = {
  r: "browse.histogramRed",
  g: "browse.histogramGreen",
  b: "browse.histogramBlue",
} as const;

export interface HistogramProps {
  bars: HistogramBars | null;
  class?: string;
}

export function Histogram(props: HistogramProps) {
  const [selected, setSelected] = createSignal<Channel | null>(null);
  const [level, setLevel] = createSignal<number | null>(null);

  const drawOrder = (): Channel[] => {
    const active = selected();
    return active === null
      ? [...CHANNELS]
      : [...CHANNELS.filter((channel) => channel !== active), active];
  };
  const values = (channel: Channel): readonly number[] => props.bars?.[channel] ?? [];
  /** 全通道模式的 7 个区域包络（固定色，见 `lib/histogram.ts` 的 `histogramLayers`） */
  const layers = createMemo(() =>
    histogramLayers(values("r"), values("g"), values("b")),
  );

  function track(event: PointerEvent): void {
    const rect = event.currentTarget instanceof Element
      ? event.currentTarget.getBoundingClientRect()
      : null;
    if (rect === null || rect.width <= 0) return;
    const ratio = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
    setLevel(Math.round(ratio * 255));
  }

  return (
    <div
      class={["flex h-44 w-full flex-col rounded-ui bg-surface-bar", props.class ?? ""]
        .filter(Boolean)
        .join(" ")}
      data-histogram={props.bars === null ? "empty" : "lines"}
      data-channel={selected() ?? "all"}
    >
      <div
        data-histogram-plot="on"
        class="relative min-h-0 flex-1 overflow-hidden rounded-t-(--radius)"
        onPointerMove={track}
        onPointerLeave={() => setLevel(null)}
      >
        {/*
          很浅的细密虚线（`DESIGN.md` §13.1）：纵向 4 等分（3 根）+ 横向上下 2 等分（1 根）。
          用 DOM 画而不是 SVG `stroke-dasharray`：SVG 被 `preserveAspectRatio="none"` 拉伸时
          虚线段长会跟着变形（竖线拉长、横线压扁）。

          ⚠️ **必须给 `background-size`**（2026-09-23 人类报「等分线看不到，只有两头 2px 小段」）：
          渐变的 `auto` 尺寸 = 整个元素，那条「1px 亮 + 2px 空」的图案只会画一次，
          后面全是透明 —— `repeat` 无从重复。冒烟量 `backgroundSize` 盯着这一点
          （只数 span 个数是抓不住的）。

          ⚠️ **线条中心必须落在等分点上**（2026-09-24 人类：「左右两格看着比中间的宽」）：
          旧写法 `left: 25%` 是**左边缘**贴 25%，1px 的线占 [25%, 25%+1px]，
          四格于是变成 `25% / 25%−1px / 25%−1px / 25%−1px` —— 左边那格天然宽一格。
          现在用 `-translate-x-1/2` 把中心对到 25/50/75%，四格等宽（冒烟有实测断言）。

          ⚠️ **这层在曲线下面**（2026-09-24 人类：「位置要在峰值图的背景上，不是盖在峰值图上」）：
          它没有 `z-index`，而 SVG 在 DOM 里排在它后面 —— 同层级下后画的上，
          于是虚线只在曲线的透明处露出（背景纹理），不会压在峰上。
          最上面的亮度指示线（`z-20`）不受影响。
        */}
        <div class="pointer-events-none absolute inset-0" aria-hidden="true">
          <For each={[25, 50, 75]}>
            {(percent) => (
              <span
                data-histogram-grid="v"
                class="absolute inset-y-0 w-px -translate-x-1/2 bg-[linear-gradient(to_bottom,var(--hist-grid)_0_1px,transparent_1px_3px)] bg-repeat-y [background-size:100%_3px]"
                style={{ left: `${percent}%` }}
              />
            )}
          </For>
          <span
            data-histogram-grid="h"
            class="absolute inset-x-0 top-1/2 h-px -translate-y-1/2 bg-[linear-gradient(to_right,var(--hist-grid)_0_1px,transparent_1px_3px)] bg-repeat-x [background-size:3px_100%]"
          />
        </div>

        <svg
          class="absolute inset-0 h-full w-full"
          viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
          preserveAspectRatio="none"
          role="img"
          aria-label={t("browse.histogramHint")}
        >
          {/*
            两套逻辑（人类口径，完整说明见 `lib/histogram.ts` 的 `HISTOGRAM_LAYER_ORDER`）：

            * **全通道（默认）**：7 个区域用**固定色** —— 黄/青/紫/灰就是令牌里那个色，
              不靠混合模式；层与层后画盖前画，所以没有接缝。
            * **点亮单通道**：选中那条在前台（正常浓度）、另两条半透明，**自然叠加** ——
              这时不需要也不该再处理混色（需求原文：「此时就不需要处理混色了，自然叠加就行」）。
          */}
          <Show
            when={selected() === null}
            fallback={
              <For each={drawOrder()}>
                {(channel) => (
                  <path
                    d={histogramPath(values(channel), VIEW_W, VIEW_H)}
                    class={["histogram-channel", CHANNEL_FILL[channel]].join(" ")}
                    style={{ opacity: selected() === channel ? "0.82" : "0.24" }}
                  />
                )}
              </For>
            }
          >
            <For each={HISTOGRAM_LAYER_ORDER}>
              {(key) => (
                <path
                  d={histogramPath(layers()[key], VIEW_W, VIEW_H)}
                  class={["histogram-layer", LAYER_FILL[key]].join(" ")}
                />
              )}
            </For>
          </Show>
        </svg>

        <span
          class="pointer-events-none absolute inset-y-0 z-20 w-px bg-brand"
          style={{ left: `${((level() ?? 0) / 255) * 100}%`, opacity: level() === null ? 0 : 0.8 }}
          aria-hidden="true"
        />
      </div>

      <div class="flex h-8 shrink-0 items-center px-1.5">
        <div class="flex items-center gap-1" role="group" aria-label={t("browse.histogramChannels")}>
          <For each={CHANNELS}>
            {(channel) => (
              <button
                type="button"
                aria-pressed={selected() === channel}
                aria-label={t(CHANNEL_NAME[channel])}
                title={t(CHANNEL_NAME[channel])}
                onClick={() => setSelected((current) => (current === channel ? null : channel))}
                class={[
                  "flex h-5 min-w-6 items-center justify-center rounded-ui px-1 text-fs-0 font-semibold transition-colors",
                  selected() === channel
                    ? "bg-state-selected text-fg-1"
                    : "text-fg-3 hover:bg-state-hover hover:text-fg-2",
                ].join(" ")}
              >
                {channel.toUpperCase()}
              </button>
            )}
          </For>
        </div>
        <span class="min-w-0 flex-1" />
        <span class="text-fs-0 text-fg-3 tnum" aria-live="off">
          {level() === null ? "" : t("browse.histogramLevel", { level: level() ?? 0 })}
        </span>
      </div>
    </div>
  );
}
