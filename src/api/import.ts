/**
 * 导入执行与进度（M1-6）的前端封装。
 *
 * 与 `db.ts` 同一套纪律：命令名收口在这一层、浏览器里降级返回空值、
 * 错误照原样抛。多出来的一件事是**事件订阅**：
 *
 * ```ts
 * const stop = await onImportProgress((p) => { ... });
 * // 弹窗卸载时
 * stop();
 * ```
 *
 * 事件是**后端节流后**发的（≥100ms 一条，阶段切换会立刻补一条），
 * 所以这里不做去重；快照里永远是最新状态，界面直接渲染即可。
 */

import { isTauriRuntime } from "./tauri-env.ts";
import type {
  ImportBatchProgress,
  ImportPrecheck,
  ImportStart,
  InterruptedRun,
} from "./types.ts";

/** 一个待导入的源目录（「包含子目录」是每个目录各一份的开关）。 */
export interface ImportSource {
  path: string;
  includeSubdirs: boolean;
}

/** 进度事件名（与 `src-tauri/src/import.rs` 的 `PROGRESS_EVENT` 一致）。 */
export const IMPORT_PROGRESS_EVENT = "import://progress";

let coreModule: Promise<typeof import("@tauri-apps/api/core")> | undefined;
let eventModule: Promise<typeof import("@tauri-apps/api/event")> | undefined;

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  coreModule ??= import("@tauri-apps/api/core");
  return coreModule.then((core) => core.invoke<T>(cmd, args));
}

/**
 * 开工前估一下要搬多少、目标卷够不够。
 *
 * 会**轻量走一遍源目录**（只 stat，不读内容）—— 所以界面把它放在
 * 「按下导入」之后、真正开跑之前。
 */
export async function importPrecheck(
  repositoryId: string,
  sources: readonly ImportSource[],
): Promise<ImportPrecheck> {
  if (!isTauriRuntime()) {
    return { totalBytes: 0, freeBytes: null, tight: false, neededBytes: 0 };
  }
  return call<ImportPrecheck>("import_precheck", {
    repositoryId,
    sources: sources.map((source) => ({ ...source })),
  });
}

/**
 * 开始导入（每个源目录一个 run，整批一个 batchId）。
 *
 * `excluded` = 用户排除掉的文件（**绝对路径**，跨目录、跨源共用一份）。
 * 后端会把它们从扫描结果里剔除：不进规划、不计入 total/skipped ——
 * 「不导入这张」与「试过了、跳过」不是一回事。
 */
export async function importStart(
  repositoryId: string,
  sources: readonly ImportSource[],
  avoidDuplicates: boolean,
  excluded: readonly string[] = [],
): Promise<ImportStart> {
  if (!isTauriRuntime()) {
    return { batchId: "", runs: [] };
  }
  return call<ImportStart>("import_start", {
    repositoryId,
    sources: sources.map((source) => ({ ...source })),
    avoidDuplicates,
    excluded: [...excluded],
  });
}

/** 暂停整批（生效点在文件之间）。 */
export async function importPause(
  batchId: string,
): Promise<ImportBatchProgress> {
  if (!isTauriRuntime()) {
    throw new Error("不在 Tauri 运行环境里");
  }
  return call<ImportBatchProgress>("import_pause", { batchId });
}

/** 继续。 */
export async function importResume(
  batchId: string,
): Promise<ImportBatchProgress> {
  if (!isTauriRuntime()) {
    throw new Error("不在 Tauri 运行环境里");
  }
  return call<ImportBatchProgress>("import_resume", { batchId });
}

/** 取消（**已导入的部分保留**）。 */
export async function importCancel(
  batchId: string,
): Promise<ImportBatchProgress> {
  if (!isTauriRuntime()) {
    throw new Error("不在 Tauri 运行环境里");
  }
  return call<ImportBatchProgress>("import_cancel", { batchId });
}

/** 当前快照（弹窗重开、断线重连后恢复现场）。 */
export async function importStatus(
  batchId: string,
): Promise<ImportBatchProgress> {
  if (!isTauriRuntime()) {
    throw new Error("不在 Tauri 运行环境里");
  }
  return call<ImportBatchProgress>("import_status", { batchId });
}

/** 把错误清单写成 JSON，返回写出去多少条。 */
export async function importErrorsExport(
  batchId: string,
  path: string,
): Promise<number> {
  if (!isTauriRuntime()) return 0;
  return call<number>("import_errors_export", { batchId, path });
}

/** 上次被中断的导入（重启后提示「可继续」）。 */
export async function importInterrupted(
  repositoryId: string,
): Promise<InterruptedRun[]> {
  if (!isTauriRuntime()) return [];
  return call<InterruptedRun[]>("import_interrupted", { repositoryId });
}

/**
 * 订阅导入进度；返回**取消订阅**的函数。
 *
 * 浏览器里返回一个什么都不做的函数（开发预览没有后端，弹窗显示空态）。
 */
export async function onImportProgress(
  handler: (progress: ImportBatchProgress) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  eventModule ??= import("@tauri-apps/api/event");
  const { listen } = await eventModule;
  return listen<ImportBatchProgress>(IMPORT_PROGRESS_EVENT, (event) =>
    handler(event.payload),
  );
}
