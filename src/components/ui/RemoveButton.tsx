/**
 * RemoveButton —— 「移除 / 排除」按钮（DESIGN.md §12.2、AGENTS.md §11.3）。
 *
 * 图标是**禁行标志 = 填充圆 + 中间一条横杠**，不是叉号 ——
 * 因为移除/排除**不是破坏性删除**，用叉号会让人以为文件要没了。
 *
 * 命中区不小于 `--remove-hit`（紧凑 22px / 宽松 26px，§8.3）：
 * 图标可以小，但**点击区必须是舒服的尺寸**（用户明确反馈过原稿 16×16 太小）。
 *
 * 状态：
 *   默认     注释色图标，无底
 *   指向     辅色底 + `fg-1` 图标（§5）
 *   按下     主色底
 *   禁用     前景降次级，不响应
 *
 * 语义红线：这个按钮**只从集合里拿掉**（不动磁盘）。真正删文件第一阶段不做，
 * 所以文案只用「移除 / 排除」，不要出现「删除」字样。
 */

import { splitProps, type JSX } from "solid-js";

export interface RemoveGlyphProps {
  /** 图标本身的大小（px）。默认走密度令牌 */
  size?: number | string;
  class?: string;
}

/**
 * 禁行标志：填充圆 + 中间一条**真正镂空**的横杠。
 *
 * 用 `fill-rule="evenodd"` 把横杠挖成洞，而不是在圆上盖一条背景色的杠 ——
 * 后者在选中底（主色低透明度）或 hover 底（辅色底）上会露出错误颜色。
 */
export function RemoveGlyph(props: RemoveGlyphProps) {
  const [local, rest] = splitProps(props, ["size", "class"]);

  return (
    <svg
      {...rest}
      viewBox="0 0 16 16"
      width={local.size ?? "var(--remove-icon)"}
      height={local.size ?? "var(--remove-icon)"}
      fill="currentColor"
      aria-hidden="true"
      class={local.class}
    >
      {/*
        外圈：半径 7 的圆；内圈：横杠挖空（evenodd 下重叠区域成为洞）。
        横杠 y 从 7.2 到 8.8 → 高 1.6，与圆的直径 14 成约 1:8.75，视觉上不显粗。
      */}
      <path
        fill-rule="evenodd"
        clip-rule="evenodd"
        d="M8 1a7 7 0 1 0 0 14A7 7 0 0 0 8 1Zm0 6.2H3.6v1.6H12.4V7.2H8Z"
      />
    </svg>
  );
}

export interface RemoveButtonProps
  extends Omit<JSX.ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  /** 无障碍名。默认「移除」（走 i18n），排除场景传入「排除」 */
  label: string;
  size?: number | string;
}

export function RemoveButton(props: RemoveButtonProps) {
  const [local, rest] = splitProps(props, [
    "label",
    "size",
    "class",
    "disabled",
  ]);

  return (
    <button
      {...rest}
      type={rest.type ?? "button"}
      disabled={local.disabled}
      aria-label={local.label}
      title={rest.title ?? local.label}
      class={[
        "inline-flex shrink-0 cursor-pointer items-center justify-center rounded-ui transition-colors",
        // 点击区靠令牌撑到 22×26，图标本身可以更小
        "size-remove-hit text-fg-3",
        "hover:bg-state-hover hover:text-fg-1 active:bg-state-active",
        local.disabled
          ? "pointer-events-none text-fg-3 opacity-60"
          : "cursor-pointer",
        local.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <RemoveGlyph size={local.size} />
    </button>
  );
}
