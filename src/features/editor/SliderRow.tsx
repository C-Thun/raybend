/**
 * `SliderRow` —— 编辑右栏的**参数拉杆**（`DESIGN.md` §14.10，人类 2026-09-23 定）。
 *
 * ```text
 * 曝光                                 +0.35      ← 上行：标签（小一号 / fg-2）居左，当前值（fg-1）居右
 * [-2] ──────●───────────── [+2]                  ← 下行：两端**可选**极值 + **等宽**轨道
 * ```
 *
 * 几条硬规则（都来自人类口述，别自作主张简化）：
 *
 * 1. **标签不与轨道同行**（英文标签铁定放不下）；当前值同行居右；
 * 2. **轨道等宽**：两端为极值预留**固定槽位**（`--spacing-pad-x` 的两倍 ≈ 40px）——
 *    不显示极值时**槽位仍然保留**，否则同一列里的轨道会左右不齐；
 * 3. **双极/单极决定填充方向**（`origin`），**把手位置只由当前值决定**；
 * 4. 未填充段用 `--slider-track`：**反色 + 50% 半透**（实心时会把主色填充线压下去，主次颠倒）。
 *
 * 几何 / 键盘 / 无障碍 / 指针捕获由 Ark UI 的 `Slider` 承载（`origin="center"` 正是
 * Zag 为双极填充提供的开关）；这里只负责我们的排版与配色。
 *
 * ⚠️ 与 `components/ui/Slider.tsx` 的分工：那个是「图标 + 轨道」的**档位**滑块（网格尺寸在用），
 * 不需要标签 / 值 / 极值；本组件是**参数**滑块。两者骨架同源（都是 Ark Slider），
 * 但排版差异太大，硬合成一个会变成一个到处是开关的怪物。
 */

import { Slider as ArkSlider } from "@ark-ui/solid";
import type { JSX } from "solid-js";
import { Show } from "solid-js";

import { formatLimit, formatParamValue, type ParamSpec } from "./params.ts";
import { t } from "../../i18n/index.ts";

export interface SliderRowProps {
  spec: ParamSpec;
  value: number;
  onValueChange: (value: number) => void;
  /** 拖拽结束（W3 用它落库；本波只接不写） */
  onValueCommit?: (value: number) => void;
  disabled?: boolean;
  class?: string;
}

export function SliderRow(props: SliderRowProps): JSX.Element {
  const valueText = (): string => formatParamValue(props.spec, props.value);

  return (
    <ArkSlider.Root
      value={[props.value]}
      min={props.spec.min}
      max={props.spec.max}
      step={props.spec.step}
      // 双极参数的填充从正中长出（Zag 的 `origin`），单极从左端长
      origin={props.spec.origin === "center" ? "center" : "start"}
      disabled={props.disabled ?? false}
      onValueChange={(details) => {
        const next = details.value[0];
        if (typeof next === "number" && next !== props.value) props.onValueChange(next);
      }}
      onValueChangeEnd={(details) => {
        const next = details.value[0];
        if (typeof next === "number") props.onValueCommit?.(next);
      }}
      class={["flex min-w-0 flex-col gap-0.5 select-none", props.class ?? ""]
        .filter(Boolean)
        .join(" ")}
    >
      {/* 上行：标签 + 当前值 */}
      <div class="flex items-baseline gap-2">
        <ArkSlider.Label class="min-w-0 truncate text-fs-0 text-fg-2">
          {t(props.spec.labelKey)}
        </ArkSlider.Label>
        <span class="flex-1" />
        <ArkSlider.ValueText class="shrink-0 text-fs-1 tabular-nums text-fg-1">
          {valueText()}
        </ArkSlider.ValueText>
      </div>

      {/* 下行：极值槽位（恒定）+ 轨道 */}
      <div class="flex items-center gap-gap">
        <LimitSlot text={props.spec.limits ? formatLimit(props.spec, "min") : null} side="min" />
        <ArkSlider.Control class="relative flex min-w-0 flex-1 items-center">
          <ArkSlider.Track class="relative h-1 w-full rounded-full bg-slider-track">
            <ArkSlider.Range class="absolute h-full rounded-full bg-brand" />
          </ArkSlider.Track>
          {/*
            Thumb 必须带自己的尺寸类：Ark 只给位置（`--slider-thumb-offset-*`），
            不给大小 —— 不给尺寸会退化成一个 0×0 的点，看起来「把手不见了」。
          */}
          <ArkSlider.Thumb
            index={0}
            class={[
              "block size-3.5 rounded-full bg-brand shadow-ui outline-none",
              "transition-transform hover:scale-110 focus-visible:scale-110",
              "data-[dragging]:scale-110 data-[dragging]:ring-2 data-[dragging]:ring-brand/40",
            ].join(" ")}
          >
            <ArkSlider.HiddenInput aria-label={t(props.spec.labelKey)} />
          </ArkSlider.Thumb>
        </ArkSlider.Control>
        <LimitSlot text={props.spec.limits ? formatLimit(props.spec, "max") : null} side="max" />
      </div>
    </ArkSlider.Root>
  );
}

/**
 * 极值槽位：**宽度恒定**（40px），有没有文字都占着。
 *
 * 这就是「轨道等宽」的实现方式 —— 不给极值的时候若把槽位也去掉，
 * 上下两根拉杆的轨道就会左右不齐（人类 2026-09-23 特意点过这一条）。
 */
function LimitSlot(props: { text: string | null; side: "min" | "max" }): JSX.Element {
  return (
    <span
      class={[
        "w-10 shrink-0 text-fs-0 tabular-nums text-fg-3",
        props.side === "min" ? "text-right" : "text-left",
        props.text === null ? "opacity-0" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      aria-hidden={props.text === null ? "true" : undefined}
    >
      <Show when={props.text !== null} fallback={"0"}>
        {props.text}
      </Show>
    </span>
  );
}
