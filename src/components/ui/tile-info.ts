/**
 * tiles 的「信息」档位。视图实现只有一份，偏好按 import / browse 分开持久化。
 *
 * browse 的三态循环（statusbar 的 `i` 按钮或 `i` 键切）：
 *
 *   `off`（默认）→ `marks`（只显示标记）→ `marks-name`（标记 + 文件名）→ `off`
 *
 * 为什么信号放在共享模块而不是塞进某个 store：两个工作区的网格 store 是两个类型，
 * 但它们应当复用同一套读写与迁移框架；具体值按 import / browse 隔离。
 */

import {
  browseDisplayInfoMode,
  importDisplayInfoMode,
  setBrowseDisplayInfoMode,
  setImportDisplayInfoMode,
  type TileInfoMode,
} from "../../lib/display-prefs.ts";

export const TILE_INFO_CYCLE: readonly TileInfoMode[] = ["off", "marks", "marks-name"];

/**
 * 两个作用域各自的当前档位。
 *
 * 状态本身住在 `lib/display-prefs.ts`：那里同时负责**落盘**（人类 2026-09-20：
 * 信息显示级别要持久化）由 `display-prefs.ts` 统一管理。这里只保留「tiles 信息档位」
 * 这个业务名字与循环规则，不另造持久化渠道。
 */
export const browseInfoMode = browseDisplayInfoMode;
export const importInfoMode = importDisplayInfoMode;

/** 下一个档位（`off → marks → marks-name → off`） */
export function nextTileInfoMode(current: TileInfoMode): TileInfoMode {
  const at = TILE_INFO_CYCLE.indexOf(current);
  if (at === -1) return "off";
  return TILE_INFO_CYCLE[(at + 1) % TILE_INFO_CYCLE.length] ?? "off";
}

/** 切到下一档（按钮与 `i` 键都走这里）—— 切完立刻落盘 */
export function cycleBrowseTileInfo(): void {
  setBrowseDisplayInfoMode(nextTileInfoMode(browseInfoMode()));
}

/** import 只有一级：关 ↔ 显示全部（当前就是文件名）。 */
export function toggleImportTileInfo(): void {
  setImportDisplayInfoMode(importInfoMode() === "off" ? "marks-name" : "off");
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
