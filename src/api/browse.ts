/**
 * 浏览工作流的命令封装（M2-W1）。
 *
 * 与 `db.ts` / `import.ts` 同一套纪律：**命令名只出现在这一层**、浏览器里降级、
 * 错误照原样抛。`src/features/browse/` 的 store 依赖本模块的接口类型，
 * 测试里换成假实现即可（不需要 Tauri）。
 *
 * 参数形状见 `src-tauri/src/browse.rs`（serde 用 camelCase；标记动作靠 `kind` 标签区分）。
 */

import { isTauriRuntime } from "./tauri-env.ts";
import type {
  BrowseFacets,
  BrowseQuery,
  BrowseTimeline,
  BrowseWindow,
  DeleteResult,
  FlagsView,
  MarkAction,
  MarkResult,
  MarkingItem,
} from "./types.ts";

let coreModule: Promise<typeof import("@tauri-apps/api/core")> | undefined;

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  coreModule ??= import("@tauri-apps/api/core");
  return coreModule.then((core) => core.invoke<T>(cmd, args));
}

/** 浏览器里没有后端 —— 返回空结果，界面自然显示空态（不抛错、不影响预览）。 */
const NO_BACKEND_WINDOW: BrowseWindow = { total: 0, offset: 0, items: [] };
const NO_BACKEND_TIMELINE: BrowseTimeline = { total: 0, entries: [] };
const NO_BACKEND_FACETS: BrowseFacets = {
  ratings: [],
  colors: [],
  likes: [],
  locks: [],
};
const NO_BACKEND_FLAGS: FlagsView = { total: 0, picks: [], rejects: [] };
const NO_BACKEND_MARK: MarkResult = {
  changed: 0,
  skippedLocked: [],
  undoLabel: null,
  redoLabel: null,
  canUndo: false,
  canRedo: false,
};
const NO_BACKEND_DELETE: DeleteResult = {
  deleted: 0,
  blockedLocked: [],
  alreadyGone: 0,
  failed: [],
};

/** 取一页照片（虚拟网格的可视窗口）。 */
export async function browsePage(
  query: BrowseQuery,
  offset: number,
  limit: number,
): Promise<BrowseWindow> {
  if (!isTauriRuntime()) return NO_BACKEND_WINDOW;
  return call<BrowseWindow>("browse_page", { query, offset, limit });
}

/** 取时间线（顺序 + 拍摄时间；分组与键盘导航要用）。 */
export async function browseTimeline(
  query: BrowseQuery,
  limit = 0,
): Promise<BrowseTimeline> {
  if (!isTauriRuntime()) return NO_BACKEND_TIMELINE;
  return call<BrowseTimeline>("browse_timeline", { query, limit });
}

/** 取各取值的分布（筛选面板）。 */
export async function browseFacets(query: BrowseQuery): Promise<BrowseFacets> {
  if (!isTauriRuntime()) return NO_BACKEND_FACETS;
  return call<BrowseFacets>("browse_facets", { query });
}

/** 读一组照片当前的标记（三态控件的输入）。 */
export async function browseMarkings(
  repositoryId: string,
  ids: readonly number[],
): Promise<MarkingItem[]> {
  if (!isTauriRuntime()) return [];
  return call<MarkingItem[]>("browse_markings", {
    repositoryId,
    ids: [...ids],
  });
}

/** 打标记 / 改标签（会进后端的撤销栈）。 */
export async function browseMark(
  repositoryId: string,
  ids: readonly number[],
  action: MarkAction,
): Promise<MarkResult> {
  if (!isTauriRuntime()) return NO_BACKEND_MARK;
  return call<MarkResult>("browse_mark", {
    repositoryId,
    ids: [...ids],
    action,
  });
}

/** 撤销一步。 */
export async function browseUndo(repositoryId: string): Promise<MarkResult> {
  if (!isTauriRuntime()) return NO_BACKEND_MARK;
  return call<MarkResult>("browse_undo", { repositoryId });
}

/** 重做一步。 */
export async function browseRedo(repositoryId: string): Promise<MarkResult> {
  if (!isTauriRuntime()) return NO_BACKEND_MARK;
  return call<MarkResult>("browse_redo", { repositoryId });
}

/** 删除（进系统回收站）。 */
export async function browseDelete(
  repositoryId: string,
  ids: readonly number[],
): Promise<DeleteResult> {
  if (!isTauriRuntime()) return NO_BACKEND_DELETE;
  return call<DeleteResult>("browse_delete", {
    repositoryId,
    ids: [...ids],
  });
}

/** 当前库的旗标快照。 */
export async function flagsGet(repositoryId: string): Promise<FlagsView> {
  if (!isTauriRuntime()) return NO_BACKEND_FLAGS;
  return call<FlagsView>("flags_get", { repositoryId });
}

/** 打 / 清旗标（`flag = null` = 清掉这些照片的旗标）。 */
export async function flagsSet(
  repositoryId: string,
  ids: readonly number[],
  flag: "pick" | "reject" | null,
): Promise<FlagsView> {
  if (!isTauriRuntime()) return NO_BACKEND_FLAGS;
  return call<FlagsView>("flags_set", {
    repositoryId,
    ids: [...ids],
    flag,
  });
}

/** 清空**所有**旗标（跨库；调用方要先确认）。 */
export async function flagsClear(repositoryId: string): Promise<FlagsView> {
  if (!isTauriRuntime()) return NO_BACKEND_FLAGS;
  return call<FlagsView>("flags_clear", { repositoryId });
}
