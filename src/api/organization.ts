/** 相片整理 IPC。界面只见有类型的动作，不直接拼 Tauri 命令。 */

import { invokeBrowseCommand } from "./browse.ts";
import { isTauriRuntime } from "./tauri-env.ts";
import type { AssetItem, BrowseFilter, BrowseTimeline } from "./types.ts";

export interface RuleGroup {
  repositoryIds: string[];
  minRating?: number | null;
  colors?: string[];
  likes?: string[];
  locks?: number[];
  tagKeys?: string[];
}

export interface RuleSet { groups: RuleGroup[] }

export interface PhotoBucket {
  id: number;
  name: string;
  pinned: boolean;
  paused: boolean;
  rules: RuleSet;
  ruleRevision: number;
  memberCount: number;
}

export interface PhotoRef {
  repositoryId: string;
  photoUid: string;
  assetId: number;
}

export interface AssetIdentity {
  repositoryId: string;
  assetId: number;
}

export async function listPhotoBuckets(): Promise<PhotoBucket[]> {
  return isTauriRuntime() ? invokeBrowseCommand("organization_buckets") : [];
}

export async function createPhotoBucket(name: string, rules: RuleSet = { groups: [] }): Promise<PhotoBucket> {
  return invokeBrowseCommand("organization_bucket_create", { name, rules });
}

export async function renamePhotoBucket(id: number, name: string): Promise<boolean> {
  return invokeBrowseCommand("organization_bucket_rename", { id, name });
}

export async function savePhotoBucketRules(id: number, name: string, rules: RuleSet): Promise<PhotoBucket | null> {
  return invokeBrowseCommand("organization_bucket_rules", { id, name, rules });
}

export async function pinPhotoBucket(id: number, pinned: boolean): Promise<boolean> {
  return invokeBrowseCommand("organization_bucket_pin", { id, pinned });
}

export async function pausePhotoBucket(id: number, paused: boolean): Promise<boolean> {
  return invokeBrowseCommand("organization_bucket_pause", { id, paused });
}

export async function deletePhotoBucket(id: number): Promise<boolean> {
  return invokeBrowseCommand("organization_bucket_delete", { id });
}

export async function addPhotosToBucket(bucketId: number, photos: AssetIdentity[]): Promise<number> {
  return invokeBrowseCommand("organization_bucket_add", { bucketId, photos });
}

export async function removePhotoFromBucket(bucketId: number, photo: AssetIdentity): Promise<boolean> {
  return invokeBrowseCommand("organization_bucket_remove", { bucketId, photo });
}

export async function bucketMembers(bucketId: number, repositoryId: string): Promise<PhotoRef[]> {
  return isTauriRuntime()
    ? invokeBrowseCommand("organization_bucket_members", { bucketId, repositoryId })
    : [];
}

export async function syncPhotoTags(repositoryId: string): Promise<number> {
  return isTauriRuntime() ? invokeBrowseCommand("organization_tag_sync", { repositoryId }) : 0;
}

export async function setDirectoryTag(
  repositoryId: string, directoryKey: string, name: string, enabled: boolean,
): Promise<boolean> {
  return invokeBrowseCommand("organization_directory_tag", { repositoryId, directoryKey, name, enabled });
}

export async function directoriesForTag(
  repositoryId: string, tagKey: string, offset = 0, limit = 100,
): Promise<string[]> {
  return isTauriRuntime()
    ? invokeBrowseCommand("organization_tag_directories", { repositoryId, tagKey, offset, limit })
    : [];
}

export async function directoryTagCounts(repositoryId: string): Promise<Record<string, number>> {
  return isTauriRuntime()
    ? invokeBrowseCommand("organization_tag_directory_counts", { repositoryId })
    : {};
}

export async function tagsForDirectory(repositoryId: string, directoryKey: string): Promise<string[]> {
  return isTauriRuntime()
    ? invokeBrowseCommand("organization_directory_tags", { repositoryId, directoryKey })
    : [];
}

export async function timelineForTag(repositoryId: string, tagKey: string, filter: BrowseFilter): Promise<BrowseTimeline> {
  return isTauriRuntime()
    ? invokeBrowseCommand("organization_tag_timeline", { repositoryId, tagKey, filter })
    : { total: 0, entries: [] };
}

export async function timelineForBucket(bucketId: number, repositoryId: string, filter: BrowseFilter): Promise<BrowseTimeline> {
  return isTauriRuntime()
    ? invokeBrowseCommand("organization_bucket_timeline", { bucketId, repositoryId, filter })
    : { total: 0, entries: [] };
}

export async function organizationAssets(repositoryId: string, ids: number[]): Promise<AssetItem[]> {
  return isTauriRuntime()
    ? invokeBrowseCommand("organization_assets", { repositoryId, ids })
    : [];
}

export interface ReconcileReport { processedBatches: number; failedRepositories: string[] }
export interface RulesPreview { matches: number; failedRepositories: string[] }

export async function previewBucketRules(rules: RuleSet): Promise<RulesPreview> {
  return isTauriRuntime()
    ? invokeBrowseCommand("organization_rules_preview", { rules })
    : { matches: 0, failedRepositories: [] };
}

export async function reconcileOrganization(repositoryIds: string[]): Promise<ReconcileReport> {
  return isTauriRuntime()
    ? invokeBrowseCommand("organization_reconcile", { repositoryIds })
    : { processedBatches: 0, failedRepositories: [] };
}
