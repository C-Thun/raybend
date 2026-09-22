/**
 * 编辑右栏与左栏的**小零件**（都是「一句话 / 一个开关」那种，不值得各开一个文件）。
 *
 * * `PendingNote` —— 本波还没接线的地方**明说**，不假装能用（W1 的诚实边界）；
 * * `PanelTitle` —— 面板小标题（沿用 `DESIGN.md` §13 的面板口径）。
 */

import { IconClockHour4 } from "@tabler/icons-solidjs";
import { Show, type JSX } from "solid-js";

export interface PendingNoteProps {
  /** 已是译好的文案 */
  text: string;
  /** 前面那颗小时钟图标（默认显示） */
  icon?: boolean;
  class?: string;
}

/**
 * 「这一块还没接线」的说明条。
 *
 * 为什么要有它（而不是留白或塞假数据）：W1 是**界面骨架先立**的一波，
 * 右栏有一半控件要等 W2/W3 才有真实效果。把话说在界面上，比让用户以为「拖了没反应是坏了」好；
 * 也让人类评审时一眼分清「画错了」与「还没接」。
 *
 * 视觉：**不成卡、不加边框**（`DESIGN.md` §12.10 的水印口径），
 * 只用次级前景色 + 一颗小时钟 —— 它不该抢面板里真正控件的注意力。
 */
export function PendingNote(props: PendingNoteProps): JSX.Element {
  return (
    <p
      data-pending-note
      class={[
        "flex items-start gap-1.5 text-fs-0 leading-snug text-fg-3",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <Show when={props.icon !== false}>
        <IconClockHour4 size={12} class="mt-px shrink-0" aria-hidden="true" />
      </Show>
      <span>{props.text}</span>
    </p>
  );
}

/** 面板里的小标题（「影调」「色彩」那一层之下不再分小标题，这是给分组用的）。 */
export function PanelTitle(props: { children: JSX.Element; class?: string }): JSX.Element {
  return (
    <h3
      class={["text-fs-3 font-semibold text-fg-2", props.class ?? ""]
        .filter(Boolean)
        .join(" ")}
    >
      {props.children}
    </h3>
  );
}
