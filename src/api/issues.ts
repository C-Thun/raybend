import { toBytes } from "./db.ts";
import { isTauriRuntime } from "./tauri-env.ts";
import type { DevelopStack } from "./editor.ts";

let coreModule: Promise<typeof import("@tauri-apps/api/core")> | undefined;
function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  coreModule ??= import("@tauri-apps/api/core");
  return coreModule.then((core) => core.invoke<T>(command, args));
}

export interface Issue {
  id: number;
  name: string;
  profileHash: string;
  sourceBase: "raw" | "sooc";
  createdAt: number;
  stack: DevelopStack;
}
export type IssueSelection = "sooc" | "raw" | "latest" | { issue: number };
export interface IssueLibrary {
  issues: Issue[];
  selection: IssueSelection;
  canFinalize: boolean;
  suggestedName: string | null;
  snapshotError: string | null;
}
export async function getIssueLibrary(repositoryId: string, assetId: number, english: boolean, currentStack?: DevelopStack): Promise<IssueLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<IssueLibrary>("issue_library", { repositoryId, assetId, english, currentStack: currentStack ?? null });
}
export async function createIssue(repositoryId: string, assetId: number, name: string, english: boolean): Promise<IssueLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<IssueLibrary>("issue_create", { repositoryId, assetId, name, english });
}
/**
 * 改一条定稿的名字（只动名字：配置 / 基准 / 创建时间都不变）。
 *
 * 与 `create` / `delete` 同一条口径：返回**整份列表**，调用方拿它直接回填
 * （改完不用再拉一次）。
 */
export async function renameIssue(repositoryId: string, assetId: number, issueId: number, name: string, english: boolean): Promise<IssueLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<IssueLibrary>("issue_rename", { repositoryId, assetId, issueId, name, english });
}
export async function deleteIssue(repositoryId: string, assetId: number, issueId: number, english: boolean): Promise<IssueLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<IssueLibrary>("issue_delete", { repositoryId, assetId, issueId, english });
}
export async function getIssueThumb(repositoryId: string, assetId: number, issueId: number, size: "grid" | "strip"): Promise<Uint8Array | null> {
  if (!isTauriRuntime()) return null;
  const bytes = await call<number[] | Uint8Array | ArrayBuffer>("issue_thumb_get", { repositoryId, assetId, issueId, size });
  return toBytes(bytes);
}

export async function prepareIssueSources(path: string): Promise<void> {
  if (isTauriRuntime()) await call("issue_sources_prepare", { path });
}
