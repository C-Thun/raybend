/**
 * 库内目录的**显示口径**（`memory/FUNCTION-BROWSE.md` §4.3；三条口径来自人类 2026-09-18）。
 *
 * | 口径 | 做法 | 为什么 |
 * | --- | --- | --- |
 * | 树的根 = `photos/` 之内 | 从 `photos` 这一级往下走，树上**不出现** `photos` 本身 | 「打开库就直接是 `photos/` 下的内容」——库根那一层没有用户要看的东西 |
 * | `_RAW` 不显示 | 任何层级都滤掉这个名字 | 它是位图/RAW 配对的**保留目录**（`memory/FUNCTION-REPOSITORY.md` §4.1），不是用户的文件夹 |
 * | 行名 = 相对路径的最后一段 | `dirDisplayName()` | 树的缩进已经表达了层级，行里再写全路径是噪声 |
 *
 * **只影响显示**：`scopePath` 与 `asset_files.rel_path` 仍然带 `photos/` 前缀
 * （后端要的就是那个口径，见 `crates/raybend/src/store/assets.rs`）。
 * 这里全部是纯函数，没有查询参数、没有 Tauri。
 */

import type { DirEntry } from "../../api/types.ts";

/** `photos/` —— 库内照片的落地根，也是目录树的根（库根不再是树的根）。 */
export const PHOTOS_DIR = "photos";

/** `recycle/` —— 库内回收站目录（与 `photos/` 同级；`specs/library-recycle-bin.md`）。 */
export const RECYCLE_DIR = "recycle";

/** `_RAW/` —— 保留目录名。Rust 侧同名常量：`crates/raybend/src/import/plan.rs` 的 `RAW_DIR`。 */
export const RAW_DIR = "_RAW";

/** 这个名字是保留目录 `_RAW` 吗（Windows 口径：**大小写不敏感**，前后空白忽略）。 */
export function isRawDirName(name: string): boolean {
  return name.trim().toLowerCase() === RAW_DIR.toLowerCase();
}

/**
 * 树里该显示的子目录（把保留目录滤掉）。
 *
 * 输入输出都是 `DirEntry`（`name` + 绝对 `path`），**不动 `path`** ——
 * 上层还要用它换算库内相对路径（`BrowsePanels` 的 `toRelPath`）。
 */
export function visibleChildDirs(entries: readonly DirEntry[]): DirEntry[] {
  return entries.filter((entry) => !isRawDirName(entry.name));
}

/** 库内相对路径 → 树行上显示的名字（最后一段）。空路径与 `photos` 本身都显示成 `photos`。 */
export function dirDisplayName(relPath: string): string {
  const trimmed = relPath.trim().replace(/[\\/]+$/, "");
  if (trimmed === "") return PHOTOS_DIR;
  const parts = trimmed.split(/[\\/]/);
  return parts[parts.length - 1] ?? trimmed;
}

/**
 * 点一下目录行之后，浏览范围应该变成什么（崔总 2026-11-05 定）。
 *
 * | 当前范围 | 点的是 | 结果 |
 * | --- | --- | --- |
 * | 别的目录 / 库根 | A | `A`（选中） |
 * | A | A | `null` = **库根**（再点一次 = 取消选中并上到库根） |
 *
 * 库根不是一个能"取消选中"的行：它就是"没选任何目录"的那个状态，
 * 中列显示「当前处于相片库根目录」的水印（见 `gridWatermark`）。
 */
export function nextDirScope(current: string | null, clicked: string): string | null {
  return current === clicked ? null : clicked;
}

/**
 * 「新建目录」应该建在哪一级（崔总 2026-11-05 定）。
 *
 * 选中某个目录 → 建在它下面；库根状态（`scope === null`）→ 建在树的根那一级。
 * 树的根是 `photos/`（`memory/FUNCTION-BROWSE.md` §4.3），**不是库根** ——
 * 建在库根会让目录在树里看不见。
 */
export function createParentOf(scope: string | null): string {
  return scope === null ? PHOTOS_DIR : scope;
}

/**
 * 一张照片**现在所在的目录**（库内相对路径）。
 *
 * RAW 在 `<目录>/_RAW/` 里，所以要把末段 `_RAW` 折算回去再比 —— 否则
 * 「已经在这个目录里的 RAW-only 资产」会被误判成「还要搬一次」。
 * `null` / 空串 = 本地还不知道它在哪（没加载到），调用方**不许**据此跳过搬运。
 */
export function photoDirOf(relPath: string | null): string | null {
  if (relPath === null) return null;
  const normalized = relPath.replace(/\\/g, "/");
  const dir = normalized.includes("/")
    ? normalized.slice(0, normalized.lastIndexOf("/"))
    : "";
  if (dir.toLowerCase().endsWith("/_raw")) return dir.slice(0, -5);
  return dir;
}
