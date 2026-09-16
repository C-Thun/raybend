/**
 * 导入进度的状态（`features/import`）。
 *
 * 三条约定（与 M1-5 的两个 store 一致）：
 *
 * 1. **api 从外面注入** —— 单测喂假事件流，不碰 Tauri；
 * 2. **派生值用普通函数** —— 不用 `createMemo`：Node 里跑的是 solid 的 SSR 构建，
 *    `createMemo` 只会算一次，单测会与真实运行不一致（M1-5 踩过）；
 * 3. **聚合在 Rust 侧算**（`BatchProgress` 的 total/done/... 已经是各 run 之和），
 *    这里只做「取当前 run、拼 `[2/3] 目录名`」这类展示层的事 —— 两边都算一遍迟早会不一致。
 */

import type { ImportSource } from "../../api/import.ts";
import type {
  ImportBatchProgress,
  ImportError,
  ImportStage,
  ImportState,
  ImportStart,
} from "../../api/types.ts";

/** store 需要的全部后端能力（真实实现来自 `src/api/import.ts`）。 */
export interface ImportApi {
  start: (
    repositoryId: string,
    sources: readonly ImportSource[],
    avoidDuplicates: boolean,
  ) => Promise<ImportStart>;
  pause: (batchId: string) => Promise<ImportBatchProgress>;
  resume: (batchId: string) => Promise<ImportBatchProgress>;
  cancel: (batchId: string) => Promise<ImportBatchProgress>;
  status: (batchId: string) => Promise<ImportBatchProgress>;
  exportErrors: (batchId: string, path: string) => Promise<number>;
  /** 订阅进度；返回取消订阅的函数。 */
  subscribe: (
    handler: (progress: ImportBatchProgress) => void,
  ) => Promise<() => void>;
}

/** 一次导入请求。 */
export interface ImportRequest {
  repositoryId: string;
  /** 每个源目录带自己的「包含子目录」开关。 */
  sources: readonly ImportSource[];
  avoidDuplicates: boolean;
}

/** 计数（界面上那几栏）。 */
export interface ImportCounts {
  total: number;
  done: number;
  imported: number;
  skipped: number;
  duplicates: number;
  failed: number;
}

export interface ImportStore {
  /** 弹窗开不开。 */
  open: () => boolean;
  /** 打开弹窗（还没开始跑时也能开：显示「准备中」）。 */
  openDialog: () => void;
  /** 收起弹窗（终态之后）。 */
  dismiss: () => void;

  /** 当前快照（还没开始跑时是 `null`）。 */
  progress: () => ImportBatchProgress | null;
  /** 批次 id。 */
  batchId: () => string | null;
  /** 操作失败的原因（开始导入失败、暂停失败……）。 */
  error: () => string | null;
  /** 正在发命令（按钮防连点）。 */
  busy: () => boolean;

  /** 阶段 / 状态。 */
  stage: () => ImportStage;
  state: () => ImportState;
  /** 百分比；`null` = 扫描/规划阶段（没有总数，界面用不确定进度条）。 */
  percent: () => number | null;
  counts: () => ImportCounts;
  /** 当前在处理的文件（`源 → 库内路径`）。 */
  currentLabel: () => string | null;
  /** `[2/3] 目录名`（多源目录时才有意义）。 */
  runLabel: () => string | null;
  /** 错误清单（只留尾巴）。 */
  errors: () => readonly ImportError[];
  /** run 级提示（例如缩略图入队失败）。 */
  runNote: () => string | null;
  /** 已经结束了吗（终态）。 */
  finished: () => boolean;

  /** 开始导入（订阅进度事件）。 */
  begin: (request: ImportRequest) => Promise<void>;
  /** 暂停 / 继续。 */
  pause: () => Promise<void>;
  resume: () => Promise<void>;
  /** 取消（**已导入的部分保留**）。 */
  cancel: () => Promise<void>;
  /** 导出错误清单，返回写出去多少条。 */
  exportErrors: (path: string) => Promise<number>;
  /** 退订（组件卸载时调；不影响后端继续跑）。 */
  dispose: () => void;
}

/** 终态：不会再变的状态。 */
function isFinal(state: ImportState): boolean {
  return state === "cancelled" || state === "done" || state === "failed";
}

export function createImportStore(deps: { api: ImportApi }): ImportStore {
  let progress: ImportBatchProgress | null = null;
  let batchId: string | null = null;
  let error: string | null = null;
  let busy = false;
  let open = false;
  let unlisten: (() => void) | null = null;

  function unsubscribe(): void {
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
  }

  function fail(caught: unknown): void {
    error = caught instanceof Error ? caught.message : String(caught);
  }

  /** 命令的统一走法：置忙 → 跑 → 更新快照 → 清忙。 */
  async function command(
    run: (id: string) => Promise<ImportBatchProgress>,
  ): Promise<void> {
    if (batchId === null || busy) return;
    busy = true;
    error = null;
    try {
      const snapshot = await run(batchId);
      apply(snapshot);
    } catch (caught) {
      fail(caught);
    } finally {
      busy = false;
    }
  }

  function apply(snapshot: ImportBatchProgress): void {
    progress = snapshot;
    // 终态到了就退订：后端不会再发有意义的事件了
    if (isFinal(snapshot.state)) unsubscribe();
  }

  async function begin(request: ImportRequest): Promise<void> {
    if (busy) return;
    busy = true;
    error = null;
    progress = null;
    open = true;
    unsubscribe();
    try {
      const started = await deps.api.start(
        request.repositoryId,
        request.sources,
        request.avoidDuplicates,
      );
      if (started.batchId === "") {
        // 浏览器降级（`api/import.ts` 在没有 Tauri 时返回空批次）：
        // 这不是「运行中」，也不该让弹窗永远转圈 —— 说清是环境问题
        error = "当前环境没有导入后端（开发预览里只有界面）";
        batchId = null;
        return;
      }
      batchId = started.batchId;
      // 先订阅再取一次快照：两者之间的空档不会漏事件
      unlisten = await deps.api.subscribe((incoming) => {
        // 只认自己这一批（理论上只有一个批次在跑，但别留隐患）
        if (incoming.batchId === batchId) apply(incoming);
      });
      apply(await deps.api.status(started.batchId));
    } catch (caught) {
      fail(caught);
      batchId = null;
    } finally {
      busy = false;
    }
  }

  const counts = (): ImportCounts => ({
    total: progress?.total ?? 0,
    done: progress?.done ?? 0,
    imported: progress?.imported ?? 0,
    skipped: progress?.skipped ?? 0,
    duplicates: progress?.duplicates ?? 0,
    failed: progress?.failed ?? 0,
  });

  return {
    open: () => open,
    openDialog: () => {
      open = true;
    },
    dismiss: () => {
      open = false;
    },

    progress: () => progress,
    batchId: () => batchId,
    error: () => error,
    busy: () => busy,

    stage: () => progress?.stage ?? "scan",
    state: () => progress?.state ?? "running",
    percent: () => {
      const snapshot = progress;
      if (snapshot === null) return null;
      if (snapshot.stage === "scan" || snapshot.stage === "plan") return null;
      if (snapshot.stage === "thumbs" || snapshot.stage === "done") return 100;
      if (snapshot.total === 0) return 100;
      return Math.round((snapshot.done / snapshot.total) * 100);
    },
    counts,
    currentLabel: () => {
      const current = progress?.runs
        .map((run) => run.current)
        .find((item) => item !== null && item !== undefined);
      if (current === null || current === undefined) return null;
      return current.target === null
        ? current.source
        : `${current.source} → ${current.target}`;
    },
    runLabel: () => {
      const snapshot = progress;
      if (snapshot === null || snapshot.runs.length <= 1) return null;
      const index = snapshot.currentRun ?? 0;
      const run = snapshot.runs[index];
      if (run === undefined) return null;
      return `[${index + 1}/${snapshot.runs.length}] ${run.sourceRoot}`;
    },
    errors: () => progress?.errors ?? [],
    runNote: () => {
      const snapshot = progress;
      if (snapshot === null) return null;
      for (const run of snapshot.runs) {
        if (run.note !== null && run.note !== "") return run.note;
      }
      return null;
    },
    finished: () => progress !== null && isFinal(progress.state),

    begin,
    pause: () => command((id) => deps.api.pause(id)),
    resume: () => command((id) => deps.api.resume(id)),
    cancel: () => command((id) => deps.api.cancel(id)),
    exportErrors: async (path: string) => {
      if (batchId === null) return 0;
      try {
        return await deps.api.exportErrors(batchId, path);
      } catch (caught) {
        fail(caught);
        return 0;
      }
    },
    dispose: unsubscribe,
  };
}
