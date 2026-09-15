/**
 * SegmentedControl —— 凹槽 + 移动色块（DESIGN.md §10.1 #5）。
 *
 * 两个用途共用它：**工作流切换**（导入/浏览/编辑/导出）与**密度切换**（紧凑/宽松）。
 *
 * 状态：
 *   默认      凹槽 `surface-track`，选中项文字 = `fg-on-brand`（坐在主色块上）
 *   指向      未选中项叠**辅色底**（§5）
 *   选中      色块 = **主色实色**（小控件用实色，§5.1），文字转 `fg-on-brand`
 *   选中+指向 保持主色块，不回落成辅色底
 *   禁用      前景降次级，不响应
 *
 * 语义是**单选**：同一时刻只有一个工作流、一个密度。
 * 「横向多选」在设计文档里指的是**横向排列的一组选项**，不是可以同时选中多个。
 *
 * 色块的位置与宽度交给 Ark 的 `Indicator`（它内部量测每个 item 并给出 transform）——
 * 自己算就得处理字体加载、缩放、文案变化后的重新测量，全是白费力气。
 *
 * 尺寸随密度档（`--seg-track-pad` / `--seg-chip-h` / `--seg-chip-pad-x`），
 * **字号不随密度变**（§8.1）。
 */

import { SegmentGroup as ArkSegmentGroup } from "@ark-ui/solid";
import { For, Show, type JSX } from "solid-js";

export interface SegmentOption<TValue extends string> {
  value: TValue;
  /** 文字标签（已经是翻译后的文案） */
  label: string;
  /** 可选图标 */
  icon?: JSX.Element;
  disabled?: boolean;
}

export interface SegmentedControlProps<TValue extends string> {
  value: TValue;
  options: readonly SegmentOption<TValue>[];
  onValueChange: (value: TValue) => void;
  /** 无障碍名（如「工作流」「密度」）。必填：纯图形分组对读屏是空白 */
  label: string;
  class?: string;
}

export function SegmentedControl<TValue extends string>(
  props: SegmentedControlProps<TValue>,
) {
  return (
    <ArkSegmentGroup.Root
      value={props.value}
      onValueChange={(details) => {
        // Ark 传回来的可能是 undefined（清空选中）—— 本控件不允许「什么都不选」
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
      <ArkSegmentGroup.Indicator class="absolute top-seg-track-pad bottom-seg-track-pad left-0 rounded-full bg-brand transition-all duration-150" />
      <For each={props.options}>
        {(option) => (
          <ArkSegmentGroup.Item
            value={option.value}
            disabled={option.disabled}
            class={[
              "relative z-10 flex h-seg-chip-h shrink-0 items-center justify-center",
              "rounded-full px-seg-chip-pad-x transition-colors",
              option.disabled ? "cursor-default" : "cursor-pointer",
            ].join(" ")}
          >
            <ArkSegmentGroup.ItemControl
              class={[
                "flex items-center gap-1.5 text-[12px] font-medium whitespace-nowrap",
                "text-fg-2 transition-colors",
                "data-[state=checked]:text-fg-on-brand",
                "data-[disabled]:text-fg-3 data-[disabled]:opacity-60",
              ].join(" ")}
            >
              <Show when={option.icon}>
                <span class="flex size-3.5 items-center justify-center" aria-hidden="true">
                  {option.icon}
                </span>
              </Show>
              <ArkSegmentGroup.ItemText>{option.label}</ArkSegmentGroup.ItemText>
            </ArkSegmentGroup.ItemControl>
            <ArkSegmentGroup.ItemHiddenInput />
          </ArkSegmentGroup.Item>
        )}
      </For>
    </ArkSegmentGroup.Root>
  );
}
