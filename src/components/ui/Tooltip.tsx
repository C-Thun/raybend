/**
 * Tooltip —— 提示气泡（DESIGN.md §10.1 #7）。
 *
 * 它同时是 `easy copy`（§12.1）的载体：那套范式里的「点击复制 / 已复制」小窗
 * 就是一个由下向上弹出、内容会翻转的 tooltip。所以本组件支持**受控 `open`**
 * —— 否则「点击后保持显示并换文案」做不到（鼠标没动，非受控的 tooltip 会自己关掉）。
 *
 * 状态：
 *   关       什么都不渲染（`Presence` 等退场动画结束再卸载）
 *   开       浮层底（`surface-layer`）+ 11px 文字 + 窄内边距；**无阴影**（§6 不做阴影）
 *
 * 触发器用 **Ark 的 `asChild` 约定**（传一个渲染函数）而不是包一层元素：
 * 包一层会产出 `<button><button/></button>` 这种非法嵌套 —— 而本项目最常用它的地方
 * 恰恰就是图标按钮。
 *
 * 位置器**必须自己套 `Portal`**：Ark Solid 的 `Positioner` 不会自动传送门，
 * 不套的话气泡会被父级的 `overflow: hidden` 裁掉（面板里到处都是这种父级）。
 */

import { Tooltip as ArkTooltip } from "@ark-ui/solid";
import type { PolymorphicProps } from "@ark-ui/solid";
import { Show, splitProps, type JSX } from "solid-js";
import { Portal } from "solid-js/web";

export type TooltipPlacement = "top" | "bottom" | "left" | "right";

/**
 * 触发器渲染函数 —— **直接复用 Ark 的 `asChild` 类型**，不自己另立一个。
 *
 * 这一步不是洁癖：自己写的等价类型会把属性退化成 `JSX.HTMLAttributes<HTMLElement>`，
 * 而 `<Button {...triggerProps()} />` 就会因为 `ref` 是 `HTMLElement` 而不是
 * `HTMLButtonElement` 而编译不过（实测踩过）。用 Ark 自己的类型就天然对得上。
 */
export type TriggerRender<
  TElement extends keyof JSX.IntrinsicElements = "button",
> = NonNullable<PolymorphicProps<TElement>["asChild"]>;

export interface TooltipProps {
  /** 气泡内容（一般是一行文字） */
  content?: JSX.Element;
  placement?: TooltipPlacement;
  /** 悬停多久才弹出（ms）。提示类文案要**快**，默认 150 */
  openDelay?: number;
  closeDelay?: number;
  /** 受控开关；给了它就完全由调用方决定（`easy copy` 用这个） */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  disabled?: boolean;
  /** easy copy 的 Portal 气泡单独走高于弹窗的层级。 */
  layer?: "easy-copy";
  /** 触发器（渲染函数） */
  children: TriggerRender;
  class?: string;
}

export function Tooltip(props: TooltipProps) {
  const [local] = splitProps(props, [
    "content",
    "placement",
    "openDelay",
    "closeDelay",
    "open",
    "onOpenChange",
    "disabled",
    "layer",
    "children",
    "class",
  ]);

  return (
    <ArkTooltip.Root
      openDelay={local.openDelay ?? 150}
      closeDelay={local.closeDelay ?? 80}
      positioning={{ placement: local.placement ?? "top" }}
      disabled={local.disabled}
      open={local.open}
      onOpenChange={(details) => local.onOpenChange?.(details.open)}
      lazyMount
      unmountOnExit
    >
      <ArkTooltip.Trigger asChild={local.children} />
      <Portal>
        {/* 同 Menu：z 轴由基础层统一赋值（见 index.css） */}
        <ArkTooltip.Positioner data-easy-copy-tip={local.layer === "easy-copy" ? "" : undefined}>
          <ArkTooltip.Content
            class={[
              "max-w-64 rounded-ui bg-surface-layer px-1.5 py-0.5 text-[11px] leading-4 text-fg-1",
              local.class ?? "",
            ].join(" ")}
          >
            <Show when={local.content != null}>{local.content}</Show>
          </ArkTooltip.Content>
        </ArkTooltip.Positioner>
      </Portal>
    </ArkTooltip.Root>
  );
}
