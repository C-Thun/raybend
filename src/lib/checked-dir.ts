/**
 * 已勾选目录的模型与统计（`design/main.md` §3.1.3 的「已选目录」）。
 *
 * 放在 `lib/` 而不是某个 feature 里，是因为它同时被三处用到：
 * 左列的横条（`features/selected-dirs`）、树的勾选态、以及右列底部
 * 「已选择 N 个目录 · M 张照片」的统计。feature 之间不能互相 import，
 * 所以这类**跨模块的领域模型**只能住在大家都允许 import 的层。
 */

/** 一条已勾选的目录（要一起导入的那些）。 */
export interface CheckedDir {
  /** 原始路径（用户挑的那一条） */
  path: string;
  /** 「包含子目录」——默认关（`design/main.md` §3.1.3） */
  includeSubdirs: boolean;
  /** 这个目录（按其包含子目录设置）有多少张照片；还没数出来时是 `null` */
  photoCount: number | null;
  /** 正在数 */
  counting: boolean;
}

/**
 * 已勾选目录的照片总数。
 *
 * **只要还有一条没数完（或者数失败）就返回 `null`** —— 界面据此显示「统计中…」，
 * 而不是编一个偏小的数字给用户看。「已选择 3 个目录 · 12 张照片」里的 12
 * 如果其实是「12 张已数完的」，那这个数字就是错的。
 */
export function sumPhotoCounts(entries: readonly CheckedDir[]): number | null {
  if (entries.length === 0) return 0;
  let total = 0;
  for (const entry of entries) {
    if (entry.photoCount === null) return null;
    total += entry.photoCount;
  }
  return total;
}

/** 还有目录没数完（或数失败） */
export function hasUncounted(entries: readonly CheckedDir[]): boolean {
  return entries.some(
    (entry) => entry.counting || entry.photoCount === null,
  );
}

/** 按折叠后的路径找一条（大小写/分隔符差异不算不同目录，`AGENTS.md` §7.3） */
export function findChecked(
  entries: readonly CheckedDir[],
  path: string,
): CheckedDir | undefined {
  return entries.find((entry) => entry.path === path);
}
