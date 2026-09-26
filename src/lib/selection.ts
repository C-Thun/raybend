/**
 * 照片选择模型（`design/main.md` §3.2 / §4.8.4）。
 *
 * 四种选择方式，语义各不相同，**必须分清**：
 *
 * | 操作 | 语义 |
 * | --- | --- |
 * | 单张点选 | 只选这张（替换掉之前的选择） |
 * | `Ctrl` / `Cmd` 点击 | 增减这一张（其余不动） |
 * | `Shift` 点击 | 把「上一次的主选」到「这次点的主选」之间每一项**置为选中**；区间外一律不动 |
 * | 点日组 / 时间片标题 | 「整段」开关：全选中 → 全取消；否则 → 全选中 |
 *
 * ## 主选（锚点）—— 两端算法**固定不变**（人类 2026-09-26 重申）
 *
 * **主选目标 = 最后一次鼠标点到的那个**（不管当时有没有按 `Shift` / `Ctrl`，点一次就把它设为
 * 主选）。它就是 `SelectionState.anchor`，也用于浏览侧右栏显示哪张的档案、胶片带定位等。
 *
 * `Shift` 的区间**两端永远是「前一个主选」与「当前主选」** ——
 * **不是**「当前所有选中范围」到主选，也不是「最小/最大已选下标记」。
 * 没有反选（`Ctrl` 取消某一项）时用户感觉不到两者差别，但算法是两回事，**不许改**：
 * 以后不管加多少种新模式，这两端的取法都是一样的。
 *
 * 区间**不含起点主选、含终点主选**：起点那张已经是用户想要的样子了，再动它就会把
 * 上一次点选的结果破坏掉。
 *
 * 锚点不在当前列表里时（换了目录、列表被筛过）**退化成单张选中**，
 * 而不是猜一个位置：猜错会让用户一次选中一大片不该选的照片。
 *
 * ## 两个术语（人类 2026-09-26 钉定，不要混用）
 *
 * ### 反转（inversion）—— **固有能力**
 *
 * **单击 ↔ `Ctrl`+单击 可以对调**（`TilesSource.invertedCtrl`）。它是一等的、受支持的
 * 能力，**不是特例、不是 bug**；完整口径见 `BROWSE.md` § 5.2.4。
 *
 * ### 回退（fallback）—— `Shift`+`Ctrl`+单击 **不是操作**
 *
 * 设定上**不存在这个操作**，而且「可能永远也不会出现」—— 人类从反对「按住两个辅助键才能实现的功能」
 * （类比：苹果反对鼠标右键）。为了**避免出现意外的操作功能**，对 `Shift`+`Ctrl`+单击做了
 * **捕获**，并**回退到「当前场景的单击」**。
 *
 * ❗ **回退到什么，由当前场景决定**：当前场景的单击是什么，回退到的就是什么 ——
 *
 * ```text
 * 标准场景：回退到 `replace`（只选这一张）
 * 反转场景：回退到 `toggle`（加减这一张）
 * ```
 *
 * 所以它**没有自己的语义、没有要定案的东西**，也**不要**给它设计行为。
 *
 * 相应地也**不提供区间取消选中**：需要大范围持续性的选中应该用
 * 排除 / 旗标这类持久化手段，而不是“点一下就丢”的选中状态。
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
       * **区间内的每一项置为选中**（人类 2026-09-26 定；在此之前是「翻转」）：
       *   * **不含**起点主选（上一次点的那个）—— 它已经是用户想要的样子了；
       *   * **含**终点主选（这一次点的）—— 它必须变成选中的；
       *   * 区间外的每一项**原样不动**：已经选中的不会被顺手清掉。
       *
       * 两端怎么取（从 `state.anchor` 到 `target`）是**固定算法**，不是本轮可调项 ——
       * 人类原话：「不管有没有按 shift/ctrl 键，最后鼠标点到哪里那个就是主选」，
       * 而区间永远是「前一个主选 → 当前主选」，与「当前所有选中范围」无关。
       * 详见文件头的那一节。
       *
       * 因此这里也**不需要保存「上一次的区间」**：每一步都只看主选与本次目标。
       */
      const ids = new Set(state.ids);
      for (let index = from; index <= to; index += 1) {
        if (index === anchorIndex) continue;
        const id = orderedIds[index];
        if (id === undefined) continue;
        ids.add(id);
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

/**
 * 「整段」开关：日组 / 时间片标题上那颗药丸的语义（人类 2026-09-26 定）。
 *
 * ```text
 * 整段已全选中   →  整段取消
 * 否则（含部分选中）→  整段置为选中（并进现有选择，其它段不动）
 * ```
 *
 * 三条口径：
 *
 * 1. **不是逐项反转**。「部分选中」时是把缺的那些补上，**不是**把已选的那几张翻掉 ——
 *    人类原话：「点一下全段选中，再点一下全段取消；全段选中状态下点其他段增加选中」。
 * 2. **与「点单张照片」是两码事**：这里不走 `applySelection`、不看 `Shift` / `Ctrl` ——
 *    它更接近「整体勾选」那个概念，鼠标怎么按都一样（`BROWSE.md` §3.2 的标记才是带修饰键的）。
 * 3. **锚点取这一组的第一张**：这就是人类说的「主选目标」—— 点一下药丸也是一次点击，
 *    所以主选跟着走到这一组的开头（接着按 `Shift` 点别处时，区间从组首算起）。
 *
 * 和 `extendSelection` 的区别：那个是**只加不减**（导出画廊的整组勾选还在用它），
 * 这个是**开关**。两者不要互相代替。
 */
export function toggleGroupSelection(
  state: SelectionState,
  idsToToggle: readonly string[],
): SelectionState {
  if (idsToToggle.length === 0) return state;
  const allSelected = idsToToggle.every((id) => state.ids.has(id));
  const ids = new Set(state.ids);
  for (const id of idsToToggle) {
    if (allSelected) ids.delete(id);
    else ids.add(id);
  }
  return { ids, anchor: idsToToggle[0] ?? state.anchor };
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
 * ## `Shift` + `Ctrl` + 单击 → **回退**到本场景的单击（**不是一个操作**）
 *
 * 人类 2026-09-26 钉定：这个组合**设定上不存在**，而且「可能永远也不会出现这个操作」——
 * 他**从来反对「按住两个辅助键才能实现的功能」**（类比：苹果反对鼠标右键）。
 * 为了**避免出现意外的操作功能**，对它做了捕获，并**回退到「当前场景的单击」**。
 *
 * ❗ **回退到什么，由当前场景决定**（`plainClick` 就是它）；它没有自己的语义，
 * **没有要定案的东西**，也**不要**给它设计行为。详见文件头「两个术语」那一节。
 * （曾经的实现是「`Shift` 优先」，而网格那一份内联三元在同样情形下会退到 `toggle` ——
 * 两边不一致，现已收归这里一处。）
 *
 * ## 反转（`invertedCtrl`，导出画廊；`design/export.md` §4.2）
 *
 * 导出画廊以 issue 为第一公民，**默认就是多选**语义，所以那里「单击 = 加/减一张」
 * 而「`Ctrl` 单击 = 只选这一张」。**反转只翻转「有／无 `Ctrl`」这两个结果**；
 * `Shift` 区间与上面那条回退两边一致。
 */
export function clickMode(event: {
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
}, invertedCtrl = false): SelectMode {
  /** **当前场景的单击** —— 回退目标就是它（没有反转时 `replace`，反转时 `toggle`） */
  const plainClick: SelectMode = invertedCtrl ? "toggle" : "replace";
  /** 当前场景的 **`Ctrl`+单击** */
  const ctrlClick: SelectMode = invertedCtrl ? "replace" : "toggle";
  const shift = event.shiftKey;
  const ctrl = event.ctrlKey || event.metaKey;
  // `Shift`+`Ctrl` **不是操作** → 捕获并回退到本场景的单击；否则两个修饰键各管一件事
  if (shift && ctrl) return plainClick;
  if (shift) return "range";
  return ctrl ? ctrlClick : plainClick;
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
