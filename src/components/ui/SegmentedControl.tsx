/**
 * SegmentedControl —— 「小而安静」的分段控件（DESIGN.md §10.1 #5）。
 *
 * **它不再是 flowbar 的牌子。** 用户 2026-09-15 明确：
 * 「横向选择器不要复用 flowbar 上的，flowbar 是非常特殊的部分……这种大模块切换的概念
 * 不应该跟一个小小的界面松紧调整挂钩」。所以：
 *
 * | 用途 | 用谁 | 形态 |
 * | --- | --- | --- |
 * | 工作流（导入/浏览/编辑/导出） | `src/shell/FlowSwitcher.tsx` | 大胶囊 + 图标 + 文字，品牌实色块 |
 * | 密度（紧凑/宽松）、语言这类小开关 | **本组件** | 小、方一些的圆角、更安静，不与前者混淆 |
 *
 * 状态：
 *   默认      凹槽 `surface-track`，未选中项次要色
 *   指向      未选中项叠**辅色底**（§5）
 *   选中      **主色实色块**（§5.1：小控件用实色），文字 `fg-on-brand`
 *   选中+指向 保持主色块，不回落成辅色底
 *   禁用      前景降次级，不响应
 *
 * ⚠️ **教训（实测踩过）**：Ark（Zag）的指示块只给 `--left/--top/--width/--height` 变量，
 * **不写 width / height 本身**。消费方不接的话它塌成 0 宽 —— 选中项就变成
 * 「深色文字落在深色轨道上」，肉眼完全看不见（用户报的「选中字体全黑背景全黑」）。
 * 所以 `Indicator` 上必须写 `h-[var(--height)] w-[var(--width)]` 与 `top/left`。
 */

import { SegmentGroup as ArkSegmentGroup } from "@ark-ui/solid";
import { For, Show, type JSX } from "solid-js";

export interface SegmentOption<TValue extends string> {
  value: TValue;
  /** 文字标签（已经是翻译后的文案） */
  label: string;
  icon?: JSX.Element;
  disabled?: boolean;
}

export interface SegmentedControlProps<TValue extends string> {
  value: TValue;
  options: readonly SegmentOption<TValue>[];
  onValueChange: (value: TValue) => void;
  /** 无障碍名（如「密度」）。必填：纯图形分组对读屏是空白 */
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
        "relative inline-flex shrink-0 items-center rounded-ui bg-surface-track p-seg-track-pad",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {/* 指示块：必须自己接上 Ark 给的四个变量，否则 0 宽不可见（见文件头注释） */}
      <ArkSegmentGroup.Indicator class="absolute top-[var(--top)] left-[var(--left)] h-[var(--height)] w-[var(--width)] rounded-ui bg-brand transition-all duration-150" />
      <For each={props.options}>
        {(option) => (
          <ArkSegmentGroup.Item
            value={option.value}
            disabled={option.disabled}
            class={[
              "relative z-(--z-bar) flex h-seg-chip-h shrink-0 items-center justify-center",
              "rounded-ui px-seg-chip-pad-x transition-colors",
              option.disabled ? "cursor-default" : "cursor-pointer",
            ].join(" ")}
          >
            <ArkSegmentGroup.ItemControl
              class={[
                "flex items-center gap-1.5 text-fs-2 font-medium whitespace-nowrap",
                "text-fg-2 transition-colors",
                "data-[state=checked]:text-fg-on-brand",
                "data-[disabled]:text-fg-3 data-[disabled]:opacity-60",
              ].join(" ")}
            >
              <Show when={option.icon}>
                <span class="flex size-4 items-center justify-center" aria-hidden="true">
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
