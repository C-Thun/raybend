/**
 * **整行拖动排序**（全项目唯一一份：LUT 应用层 / 编辑区层）。
 *
 * ```text
 *  ┌──────────────┐        行抓起来 → 原位留空、目标槽露出一个洞、其它行让位，
 *  │ ▢ 天空    ⏻ 🚫 │        光标下面跟一份**整行幽灵**（`position: fixed`，只做视觉）
 *  └──────────────┘
 * ```
 *
 * # 为什么抽出来
 *
 * 这段手势（阈值 4px、出窗即取消、指针捕获、让位动画、槽位几何、边缘自动滚）
 * 在 LUT 链与编辑区层里是**同一件事**（崔总 2026-11-01：不要再造轮子）。
 * 面板只管「我有哪些行」「松手后怎么改数据」，几何与状态机在这里。
 *
 * 几何是纯函数（`rowDropTarget` / `rowShift`），有单测：`lib/row-drag-sort.ts`。
 *
 * 用法（两个面板一样）：
 *
 * ```tsx
 * const sort = createRowDragSort({
 *   list: () => listElement,
 *   rowSelector: "[data-my-row]",
 *   count: () => rows().length,
 *   onDrop: (from, to) => move(from, to),
 * });
 * // 行： onPointerDown={(e) => sort.start(e, index())}
 * //     style={{ transform: `translateY(${sort.shift(index())}px)` }}
 * //     拖动中的行自己加 `invisible`
 * // 列表里： {sort.slot()}   列表外： {sort.ghost()}
 * ```
 */

import { Portal } from "solid-js/web";
import { createSignal, Show, type Accessor, type JSX } from "solid-js";

import { rowDropTarget, rowShift } from "../../lib/row-drag-sort.ts";
import { trackPointerDrag } from "../../lib/pointer-drag.ts";

/** 让位动画时长（与 CSS 里的过渡一致）。 */
const SHIFT_MS = 140;

export interface RowDragSortOptions {
  /** 行容器（几何、`row-gap`、`getBoundingClientRect` 都测它） */
  list: () => HTMLElement | undefined;
  /** 行元素选择器（`querySelectorAll` 的顺序必须与 `index` 一致） */
  rowSelector: string;
  /** 行数 */
  count: () => number;
  /** 行内这些元素上的按下**不**算整行拖动（开关 / 按钮 / 输入框） */
  noDragSelector?: string;
  /** 拖动中自动滚动的容器（可选；不传就不滚） */
  scrollSelector?: string;
  /** 跟着光标的那份行内容（视觉副本；松手后丢弃） */
  ghost: (index: number) => JSX.Element;
  /** 松手且位置变了 */
  onDrop: (from: number, to: number) => void;
}

export interface RowDragSort {
  /** 行上的 `pointerdown` 调它 */
  start(event: PointerEvent, index: number): void;
  /** 正在拖的行（`null` = 没在拖） */
  from: Accessor<number | null>;
  /** 目标槽位（0..count-1） */
  to: Accessor<number>;
  /** 第 `index` 行该让多少像素（140ms 过渡由调用方写在样式里） */
  shift(index: number): number;
  /** 目标槽位的「洞」——放在**行容器内部**（绝对定位） */
  slot(): JSX.Element;
  /** 跟光标的整行幽灵——放在**行容器外面**（`Portal` + `fixed`） */
  ghost(): JSX.Element;
}

/** 默认「这些元素不参与整行拖动」：按钮 / 输入 / 开关 / 滑杆。 */
const NO_DRAG = "button, input, label, [role='slider'], [role='switch']";

export function createRowDragSort(options: RowDragSortOptions): RowDragSort {
  const [from, setFrom] = createSignal<number | null>(null);
  const [to, setTo] = createSignal(0);
  const [slotPx, setSlotPx] = createSignal(0);
  const [rowHeight, setRowHeight] = createSignal(0);
  const [ghostAt, setGhostAt] = createSignal<{ left: number; top: number; width: number } | null>(null);

  const shift = (index: number): number => {
    const source = from();
    if (source === null) return 0;
    return rowShift(source, to(), index, slotPx());
  };

  const start = (event: PointerEvent, index: number): void => {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement | null;
    // 调用方的「不拖」选择器是**追加**在默认那一组后面的：
    // 行内的按钮 / 输入 / 开关（Ark 的根是 `<label>`）/ 滑杆永远不参与整行拖动。
    const noDrag = options.noDragSelector === undefined
      ? NO_DRAG
      : `${NO_DRAG}, ${options.noDragSelector}`;
    if (target?.closest(noDrag)) return;
    const list = options.list();
    if (list === undefined) return;
    const row = list.querySelectorAll<HTMLElement>(options.rowSelector)[index];
    if (row === undefined) return;
    const count = options.count();
    if (count <= 1) return;
    const gap = Number.parseFloat(getComputedStyle(list).rowGap) || 0;
    const rect = row.getBoundingClientRect();
    const slot = rect.height + gap;
    const grabY = event.clientY - rect.top;
    const update = (point: { y: number }): void => {
      const box = list.getBoundingClientRect();
      setTo(rowDropTarget(point.y, box.top, slot, count));
      setGhostAt({ left: box.left, width: box.width, top: point.y - grabY });
      const scroller = options.scrollSelector === undefined
        ? null
        : list.closest<HTMLElement>(options.scrollSelector);
      if (scroller === null) return;
      const area = scroller.getBoundingClientRect();
      if (point.y < area.top + 24) scroller.scrollTop -= 10;
      else if (point.y > area.bottom - 24) scroller.scrollTop += 10;
    };
    trackPointerDrag(event, {
      capture: true,
      threshold: 4,
      cancelOutsideWindow: true,
      start: (point) => {
        setSlotPx(slot);
        setRowHeight(rect.height);
        setFrom(index);
        setTo(index);
        update(point);
      },
      move: update,
      end: (_point, cancelled) => {
        const source = from();
        const destination = to();
        setFrom(null);
        setGhostAt(null);
        setSlotPx(0);
        setRowHeight(0);
        if (!cancelled && source !== null && source !== destination) {
          options.onDrop(source, destination);
        }
      },
    });
  };

  return {
    start,
    from,
    to,
    shift,
    slot: () => (
      <Show when={from() !== null}>
        <div
          data-row-drop-slot
          aria-hidden="true"
          class="pointer-events-none absolute inset-x-0 rounded-ui bg-state-selected ring-1 ring-brand"
          style={{
            top: `${to() * slotPx()}px`,
            height: `${rowHeight()}px`,
            transition: `top ${SHIFT_MS}ms ease`,
          }}
        />
      </Show>
    ),
    ghost: () => (
      <Show when={ghostAt()}>
        {(at) => (
          <Portal>
            <div
              data-row-drag-ghost
              aria-hidden="true"
              class="pointer-events-none fixed z-(--z-modal) rounded-ui bg-surface-layer shadow-ui ring-1 ring-brand/40"
              style={{
                left: `${at().left}px`,
                top: `${at().top}px`,
                width: `${at().width}px`,
              }}
            >
              {from() === null ? null : options.ghost(from() as number)}
            </div>
          </Portal>
        )}
      </Show>
    ),
  };
}

/** 行的让位过渡（拖动中才有；不拖时不动，避免每次渲染都排一次动画）。 */
export function rowShiftTransition(dragging: boolean): string {
  return dragging ? `transform ${SHIFT_MS}ms ease` : "none";
}
