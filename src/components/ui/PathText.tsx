/**
 * PathText —— 路径缩写显示（DESIGN.md §10.1 #10、§12.3）。
 *
 * 显示的是 `shortpath`（盘符 + 中间各级首字母 + 末级全名），
 * **完整路径通过 `Tooltip` 在悬停时给出** —— 缩写是为了省横向空间，
 * 但用户需要能随时确认自己指哪一条。
 *
 * 状态：
 *   默认      次级文字色，单行截断
 *   指向      **辅色底**（§5）+ 弹出完整路径的气泡
 *   选中宿主  宿主给 `selected` 时提到 `fg-1`
 *
 * 长度上限由调用方传（`maxLength`）：`已选目录` 横条给得宽，
 * `Recent` 列表给得窄，同一份算法两种长度。
 */

import { Show, splitProps, type JSX } from "solid-js";
import { shortPath } from "../../lib/shortpath.ts";
import { Tooltip } from "./Tooltip.tsx";

export interface PathTextProps {
  /** 完整路径 */
  path: string;
  /**
   * 缩写后的长度上限（字符数）。
   * 省略时不限制长度，只做首字母缩写。
   */
  maxLength?: number;
  /**
   * 是否显示完整路径而不是缩写。
   * `已选目录` 横条空间充足，用的是完整路径（`design/main.md` §7 待决项 9）。
   */
  full?: boolean;
  /** 宿主处于选中态时把文字提到 `fg-1` */
  selected?: boolean;
  /** 前置图标（如文件夹图标） */
  icon?: JSX.Element;
  class?: string;
}

export function PathText(props: PathTextProps) {
  const [local, rest] = splitProps(props, [
    "path",
    "maxLength",
    "full",
    "selected",
    "icon",
    "class",
  ]);

  const display = () =>
    local.full
      ? local.path
      : shortPath(
          local.path,
          local.maxLength === undefined ? {} : { maxLength: local.maxLength },
        );

  const isAbbreviated = () => display() !== local.path;

  return (
    <Tooltip
      content={local.path}
      placement="top"
      // 路径本身就是它的说明，没必要为同一段文字弹两次
      disabled={!local.path}
    >
      {(triggerProps) => (
        <span
          {...triggerProps()}
          {...rest}
          class={[
            "flex min-w-0 items-center gap-1.5 rounded-ui px-0.5",
            "transition-colors hover:bg-state-hover",
            local.selected ? "text-fg-1" : "text-fg-2",
            local.class ?? "",
          ]
            .filter(Boolean)
            .join(" ")}
        >
          <Show when={local.icon}>
            <span
              class="flex size-4 shrink-0 items-center justify-center"
              aria-hidden="true"
            >
              {local.icon}
            </span>
          </Show>
          {/*
            缩写过的路径用 `dir=ltr` 固定方向：路径永远是 LTR 的，
            在 RTL 环境里不该被翻转（`unicode-bidi: plaintext` 会按首字符猜方向）。
          */}
          <span
            dir="ltr"
            class="min-w-0 flex-1 truncate text-fs-2"
            title={isAbbreviated() ? undefined : local.path}
          >
            {display()}
          </span>
        </span>
      )}
    </Tooltip>
  );
}
