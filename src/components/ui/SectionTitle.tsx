/**
 * SectionTitle —— 面板里的**段标题**（`memory/DESIGN.md` §10.1）。
 *
 * 形态（崔总 2026-11-05 定）＝ 左侧一道 3px **主色**竖条 + 加粗标题字。
 *
 * 为什么要有它：段标题原先只是「和正文同字号、同字重的一行次要文字」，
 * 一眼看过去和字段名分不出来（导出右栏的「预设列表」被当成普通文字就是这么来的）。
 * 竖条是**装饰**（`aria-hidden`）：它承担「视线落点」，语义仍由 `<h3>` 文本承担。
 *
 * 状态：只有一种静态形态，没有 hover / 选中态 —— 它不是控件。
 * 用法：`<SectionTitle label={t("export.presetList")} />`；`data-*` 等属性原样透传。
 */

import { splitProps, type JSX } from "solid-js";

export interface SectionTitleProps extends JSX.HTMLAttributes<HTMLHeadingElement> {
  /** 标题文字（已经是翻译后的文案） */
  label: string;
  class?: string;
}

export function SectionTitle(props: SectionTitleProps) {
  const [local, rest] = splitProps(props, ["label", "class"]);
  return (
    <h3
      {...rest}
      class={["flex shrink-0 items-center gap-2", local.class ?? ""]
        .filter(Boolean)
        .join(" ")}
    >
      <span class="h-3.5 w-[3px] shrink-0 rounded-full bg-brand" aria-hidden="true" />
      <span class="text-fs-2 font-semibold text-fg-1">{local.label}</span>
    </h3>
  );
}
