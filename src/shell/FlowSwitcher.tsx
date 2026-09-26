/**
 * `FlowSwitcher` —— flowbar 的工作流切换器（`design/main.md` §2.2）。
 *
 * **它是外壳的招牌部件，刻意不复用通用 `SegmentedControl`。**
 * 原因（用户 2026-09-15 明确）：
 *   1. 工作流是**有序流水线**（导入 → 浏览 → 编辑 → 导出），不是并列的多选项 ——
 *      这一行本身就是软件的重要特色，值得一个专属部件
 *   2. 它的形态也应该与别处不同：**更大、更圆润、图标 + 文字**，
 *      而「紧凑 / 宽松」那种小工具开关不该长得像它
 *
 * 与通用分段控件的差异（都在这里，改起来是一处）：
 *   - 尺寸走 `--flow-chip-h` / `--flow-chip-pad-x`（比 `--seg-chip-h` 大一档）
 *   - 轨道内边距与圆角更圆润，当前项是**品牌主色实色胶囊**
 *   - 文字用正文基准字号 `text-fs-3`（通用控件用 `text-fs-2`）
 *
 * 无障碍与键盘行为仍交给 Ark 的 `SegmentGroup`（下面的 indicator 修正见注释）。
 */

import { SegmentGroup as ArkSegmentGroup } from "@ark-ui/solid";
import { For, Show, type JSX } from "solid-js";

export interface FlowSwitcherOption<TValue extends string> {
  value: TValue;
  /** 文案（已翻译） */
  label: string;
  icon?: () => JSX.Element;
  disabled?: boolean;
  processing?: boolean;
}

export interface FlowSwitcherProps<TValue extends string> {
  value: TValue;
  options: readonly FlowSwitcherOption<TValue>[];
  onValueChange: (value: TValue) => void;
  /** 无障碍名（工作流） */
  label: string;
  class?: string;
}

export function FlowSwitcher<TValue extends string>(props: FlowSwitcherProps<TValue>) {
  return (
    <ArkSegmentGroup.Root
      value={props.value}
      onValueChange={(details) => {
        if (details.value != null) props.onValueChange(details.value as TValue);
      }}
      aria-label={props.label}
      class={[
        "relative inline-flex shrink-0 items-center rounded-full bg-surface-track p-seg-track-pad",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {/*
        ⚠️ Ark（Zag）只给指示块写 `--left/--top/--width/--height` 这几个变量，
        **不写 width / height 本身** —— 消费方必须自己接上，
        否则它塌成 0 宽：选中项变成「深色文字落在深色轨道上」，肉眼就是**看不见**。
        （实测踩过：这就是「横向选择器选中态全黑」的根因。）
      */}
      <ArkSegmentGroup.Indicator class="pointer-events-none absolute top-[var(--top)] left-[var(--left)] h-[var(--height)] w-[var(--width)] rounded-full bg-brand transition-[left,top,width,height] duration-150 ease-out" />
      <For each={props.options}>
        {(option) => (
          <ArkSegmentGroup.Item
            value={option.value}
            disabled={option.disabled}
            /* 只有 Indicator 画选中底；item 自身绝不预亮，避免目标先出现再被滑块覆盖。 */
            class={[
              "relative z-(--z-bar) flex h-flow-chip-h shrink-0 cursor-pointer items-center justify-center",
              "rounded-full px-flow-chip-pad-x",
              "cursor-pointer",
            ].join(" ")}
          >
            <ArkSegmentGroup.ItemControl
              class={[
                "flex items-center gap-2 text-fs-3 font-medium whitespace-nowrap",
                "text-fg-2 transition-colors",
                "data-[state=checked]:text-fg-on-brand",
                "data-[disabled]:text-fg-3 data-[disabled]:opacity-60",
              ].join(" ")}
            >
              <ArkSegmentGroup.ItemText>
                <span class="relative grid overflow-hidden" aria-busy={option.processing===true}>
                  <span class="col-start-1 row-start-1 flex items-center gap-2" classList={{"opacity-50":option.processing===true}}>
                    <Show when={option.icon}><span class="flex size-4 items-center justify-center" aria-hidden="true">{option.icon?.()}</span></Show>
                    {option.label}
                  </span>
                  <Show when={option.processing}><span aria-hidden="true" data-flow-processing class="rb-shimmer-mask pointer-events-none col-start-1 row-start-1 flex items-center gap-2">
                    <Show when={option.icon}><span class="flex size-4 items-center justify-center">{option.icon?.()}</span></Show>
                    {option.label}
                  </span></Show>
                </span>
              </ArkSegmentGroup.ItemText>
            </ArkSegmentGroup.ItemControl>
            <ArkSegmentGroup.ItemHiddenInput />
          </ArkSegmentGroup.Item>
        )}
      </For>
    </ArkSegmentGroup.Root>
  );
}
