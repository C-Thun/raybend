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

import { createSignal } from "solid-js";
import { withTimeout } from "../../lib/timeout.ts";
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
    excluded: readonly string[],
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
  /**
   * 用户**排除**掉的文件（绝对路径）。默认空 —— 老的调用点不必改。
   * 后端会把它们整批剔掉（见 `import/runner.rs`）：不进规划、不计入 total/skipped。
   */
  excluded?: readonly string[];
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

export interface ImportStoreDeps {
  api: ImportApi;
  /**
   * 每条命令的时限（毫秒；不传用 `DEFAULT_COMMAND_TIMEOUT_MS`）。
   *
   * 存在的理由见 `lib/timeout.ts`：后端 panic 时命令的 promise **永远不会 settle**，
   * 没有时限的话界面会卡在忙碌态、连「取消」都发不出去。时限把它变成一条错误消息。
   */
  timeoutMs?: number;
}

/** 命令时限默认 15 秒：正常命令都是毫秒级，15 秒只可能是「后端挂了」。 */
export const DEFAULT_COMMAND_TIMEOUT_MS = 15_000;

/** 暂停 / 继续 / 取消 这三个命令超时时的说法（拼进错误消息） */
const DANG_WHAT = "导入命令";

export function createImportStore(deps: ImportStoreDeps): ImportStore {
  // 界面要读的这五个必须是 **signal**（文件头那条只禁 `createMemo`，不禁 `createSignal`）：
  // `<Dialog open={store.open()}>` 里 Solid 只在读到 props 时求值 —— 普通变量读一次就定死，
  // 弹窗永远弹不出来（步骤 15 的冒烟抓到的正是这个，且它在真机上同样弹不出来）。
  // `createSignal` 在 Node 的 SSR 构建里就是 `[() => value, setter]`，读写语义与真机一致，
  // 所以单测依旧是同步可断言的。
  const [progress, setProgress] = createSignal<ImportBatchProgress | null>(null);
  const [batchId, setBatchId] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [open, setOpen] = createSignal(false);
  let unlisten: (() => void) | null = null;
  const timeout = deps.timeoutMs ?? DEFAULT_COMMAND_TIMEOUT_MS;

  function unsubscribe(): void {
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
  }

  function fail(caught: unknown): void {
    setError(caught instanceof Error ? caught.message : String(caught));
  }

  /** 命令的统一走法：置忙 → 跑 → 更新快照 → 清忙。 */
  async function command(
    run: (id: string) => Promise<ImportBatchProgress>,
  ): Promise<void> {
    // 取一次 id 再判空：TS 不会把 `batchId()` 的收窄带过 await
    const id = batchId();
    if (id === null || busy()) return;
    setBusy(true);
    setError(null);
    try {
      const snapshot = await withTimeout(run(id), timeout, DANG_WHAT);
      apply(snapshot);
    } catch (caught) {
      fail(caught);
    } finally {
      setBusy(false);
    }
  }

  function apply(snapshot: ImportBatchProgress): void {
    setProgress(snapshot);
    // 终态到了就退订：后端不会再发有意义的事件了
    if (isFinal(snapshot.state)) unsubscribe();
  }

  async function begin(request: ImportRequest): Promise<void> {
    if (busy()) return;
    setBusy(true);
    setError(null);
    setProgress(null);
    setOpen(true);
    unsubscribe();
    try {
      const started = await withTimeout(
        deps.api.start(
          request.repositoryId,
          request.sources,
          request.avoidDuplicates,
          request.excluded ?? [],
        ),
        timeout,
        "启动导入",
      );
      if (started.batchId === "") {
        // 浏览器降级（`api/import.ts` 在没有 Tauri 时返回空批次）：
        // 这不是「运行中」，也不该让弹窗永远转圈 —— 说清是环境问题
        setError("当前环境没有导入后端（开发预览里只有界面）");
        setBatchId(null);
        return;
      }
      setBatchId(started.batchId);
      // 先订阅再取一次快照：两者之间的空档不会漏事件
      unlisten = await withTimeout(deps.api.subscribe((incoming) => {
        // 只认自己这一批（理论上只有一个批次在跑，但别留隐患）
        if (incoming.batchId === batchId()) apply(incoming);
      }), timeout, "订阅导入进度");
      apply(await withTimeout(deps.api.status(started.batchId), timeout, "读导入状态"));
    } catch (caught) {
      fail(caught);
      setBatchId(null);
    } finally {
      setBusy(false);
    }
  }

  const counts = (): ImportCounts => ({
    total: progress()?.total ?? 0,
    done: progress()?.done ?? 0,
    imported: progress()?.imported ?? 0,
    skipped: progress()?.skipped ?? 0,
    duplicates: progress()?.duplicates ?? 0,
    failed: progress()?.failed ?? 0,
  });

  return {
    open,
    openDialog: () => {
      setOpen(true);
    },
    dismiss: () => {
      setOpen(false);
    },

    progress,
    batchId,
    error,
    busy,

    stage: () => progress()?.stage ?? "scan",
    state: () => progress()?.state ?? "running",
    percent: () => {
      const snapshot = progress();
      if (snapshot === null) return null;
      if (snapshot.stage === "scan" || snapshot.stage === "plan") return null;
      if (snapshot.stage === "thumbs" || snapshot.stage === "done") return 100;
      if (snapshot.total === 0) return 100;
      return Math.round((snapshot.done / snapshot.total) * 100);
    },
    counts,
    currentLabel: () => {
      const current = progress()?.runs
        .map((run) => run.current)
        .find((item) => item !== null && item !== undefined);
      if (current === null || current === undefined) return null;
      return current.target === null
        ? current.source
        : `${current.source} → ${current.target}`;
    },
    runLabel: () => {
      const snapshot = progress();
      if (snapshot === null || snapshot.runs.length <= 1) return null;
      const index = snapshot.currentRun ?? 0;
      const run = snapshot.runs[index];
      if (run === undefined) return null;
      return `[${index + 1}/${snapshot.runs.length}] ${run.sourceRoot}`;
    },
    errors: () => progress()?.errors ?? [],
    runNote: () => {
      const snapshot = progress();
      if (snapshot === null) return null;
      for (const run of snapshot.runs) {
        if (run.note !== null && run.note !== "") return run.note;
      }
      return null;
    },
    finished: () => {
      const snapshot = progress();
      return snapshot !== null && isFinal(snapshot.state);
    },

    begin,
    pause: () => command((id) => deps.api.pause(id)),
    resume: () => command((id) => deps.api.resume(id)),
    cancel: () => command((id) => deps.api.cancel(id)),
    exportErrors: async (path: string) => {
      const id = batchId();
      if (id === null) return 0;
      try {
        return await withTimeout(deps.api.exportErrors(id, path), timeout, "导出错误清单");
      } catch (caught) {
        fail(caught);
        return 0;
      }
    },
    dispose: unsubscribe,
  };
}
