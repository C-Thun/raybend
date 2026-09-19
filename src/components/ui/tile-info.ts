/**
 * tiles 的「信息」档位 —— **import 与 browse 共用同一个状态**（人类 2026-09-19）。
 *
 * 三态循环（状态栏那个 `i` 按钮或 `i` 键切）：
 *
 *   `off`（默认）→ `marks`（只显示标记）→ `marks-name`（标记 + 文件名）→ `off`
 *
 * 为什么做成**模块级单例信号**而不是塞进某个 store：
 *   - 两个工作区的网格 store 是两个类型，塞进任何一个都会逼另一侧去适配；
 *   - 而「tiles 的信息档位」在业务上就是**一个**用户偏好（他看的是同一批 tiles），
 *     两个地方各存一份反而会出现「切了工作流档位变了」的怪事。
 * 状态栏按钮、两个网格、两处键盘处理都读它 —— 只有这一份实现。
 */

import { createSignal } from "solid-js";

import type { TileInfoMode } from "./Tile.tsx";

export const TILE_INFO_CYCLE: readonly TileInfoMode[] = ["off", "marks", "marks-name"];

const [infoMode, setInfoMode] = createSignal<TileInfoMode>("off");

export { infoMode };

/** 下一个档位（`off → marks → marks-name → off`） */
export function nextTileInfoMode(current: TileInfoMode): TileInfoMode {
  const at = TILE_INFO_CYCLE.indexOf(current);
  if (at === -1) return "off";
  return TILE_INFO_CYCLE[(at + 1) % TILE_INFO_CYCLE.length] ?? "off";
}

/** 切到下一档（按钮与 `i` 键都走这里） */
export function cycleTileInfo(): void {
  setInfoMode((current) => nextTileInfoMode(current));
}
