/**
 * **一遍流光**（`styles/motion.css` 的 `.rb-shimmer-mask`）—— 全项目唯一一份。
 *
 * ```text
 *  开始导出   →   开[亮]始导出   →   开[亮]始导出       光带在字形上扫过一遍
 * ```
 *
 * 实现不是「盖一条半透明渐变条」（人类 2026-09-23 明确否掉：那是给文字加个方块背景），
 * 而是**同一份内容画两遍**，上面那遍用移动的 `mask-image` 裁成一条光带 ——
 * 变亮的是字与图标自己的笔画。
 *
 * # 两个用法（都走这里）
 *
 * * **状态指示**（导出 / 工作流 chip 的「正在处理」）：`active` 常真，循环走；
 * * **一次性强调**（编辑右栏的层名条切换层）：`once` + 自己控一个 ~900ms 的 `active`。
 *
 * # 参数
 *
 * | 参数 | 作用 |
 * | --- | --- |
 * | `active` | 正在走光带（关掉就是普通文字，DOM 也只留一层） |
 * | `once` | **只走一遍**（不是循环的状态指示）；`prefers-reduced-motion` 下整层去掉 |
 * | `dimBase` | 底层内容压到 50%（导出/工作流那种「停住处理中」的观感用它） |
 * | `highlight` | 上层（被光带扫过的那份）的额外类：想让高光更亮就传 `text-fg-1` |
 *
 * `class` / `highlight` 都只加在**文字所在的那一层**，外层宽度由调用方给足。
 */

import { Show, untrack, type JSX } from "solid-js";

export interface ShimmerTextProps {
  /** 正在走光带 */
  active: boolean;
  /** 只走一遍（一次性强调）；默认是循环的状态指示 */
  once?: boolean;
  /** 底层内容压暗（导出 / 工作流「处理中」的观感） */
  dimBase?: boolean;
  /** 高光那层的额外类（例如 `text-fg-1` 让扫过的字更亮） */
  highlight?: string;
  /** 外层类（宽度、对齐、渐隐都由调用方给） */
  class?: string;
  children: JSX.Element;
}

export function ShimmerText(props: ShimmerTextProps): JSX.Element {
  return (
    <span class={["relative grid overflow-hidden", props.class ?? ""].filter(Boolean).join(" ")}>
      <span
        class="col-start-1 row-start-1 flex min-w-0 items-center"
        classList={{ "opacity-50": props.dimBase === true && props.active }}
      >
        {untrack(() => props.children)}
      </span>
      <Show when={props.active}>
        <span
          aria-hidden="true"
          class={[
            "rb-shimmer-mask pointer-events-none col-start-1 row-start-1 flex min-w-0 items-center",
            props.once === true ? "rb-shimmer-once" : "",
            props.highlight ?? "",
          ]
            .filter(Boolean)
            .join(" ")}
        >
          {untrack(() => props.children)}
        </span>
      </Show>
    </span>
  );
}
