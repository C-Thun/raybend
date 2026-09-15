/**
 * ToggleBlock —— 可按下按钮（DESIGN.md §12.8）。
 *
 * 用于「按下后切换某个模式」的按钮：**吸附**、**按时间** 等。
 *
 * 状态：
 *   未按下   无底色（与宿主面同色，即「平的」）+ 次级色内容
 *   指向     辅色底（§5）
 *   已按下   **主色实色底** + `fg-on-brand`
 *   已按下+指向  保持主色底（不回落成辅色底）
 *   禁用     前景降次级色，面不变
 *
 * 与 `Switch` 的区别（文档里明确写死，免得后面做混）：
 *   `Switch`      = 左右滑动的圆点开关，表达「某个**属性**开 / 关」（如「包含子目录」）
 *   `ToggleBlock` = 方块按下，表达「切换到某个**模式**」（如「按时间」「吸附」）
 */

import { Show, splitProps, type JSX } from "solid-js";

export interface ToggleBlockProps
  extends Omit<JSX.ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  pressed: boolean;
  onPressedChange?: (pressed: boolean) => void;
  /** 图标（Tabler 图标渲染结果） */
  icon?: JSX.Element;
  /** 文字标签。省略则退化为纯图标方块 —— 那时必须给 `label` 作无障碍名 */
  children?: JSX.Element;
  label?: string;
}

export function ToggleBlock(props: ToggleBlockProps) {
  const [local, rest] = splitProps(props, [
    "pressed",
    "onPressedChange",
    "icon",
    "children",
    "label",
    "class",
    "disabled",
  ]);

  return (
    <button
      {...rest}
      type={rest.type ?? "button"}
      disabled={local.disabled}
      aria-pressed={local.pressed}
      aria-label={local.label}
      title={rest.title ?? local.label}
      onClick={() => local.onPressedChange?.(!local.pressed)}
      class={[
        "inline-flex shrink-0 cursor-pointer select-none items-center justify-center gap-1.5 rounded-ui transition-colors",
        // 方块尺寸随密度档变化，图标尺寸也跟着档位但**不随字号**（§8.1）
        "min-h-toggle-block min-w-toggle-block px-1.5",
        local.pressed
          ? "bg-brand text-fg-on-brand hover:bg-brand"
          : "text-fg-2 hover:bg-state-hover hover:text-fg-1",
        local.disabled
          ? "pointer-events-none text-fg-3 opacity-60"
          : "cursor-pointer",
        local.class ?? "",
      ].join(" ")}
    >
      <Show when={local.icon}>
        <span
          class="flex items-center justify-center"
          style={{
            width: "var(--toggle-block-icon)",
            height: "var(--toggle-block-icon)",
          }}
          aria-hidden="true"
        >
          {local.icon}
        </span>
      </Show>
      <Show when={local.children}>
        <span class="text-fs-2 whitespace-nowrap">{local.children}</span>
      </Show>
    </button>
  );
}
