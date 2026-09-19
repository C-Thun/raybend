/**
 * 数据库升级通知的**纯逻辑**（`AGENTS.md` §6.4 的版本闸门）。
 *
 * 为什么要有这个遮罩：升级（迁移）期间库的结构正在被改写，用户此刻点任何东西
 * 都是在**踩半成品结构**。所以人类 2026-09-19 定：升级时弹窗挡住操作，完成才可用。
 *
 * 这一层只做「收到通知 → 该显示什么」的推导，不碰 DOM、不碰 Tauri：
 * 界面（`MigrationGate.tsx`）与外壳（`App.tsx` 的订阅）都从它拿状态。
 *
 * 关键设计：**按库种类记账**，不是一根全局布尔量 ——
 * 同时只有一类库在升级是常态，但「一类升级完发 Done、把另一类的遮罩也撤了」
 * 是那种一旦发生就找不到原因的 bug。用一个 `Map<kind, notice>` 就没有这种可能。
 */

import type { MigrationNotice } from "../../api/types.ts";

/** `kind` → 最近一次「开始」通知（升级中的库都在这张表里）。 */
export type MigrationMap = ReadonlyMap<string, MigrationNotice>;

/** 空表（常量，避免每次渲染都造一个新 Map）。 */
export const NO_MIGRATIONS: MigrationMap = new Map();

/**
 * 收一条通知，返回**新的**表（不改入参 —— 纯函数，测试与渲染都好推理）。
 *
 * * `running: true` → 记下这一类库（重复的 Start 只覆盖不重复计数）；
 * * `running: false` → 删掉这一类库（**没 Start 过也照样删**：宁可什么都不发生，
 *   也不能因为「没配过对」而把遮罩留在屏幕上）。
 */
export function applyNotice(
  current: MigrationMap,
  notice: MigrationNotice,
): MigrationMap {
  const next = new Map(current);
  if (notice.running) next.set(notice.kind, notice);
  else next.delete(notice.kind);
  return next;
}

/**
 * 当前该显示哪一条（没有 = `null`）。
 *
 * 同时有两类库在升级时，取**版本跨度大的那条**（信息更相关）；
 * 跨度相同就按 `kind` 的字典序，保证渲染稳定（不随插入顺序抖动）。
 */
export function activeNotice(map: MigrationMap): MigrationNotice | null {
  let best: MigrationNotice | null = null;
  for (const notice of map.values()) {
    if (best === null) {
      best = notice;
      continue;
    }
    const span = notice.to - notice.from;
    const bestSpan = best.to - best.from;
    if (span > bestSpan || (span === bestSpan && notice.kind < best.kind)) {
      best = notice;
    }
  }
  return best;
}
