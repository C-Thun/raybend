/**
 * Menu —— 下拉菜单（DESIGN.md §10.1 #8）。
 *
 * 用在标题栏（`帮助 → 关于`），后续也可能用在右键菜单上。
 *
 * 状态：
 *   关闭     不渲染
 *   打开     浮层底（`surface-layer`）
 *   项·默认  次级文字色
 *   项·指向  **辅色底**（§5：指向 = 辅色底）
 *   项·选中  **主色底**（低透明度；本项是「当前生效的那个」时）
 *   项·禁用  前景降次级，不可高亮
 *
 * 键盘（↑↓ / Home / End / Esc / 首字母跳转）与焦点管理全部交给 Ark ——
 * 这些是「自己写一定会漏」的部分。
 * 触发器沿用 Ark 的 `asChild` 渲染函数约定（同 `Tooltip`，见那个文件里的说明）。
 */

import { Menu as ArkMenu } from "@ark-ui/solid";
import { For, Show, type JSX } from "solid-js";
import { Portal } from "solid-js/web";
import type { TriggerRender } from "./Tooltip.tsx";

export interface MenuItemSpec {
  value: string;
  label: string;
  icon?: JSX.Element;
  disabled?: boolean;
  /** 选中态：本项是当前生效项（如当前主题） */
  selected?: boolean;
  /** 在这项**之前**留一段分组空隙 */
  separatorBefore?: boolean;
}

export interface MenuProps {
  items: readonly MenuItemSpec[];
  onSelect: (value: string) => void;
  /** 触发器（渲染函数） */
  children: TriggerRender;
  /** 无障碍名（如「帮助」菜单） */
  label?: string;
  placement?: "top" | "bottom" | "left" | "right" | "bottom-start" | "bottom-end";
}

export function Menu(props: MenuProps) {
  return (
    <ArkMenu.Root
      positioning={{ placement: props.placement ?? "bottom-start" }}
      onSelect={(details) => props.onSelect(String(details.value))}
      lazyMount
      unmountOnExit
    >
      {/*
        `aria-label` 交给触发器自己带（渲染函数里能拿到 Ark 的属性，
        调用方的 IconButton 也接受 `aria-label`），这里不额外包一层。
      */}
      <ArkMenu.Trigger asChild={props.children} />
      <Portal>
        <ArkMenu.Positioner class="z-50">
          <ArkMenu.Content class="flex min-w-32 flex-col rounded-ui bg-surface-layer py-1 outline-none">
            <For each={props.items}>
              {(item) => (
                <>
                  <Show when={item.separatorBefore}>
                    {/* 分组用留白而不是分割线（§6 无边线设计） */}
                    <div class="h-1" role="presentation" />
                  </Show>
                  <ArkMenu.Item
                    value={item.value}
                    disabled={item.disabled}
                    class={[
                      "flex h-row-h cursor-pointer items-center gap-1.5 px-2 text-[12px]",
                      "outline-none transition-colors",
                      item.selected
                        ? "bg-state-selected text-fg-1"
                        : "text-fg-2 data-[highlighted]:bg-state-hover data-[highlighted]:text-fg-1",
                      item.disabled
                        ? "pointer-events-none text-fg-3 opacity-60"
                        : "",
                    ]
                      .filter(Boolean)
                      .join(" ")}
                  >
                    <Show when={item.icon}>
                      <span
                        class="flex size-4 shrink-0 items-center justify-center"
                        aria-hidden="true"
                      >
                        {item.icon}
                      </span>
                    </Show>
                    <ArkMenu.ItemText class="min-w-0 flex-1 truncate">
                      {item.label}
                    </ArkMenu.ItemText>
                  </ArkMenu.Item>
                </>
              )}
            </For>
          </ArkMenu.Content>
        </ArkMenu.Positioner>
      </Portal>
    </ArkMenu.Root>
  );
}
