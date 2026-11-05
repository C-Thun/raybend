/**
 * 「空格键在默认层与当前层之间来回切」的**唯一判定**（崔总 2026-11-05）。
 *
 * 为什么抽成纯函数：这条逻辑有三个容易写错的边界 ——
 * ① 只有默认层时**什么都不做**（不建层、不提示）；
 * ② 从默认层回到**刚才在改的那一层**，而不是列表末尾那一层；
 * ③ 记着的那一层已经被删掉时要退到最后一层，且不能返回一个不存在的 id。
 *
 * 全是数组与 `null` 的组合，最适合在单测里逐条钉死（`editor-area-focus.test.ts`），
 * 组件那边只剩「算出来的 id 交给 `store.selectArea`」一步。
 */

/**
 * 按一次空格之后应该选中哪一层。
 *
 * @param current  当前层 id（`null` = 默认层）
 * @param areaIds  层列表里的**非默认层** id（列表顺序自上而下 = 计算顺序）
 * @param lastArea 上一次停留的非默认层（用户刚从那一层回默认层的）
 * @returns 下一步选中的 id；`null` = 回默认层；`undefined` = **这一次什么都不该发生**
 */
export function nextAreaFocus(
  current: number | null,
  areaIds: readonly number[],
  lastArea: number | null,
): number | null | undefined {
  // 没有任何非默认层：空格不归这件事管（不建层、不提示）
  if (areaIds.length === 0) return undefined;
  // 在某个层里 → 回默认层
  if (current !== null) return null;
  // 在默认层 → 回到刚才那一层；那一层没了就退到最后一层
  if (lastArea !== null && areaIds.includes(lastArea)) return lastArea;
  // `[length - 1]` 而不是 `.at(-1)`：tsconfig 的 lib 里没有 ES2022 的 `at`（会直接编译不过）
  return areaIds[areaIds.length - 1];
}
