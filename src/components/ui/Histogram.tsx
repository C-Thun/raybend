/**
 * RGB 直方图：三条完整填充曲线 + 内置通道选择器与亮度指示。
 *
 * 不再把交叠区切成七块硬填色。那套算法在通道交叉的亚像素边界会留下细缝，而且
 * 每次输入变化都要构造 14 条上下边界。现在始终只有三条闭合 path，由浏览器合成：
 * 没有几何缝隙，数据源实时变化时也只更新三条路径。
 */

import { For, createSignal } from "solid-js";
import { t } from "../../i18n/index.ts";
import { histogramPath, type HistogramBars } from "../../lib/histogram.ts";

const VIEW_W = 256;
const VIEW_H = 100;
type Channel = "r" | "g" | "b";

const CHANNELS: readonly Channel[] = ["r", "g", "b"];
const CHANNEL_FILL: Record<Channel, string> = {
  r: "fill-(--label-red)",
  g: "fill-(--label-green)",
  b: "fill-(--label-blue)",
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
        class="relative min-h-0 flex-1 overflow-hidden rounded-t-(--radius)"
        onPointerMove={track}
        onPointerLeave={() => setLevel(null)}
      >
        <div class="pointer-events-none absolute inset-0 z-10" aria-hidden="true">
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
          class="absolute inset-0 h-full w-full"
          viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
          preserveAspectRatio="none"
          role="img"
          aria-label={t("browse.histogramHint")}
        >
          <For each={drawOrder()}>
            {(channel) => (
              <path
                d={histogramPath(values(channel), VIEW_W, VIEW_H)}
                class={["histogram-channel", CHANNEL_FILL[channel]].join(" ")}
                style={{
                  opacity:
                    selected() === null || selected() === channel ? "0.82" : "0.24",
                }}
              />
            )}
          </For>
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
