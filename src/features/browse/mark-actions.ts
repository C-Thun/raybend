/**
 * 标记动作的**唯一实现**（工具条按钮与命令面板共用）。
 *
 * ## 为什么要抽出来
 *
 * 「点 3 星」「点红色」「点赞」「点锁」在**两种模式**下的含义不同：
 *
 * | 模式 | 含义 |
 * | --- | --- |
 * | 标记态（筛选开关没开） | 给**选中的照片**打标（三态：再点一次同一个值 = 清零/取消） |
 * | 筛选态（筛选开关开了） | 给**当前 tiles** 加一条筛选条件（阈值 / 组内或 / 互斥 / 逐个切换） |
 *
 * 这段判断以前长在 `BrowseToolbar` 里。W3 加了命令面板之后，命令也必须走**同一套**——
 * 否则「点工具条」与「从命令面板搜到同一条命令」会做出两件不同的事
 * （`AGENTS.md` §2.12：同一个东西两份实现本身就是 bug）。
 *
 * ## 提示也在这里
 *
 * 「被锁挡住」「没有需要改动的照片」这类反馈由 `runMark` 统一说（toast 是可选注入的：
 * 单测与陈列室里不必准备 toast）。
 */

import type { MarkAction, MarkResult } from "../../api/types.ts";
import type { ToastStore } from "../../components/ui/Toast.tsx";
import { LOCK_LEVELS, nextRating } from "../../lib/marking-state.ts";
import { t } from "../../i18n/index.ts";
import { markNotice } from "./mark-feedback.ts";
import type { BrowseStore } from "./store.ts";

/** 命令 / 按钮要表达的标记意图（值与 `MarkAction` 对齐，`flag` 例外：旗标不进筛选） */
export type MarkIntent =
  | { kind: "rating"; value: number }
  | { kind: "color"; value: string | null }
  | { kind: "like"; value: "like" | "dislike" }
  | { kind: "lock"; value: number }
  | { kind: "flag"; value: "pick" | "reject" | null };

/**
 * 走一次标记动作，并把「该说的话」收下来。
 *
 * 所有入口（工具条 / 命令面板）都经过它 —— 否则总有一条路径忘了提示，
 * 而「被锁挡住」恰恰是最需要说话的那种（照片上一个像素都不会变）。
 */
export async function runMark(
  store: BrowseStore,
  action: MarkAction,
  toast?: ToastStore,
): Promise<void> {
  const result: MarkResult | null = await store.mark(action);
  const warning = markNotice(result);
  if (warning !== null) {
    // 边界情况（被锁挡住 / 一个都没改）：必须说话，否则用户以为点错了
    toast?.show({
      tone: warning.key === "browse.markSkippedLocked" ? "danger" : "info",
      message: t(warning.key).replace("{n}", String(warning.count)),
    });
    return;
  }
  if (result === null) return;
  // 成功：给一条带「撤销」的提示 —— 人对误操作的第一反应就是找撤销
  if (result.changed > 0) {
    toast?.show({
      tone: "success",
      message: t("browse.markedCount").replace("{n}", String(result.changed)),
      action: result.canUndo
        ? { label: t("browse.undo"), onAction: () => void store.undo() }
        : undefined,
    });
  }
}

/**
 * 按**当前模式**应用一个标记意图（工具条与命令面板的唯一入口）。
 *
 * 筛选态的分支规则（人类 2026-09-19 定的口径，别改）：
 * * 星：**阈值**——再点同一个值 = 取消这个条件；
 * * 色标：组内是「或」——再点同一个 = 取消那一个；无色算 `"none"` 这一档；
 * * 赞 / 踩：**互斥**——点另一个会换掉前一个，再点同一个 = 取消；
 * * 锁：再点已有的级别 = 取消。
 */
export async function applyMarkIntent(
  store: BrowseStore,
  intent: MarkIntent,
  toast?: ToastStore,
): Promise<void> {
  const selected = store.selectedItems();

  if (store.filterMode()) {
    const filter = store.filter();
    switch (intent.kind) {
      case "rating":
        store.patchFilter({ minRating: (filter.minRating ?? null) === intent.value ? null : intent.value });
        return;
      case "color": {
        const key = intent.value ?? "none";
        const current = filter.colors ?? [];
        store.patchFilter({
          colors: current.includes(key) ? current.filter((value) => value !== key) : [...current, key],
        });
        return;
      }
      case "like": {
        const current = (filter.likes ?? [])[0] ?? null;
        store.patchFilter({ likes: current === intent.value ? [] : [intent.value] });
        return;
      }
      case "lock": {
        const current = filter.locks ?? [];
        store.patchFilter({
          locks: current.includes(intent.value)
            ? current.filter((value) => value !== intent.value)
            : [intent.value],
        });
        return;
      }
      case "flag":
        // 旗标不进筛选（`BROWSE.md` §3.1：它只活在内存里）——筛选态下不响应
        return;
    }
  }

  switch (intent.kind) {
    case "rating":
      await runMark(
        store,
        { kind: "rating", value: nextRating(selected.map((item) => item.rating), intent.value) },
        toast,
      );
      return;
    case "color":
      await runMark(store, { kind: "color", value: intent.value }, toast);
      return;
    case "like": {
      // 已经全是这个值 ⇒ 取消（发 `null`；原样再发一次后端会判定「没有改动」）
      const allSame = selected.length > 0 && selected.every((item) => item.likeState === intent.value);
      await runMark(store, { kind: "like", value: allSame ? null : intent.value }, toast);
      return;
    }
    case "lock": {
      const allSame =
        selected.length > 0 && selected.every((item) => item.lockLevel >= intent.value);
      await runMark(
        store,
        { kind: "lock", value: allSame ? LOCK_LEVELS.none : intent.value },
        toast,
      );
      return;
    }
    case "flag":
      await store.setFlag(store.selectedIds(), intent.value);
      return;
  }
}
