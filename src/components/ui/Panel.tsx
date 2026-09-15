/**
 * Panel —— 面板容器（DESIGN.md §10.1 #13）。
 *
 * 带标题行、可折叠、可被放进 `Splitter` 参与拖拽分段。
 *
 * 它是个**纯容器**，自身的视觉规则只有三条：
 *   1. 面由宿主决定（`bg-surface-main` / `bar` 由调用方给，§2 的表面分层地图）
 *   2. **不加分隔线**（§6 无边线设计）：标题行与内容之间只靠留白与字号分组
 *   3. 标题行高度用 `--row-h`、内边距用 `--panel-pad`（密度生效，§8.1）
 *
 * 状态：
 *   默认     ——
 *   折叠     标题行保留、内容整块收起（`aria-expanded=false`）
 *   空态     调用方给 `empty` 时在内容区居中显示提示（如「还没有库，先建一个」）
 *
 * 为什么折叠不引 Ark UI 的 `Collapsible`：这里只用到「开关一个布尔量 + 收起内容」，
 * 需要自己保证的只有 `aria-expanded` / `aria-controls` 两件事 —— 手写就够了，
 * 少一层抽象在排查布局问题时更省事。
 */

import { Show, splitProps, type JSX } from "solid-js";
import { IconChevronDown, IconChevronRight } from "@tabler/icons-solidjs";

export interface PanelProps {
  /** 标题行文字（已经是翻译后的文案） */
  title?: string;
  /** 标题行右侧的动作区（如「+ 新建库」按钮） */
  actions?: JSX.Element;
  /** 标题行下方的固定内容（如「已选目录」横条列表） */
  header?: JSX.Element;
  collapsible?: boolean;
  collapsed?: boolean;
  onCollapsedChange?: (collapsed: boolean) => void;
  /** 内容区是否可滚动。列表/树一般开，固定高度的小区块关掉 */
  scroll?: boolean;
  /** 是否套 `--panel-pad` 内边距。横条列表要自己贴边，所以可关 */
  pad?: boolean;
  /** 空态内容；有值时内容区显示它而不是 children */
  empty?: JSX.Element;
  class?: string;
  bodyClass?: string;
  children?: JSX.Element;
}

export function Panel(props: PanelProps) {
  const [local, rest] = splitProps(props, [
    "title",
    "actions",
    "header",
    "collapsible",
    "collapsed",
    "onCollapsedChange",
    "scroll",
    "pad",
    "empty",
    "class",
    "bodyClass",
    "children",
  ]);

  const collapsed = () => Boolean(local.collapsed);

  return (
    <section
      {...rest}
      class={["flex min-h-0 min-w-0 flex-col", local.class ?? ""].join(" ")}
    >
      <Show when={local.title || local.actions}>
        <div class="flex h-row-h shrink-0 items-center gap-1 px-panel-pad">
          <Show when={local.collapsible}>
            <button
              type="button"
              aria-expanded={!collapsed()}
              aria-label={local.title}
              onClick={() => local.onCollapsedChange?.(!collapsed())}
              class="flex size-4 shrink-0 cursor-pointer items-center justify-center rounded-[2px] text-fg-3 hover:bg-state-hover hover:text-fg-1"
            >
              {collapsed() ? (
                <IconChevronRight size={12} aria-hidden="true" />
              ) : (
                <IconChevronDown size={12} aria-hidden="true" />
              )}
            </button>
          </Show>
          <Show when={local.title}>
            <h2 class="min-w-0 flex-1 truncate text-[11px] font-medium tracking-wide text-fg-2 uppercase">
              {local.title}
            </h2>
          </Show>
          <Show when={local.actions}>
            <div class="flex shrink-0 items-center gap-1">{local.actions}</div>
          </Show>
        </div>
      </Show>

      <Show when={local.header}>{local.header}</Show>

      <Show when={!collapsed()}>
        <div
          class={[
            "flex min-h-0 min-w-0 flex-1 flex-col",
            local.scroll ? "overflow-x-hidden overflow-y-auto" : "",
            local.pad ? "p-panel-pad" : "",
            local.bodyClass ?? "",
          ]
            .filter(Boolean)
            .join(" ")}
        >
          <Show when={local.empty} fallback={local.children}>
            <div class="flex flex-1 items-center justify-center p-panel-pad text-center text-[11px] text-fg-3">
              {local.empty}
            </div>
          </Show>
        </div>
      </Show>
    </section>
  );
}
