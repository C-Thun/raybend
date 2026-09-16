/**
 * Button / IconButton（DESIGN.md §10.1 #1、#2）。
 *
 * 状态与全局反馈规则（DESIGN.md §5）：
 *   默认   → 无底
 *   指向   → **辅色底**（`bg-state-hover`）
 *   点击后 → **主色底**（`bg-state-selected` / 主按钮用实色 `bg-brand`）
 *   选中时指向 → 保持主色底，**不回落**成辅色底
 *   禁用   → 前景降到次级色，**面不变**
 *   加载中 → 保持布局尺寸，内容换成指示器（避免按钮跳动）
 *
 * 危险（danger）变体**暂不实现** —— 它需要一个 `--danger` 令牌，而
 * 「窗口关闭键要不要红」还是 design/main.md §7 的待决项。在令牌定案前
 * 实现它必然要硬编码色值，违反 DESIGN.md §9.1。
 */

import type { JSX } from "solid-js";
import { Show, splitProps } from "solid-js";

export type ButtonVariant = "primary" | "secondary" | "ghost";
export type ButtonSize = "sm" | "md";

export interface ButtonProps
  extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** 选中态（如 toggle 按钮处于开启） */
  selected?: boolean;
  loading?: boolean;
  /** 前置图标（一般是 Tabler 图标组件渲染结果） */
  icon?: JSX.Element;
}

/** 变体 → 态色的对应表。集中在这里，方便对照 DESIGN.md §5 复核。 */
function variantClasses(variant: ButtonVariant, selected: boolean): string {
  if (selected) {
    // 选中：主色实色底（`fg-on-brand` 是唯一在品牌色上可读的前景色）
    return "bg-brand text-fg-on-brand hover:bg-brand";
  }
  switch (variant) {
    case "primary":
      return "bg-brand text-fg-on-brand hover:bg-brand";
    case "secondary":
      return "bg-surface-track text-fg-1 hover:bg-state-hover";
    case "ghost":
    default:
      return "text-fg-2 hover:bg-state-hover hover:text-fg-1";
  }
}

/*
 * 尺寸：**高度显式写死**，并配 `leading-none`。
 *
 * 为什么不留 `py-*` 让内容撑高（2026-09-16 人类反馈「按钮里的文字明显往上偏」）：
 * 行高是继承来的（1.5 那种），行盒比字形盒高，flex 居中的是**行盒** ——
 * 拉丁字体的 ascent / descent 不对称，于是文字看上去就偏上。
 * 把行高钉成 `1` + 高度写死：居中的就是字形盒本身，位置可预期、不随字重漂移。
 */
const SIZE_CLASSES: Record<ButtonSize, string> = {
  sm: "h-6 gap-1 px-2 text-fs-1 leading-none",
  md: "h-7 gap-1.5 px-3 text-fs-2 leading-none",
};

export function Button(props: ButtonProps) {
  const [local, rest] = splitProps(props, [
    "variant",
    "size",
    "selected",
    "loading",
    "icon",
    "class",
    "children",
    "disabled",
  ]);

  const variant = () => local.variant ?? "ghost";
  const isDisabled = () => local.disabled || local.loading;

  return (
    <button
      {...rest}
      type={rest.type ?? "button"}
      disabled={isDisabled()}
      aria-busy={local.loading || undefined}
      aria-pressed={local.selected || undefined}
      class={[
        "relative inline-flex select-none items-center justify-center rounded-ui font-medium transition-colors",
        SIZE_CLASSES[local.size ?? "md"],
        variantClasses(variant(), Boolean(local.selected)),
        // 禁用态只降前景、不动面（DESIGN.md §5）
        isDisabled()
          ? "pointer-events-none text-fg-3 opacity-60"
          : "cursor-pointer",
        local.class ?? "",
      ].join(" ")}
    >
      {/*
        加载中仍然渲染图标/文字但置为透明 —— 这样按钮宽度不会在加载时跳变。
        禁用态优先级高于 loading，所以这里用 loading 单独判断。
      */}
      <span
        class={
          local.loading ? "invisible flex items-center gap-1.5" : "contents"
        }
      >
        <Show when={local.icon}>
          <span class="flex size-3.5 items-center justify-center">
            {local.icon}
          </span>
        </Show>
        {local.children}
      </span>
      <Show when={local.loading}>
        <span
          class="absolute size-3 animate-spin rounded-full border border-current border-t-transparent"
          aria-hidden="true"
        />
      </Show>
    </button>
  );
}

/* ══════════════════════════════════════════════════════════════
 * IconButton：纯图标按钮。方/圆两种形态。
 * ══════════════════════════════════════════════════════════════ */

export type IconButtonShape = "square" | "circle";

export interface IconButtonProps
  extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  shape?: IconButtonShape;
  selected?: boolean;
  /** 无障碍名。**必填** —— 纯图标按钮没有可读文本，缺了它对屏幕阅读器就是空白。 */
  label: string;
}

export function IconButton(props: IconButtonProps) {
  const [local, rest] = splitProps(props, [
    "shape",
    "selected",
    "label",
    "class",
    "children",
    "disabled",
  ]);

  return (
    <button
      {...rest}
      type={rest.type ?? "button"}
      disabled={local.disabled}
      title={rest.title ?? local.label}
      aria-label={local.label}
      aria-pressed={local.selected || undefined}
      class={[
        "inline-flex shrink-0 select-none items-center justify-center transition-colors",
        (local.shape ?? "square") === "circle" ? "rounded-full" : "rounded-ui",
        "size-6",
        local.selected
          ? "bg-brand text-fg-on-brand hover:bg-brand"
          : "text-fg-2 hover:bg-state-hover hover:text-fg-1",
        local.disabled
          ? "pointer-events-none text-fg-3 opacity-60"
          : "cursor-pointer",
        local.class ?? "",
      ].join(" ")}
    >
      {local.children}
    </button>
  );
}
