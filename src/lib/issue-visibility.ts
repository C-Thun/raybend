/**
 * 定稿面板里「哪几行该出现」的**纯判定**（崔总 2026-10-31）。
 *
 * ## 为什么「最近编辑」要按哈希藏起来，而不是想办法让它能选中
 *
 * `最近编辑`（latest）与命名定稿是**两种不同的东西**：latest 是工作副本（会随编辑继续变），
 * 命名定稿是不可变快照。但它们的画面在「刚定完稿」那一刻**完全一样** —— 用户既无法区分，
 * 选中 latest 也等于选中那条定稿（高亮还会打架）。
 *
 * 所以口径是：**current 栈与任何一条定稿的指纹都不同时，才显示「最近编辑」**；
 * 一旦相同就整行不画。用户再动一格参数，指纹变了，该行自然回来。
 *
 * 判据由后端给（`IssueLibrary.latestMatchesIssue`，与 `can_finalize` 同一个
 * `store::issues::matching_issue`），前端**不自己算哈希** —— 那是唯一实现。
 */

/** 只取这条判定要用的字段（`null` = 还没拿到库列表）。 */
export interface LatestVisibility {
  /** 当前栈是否与某条定稿完全相同。 */
  latestMatchesIssue?: boolean;
}

/**
 * 「最近编辑」行要不要显示。
 *
 * 列表还没回来（`null`）时**显示**：那时它是用户唯一能回退的目标，
 * 藏起来只会让人以为左侧坏了；列表一到（60ms 内）再按真实判定收敛。
 */
export function showsLatestRow(library: LatestVisibility | null): boolean {
  return !(library?.latestMatchesIssue ?? false);
}
