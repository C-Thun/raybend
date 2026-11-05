/**
 * **剪切中的照片**（`Ctrl+X` 之后、`Ctrl+V` / `Esc` 之前的那个状态）。
 *
 * 为什么是模块级单例：这个状态**跨组件**（中列网格里的 tile 要半透明、左列目录要当粘贴
 * 目标、flowbar 的提示带要播报），而它是**临时手势态**，不进任何业务 store ——
 * 与 `photo-drag.ts` / `lib/action-slot.ts` 同一种做法。
 *
 * 三条纪律（崔总 2026-11-05）：
 *
 * 1. **只剪切、不复制**：暂时没有 `Ctrl+C`（命令都没登记，面板里也搜不到）；
 * 2. **跟库绑定**：换库时旧库的剪切自动失效（`cutIdsOf` 里按 `repositoryId` 过滤，
 *    切库时也不用到处清）；
 * 3. **提示走 flowbar 的默认槽**：`setFlowHintDefault` 是那一份唯一的事实文字 ——
 *    指向别处时被那条提示盖住，手一移开又回来，不占额外界面空间（`lib/flow-hint.ts`）。
 */

import { createSignal } from "solid-js";

import { t } from "../../i18n/index.ts";
import { setFlowHintDefault } from "../../lib/flow-hint.ts";

export interface PhotoCut {
  repositoryId: string;
  /** 被剪的资产 id（字符串，与 selection 同一口径） */
  ids: readonly string[];
}

const EMPTY: ReadonlySet<string> = new Set();

const [cut, setCut] = createSignal<PhotoCut | null>(null);

/** 把剪切状态同步到 flowbar 的默认槽（空串 = 清掉这条状态播报）。 */
function syncHint(next: PhotoCut | null): void {
  setFlowHintDefault(next === null ? "" : t("browse.cutHint", { n: next.ids.length }));
}

/** 当前剪切状态（`null` = 没在剪切）。**只读**——改它走 `beginPhotoCut` / `clearPhotoCut`。 */
export function photoCut(): PhotoCut | null {
  return cut();
}

/** 开始剪切（`ids` 为空 = 等于取消）。 */
export function beginPhotoCut(repositoryId: string, ids: readonly string[]): void {
  if (ids.length === 0) {
    clearPhotoCut();
    return;
  }
  const next: PhotoCut = { repositoryId, ids: [...ids] };
  setCut(next);
  syncHint(next);
}

/** 取消剪切（`Ctrl+V` 搬完、按 `Esc`、换库都会走到这里）。幂等。 */
export function clearPhotoCut(): void {
  if (cut() === null) return;
  setCut(null);
  syncHint(null);
}

/**
 * 这个库里正在被剪的 id 集合（tile 的 70% 半透明用它）。
 *
 * 换库之后旧库的剪切**自然不算**——不需要谁记得去清（真正清掉是下一次 `beginPhotoCut`
 * 或 `clearPhotoCut`）。
 */
export function cutIdsOf(repositoryId: string | null): ReadonlySet<string> {
  const current = cut();
  if (current === null || repositoryId === null) return EMPTY;
  if (current.repositoryId !== repositoryId) return EMPTY;
  return new Set(current.ids);
}
