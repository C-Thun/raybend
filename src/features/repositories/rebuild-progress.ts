import type { RebuildProgress } from "../../api/types.ts";

export type RebuildProgressMessage =
  | { key: "repo.rebuild_progress_scan"; params: { done: number } }
  | { key: "repo.rebuild_progress_apply" }
  | { key: "repo.rebuild_progress_metadata"; params: { done: number; total: number } }
  | { key: "repo.rebuild_progress_counts" }
  | { key: "repo.rebuild_progress_finishing" };

/**
 * 后端阶段事实 → 前端文案 key。
 *
 * 单独放成纯函数，既能覆盖边界值，也让中英文只在语言包里各写一份。
 */
export function rebuildProgressMessage(
  progress: RebuildProgress,
): RebuildProgressMessage {
  switch (progress.phase) {
    case "scan":
      return { key: "repo.rebuild_progress_scan", params: { done: progress.done } };
    case "apply":
      return { key: "repo.rebuild_progress_apply" };
    case "metadata":
      return {
        key: "repo.rebuild_progress_metadata",
        params: { done: progress.done, total: progress.total },
      };
    case "counts":
      return { key: "repo.rebuild_progress_counts" };
    case "done":
      return { key: "repo.rebuild_progress_finishing" };
  }
}
