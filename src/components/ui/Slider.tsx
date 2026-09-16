/**
 * `Slider` —— 连续 / 定档滑块（`DESIGN.md` §10.1；照片网格的缩放档位用它）。
 *
 * 由 Ark UI 的 `Slider` 承载：它给了键盘（方向键 / Home / End / PageUp）、
 * `aria-valuenow`、拖拽时的指针捕获与触摸支持 —— 这些手写一遍不值得。
 * 这里只负责**我们的视觉**：
 *
 *   - 轨道 `$surface-track`（凹槽感），已选区间用品牌主色；
 *   - 滑块 `16px` 圆点，指向放大、拖拽时加一圈主色外环（`DESIGN.md` §5）；
 *   - **档位离散**：`step = 1` + `min/max` 覆盖档位下标，
 *     这样「9 档」是滑块自己的行为，不需要调用方自己吸附。
 *
 * 数值本身不进组件：调用方拿 `onValueChange`，并负责把下标映射成实际尺寸
 * （`lib/tile-flow.ts` 的 `tileSizeAt`）。
 */

import { Slider as ArkSlider } from "@ark-ui/solid";
import type { JSX } from "solid-js";

export interface SliderProps {
  /** 当前值（受控）。整数档位时请传档位下标 */
  value: number;
  min: number;
  max: number;
  /** 步长（默认 1 —— 档位滑块就是要离散） */
  step?: number;
  onValueChange: (value: number) => void;
  /**
   * **拖拽结束**时给一次（键盘调整也会给）。
   *
   * 用来做「只在结束时落盘/做重活」——拖动过程中每动一格都写一次设置，
   * 一拖就是几百次 IPC + 几百次数据库写（2026-09-16 实测「尺寸调节非常卡」的原因之一）。
   */
  onValueCommit?: (value: number) => void;
  /** 无障碍名（如「照片大小」） */
  label: string;
  disabled?: boolean;
  /** 左右两端的图标（设计稿：图标 + 滑块） */
  startIcon?: JSX.Element;
  endIcon?: JSX.Element;
  class?: string;
}

export function Slider(props: SliderProps) {
  const step = () => props.step ?? 1;

  return (
    <ArkSlider.Root
      value={[props.value]}
      min={props.min}
      max={props.max}
      step={step()}
      disabled={props.disabled ?? false}
      // Ark 给的是数组（支持区间选择），我们只要单值：取第一个
      onValueChange={(details) => {
        const next = details.value[0];
        if (typeof next === "number" && next !== props.value) {
          props.onValueChange(next);
        }
      }}
      onValueChangeEnd={(details) => {
        const next = details.value[0];
        if (typeof next === "number") props.onValueCommit?.(next);
      }}
      class={[
        "flex min-w-0 items-center gap-1.5 select-none",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {/* 两端图标（纯装饰，点它们不改变值） */}
      {props.startIcon ? (
        <span class="shrink-0 text-fg-3" aria-hidden="true">
          {props.startIcon}
        </span>
      ) : null}

      <ArkSlider.Control class="flex min-w-0 flex-1 items-center">
        <ArkSlider.Track class="relative h-1 w-full rounded-full bg-surface-track">
          <ArkSlider.Range class="absolute h-full rounded-full bg-brand" />
        </ArkSlider.Track>
        {/*
          Thumb 必须**带自己的尺寸类**：Ark 只给位置（`--percent` 之类），
          不给大小；不给尺寸就会退化成一个 0×0 的点，看起来「滑块不见了」。
        */}
        <ArkSlider.Thumb
          index={0}
          class={[
            "block size-4 rounded-full bg-brand shadow-ui outline-none",
            "transition-transform",
            "hover:scale-110 focus-visible:scale-110",
            "data-[dragging]:scale-110 data-[dragging]:ring-2 data-[dragging]:ring-brand/40",
          ].join(" ")}
        >
          <ArkSlider.HiddenInput aria-label={props.label} />
        </ArkSlider.Thumb>
      </ArkSlider.Control>

      {props.endIcon ? (
        <span class="shrink-0 text-fg-3" aria-hidden="true">
          {props.endIcon}
        </span>
      ) : null}
    </ArkSlider.Root>
  );
}
