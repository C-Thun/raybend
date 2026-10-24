/**
 * 相片桶成员计数的**唯一**换算口（规格 `specs/photo-organization.md` §5.2）。
 *
 * 桶成员跨库存在 app.db；卡片要显示「当前挂载 42 / 全部成员 80」，
 * 而「当前挂载」只能由**在线库集合**决定 —— 这个判断只写在这里，
 * 面板与照片区空态共用同一份。
 */

import type { PhotoBucket } from "../../api/organization.ts";

/** 当前挂载库里的成员数（按库计数求和；离线库不计）。 */
export function mountedMemberCount(
  bucket: Pick<PhotoBucket, "memberCountsByRepository">,
  onlineRepositoryIds: ReadonlySet<string>,
): number {
  let total = 0;
  for (const [repositoryId, count] of Object.entries(bucket.memberCountsByRepository)) {
    if (onlineRepositoryIds.has(repositoryId)) total += count;
  }
  return total;
}

/** 桶有成员、但一个都不在当前挂载库 —— 空态应该解释成「所在库未挂载」而不是「空桶」。 */
export function membersAllOffline(
  bucket: Pick<PhotoBucket, "memberCount" | "memberCountsByRepository">,
  onlineRepositoryIds: ReadonlySet<string>,
): boolean {
  return bucket.memberCount > 0 && mountedMemberCount(bucket, onlineRepositoryIds) === 0;
}
