/**
 * 侧栏宽度拖拽的**数学**（`plans/M2-W2-tail.md` 6.1）。
 *
 * 单独抽出来是为了两件事：一是**能测**（拖拽本身在冒烟里不好造），
 * 二是「哪边往哪拖是变宽」这种容易搞反的符号判断只写一次。
 */

export interface WidthBounds {
  min: number;
  max: number;
}

/** 夹在范围内、取整（半像素宽度在 subpixel 布局下会渗出 1px 的缝） */
export function clampWidth(width: number, bounds: WidthBounds): number {
  if (!Number.isFinite(width)) return bounds.min;
  return Math.round(Math.min(bounds.max, Math.max(bounds.min, width)));
}

/**
 * 拖拽中的新宽度。
 *
 * `invert` 用在**右列**：手柄在右列的左边，往右拖（`dx > 0`）是把右列**缩小**。
 * 搞反的话表现就是「拖右边那根，左边跟着变」——一眼能看出来但很难回推是符号错了。
 */
export function resizeWidth(input: {
  /** 拖拽开始时那一列的宽度 */
  start: number;
  /** 指针横向位移（像素，右为正） */
  dx: number;
  /** 手柄在列的右侧（右列）时为 true */
  invert?: boolean;
  bounds: WidthBounds;
}): number {
  const sign = input.invert === true ? -1 : 1;
  return clampWidth(input.start + input.dx * sign, input.bounds);
}

/** 键盘微调：每次给一个增量（像素），照样夹范围 */
export function nudgeWidth(current: number, delta: number, bounds: WidthBounds): number {
  return clampWidth(current + delta, bounds);
}
