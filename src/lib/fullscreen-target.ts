/**
 * 全屏看图的**清单构造**（纯函数，`lib/` 这一层不许碰 DOM / feature）。
 *
 * 它回答一个问题：拿「当前显示序 + 锚点」算出「全屏窗口要的那份清单 + 从第几张开始」。
 *
 * ```text
 *   photos（与网格同一份显示序）    anchorId（当前那张）
 *            │                            │
 *            └────────────┬───────────────┘
 *                         ▼
 *        { items: [{id, path, fileName}], index }   ← 或 null（没有当前照片）
 * ```
 *
 * 三条口径：
 *
 * 1. **清单就是网格的显示序**（同一份 `photosFromSource(source)`）—— 所以全屏里的
 *    ←/→ 与网格里的邻居**逐张一致**，筛选 / 排序也一致；
 * 2. **锚点不在清单里就是 `null`**（刚换目录、被筛掉、还没分页到）—— 宁可不开，
 *    也不开到一个「看起来对但不一致」的位置；
 * 3. **只装页面真的用到的三个字段**（`id` / `path` / `fileName`）——
 *    清单可能上千条，把 `marks` 那种壳层字段塞进去只会让 IPC 变胖。
 *
 * ⚠️ 这里**只有「造清单」这一件事**：步进（←/→、PageUp/PageDown）直接用看图 store 的
 * `next()` / `prev()`（它们本来就「到头停住 + 重置成适配窗口」）——
 * 不再写第二套步进规则（§2.12）。
 *
 * 结构类型而不是 import `api/types.ts` 里的 DTO：`lib` 不许反向依赖 `api`（api 里
 * `editor.ts` 已经 import 了 `lib`，再反向就是环）。TS 按结构比较，两边形状一致即可
 * （与 `CommandMarkIntent` 那处同一个做法）。
 */

/** 清单里的一张照片（`path` 是**绝对路径**，`view_image` 直接吃它）。 */
export interface FullscreenTargetPhoto {
  id: string;
  path: string;
  fileName: string;
}

export interface FullscreenTarget {
  items: FullscreenTargetPhoto[];
  index: number;
}

/**
 * 造一份全屏清单；没有当前照片返回 `null`。
 *
 * `anchorId` 为 `null`、或在清单里找不到时都返回 `null`（见文件头第 2 条）。
 */
export function buildFullscreenTarget(
  photos: readonly FullscreenTargetPhoto[],
  anchorId: string | null,
): FullscreenTarget | null {
  if (anchorId === null) return null;
  const index = photos.findIndex((photo) => photo.id === anchorId);
  if (index < 0) return null;
  return {
    items: photos.map((photo) => ({
      id: photo.id,
      path: photo.path,
      fileName: photo.fileName,
    })),
    index,
  };
}
