/**
 * **对比态**的数学（`BROWSE.md` §5.5/§5.7、`plans/M2-W2.md` 2.2–2.3）。
 *
 * ## 状态从哪来：不需要「进入对比」这个动作
 *
 * 对比态不是开关，而是**选择状态的直接映射**（人类 2026-09-18 的口述：
 * 「胶片带中多选 → **自然进入**多图对比；反选图片 → **自然从对比中移除**」）：
 *
 * ```text
 * 看图模式 + 选中 ≥ 2 张  ⇒ 对比态
 * 反选到只剩 1 张         ⇒ 自动退出对比（回到单张看图）
 * ```
 *
 * 所以这里没有任何 `enterCompare()` / `exitCompare()` —— 只有 `compareIds()`：
 * 「现在该对比哪几张」。
 *
 * ## 超过 4 张：只对比**最近选中的 4 张**（人类 2026-09-19 定）
 *
 * 「任何状态下，选中超过 4 张时，仅对比最近选中的 4 张；不支持超过 4 张的照片对比。」
 *
 * * **最近**从哪看：`SelectionState.ids` 是个 `Set`，而 `toggle` / `range` / `extendSelection`
 *   全都是「`new Set(旧)` 再 `add`」—— 新增的落在**末尾**，所以 **Set 的插入顺序就是选择先后**
 *   （`pruneSelection` 也保序）。不新增字段、不动选择内核。
 * * **窗口内从左到右**也按选择先后：最早选中的在最左，它就是画幅基准 ——
 *   例：Ctrl 依次点 A B C D E F ⇒ 窗口 `C D E F`，基准 C，A/B 被挤掉。
 * * **锚点必含**：区间选择时锚点在中间，可能被挤出窗口 —— 那就挤掉窗口里最早的那张，
 *   把锚点放到末尾（它是用户最后一次动作的那张）。
 * * 批量选择（`Ctrl+A`、「日组全选」）没有「先后」可言：插入顺序 = 列表/组内顺序，
 *   于是窗口 = 这批的最后 4 张。行为确定、可解释，只是不是一张张点出来的意图。
 *
 * ## 画幅不一致时怎么办：以**第一幅**为准扣等比例区域
 *
 * 人类原话：「**画幅比例不一致时以第一幅图的画幅比例为准**，对后面的图**扣等比例区域**
 * 做位移同步（例：第一张 4:3、后面是 3:2 → 只能对比『3:2 区域中间抽出来的 4:3』那部分）」。
 * 这里的「第一幅」= 对比窗口里**最早选中**的那张（见上）。
 *
 * * **缩放同步**：同一个倍率对所有画幅都成立（都是同一个盒子）；
 * * **位移按百分比同步**：平移量以「画幅尺寸的百分比」表达，而不是像素 ——
 *   因为两张图的像素尺寸本来就不同（4000×3000 与 6000×4000），
 *   同一个像素位移在两边的观感完全不同。
 *
 * 这两条正是 `plans/M2-W2.md` 2.3 的两条硬要求。
 */

import type { ViewerPhoto } from "../components/ui/viewer/index.ts";

/** 同时对比的**上限**（人类 2026-09-18：对比 2–4 张） */
export const COMPARE_MAX = 4;

export interface Size {
  width: number;
  height: number;
}

export interface CropRect extends Size {
  x: number;
  y: number;
}

/**
 * 算出「现在该对比哪几张」——**最近选中的 `COMPARE_MAX` 张**，按选择先后排。
 *
 * * `selected` 传 `store.selection().ids`（`Set` 的插入顺序 = 选择先后）；
 * * `orderedIds` 是当前列表的显示顺序，只用来**判定 id 还在不在当前列表里**
 *   （换目录后可能残留旧选择）；
 * * `anchor` 是「最后一次点中的那张」（`store.selection().anchor`）：它在窗口外时
 *   挤掉窗口里最早的那张，自己放到末尾 —— 区间选择会让锚点落在中间。
 *
 * 少于 2 张（或当前列表里不足 2 张）返回空数组 —— 调用方据此回到单张看图。
 */
export function compareIds(
  selected: ReadonlySet<string>,
  orderedIds: readonly string[],
  anchor: string | null = null,
): string[] {
  if (selected.size < 2) return [];
  const present = new Set(orderedIds);
  // 选择先后：Set 的迭代顺序就是插入顺序（见文件头的说明）
  const recent: string[] = [];
  for (const id of selected) {
    if (present.has(id)) recent.push(id);
  }
  if (recent.length < 2) return [];

  // 窗口 = 最近选中的那几张（不够上限时就是全部）
  let window = recent.slice(-COMPARE_MAX);
  if (anchor !== null && recent.includes(anchor) && !window.includes(anchor)) {
    window = [...window.slice(1), anchor];
  }
  return window;
}

/**
 * 布局：几张画幅怎么摆（人类 2026-09-19 定：2 张一排 / 3 张一排三个 / 4 张 2×2）。
 *
 * 为啥不是「永远一排」：1600 宽的窗口减去两侧栏后，4 张一排每张只剩 ≈240px，
 * 看细节不够；2×2 每张能大一倍。每种布局里**每格尺寸一致**，
 * 所以「所有画幅扣成同一个比例」仍然成立（格子大小不影响比例与百分比同步）。
 */
export function compareLayout(count: number): { columns: number; rows: number } {
  const safe = Number.isFinite(count) ? Math.floor(count) : 0;
  const n = Math.max(0, Math.min(COMPARE_MAX, safe));
  if (n >= 4) return { columns: 2, rows: 2 };
  if (n === 3) return { columns: 3, rows: 1 };
  return { columns: Math.max(1, n), rows: 1 };
}

/**
 * 基准比例 = **第一幅**的宽高比。
 *
 * 尺寸未知（`natural` 缺失）时返回 `null` —— 调用方退回「各自用自己的比例」，
 * 而不是编一个比例出来（编错了整屏都会歪）。
 */
export function baselineAspect(photos: readonly ViewerPhoto[]): number | null {
  const first = photos[0];
  const size = first?.natural;
  if (!size || size.width <= 0 || size.height <= 0) return null;
  return size.width / size.height;
}

/**
 * 在**一张图自己的像素**里，居中扣出比例为 `aspect` 的那块。
 *
 * 三种情况都要对（都有单测）：
 * * 比基准**宽** → 扣两侧（保留高度）；
 * * 比基准**窄/高** → 扣上下（保留宽度）；
 * * 比例已经一致 → 整张，一点不扣（不许出现 ±0.5px 的滑动）；
 * * 尺寸未知或非法 → 原样返回（不猜）。
 */
export function cropToAspect(size: Size, aspect: number): CropRect {
  const { width, height } = size;
  if (!(width > 0) || !(height > 0) || !(aspect > 0) || !Number.isFinite(aspect)) {
    return { x: 0, y: 0, width: Math.max(0, width), height: Math.max(0, height) };
  }
  const current = width / height;
  // 比例一致（含浮点误差）：整张，别为了 1e-9 的差别去裁一像素
  if (Math.abs(current - aspect) <= 1e-6) {
    return { x: 0, y: 0, width, height };
  }
  if (current > aspect) {
    // 太宽：扣两侧
    const cropped = height * aspect;
    return { x: (width - cropped) / 2, y: 0, width: cropped, height };
  }
  // 太窄/太高：扣上下
  const cropped = width / aspect;
  return { x: 0, y: (height - cropped) / 2, width, height: cropped };
}

/** 一帧：某张照片 + 它在自己像素里要显示的那块 */
export interface CompareFrame {
  photo: ViewerPhoto;
  /** 扣出来的那块（比例 = 基准比例） */
  crop: CropRect;
}

/**
 * 算出每一帧该显示哪块（**以第一幅的比例为准**）。
 *
 * `photos` 传 `compareShown(compareIds(...))` 的结果（已经按显示顺序、已截到上限）。
 */
export function compareFrames(photos: readonly ViewerPhoto[]): CompareFrame[] {
  const aspect = baselineAspect(photos);
  return photos.map((photo) => ({
    photo,
    crop:
      aspect === null
        ? cropToAspect(photo.natural ?? { width: 0, height: 0 }, 0)
        : cropToAspect(photo.natural ?? { width: 0, height: 0 }, aspect),
  }));
}

/**
 * 位移归一化：像素 → **画幅的百分比**。
 *
 * 同步位移必须按百分比换算（`plans/M2-W2.md` 2.3 的原话）：
 * 4000px 宽的图往右挪 200px 与 6000px 宽的图往右挪 200px，观感完全不同；
 * 换算成「挪了画幅的 5%」两边才一致。
 *
 * 尺寸未知时返回 0（宁可不同步，也不要拿一个假的百分比去乱挪）。
 */
export function panPercent(pan: { x: number; y: number }, size: Size): { x: number; y: number } {
  return {
    x: size.width > 0 ? pan.x / size.width : 0,
    y: size.height > 0 ? pan.y / size.height : 0,
  };
}

/** 位移还原：画幅的百分比 → 某个具体画幅的像素（`panPercent` 的反函数） */
export function percentToPan(
  percent: { x: number; y: number },
  size: Size,
): { x: number; y: number } {
  return { x: percent.x * size.width, y: percent.y * size.height };
}
