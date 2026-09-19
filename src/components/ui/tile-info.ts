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

import {
  displayInfoMode,
  setDisplayInfoMode,
  type TileInfoMode,
} from "../../lib/display-prefs.ts";

export const TILE_INFO_CYCLE: readonly TileInfoMode[] = ["off", "marks", "marks-name"];

/**
 * 当前档位（模块级单例）。
 *
 * 状态本身住在 `lib/display-prefs.ts`：那里同时负责**落盘**（人类 2026-09-20：
 * 信息显示级别要持久化）与 import / browse 共用一份。这里只保留「tiles 信息档位」
 * 这个业务名字与循环规则 —— 没有第二份信号。
 */
export const infoMode = displayInfoMode;

/** 下一个档位（`off → marks → marks-name → off`） */
export function nextTileInfoMode(current: TileInfoMode): TileInfoMode {
  const at = TILE_INFO_CYCLE.indexOf(current);
  if (at === -1) return "off";
  return TILE_INFO_CYCLE[(at + 1) % TILE_INFO_CYCLE.length] ?? "off";
}

/** 切到下一档（按钮与 `i` 键都走这里）—— 切完立刻落盘 */
export function cycleTileInfo(): void {
  setDisplayInfoMode(nextTileInfoMode(infoMode()));
}

/**
 * `i` 键在当前状态下**该不该接**（人类 2026-09-19 定的范围：tiles / film 才接，
 * 纯看图态不接 —— 那时 `i` 不该有任何作用）。
 *
 * 两个工作区共用这一条判据：各自的「是不是在看图」「胶片带在不在」是两套外壳状态，
 * 但「什么时候允许切信息档位」在业务上是同一件事。
 */
export function infoKeyApplies(state: {
  /** 是否处在看图态（film / view） */
  viewing: boolean;
  /** 看图态下胶片带可不可见（`chromeShowsFilm`） */
  filmVisible: boolean;
}): boolean {
  return !state.viewing || state.filmVisible;
}
