/** 本地 AI 的动作与状态。实验分数不以成功标签或概率返回界面。 */
import { invokeBrowseCommand } from "./browse.ts";
import { isTauriRuntime } from "./tauri-env.ts";
export interface AiModelStatus { state: "unpublished" | "missing" | "ready" | "damaged" | "disabled"; manifestSha256: string | null; bytes: number | null; bundled: boolean; compiled: boolean; buildMarker: string }
export interface AiTask { id: number; label: string; state: string; done: number; skipped: number; failed: number; waiting: number; total: number; error: string | null }
export function aiModelStatus(): Promise<AiModelStatus> {
  return isTauriRuntime() ? invokeBrowseCommand("photo_ai_model_status") : Promise.resolve({ state: "unpublished", manifestSha256: null, bytes: null, bundled: false, compiled: false, buildMarker: "web" });
}
export async function aiTasks(): Promise<AiTask[]> { return isTauriRuntime() ? invokeBrowseCommand("photo_ai_tasks") : []; }
export async function aiTaskAction(id: number, action: "pause" | "resume" | "cancel" | "retry"): Promise<boolean> { return invokeBrowseCommand("photo_ai_task_action", { id, action }); }

export type AiRange = { kind: "selected"; photos: { repositoryId: string; assetId: number }[] }
  | { kind: "directory"; repositoryId: string; directory: string; recursive: boolean }
  | { kind: "repository"; repositoryId: string };
export async function aiStart(range: AiRange, rerun: boolean, zh: boolean): Promise<number> { return invokeBrowseCommand("photo_ai_start", { range, rerun, zh }); }
export async function aiInstall(directory: string | null = null): Promise<void> { return invokeBrowseCommand("photo_ai_model_install", { directory }); }
export async function aiUninstall(): Promise<boolean> { return invokeBrowseCommand("photo_ai_model_uninstall"); }
export async function aiForeground(busy: boolean): Promise<void> { if (isTauriRuntime()) await invokeBrowseCommand("photo_ai_foreground", { busy }); }
