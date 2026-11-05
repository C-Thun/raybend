/**
 * Tooltip —— 提示气泡（memory/DESIGN.md §10.1 #7）。
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
import { Show, createContext, splitProps, useContext, type JSX } from "solid-js";
import { Portal } from "solid-js/web";

export type TooltipPlacement = "top" | "bottom" | "left" | "right";

/**
 * 「这段子树已经在某个 `Tooltip` 的触发区里」——由 `Tooltip` 打在触发子树上，
 * `ButtonTip` 读到它就不再自己套一层（双提示的根因与症状见 `ButtonTip` 注释）。
 */
const TooltipOwnerContext = createContext<boolean>(false);

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

/**
 * `ButtonTip` —— 给**按钮类原语**（`Button` / `IconButton` / `ToggleBlock`）用的内置提示。
 *
 * 三条口径（崔总 2026-10-11）：
 *
 * 1. **有提示才包**：没有提示就原样渲染子节点，不多一层 DOM；
 * 2. **禁用态也要能弹**：禁用的 `<button>` 在浏览器里不派发指针事件，所以提示态下
 *    在触发层包一个 `inline-flex` 的 `span` 承接命中（按钮自己的 `pointer-events` 是 none）——
 *    而「为什么不能点」恰恰是最需要说话的场景；
 * 3. **默认朝上**（崔总 2026-11-05 改）：朝下时气泡正好落在鼠标光标底下，要看清得把光标
 *    挪到按钮上缘去 —— 提示自己成了遮挡物。所以一律先朝上；**只有上方放不下时才翻到下方**
 *    （气泡高 + 8px 间距超出窗口上缘，典型就是 `titlebar` 那排按钮）。
 *    翻转由 Zag 定位器自带的 `flip` 做，边界就是**窗口**：气泡 Portal 挂在 `body` 下，
 *    而 `html/body` 都是 `overflow: hidden` 且撑满视口 —— 裁切祖先 = 视口 = 应用窗口客户区，
 *    与「哪个面板、滚没滚动」无关（`overflowPadding` 默认 8px，即真的贴边了才翻）。
 *
 * 触发属性给谁：**有提示 → 给包装 span；没有提示 → 交还给调用方（展开到按钮上）**。
 *
 * ❗**外面已经有一层 `Tooltip` 时一律让位**（2026-10-11 复现并修掉）：两套提示挂在
 * 同一条指针路径上（外层 → 按钮、内层 → 包装 span），一共两台 Ark tooltip machine 抢
 * 同一个全局 store —— 挨个按钮划过去时后一台会卡在 `opening`/`hasPointerMoveOpened`，
 * 表现为「移到下一个按钮不弹，得先移出去再回来」，而且两台的默认方位不同（top/bottom）
 * 会先上后下闪一下。判据由 `Tooltip` 自己在**触发子树**上打标（`TooltipOwnerContext`），
 * 所以调用点写不写错都不会再出现双提示。
 */
export function ButtonTip(props: {
  content?: string | null;
  placement?: TooltipPlacement;
  children: (triggerProps: Record<string, unknown>) => JSX.Element;
}): JSX.Element {
  const insideTooltip = useContext(TooltipOwnerContext);
  const hasTip = (): boolean =>
    !insideTooltip && typeof props.content === "string" && props.content.length > 0;
  return (
    <Show when={hasTip()} fallback={props.children({})}>
      <Tooltip content={props.content} placement={props.placement ?? "top"}>
        {(triggerProps) => (
          <span {...triggerProps()} class="inline-flex">
            {props.children({})}
          </span>
        )}
      </Tooltip>
    </Show>
  );
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
      <ArkTooltip.Trigger
        asChild={(triggerProps) => (
          <TooltipOwnerContext.Provider value={true}>
            {local.children(triggerProps)}
          </TooltipOwnerContext.Provider>
        )}
      />
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
