/**
 * 诊断目录的前端封装（`specs/logging-and-crash-report.md` §9 的 W1/W2 部分）。
 *
 * 现在只有一条命令：**打开诊断目录** —— 出问题时能立刻摸到 `logs/` 与 `crashes/`。
 * 「导出报告包 / 复制报告摘要 / 一键发送」属第三层（W3/S1），**未开工**（见 `memory/FUTURE.md`）。
 */

import { isTauriRuntime } from "./tauri-env.ts";

/** 一条待处理的崩溃记录摘要（字段与 Rust 侧 `bundle::CrashSummary` 对齐）。 */
export interface CrashSummary {
  reportId: string;
  kind: string;
  createdAt: string;
  dir: string;
  hasDump: boolean;
}

/** 导出结果（Rust 侧 `bundle::BundleOutcome`）。 */
export interface BundleOutcome {
  zipPath: string;
  guidePath: string;
  zipBytes: number;
  entries: string[];
}

/** 崩溃后**首次**启动：有没有一条还没问过用户的崩溃记录。 */
export async function pendingCrash(): Promise<CrashSummary | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<CrashSummary | null>("diagnostics_pending_crash");
}

/** 用户「不报告」：标记这一条已问过，下次启动不再弹（同一次崩溃只问一次）。 */
export async function dismissCrash(crashDir: string): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("diagnostics_dismiss_crash", { crashDir });
}

/** 导出报告包：在 `target` 写 zip，并在同目录写提交指南。存成功也会标记已问过。 */
export async function exportBundle(
  crashDir: string,
  target: string,
  includeDump: boolean,
): Promise<BundleOutcome> {
  if (!isTauriRuntime()) throw new Error("DIAGNOSTICS_NATIVE_REQUIRED");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<BundleOutcome>("diagnostics_export_bundle", { crashDir, target, includeDump });
}

/**
 * 打开诊断根目录（`logs/` 与 `crashes/` 的父目录），返回打开的绝对路径。
 *
 * 浏览器降级：不在 Tauri 里时抛错，由调用方决定怎么说（这里不吞异常，与 `api/db.ts` 同规矩）。
 */
export async function openDiagnosticsFolder(): Promise<string> {
  if (!isTauriRuntime()) throw new Error("DIAGNOSTICS_NATIVE_REQUIRED");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string>("diagnostics_open_folder");
}
