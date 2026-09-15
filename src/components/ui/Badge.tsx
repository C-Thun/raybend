/**
 * Badge / CountBadge（DESIGN.md §10.1 #16）。
 *
 * 计数与状态小标签。两种形态共用一个组件：
 *   - 计数：`CountBadge`（数字，**必须** tabular-nums —— 一排计数上下跳动很难看，§7.2）
 *   - 状态：`Badge`（文字标签，如「已排除」）
 *
 * 状态：默认 / 选中的宿主里（不做特殊处理，用 `tone` 表意）。
 * 禁用态不适用于它 —— 它只是标签，不是控件；要表达「不可用」请由宿主降前景。
 *
 * 颜色只用语义工具类：品牌底用 `fg-on-brand` 前景（§4.5 规定品牌色块上统一用深字）。
 */

import type { JSX } from "solid-js";
import { Show, splitProps } from "solid-js";
import { formatCount, formatCountCapped } from "../../lib/format.ts";
import type { GroupingLocale } from "../../lib/format.ts";

export type BadgeTone = "neutral" | "brand" | "accent";

const TONE_CLASSES: Record<BadgeTone, string> = {
  /** 中性：坐在凹槽面上，用于「1 248 张」这类次要计数 */
  neutral: "bg-surface-track text-fg-2",
  /** 品牌：当前生效的状态 */
  brand: "bg-brand text-fg-on-brand",
  /** 辅色：点缀性标记（§1.3：辅色不承担主信息） */
  accent: "bg-brand-2 text-fg-on-brand",
};

export interface BadgeProps {
  children?: JSX.Element;
  tone?: BadgeTone;
  /** 前置图标（Tabler 图标渲染结果） */
  icon?: JSX.Element;
  class?: string;
}

export function Badge(props: BadgeProps) {
  const [local, rest] = splitProps(props, [
    "tone",
    "icon",
    "class",
    "children",
  ]);

  return (
    <span
      {...rest}
      class={[
        "inline-flex shrink-0 items-center gap-1 rounded-full px-1.5 py-px text-fs-1 leading-4 font-medium",
        TONE_CLASSES[local.tone ?? "neutral"],
        local.class ?? "",
      ].join(" ")}
    >
      <Show when={local.icon}>
        <span
          class="flex size-3 items-center justify-center"
          aria-hidden="true"
        >
          {local.icon}
        </span>
      </Show>
      {local.children}
    </span>
  );
}

export interface CountBadgeProps {
  count: number;
  /**
   * 封顶值。超过则显示 `999+` —— 小胶囊放不下长数字，会把旁边的图标挤走。
   * 省略则不封顶。
   */
  max?: number;
  locale?: GroupingLocale;
  tone?: BadgeTone;
  icon?: JSX.Element;
  class?: string;
  /** 无障碍名（如「1248 张照片」）。省略时屏幕阅读器只会念出数字 */
  label?: string;
}

export function CountBadge(props: CountBadgeProps) {
  const [local, rest] = splitProps(props, [
    "count",
    "max",
    "locale",
    "tone",
    "icon",
    "class",
    "label",
  ]);

  const text = () => {
    const locale = local.locale ?? "zh-CN";
    return local.max === undefined
      ? formatCount(local.count, locale)
      : formatCountCapped(local.count, local.max, locale);
  };

  return (
    <Badge
      {...rest}
      tone={local.tone}
      icon={local.icon}
      class={["tnum", local.class ?? ""].join(" ")}
    >
      <span
        aria-label={local.label}
        aria-hidden={local.label ? undefined : "true"}
      >
        {text()}
      </span>
    </Badge>
  );
}
