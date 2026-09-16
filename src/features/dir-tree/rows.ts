/**
 * 目录树的行模型（纯函数，`design/main.md` §3.1.2）。
 *
 * 为什么自己算行、而不是直接用 `lib/tree.ts` 的 `flattenVisible`：
 * 树上显示的是**懒加载**的子目录 —— 没展开过的目录压根没读过它有哪些孩子。
 * 而「有没有孩子」决定要不要画展开箭头，所以这里采用**乐观**规则：
 * 只要还没读过，就先认为它可展开（读回来是空的再把箭头收掉）。
 * `flattenVisible` 是从 `children` 反推 `hasChildren` 的，表达不了这个意图。
 *
 * 另外两条设计约束在这里落地：
 *   - **只显示目录**（子目录列表由 `listDirs` 给，本身就只含目录）；
 *   - **选中不触发展开**：`isExpanded` 只看用户的展开操作，行模型不看选中。
 */

import type { DirEntry, Volume, VolumeKind } from "../../api/types.ts";

/** 树里的一行。 */
export interface TreeRow {
  /** 完整路径（同时也是身份键） */
  path: string;
  /** 显示名：卷是完整路径（`D:\`、`//nas/photos`），目录是末级名字 */
  name: string;
  /** 0 = 卷/根 */
  depth: number;
  /** 卷的来源类型；普通目录是 `null` */
  volumeKind: VolumeKind | null;
  /** 是否画展开箭头（见模块文档的「乐观」规则） */
  expandable: boolean;
  /** 是否已展开 */
  expanded: boolean;
}

export interface TreeRowInput {
  volumes: readonly Volume[];
  /** 某目录的已读子目录；没读过时返回 `undefined` */
  childrenOf: (path: string) => readonly DirEntry[] | undefined;
  isExpanded: (path: string) => boolean;
  /**
   * 深度上限（防环形引用撑爆渲染）。
   * 目录树理论上不会成环（文件系统是树），但数据来自扫描器，防御性留一道闸。
   */
  maxDepth?: number;
}

const DEFAULT_MAX_DEPTH = 32;

/**
 * 把「卷 + 已展开的子目录」摊平成可见行（深度优先、顺序即显示顺序）。
 *
 * **折叠节点的子树整棵不出现** —— 它压根不进这个数组。
 */
export function buildTreeRows(input: TreeRowInput): TreeRow[] {
  const maxDepth = Number.isFinite(input.maxDepth)
    ? Math.max(0, Math.floor(input.maxDepth as number))
    : DEFAULT_MAX_DEPTH;

  const rows: TreeRow[] = [];

  /** 递归下降：用显式栈而不是递归，深度由用户的目录结构决定 */
  const push = (path: string, name: string, depth: number, volumeKind: VolumeKind | null) => {
    const expanded = input.isExpanded(path);
    const children = volumeKind === null ? input.childrenOf(path) : input.childrenOf(path);
    const expandable = children === undefined ? true : children.length > 0;
    rows.push({ path, name, depth, volumeKind, expandable, expanded });
    if (!expanded || depth >= maxDepth || children === undefined) return;
    for (const child of children) {
      push(child.path, child.name, depth + 1, null);
    }
  };

  for (const volume of input.volumes) {
    push(volume.path, volumeDisplayName(volume.path), 0, volume.kind);
  }
  return rows;
}

/**
 * 卷在树里显示成什么。
 *
 * Windows 的 `D:\\` 已经是「盘符 + 反斜杠」，直接用；
 * Linux 的挂载点 `/` 或 `/mnt/nas` 也直接用（比只显示最后一段更好认 ——
 * `/mnt/nas` 与 `/media/usb` 的末级可能都叫 `photos`）。
 */
export function volumeDisplayName(path: string): string {
  return path;
}
