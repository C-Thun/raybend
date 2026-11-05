/**
 * 整行拖动排序的**纯几何**（LUT 应用层 / 编辑区层共用）。
 *
 * 抽成纯函数是为了能单测：面板里只剩指针事件与 DOM 测量，
 * 「拖到哪一格」「其它行该让多少」不依赖 DOM、也不依赖 Solid。
 * 手势与视觉件在 `components/ui/RowDragSort.tsx`（全项目唯一一份）。
 */

/**
 * 指针纵向位置 → 目标槽位下标。
 *
 * `pointerY` 与 `listTop` 都是**视口坐标**（`getBoundingClientRect` 的口径），
 * 所以滚动时不需要额外换算。
 */
export function rowDropTarget(pointerY: number, listTop: number, slot: number, count: number): number {
  if (count <= 0) return 0;
  if (!(slot > 0)) return 0;
  const index = Math.round((pointerY - listTop) / slot);
  return Math.min(count - 1, Math.max(0, index));
}

/**
 * 第 `index` 行在拖动中要位移多少像素（让开被拖走的那一格）。
 *
 * 规则：源行隐形留在原位；目标槽位成为空洞 ——
 * 源在目标之前时，中间的行整体上移一格；反过来则下移一格。
 */
export function rowShift(from: number, to: number, index: number, slot: number): number {
  if (from === to || index === from) return 0;
  if (from < to && index > from && index <= to) return -slot;
  if (from > to && index >= to && index < from) return slot;
  return 0;
}
