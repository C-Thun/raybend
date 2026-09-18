/**
 * 标记动作之后**界面上要说的话**（`plans/M2-W2-tail.md` 3.1）。
 *
 * 为什么单独抽出来：批量打标时有两种「什么也没发生」的情况，用户看不见它们，
 * 就会以为软件卡住或者自己点错了 ——
 *
 * * **被锁挡住**（`skippedLocked`）：二级锁 = 不能编辑，打星/标色会被跳过；
 * * **一个都没改**（`changed === 0`）：例如选中的照片全都已经是这个值。
 *
 * 反过来「改了 N 张」**不用说** —— 照片上的星/色标立刻变了，那就是最好的提示。
 * 这类「只在出问题时说话」的口径，写成纯函数才好测（工具条那边只管渲染）。
 */

import type { MarkResult } from "../../api/types.ts";

/** 要说的话：i18n 键 + 数量（`{n}` 占位） */
export interface MarkNotice {
  key: "browse.markSkippedLocked" | "browse.markNothingChanged";
  count: number;
}

/**
 * `null` = 不用说话（成功或没做动作）。
 *
 * 优先级：**先报「被锁挡住」**，再报「什么都没改」—— 前者信息量更大
 * （用户会知道「不是没选中，是有锁」）。
 */
export function markNotice(result: MarkResult | null): MarkNotice | null {
  if (result === null || typeof result !== "object") return null;
  /*
   * 形状不对时**当作「没什么可说」**，而不是炸在工具条里：
   * IPC 回来的东西不保证结构（浏览器里、或将来后端改了字段），
   * 一个显示提示的小函数不值得让整个工作区白屏。
   */
  const locked = Array.isArray(result.skippedLocked) ? result.skippedLocked.length : 0;
  if (locked > 0) return { key: "browse.markSkippedLocked", count: locked };
  if (result.changed === 0) return { key: "browse.markNothingChanged", count: 0 };
  return null;
}
