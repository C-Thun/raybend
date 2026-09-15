/**
 * 照片选择模型（`design/main.md` §3.2 / §4.8.4）。
 *
 * 四种选择方式，语义各不相同，**必须分清**：
 *
 * | 操作 | 语义 |
 * | --- | --- |
 * | 单张点选 | 只选这张（替换掉之前的选择） |
 * | `Ctrl` / `Cmd` 点击 | 增减这一张（其余不动） |
 * | `Shift` 点击 | 从**锚点**到这张之间的区间（含两端），替换掉之前的选择 |
 * | 点日组 / 时间片标题 | 把那一组**全部**加进选择（`全选当天` / `全选此段`） |
 *
 * 锚点（anchor）是「上一次单点/增减的那张」—— `Shift` 区间从它开始算。
 * 锚点不在当前列表里时（换了目录、列表被筛过）**退化成单张选中**，
 * 而不是猜一个位置：猜错会让用户一次选中一大片不该选的照片。
 *
 * 另外这里还有**排除**用的集合反转：`批量排除` 是**反转**语义
 * （未排除 → 排除；已排除 → 取消排除），不是「一律排除」。
 */

export type SelectMode = "replace" | "toggle" | "range";

export interface SelectionState {
  /** 当前选中的 id 集合 */
  ids: ReadonlySet<string>;
  /** 区间选择的锚点（`null` = 还没有锚点） */
  anchor: string | null;
}

export const EMPTY_SELECTION: SelectionState = { ids: new Set(), anchor: null };

/**
 * 按一次点击更新选择。
 *
 * `orderedIds` 是**当前视图顺序**（照片网格的显示顺序）—— 区间按它算，
 * 而不是按拍下的时间：用户看到的顺序就是选择的顺序。
 * `target` 不在 `orderedIds` 里时**原样返回**（不动选择）。
 */
export function applySelection(
  state: SelectionState,
  orderedIds: readonly string[],
  target: string,
  mode: SelectMode,
): SelectionState {
  if (orderedIds.length === 0) return state;
  const targetIndex = orderedIds.indexOf(target);
  if (targetIndex === -1) return state;

  switch (mode) {
    case "toggle": {
      const ids = new Set(state.ids);
      if (ids.has(target)) ids.delete(target);
      else ids.add(target);
      return { ids, anchor: target };
    }
    case "range": {
      const anchorIndex =
        state.anchor === null ? -1 : orderedIds.indexOf(state.anchor);
      if (anchorIndex === -1) {
        // 没有可用锚点 → 与单张点选等价（不猜位置）
        return { ids: new Set([target]), anchor: target };
      }
      const from = Math.min(anchorIndex, targetIndex);
      const to = Math.max(anchorIndex, targetIndex);
      return { ids: new Set(orderedIds.slice(from, to + 1)), anchor: target };
    }
    default: {
      return { ids: new Set([target]), anchor: target };
    }
  }
}

/**
 * 把一组 id **全部**加进选择（日组 / 时间片的「全选」）。
 *
 * 锚点取这一组的第一张：用户接着按 `Shift` 点别处时，
 * 区间会从这一组的开头算起 —— 与「我刚选了这一组」的直觉一致。
 */
export function extendSelection(
  state: SelectionState,
  idsToAdd: readonly string[],
): SelectionState {
  if (idsToAdd.length === 0) return state;
  const ids = new Set(state.ids);
  for (const id of idsToAdd) ids.add(id);
  return { ids, anchor: state.anchor ?? idsToAdd[0] };
}

/** 全选（`Ctrl+A`） */
export function selectAll(orderedIds: readonly string[]): SelectionState {
  if (orderedIds.length === 0) return EMPTY_SELECTION;
  return { ids: new Set(orderedIds), anchor: orderedIds[0] };
}

/** 清空选择 */
export function clearSelection(): SelectionState {
  return EMPTY_SELECTION;
}

/**
 * `批量排除`：对给定的一组 id 做**反转**（在集合里就拿出来，不在就放进去）。
 *
 * 这是 `DESIGN.md` §12.2 明确要求的语义：按钮文字恒定、动作恒定，
 * 不做「已排除时变成反排除」那种会让人猜的设计。
 */
export function invertSet(
  base: ReadonlySet<string>,
  keys: Iterable<string>,
): Set<string> {
  const next = new Set(base);
  for (const key of keys) {
    if (next.has(key)) next.delete(key);
    else next.add(key);
  }
  return next;
}

/** 选择状态是否为空（`toolsbar` 的批量排除据此禁用） */
export function hasSelection(state: SelectionState): boolean {
  return state.ids.size > 0;
}

/**
 * 选择里有多少张**还留在当前列表**（换目录后旧选择可能残留 ——
 * 统计与按钮可用性都该按当前列表算）。
 */
export function selectionCount(
  state: SelectionState,
  orderedIds: readonly string[],
): number {
  let count = 0;
  for (const id of orderedIds) {
    if (state.ids.has(id)) count += 1;
  }
  return count;
}

/** 把选择收敛到当前列表（换目录 / 重新扫描后调用） */
export function pruneSelection(
  state: SelectionState,
  orderedIds: readonly string[],
): SelectionState {
  const allowed = new Set(orderedIds);
  const ids = new Set<string>();
  for (const id of state.ids) {
    if (allowed.has(id)) ids.add(id);
  }
  const anchor =
    state.anchor !== null && allowed.has(state.anchor) ? state.anchor : null;
  if (ids.size === state.ids.size && anchor === state.anchor) return state;
  return { ids, anchor };
}
