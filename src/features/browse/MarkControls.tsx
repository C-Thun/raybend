/**
 * 标记 / 筛选的**控件排**（唯一一份）。
 *
 * 浏览工具条与导出工具条上的「星 / 色标 / 赞踩 / 旗标筛选」长得一样、行为也一样，
 * 差别只在**调用方传给它们的取值回调**：
 *
 * | 场景 | 取值 |
 * | --- | --- |
 * | 浏览 · 标记态 | 选中照片的标记（三态，含「混合」） |
 * | 浏览 · 筛选态 | 当前筛选条件 |
 * | 导出（恒筛选） | 当前筛选条件 |
 *
 * 所以这里只放**展示与命中**，语义全在回调里（`AGENTS.md` §2.12：同一个能力只允许一套实现）。
 * 控件排的图标是**纯图标**，悬停提示一律走 `ButtonTip`（浮层底 + 11px、默认朝下、禁用态可弹）。
 */

import { For, Show, type JSX } from "solid-js";
import {
  IconFlag,
  IconFlagFilled,
  IconStar,
  IconStarFilled,
  IconThumbDownFilled,
  IconThumbUpFilled,
} from "@tabler/icons-solidjs";

import { ToggleBlock } from "../../components/ui/ToggleBlock.tsx";
import { ButtonTip } from "../../components/ui/Tooltip.tsx";
import { COLOR_DOT_CLASS, isColorLabel } from "../../lib/color-labels.ts";
import { COLOR_VALUES } from "../../lib/marking-state.ts";
import { t } from "../../i18n/index.ts";
import { colorText } from "./labels.ts";

/** 混合态的视觉：半亮 + 一个短横。「混合」绝不能被当成「无值」（见 BrowseToolbar 的文件头）。 */
export function MixedMark(): JSX.Element {
  return (
    <span class="text-fg-2 opacity-70" aria-hidden="true">
      —
    </span>
  );
}

/** 星级排：`active(star)` 决定第 n 颗是否点亮（标记态＝有这么多星；筛选态＝阈值 ≥ n）。 */
export function RatingRow(props: {
  active: (star: number) => boolean;
  mixed: boolean;
  disabled?: boolean;
  /** 悬停提示（筛选态一般写「≥N 星」） */
  tip: (star: number) => string;
  /** 无障碍名；不给就用 `tip`。**两种模式下都保持「N 星」**，提示才按模式变 */
  label?: (star: number) => string;
  onPick: (star: number) => void;
}): JSX.Element {
  const name = (star: number): string => (props.label ?? props.tip)(star);
  return (
    <div class="flex items-center">
      <Show when={props.mixed}>
        <MixedMark />
      </Show>
      <For each={[1, 2, 3, 4, 5]}>
        {(star) => (
          <ButtonTip content={props.tip(star)}>
            {(triggerProps) => (
              <button
                {...triggerProps}
                type="button"
                disabled={props.disabled}
                aria-label={name(star)}
                onClick={() => props.onPick(star)}
                class={[
                  "rounded-(--radius) p-0.5 hover:bg-state-hover",
                  props.disabled ? "opacity-40" : "",
                ].join(" ")}
              >
                <span
                  class={
                    props.mixed
                      ? "text-fg-2 opacity-70"
                      : props.active(star)
                        ? "text-brand"
                        : "text-fg-3"
                  }
                >
                  {props.active(star) ? <IconStarFilled size={14} /> : <IconStar size={14} />}
                </span>
              </button>
            )}
          </ButtonTip>
        )}
      </For>
    </div>
  );
}

/**
 * 色标排：5 个实心点 + 1 个空心圈（无色）。
 *
 * 选中态＝**两层环**（人类 2026-09-19）：外圈固定主色，内圈取所在面的颜色，
 * 让主色与色标本色之间隔开一圈。代价是色标看起来小一点，这是**预期**的。
 */
export function ColorRow(props: {
  selected: (color: string | null) => boolean;
  mixed: boolean;
  disabled?: boolean;
  onPick: (color: string | null) => void;
}): JSX.Element {
  return (
    <div class="flex items-center gap-1">
      <Show when={props.mixed}>
        <MixedMark />
      </Show>
      <For each={COLOR_VALUES}>
        {(color) => {
          const active = () => props.selected(color);
          return (
            <ButtonTip content={colorText(color)}>
              {(triggerProps) => (
                <button
                  {...triggerProps}
                  type="button"
                  disabled={props.disabled}
                  aria-label={colorText(color)}
                  onClick={() => props.onPick(color)}
                  class={[
                    "h-3.5 w-3.5 rounded-full",
                    /*
                     * 有颜色的圆点**只要颜色**，不加描边（人类 2026-09-19：降低框线感）；
                     * 但「取消标色」那个空圆必须留一圈 —— 否则它在面色上根本看不见。
                     */
                    color === null
                      ? "ring-1 ring-line-2 bg-transparent"
                      : (isColorLabel(color) ? COLOR_DOT_CLASS[color] : ""),
                    active()
                      ? "ring-2 ring-brand inset-ring-1 inset-ring-(--label-ring-inset)"
                      : "",
                    props.disabled ? "opacity-40" : "",
                  ].join(" ")}
                />
              )}
            </ButtonTip>
          );
        }}
      </For>
    </div>
  );
}

/** 赞 / 踩（三态互斥）排。 */
export function LikeRow(props: {
  pressed: (value: "like" | "dislike") => boolean;
  mixed: boolean;
  disabled?: boolean;
  onPick: (value: "like" | "dislike") => void;
}): JSX.Element {
  return (
    <div class="flex items-center gap-0.5">
      <Show when={props.mixed}>
        <MixedMark />
      </Show>
      <ToggleBlock
        pressed={props.pressed("like")}
        disabled={props.disabled}
        onPressedChange={() => props.onPick("like")}
        icon={<IconThumbUpFilled size={16} />}
        label={t("browse.like")}
      />
      <ToggleBlock
        pressed={props.pressed("dislike")}
        disabled={props.disabled}
        onPressedChange={() => props.onPick("dislike")}
        icon={<IconThumbDownFilled size={16} />}
        label={t("browse.dislike")}
      />
    </div>
  );
}

/**
 * 旗标**筛选**对（「有旗标」实心旗 / 「无旗标」空心旗，再点一次取消条件）。
 *
 * 只在筛选语义下出现：标记态的旗标是「一个开关 + 清空」（那套在 `BrowseToolbar` 里）。
 */
export function FlagFilterPair(props: {
  mode: "pick" | "reject" | "none" | null;
  onPick: (mode: "pick" | "none" | null) => void;
}): JSX.Element {
  return (
    <div class="flex items-center gap-0.5">
      <ToggleBlock
        pressed={props.mode === "pick"}
        onPressedChange={() => props.onPick(props.mode === "pick" ? null : "pick")}
        icon={<IconFlagFilled size={14} />}
        label={t("browse.flagWith")}
      />
      <ToggleBlock
        pressed={props.mode === "none"}
        onPressedChange={() => props.onPick(props.mode === "none" ? null : "none")}
        icon={<IconFlag size={14} />}
        label={t("browse.flagWithout")}
      />
    </div>
  );
}
