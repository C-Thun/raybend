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
import { createMemo, For, type JSX } from "solid-js";
import { SplitHandleDots } from "./SplitHandle.tsx";

export interface SplitSegment {
  /** 段的 id（同时是 Ark 的 pane id，必须唯一且稳定） */
  id: string;
  /**
   * 初始尺寸：**数字=百分比**，像素请写 `"120px"`。
   *
   * ⚠️ **必填**：Ark 的 `defaultSize` 是 `PanelSize[]`，数组里不接受 `undefined`
   * （想「这段吃剩余空间」就自己算：`100 - 前几段之和`）。
   */
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
  /** 各段（顺序即布局顺序） */
  segments: readonly SplitSegment[];
  /**
   * 分段方向：`vertical`（默认，分段上下叠，拖拽改**高度**）
   * / `horizontal`（分段左右排，拖拽改**宽度**）。
   */
  orientation?: "vertical" | "horizontal";
  /** 键盘方向键每次调整的像素（默认 16；无障碍要求这条必须有） */
  keyboardResizeBy?: number;
  /** 拖拽结束：给出**各段的百分比**（与 `segments` 顺序一致）—— 持久化比例用它 */
  onResizeEnd?: (sizes: number[]) => void;
  class?: string;
}

export function SplitStack(props: SplitStackProps) {
  const vertical = (): boolean => (props.orientation ?? "vertical") === "vertical";

  /*
   * ⚠️ **同一个尺寸只喂 Ark 一次**（2026-09-16 真机回归的教训）。
   *
   * 调用方（比如 `ImportWorkspace`）会因为自己的信号变化而重渲染，`segments` 是
   * 每次渲染都新建的数组 —— 值一样、**引用不同**。Ark 的 `Splitter` 看到 props 变了
   * 就可能重新套用尺寸 → 又一次布局 → `ResizeObserver` 再触发 → 上层再写状态……
   * 真机症状：**启动抖几秒、一最大化就卡死**（最大化本身就是一次尺寸变化，正好踩进回路）。
   *
   * 所以这里按「值的形态」做记忆：数组内容没变，就继续用**同一个数组对象**。
   */
  const sameContents = (a: unknown, b: unknown): boolean =>
    JSON.stringify(a) === JSON.stringify(b);

  const shapeKey = () =>
    props.segments
      .map(
        (segment) =>
          `${segment.id}|${String(segment.defaultSize)}|${String(segment.minSize ?? "")}|${String(segment.collapsible ?? false)}`,
      )
      .join(";");

  // Ark 要求 panels 是一个数组，且每个 pane 的 id 与下面 Panel 的 id 对应
  const panels = createMemo(
    () => {
      shapeKey(); // 只在形态变化时重算
      return props.segments.map((segment) => ({
        id: segment.id,
        minSize: segment.minSize,
        collapsible: segment.collapsible ?? false,
      }));
    },
    [],
    { equals: sameContents },
  );

  const defaultSize = createMemo(
    () => {
      shapeKey();
      return props.segments.map((segment) => segment.defaultSize);
    },
    [],
    { equals: sameContents },
  );

  return (
    <div class={["flex min-h-0 flex-col", props.class ?? ""].join(" ")}>
      <Splitter.Root
        orientation={vertical() ? "vertical" : "horizontal"}
        panels={panels()}
        // `defaultSize` 只在首次渲染生效；之后再改它不会强行把用户拖过的位置改回去。
        // 数字是**百分比**（Zag 的 `toCssPanelSize`：number → `N%`）。
        defaultSize={defaultSize()}
        keyboardResizeBy={props.keyboardResizeBy ?? 16}
        onResizeEnd={(details) => props.onResizeEnd?.(details.size)}
        class={[
          "flex min-h-0 min-w-0 flex-1",
          vertical() ? "flex-col" : "flex-row",
        ].join(" ")}
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
                    "group/split flex shrink-0 items-center justify-center",
                    vertical() ? "h-2 w-full cursor-row-resize" : "h-full w-2 cursor-col-resize",
                    "transition-colors outline-none",
                    // 指向 = 辅色底；拖拽中 = 主色底（DESIGN.md §5）；键盘聚焦也要看得见
                    "hover:bg-state-hover focus-visible:bg-state-hover data-[dragging]:bg-state-selected",
                  ].join(" ")}
                >
                  {/* 点的排列方向与拖拽方向垂直：竖直分段用横躺的三点，横向分段用竖着的 */}
                  <SplitHandleDots orientation={vertical() ? "horizontal" : "vertical"} />
                </Splitter.ResizeTrigger>
              ) : null}
            </>
          )}
        </For>
      </Splitter.Root>
    </div>
  );
}
