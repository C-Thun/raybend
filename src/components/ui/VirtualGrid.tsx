/**
 * `VirtualGrid` —— 行式虚拟滚动容器（照片网格的骨架，`DESIGN.md` §12.6）。
 *
 * 它只做一件事：**把可见的那几行渲染出来**，其余用上下留白顶住，
 * 让滚动条长度与真实内容一致。行的划分（tile 行 / 分组标题行）与内容
 * 由调用方决定 —— 这个组件不认识照片，也不认识时间分组。
 *
 * 为什么不用现成的虚拟化库（`plans/M1-5.md` Q1 已定）：窗口计算是
 * 前缀和 + 二分（`lib/virtual-window.ts`），一个固定行高、不支持动态尺寸的
 * 最小实现够用；引一个库反而要迁就它的模型（`DESIGN.md` §10 也是这个口径）。
 *
 * ⚠️ **坑**：Ark/Tauri 之类的宿主元素可能给内联 `width/height: 100%`；
 * 这里自己就是滚动容器，尺寸由调用方用 class 给（`flex-1 min-h-0`），
 * 别再往里面叠一层「100%」的包装。
 */

import {
  createEffect,
  createSignal,
  For,
  onCleanup,
  onMount,
  type JSX,
} from "solid-js";
import { computeVirtualWindow, rowScrollTop } from "../../lib/virtual-window.ts";

export interface VirtualGridRow {
  /** 稳定键（重建列表时用来复用 DOM） */
  key: string;
  /** 行高（px） */
  height: number;
}

export interface VirtualGridProps<TRow extends VirtualGridRow> {
  rows: readonly TRow[];
  /** 渲染一行（index 是**全列表**下标，不是窗口内下标） */
  renderRow: (row: TRow, index: number) => JSX.Element;
  /** 上下各多渲染几行（默认 2） */
  overscan?: number;
  /**
   * 变化时把滚动位置回到顶部（例：换了目录）。
   * 传 `undefined` 表示不重置。
   */
  resetKey?: string | number;
  /**
   * 可见行区间变化时回调（`[start, end)`，含 overscan）。
   *
   * 用途：**按需取数据**。浏览网格的数据是从库里分页取的，只有知道
   * 「现在看到哪几行」才能只补缺的那几页（`features/browse/store.ts` 的 `ensureRange`）。
   */
  onVisibleRange?: (start: number, end: number) => void;
  /**
   * 把这个**行下标**滚进视野（值变化时执行）。
   *
   * 用途：键盘导航 —— `←`/`→` 换了「当前那张」之后，网格得跟上，
   * 否则焦点在屏幕外飘着，用户以为按键没反应。已经看得见时**一个像素都不动**
   * （数学在 `lib/virtual-window.ts` 的 `rowScrollTop`，有单测）。
   */
  focusRow?: number;
  class?: string;
}

export function VirtualGrid<TRow extends VirtualGridRow>(
  props: VirtualGridProps<TRow>,
) {
  let scroller: HTMLDivElement | undefined;
  const [scrollTop, setScrollTop] = createSignal(0);
  const [viewportHeight, setViewportHeight] = createSignal(0);

  onMount(() => {
    if (!scroller) return;
    const measure = (): void => {
      setViewportHeight(scroller?.clientHeight ?? 0);
    };
    measure();
    // 容器尺寸会跟着分栏拖拽 / 窗口缩放变 —— 用 ResizeObserver 而不是监听 window
    const observer = new ResizeObserver(measure);
    observer.observe(scroller);
    onCleanup(() => observer.disconnect());
  });

  // 换目录 / 换模式：回到顶部（否则会停在一个不存在的滚动位置）
  createEffect(() => {
    const key = props.resetKey;
    if (key === undefined || !scroller) return;
    scroller.scrollTop = 0;
    setScrollTop(0);
  });

  // 键盘导航：把目标行滚进视野（已经看得见就不动）
  createEffect(() => {
    const target = props.focusRow;
    if (target === undefined || !scroller) return;
    const next = rowScrollTop({
      rows: props.rows,
      target,
      scrollTop: scroller.scrollTop,
      viewportHeight: scroller.clientHeight,
    });
    if (next !== scroller.scrollTop) {
      scroller.scrollTop = next;
      setScrollTop(next);
    }
  });

  const window = () =>
    computeVirtualWindow({
      rows: props.rows,
      viewportHeight: viewportHeight(),
      scrollTop: scrollTop(),
      overscan: props.overscan,
    });

  // 可见区间变化 → 通知调用方（按需取数据）。只在真的变了的时候回调。
  let lastRange = "";
  createEffect(() => {
    const w = window();
    const key = `${w.startIndex}:${w.endIndex}`;
    if (key === lastRange) return;
    lastRange = key;
    props.onVisibleRange?.(w.startIndex, w.endIndex);
  });

  return (
    <div
      ref={scroller}
      class={[
        "min-h-0 flex-1 overflow-y-auto overflow-x-hidden",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
    >
      {/*
        上下留白撑出总高：`paddingTop + 可见行 + paddingBottom = 总高`。
        用 padding 而不是绝对定位，是让「行高就写在行自己身上」这件事成立 ——
        调用方不必再算每行的 top。
      */}
      <div
        style={{
          "padding-top": `${window().paddingTop}px`,
          "padding-bottom": `${window().paddingBottom}px`,
        }}
      >
        <For each={props.rows.slice(window().startIndex, window().endIndex)}>
          {(row, index) =>
            props.renderRow(row, window().startIndex + index())
          }
        </For>
      </div>
    </div>
  );
}

/**
 * 把一组行高累计成「每行 top 位置」——给需要绝对定位的场景用
 * （例如拖拽框选的命中测试）。M1 没用到，但放进导出免得各处自己再写一遍。
 */
export function rowOffsets(rows: readonly VirtualGridRow[]): number[] {
  const offsets: number[] = [];
  let top = 0;
  for (const row of rows) {
    offsets.push(top);
    top += Number.isFinite(row.height) && row.height > 0 ? row.height : 0;
  }
  return offsets;
}
