/**
 * `SplitStack` —— 竖直方向的三段/多段可拖拽布局（`design/main.md` §3.1）。
 *
 * 为什么用 Ark UI 的 `Splitter` 而不是自己写拖拽：它是当初选 Ark UI 的两个理由之一
 * （`design/main.md` §4.7），而且它带键盘调整、指针捕获、触摸支持与
 * `aria`/`role=separator` 语义 —— 手写这些是重复劳动。这里只做两件事：
 *
 *   1. **把分隔条换成我们的视觉**：`Splitter.ResizeTrigger` 是个 `<button>`，
 *      我们在里面放 `SplitHandleDots`（三点细空白区），并保持既有组件
 *      `SplitHandle` 定义的尺寸（高 8px）与配色（指向辅色底、拖拽中主色底）；
 *   2. **收敛 API**：调用方只给「每一段是谁、初始多高、最小多高」，
 *      不需要知道 Ark 的 `ResizeTriggerId`（形如 `"a:b"`）这种细节。
 *
 * ⚠️ **尺寸的数值语义（2026-09-16 踩过，必读）**：Ark/Zag 里
 *   * **数字 = 百分比**（`minSize: 80` 是「至少 80%」！）；
 *   * **带单位的字符串 = 那个单位**（`minSize: "120px"` 才是「至少 120 像素」，
     内部按容器实际高度换算 —— 见 `@zag-js/splitter` 的 `parsePanelSize`）。
 * 传错单位的后果不是「小一点」而是**布局数学畸变**：曾经三段的 `minSize` 写成
 * `80 / 100`（当成像素），等于要求「最近 ≥80%、来源 ≥100%」，于是最近下不去、
 * 来源拖不高、第三段被挤没 —— 这一条是那次 bug 的根因，别再犯。
 *
 * ⚠️ 两处实测坑（`design/main.md` §2.2 记过同类问题）：
 * 1. **尺寸类必须挂在外面这一层**：Ark 的根元素自带**内联** `width/height: 100%`，
 *    内联样式优先于 class —— 把 `h-64 w-80` 写给它会被 100% 覆盖掉，
 *    结果是「面板高度全是 0」（看起来渲染了，其实什么都看不到）。
 *    所以这里多包一层普通 `div` 承尺寸，Ark 的根在里面填满它。
 * 2. 拖拽中的底色要接 `data-dragging`（Zag 的 `dataAttr` 产出的是**无值属性**，
 *    不写值就能用 Tailwind 的 `data-[dragging]:` 匹配）。旧稿里写的
 *    `data-[state=dragging]` 是永远不匹配的。
 */

import { Splitter } from "@ark-ui/solid";
import { For, type JSX } from "solid-js";
import { SplitHandleDots } from "./SplitHandle.tsx";

export interface SplitSegment {
  /** 段的 id（同时是 Ark 的 pane id，必须唯一且稳定） */
  id: string;
  /** 初始高度：**数字=百分比**，像素请写 `"120px"` */
  defaultSize: number | string;
  /**
   * 最小高度：**数字=百分比**，像素请写 `"120px"`（不填则用 Ark 的默认 0）。
   *
   * 想表达「这一段不许比 120 像素还矮」的话，**必须**写成带单位的字符串 ——
   * 写成 `120` 等于「不许比 120% 还矮」，布局会直接畸变。
   */
  minSize?: number | string;
  /** 允许被拖到折叠（默认否 —— 折叠后没有「展开」的入口，会变成死界面的坑） */
  collapsible?: boolean;
  /** 段内容 */
  content: JSX.Element;
}

export interface SplitStackProps {
  /** 从上到下的各段（顺序即布局顺序） */
  segments: readonly SplitSegment[];
  /** 键盘方向键每次调整的像素（默认 16；无障碍要求这条必须有） */
  keyboardResizeBy?: number;
  /** 拖拽结束时的回调（用于持久化尺寸等；M1 不需要） */
  onResizeEnd?: () => void;
  class?: string;
}

export function SplitStack(props: SplitStackProps) {
  // Ark 要求 panels 是一个数组，且每个 pane 的 id 与下面 Panel 的 id 对应
  const panels = () =>
    props.segments.map((segment) => ({
      id: segment.id,
      minSize: segment.minSize,
      collapsible: segment.collapsible ?? false,
    }));

  const defaultSize = () => props.segments.map((segment) => segment.defaultSize);

  return (
    <div class={["flex min-h-0 flex-col", props.class ?? ""].join(" ")}>
      <Splitter.Root
        orientation="vertical"
        panels={panels()}
        // `defaultSize` 只在首次渲染生效；之后再改它不会强行把用户拖过的位置改回去。
        // 数字是**百分比**（Zag 的 `toCssPanelSize`：number → `N%`）。
        defaultSize={defaultSize()}
        keyboardResizeBy={props.keyboardResizeBy ?? 16}
        onResizeEnd={() => props.onResizeEnd?.()}
        class="flex min-h-0 flex-1 flex-col"
      >
        <For each={props.segments}>
          {(segment, index) => (
            <>
              <Splitter.Panel
                id={segment.id}
                class="flex min-h-0 flex-col overflow-hidden"
              >
                {segment.content}
              </Splitter.Panel>
              {/* 最后一段后面不画分隔条 */}
              {index() < props.segments.length - 1 ? (
                <Splitter.ResizeTrigger
                  id={`${segment.id}:${props.segments[index() + 1].id}`}
                  class={[
                    "group/split flex h-2 w-full shrink-0 cursor-row-resize items-center justify-center",
                    "transition-colors outline-none",
                    // 指向 = 辅色底；拖拽中 = 主色底（DESIGN.md §5）；键盘聚焦也要看得见
                    "hover:bg-state-hover focus-visible:bg-state-hover data-[dragging]:bg-state-selected",
                  ].join(" ")}
                >
                  <SplitHandleDots orientation="horizontal" />
                </Splitter.ResizeTrigger>
              ) : null}
            </>
          )}
        </For>
      </Splitter.Root>
    </div>
  );
}
