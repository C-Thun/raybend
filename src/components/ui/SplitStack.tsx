/**
 * SplitStack —— N 段（纵向叠 / 横向排）可拖布局。**交互全部交给 `SplitHandle`**
 * （全项目唯一的把手实现），**尺寸数学全部交给 `lib/stack-resize.ts`**（有单测）。
 *
 * ## 为什么这里没有 Ark（2026-09-16 / 2026-09-19 两次真机事故，别再改回去）
 *
 * 上一版是 Ark UI 的 `Splitter` 包装：它用 ResizeObserver **量了又写、写了又量**，
 * 拖外层时内层不断收到回调、量到过渡中的尺寸 —— 症状就是人类报的
 * 「拖不动、松手过一会儿才跳到位」，嵌套时甚至会卡死。
 * 现在只有一条路：`pointermove`（已按帧节流）→ 改比例 → 重画。没有 observer。
 *
 * ## 尺寸口径（与旧 API 一致，调用方不用改）
 *
 * 段的尺寸是**比例**，段尺寸之和恒为 1：
 *   - `defaultSize`：数字 = **百分比**，带单位的字符串（`"120px"`） = 像素；
 *   - `minSize`：同上口径 —— 写成 `120` 意思是「不许低于 120%」，布局会畸变（历史事故）。
 * 拖动一条边界只在前后的两段之间挪，别的段一个字节都不动，总和守恒。
 */

import { createSignal, For, untrack, type JSX } from "solid-js";

import { minRatiosFrom, moveBoundary, parseSize, toPercents } from "../../lib/stack-resize.ts";
import { SplitHandle } from "./SplitHandle.tsx";

export interface SplitSegment {
  /** 段的 id（稳定且唯一，用于调试与 key） */
  id: string;
  /**
   * 初始尺寸：**数字=百分比**，像素请写 `"120px"`。
   *
   * ⚠️ 想表达「这段吃剩余空间」时自己算：`一段 = 100 - 其余之和`。
   */
  defaultSize: number | string;
  /**
   * 最小尺寸：**数字=百分比**，像素请写 `"120px"`（不填 = 0）。
   * 写成 `120` 等于「不许比 120% 还矮」—— 会直接畸形，历史事故。
   */
  minSize?: number | string;
  /** 段内容 */
  content: JSX.Element;
}

export interface SplitStackProps {
  /** 各段（顺序即布局顺序） */
  segments: readonly SplitSegment[];
  /**
   * 分段方向：`vertical`（默认，上下叠，拖拽改**高度**）
   * / `horizontal`（左右排，拖拽改**宽度**）。
   */
  orientation?: "vertical" | "horizontal";
  /** 键盘方向键每次调整的像素（默认 16；无障碍要求这条必须有） */
  keyboardResizeBy?: number;
  /** 拖拽/键盘调整结束：给出**各段的百分比**（与 `segments` 顺序一致）—— 持久化比例用它 */
  onResizeEnd?: (sizes: number[]) => void;
  class?: string;
}

export function SplitStack(props: SplitStackProps) {
  const vertical = (): boolean => (props.orientation ?? "vertical") === "vertical";
  let root: HTMLDivElement | undefined;
  /** 拖动起点的那份比例（用「起点 + 位移」算，不逐帧累加 —— 丢帧时不会漂移） */
  let dragStart: number[] = [];

  const [sizes, setSizes] = createSignal<number[]>(initialRatios());

  /**
   * 首帧还没量到容器：像素型 `defaultSize` 先按 0 记，再把剩下的按比例归一。
   * 全是百分比时这就是原样（这也是 `LeftColumn` 的用法）。
   */
  function initialRatios(): number[] {
    const values = props.segments.map((segment) => parseSize(segment.defaultSize, 0).ratio);
    const sum = values.reduce((total, value) => total + value, 0);
    if (sum > 0) return values.map((value) => value / sum);
    const even = props.segments.length === 0 ? 0 : 1 / props.segments.length;
    return props.segments.map(() => even);
  }

  /** 容器主轴尺寸（纵向=高、横向=宽）；没量到就返回 0，调用方据此跳过这一次 */
  function mainAxis(): number {
    if (!root) return 0;
    const box = root.getBoundingClientRect();
    return vertical() ? box.height : box.width;
  }

  function mins(): number[] {
    return minRatiosFrom(
      props.segments.map((segment) => segment.minSize),
      mainAxis(),
    );
  }

  /** 挪一条边界：把 `deltaPx` 从后一段挪到前一段（比例换算 + 下限夹取都在纯函数里） */
  function moveBy(index: number, deltaPx: number): void {
    const axis = mainAxis();
    if (axis <= 0) return;
    setSizes(
      moveBoundary({ sizes: dragStart, index, delta: deltaPx / axis, minRatios: mins() }),
    );
  }

  /** 段的 CSS 下限（与拖动时的夹取同一口径，但要写进 CSS —— 否则容器变矮时 pane 会塔成 0） */
  function minCss(segment: SplitSegment): Record<string, string> {
    if (segment.minSize === undefined) return {};
    const size = typeof segment.minSize === "number" ? `${segment.minSize}%` : segment.minSize;
    return vertical() ? { "min-height": size } : { "min-width": size };
  }

  return (
    <div
      ref={root}
      /* 测试钩子：冒烟脚本靠这几个属性认结构（以前用的是 Ark 的 data-scope/data-part） */
      data-split-root=""
      class={[
        "flex min-h-0 min-w-0",
        vertical() ? "flex-col" : "flex-row",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <For each={props.segments}>
        {(segment, index) => (
          <>
            <div
              data-split-pane=""
              class="flex min-h-0 min-w-0 flex-col overflow-hidden"
              /*
               * `flex-grow` 取比例、`flex-basis` 为 0 —— 把手先占走自己的 8px，
               * 剩下的空间再按比例分给各段。用百分比 basis 会和把手打架（总和超过 100%）。
               */
              style={{ flex: `${sizes()[index()] ?? 0} 1 0%`, ...minCss(segment) }}
            >
              {untrack(()=>segment.content)}
            </div>
            {index() < props.segments.length - 1 ? (
              <SplitHandle
                orientation={vertical() ? "horizontal" : "vertical"}
                data-split-handle=""
                aria-label={segment.id}
                tabindex="0"
                onDragStart={() => {
                  dragStart = sizes();
                }}
                onDrag={(delta) => moveBy(index(), delta)}
                onDragEnd={() => props.onResizeEnd?.(toPercents(sizes()))}
                onKeyDown={(event) => {
                  const step = props.keyboardResizeBy ?? 16;
                  const forward = vertical() ? "ArrowDown" : "ArrowRight";
                  const backward = vertical() ? "ArrowUp" : "ArrowLeft";
                  if (event.key !== forward && event.key !== backward) return;
                  event.preventDefault();
                  // 键盘是「一次按键 = 一次结束」：直接落盘，不走拖动那条节流路径
                  dragStart = sizes();
                  moveBy(index(), event.key === forward ? step : -step);
                  props.onResizeEnd?.(toPercents(sizes()));
                }}
              />
            ) : null}
          </>
        )}
      </For>
    </div>
  );
}
