/**
 * 照片选择模型（`design/main.md` §3.2 / §4.8.4）。
 *
 * 四种选择方式，语义各不相同，**必须分清**：
 *
 * | 操作 | 语义 |
 * | --- | --- |
 * | 单张点选 | 只选这张（替换掉之前的选择） |
 * | `Ctrl` / `Cmd` 点击 | 增减这一张（其余不动） |
 * | `Shift` 点击 | **翻转**「上一次的图（不含）」到「这次点的图（含）」之间每一项的选中状态；区间外一律不动 |
 * | 点日组 / 时间片标题 | 把那一组**全部**加进选择（`全选当天` / `全选此段`） |
 *
 * 锚点（anchor）是「上一次点过的那张」—— `Shift` 翻转从它**之后**一张开始算
 * （`BROWSE.md` §5.2 的严格语义，用户逐字描述：**不含**上一次的图、**含**这次点的图，
 * 且**不需要保存任何区间状态** —— 逻辑简单、每步都是纯粹的翻转）。
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

      /*
       * **每一步都是纯粹的翻转**（`BROWSE.md` §5.2 的严格语义）：
       *   * **不含**锚点（上一次的图）—— 它已经是用户想要的样子了；
       *   * **含**这次点的图 —— 它必须跟着翻转；
       *   * 区间外的每一项**原样不动**。
       *
       * 这里刻意**不保存「上一次的区间」**：人类明确要求「逻辑简单、无需保存复杂状态」。
       * 曾经的实现是把区间**替换**进选择（`new Set(slice)`）—— 那会把区间外**已经选中**的
       * 照片一并清掉（人类 2026-09-16 报的正是这个：「不在反转范围内的选择状态，
       * 原来是什么现在还是什么，不改」）。
       */
      const ids = new Set(state.ids);
      for (let index = from; index <= to; index += 1) {
        if (index === anchorIndex) continue;
        const id = orderedIds[index];
        if (id === undefined) continue;
        if (ids.has(id)) ids.delete(id);
        else ids.add(id);
      }
      return { ids, anchor: target };
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
 * 只移动「当前照片」锚点，不改变选择集合。
 *
 * 对比视图点某一幅时用这条：若改走 `replace`，多选会当场散掉；若每个工作区
 * 各自手改 `{ ...state, anchor }`，import / browse 又会产生两套行为。
 */
export function focusSelection(
  state: SelectionState,
  target: string,
): SelectionState {
  if (!state.ids.has(target) || state.anchor === target) return state;
  return { ids: state.ids, anchor: target };
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

/**
 * 鼠标事件的**修饰键 → 选择模式**（`BROWSE.md` §5.2 的那张表）。
 *
 * 抽出来的原因：**tiles 与胶片带必须完全一致**（人类在 M2-W2 的口述里三次强调
 * 「选择逻辑所有工作流通用」）。两处各写一遍 `event.shiftKey ? … : …` 看起来无害，
 * 但改一处忘一处就是「网格里 Shift 是区间、胶片带里 Shift 是替换」这种人肉 bug ——
 * 而那类 bug 只有用户按下去才会发现。
 *
 * 判定顺序也是规范的一部分：**Shift 优先于 Ctrl**（两个一起按时按区间算）。
 */
export function clickMode(event: {
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
}): SelectMode {
  if (event.shiftKey) return "range";
  if (event.ctrlKey || event.metaKey) return "toggle";
  return "replace";
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
