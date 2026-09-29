/** 按一次执行记账；多个 catalog 的完成事件不能互相清走。 */
import type { MigrationNotice, MigrationSnapshot } from "../../api/types.ts";
import { revision } from "../../lib/revision.ts";

export type MigrationMap = ReadonlyMap<string, MigrationNotice>;
export const NO_MIGRATIONS: MigrationMap = new Map();
export interface MigrationState {
  revision: string;
  notices: MigrationMap;
}
export const NO_MIGRATION_STATE: MigrationState = { revision: "0", notices: NO_MIGRATIONS };

export function applyNotice(current: MigrationMap, notice: MigrationNotice): MigrationMap {
  const next = new Map(current);
  if (notice.running) next.set(notice.id, notice);
  else next.delete(notice.id);
  return next;
}

/** 完整快照只接纳新 revision，事件晚到也不会复活已结束的升级。 */
export function applySnapshot(current: MigrationState, snapshot: MigrationSnapshot): MigrationState {
  if (revision(snapshot.revision) <= revision(current.revision)) return current;
  return { revision: snapshot.revision, notices: snapshot.active.reduce(applyNotice, NO_MIGRATIONS) };
}

/** 显示稳定：先跨度，再种类和执行编号。所有活动执行均继续阻塞。 */
export function activeNotice(map: MigrationMap): MigrationNotice | null {
  let best: MigrationNotice | null = null;
  for (const notice of map.values()) {
    const span = notice.to - notice.from;
    const bestSpan = best === null ? -1 : best.to - best.from;
    if (best === null || span > bestSpan || (span === bestSpan &&
      (notice.kind < best.kind || (notice.kind === best.kind && revision(notice.id) < revision(best.id))))) {
      best = notice;
    }
  }
  return best;
}
