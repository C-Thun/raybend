/**
 * `TilesShell` ↔ 网格之间的**「横向适合窗口」通道**。
 *
 * 两个读数，都住在 `TilesShell`（它是状态条与网格的共同父级）：
 *
 * | 读数 | 谁写 | 谁读 | 干什么 |
 * | --- | --- | --- | --- |
 * | `request` | 状态条的按钮（+1） | 网格 | 网格看到计数变了就去算「铺满一行要多大」 |
 * | `available` | 网格（算完回填） | 状态条的按钮 | 算出来的格宽**超过最大档**时按钮无效 |
 *
 * 为什么 `available` 必须回填而不是状态条自己算：格宽要**容器宽 + 列数 + 间距**三样，
 * 而那三样只有网格知道（它拿着 `ResizeObserver` 与 `tileStep`）。
 * 状态条自己算等于把同一件事写两遍（`AGENTS.md` §2.12）。
 *
 * ⚠️ 这个 Provider 必须包住**网格与状态条两者** —— 见 `TilesShell.tsx` 里
 * 关于 `untrack` 与 owner 链的说明（2026-09-23 的成对教训，§2.17）。
 */

import { createContext, useContext, type Accessor } from "solid-js";

import type { TileSizeBounds } from "../../../lib/tile-flow.ts";
export interface TilesFitChannel {
  sizeBounds: Accessor<TileSizeBounds | undefined>;
  /** 请求计数：按钮点一下 +1，网格按它触发一次计算 */
  request: Accessor<number>;
  /** 现在**能不能**铺满（格宽不超过最大档）；网格算完回填 */
  available: Accessor<boolean>;
  setAvailable: (value: boolean) => void;
}

export const TilesFitRequestContext = createContext<TilesFitChannel>();

/** 网格侧：拿到整条通道（要 `request` 触发、要 `setAvailable` 回填） */
export function useTilesFitChannel(): TilesFitChannel | undefined {
  return useContext(TilesFitRequestContext);
}

/** 只要请求计数的调用方用这个（等价于 `useTilesFitChannel()?.request`） */
export function useTilesFitRequest(): Accessor<number> | undefined {
  return useContext(TilesFitRequestContext)?.request;
}
