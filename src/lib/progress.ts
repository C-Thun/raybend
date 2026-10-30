/**
 * 进度条取值的唯一算法（`src/components/ui/Progress.tsx` 与两处导入的调用方共用）。
 *
 * 口径：
 *   * **没有总数 → `null`**（界面走不确定态，不编百分比）；
 *   * 有总数 → 0–100 的整数，**溢出与负数都夹住**（后端计数偶尔会比 total 多一条，
 *     那是缩略图入队之类的收尾动作，界面不该显示 103%）。
 */

/** `done / total` → 0–100 的整数百分比；`total` 不是正数（或不是有限数）时返回 `null`。 */
export function percentOf(done: number, total: number): number | null {
  if (!Number.isFinite(done) || !Number.isFinite(total) || total <= 0) {
    return null;
  }
  return Math.min(100, Math.max(0, Math.round((done / total) * 100)));
}
