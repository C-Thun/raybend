/**
 * 「排除」的领域模型（`AGENTS.md` §11.3：排除 ≠ 删除 ≠ 移除）。
 *
 * 排除是**跨目录、跨源**的一件事：多源导入时用户会来回切目录，
 * 排除集合适用**会话级内存**里的一份（键就是照片的绝对路径 —— 后端给的
 * `SourceItem.path` 本身就是它，前后端一套口径，见 `import/runner.rs` 的 `excluded`）。
 *
 * 这个文件只放**纯函数**：路径归属判定与「该从总数里减掉几张」。
 * 它同时被左列/右列底部统计与后端参数用到，所以放在 `lib/`（跨 feature 的领域模型）。
 */

import type { CheckedDir } from "./checked-dir.ts";
import { pathKey } from "./tree.ts";

/**
 * 这个文件是不是落在某个已勾选目录的导入范围内。
 *
 * 语义**必须与后端一致**（`ScanOptions.max_depth`：不含子目录 = 0，只算直属文件）：
 *   - `includeSubdirs = true`：只要在它下面（任意层级）都算；
 *   - `includeSubdirs = false`：只有**直属**文件算（相对路径里不能再有分隔符）。
 *
 * 比较走 `pathKey`（NFC + 分隔符统一 + 折叠大小写 + 去尾斜杠）——
 * 与「选中是不是同一个目录」同一套判据，不另立一套。
 */
export function isUnderDir(
  filePath: string,
  dirPath: string,
  includeSubdirs: boolean,
): boolean {
  const file = pathKey(filePath);
  const dir = pathKey(dirPath);
  if (file === "" || dir === "") return false;
  if (file === dir) return true; // 目录自身（正常不会出现，保守算「在里面」）
  const prefix = dir.endsWith("/") ? dir : `${dir}/`;
  if (!file.startsWith(prefix)) return false;
  if (includeSubdirs) return true;
  return !file.slice(prefix.length).includes("/");
}

/**
 * 落在**已勾选目录**范围内的排除张数 —— 也就是「已选择 M 张照片」里该减掉的那部分。
 *
 * 为什么要按目录算而不是直接数排除集合：排除是跨源的，
 * 用户可能排除了某个目录里的照片、后来又把那个目录**取消勾选**了 ——
 * 那些排除不该再影响这次的计数（用户还能再勾回来，排除不会丢）。
 */
export function countExcludedInDirs(
  excluded: Iterable<string>,
  dirs: readonly CheckedDir[],
): number {
  if (dirs.length === 0) return 0;
  let count = 0;
  for (const file of excluded) {
    if (dirs.some((dir) => isUnderDir(file, dir.path, dir.includeSubdirs))) {
      count += 1;
    }
  }
  return count;
}

/**
 * 这次导入实际会带进来多少张。
 *
 * 已知总数 `null`（还有目录没数完）时保持 `null` —— 界面显示「统计中」，
 * 而不是拿一个偏小的数骗人（同 `sumPhotoCounts` 的口径）。
 */
export function importPhotoCount(
  total: number | null,
  excludedInDirs: number,
): number | null {
  if (total === null) return null;
  return Math.max(0, total - excludedInDirs);
}

/**
 * 反转一组文件的排除状态（`DESIGN.md` §12.2：批量排除是**反转**，不是单向设置）：
 * 未排除的排除、已排除的恢复。返回新的集合（不改原集合）。
 */
export function invertExcluded(
  current: ReadonlySet<string>,
  paths: readonly string[],
): ReadonlySet<string> {
  if (paths.length === 0) return current;
  const next = new Set(current);
  for (const path of paths) {
    if (next.has(path)) next.delete(path);
    else next.add(path);
  }
  return next;
}
