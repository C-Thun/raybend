/** 跨库标签/桶的 tiles 数据适配；照片身份始终带库 ID。 */
import { createSignal } from "solid-js";
import { organizationAssets, timelineForBucket, timelineForTag } from "../../api/organization.ts";
import { flagsGet } from "../../api/browse.ts";
import type { AssetItem, BrowseFilter, RepositoryView } from "../../api/types.ts";
import type { TilesSource, GridItem } from "../../components/ui/tiles/source.ts";
import type { ThumbEntry } from "../../components/ui/thumb-queue.ts";
import { clampDisplayAspect } from "../../lib/tile-flow.ts";
import { joinPath } from "../../lib/paths.ts";
import { applySelection, clearSelection, EMPTY_SELECTION, focusSelection, pruneSelection, selectAll, toggleGroupSelection, type SelectionState } from "../../lib/selection.ts";
import type { TileInfoMode } from "../../lib/display-prefs.ts";
import type { OrganizationSelection } from "./OrganizationPanel.tsx";

export interface OrganizationEntry {
  repositoryId: string;
  assetId: number;
  takenAt: number | null;
  relPath: string | null;
}

export const organizationPhotoId = (repo: string, assetId: number): string => JSON.stringify([repo, assetId]);

export function createOrganizationSource(deps: {
  repositories: () => readonly RepositoryView[];
  thumbs: { get(path: string): ThumbEntry; request(path: string): void };
  tileStep: () => number;
  setTileStep: (step: number) => void;
  commitTileStep: () => void;
  infoMode: () => TileInfoMode;
  filter: () => BrowseFilter;
  onPartialFailure?: (repositoryIds: string[]) => void;
  api?: {
    tagTimeline: typeof timelineForTag;
    bucketTimeline: typeof timelineForBucket;
    assets: typeof organizationAssets;
    flags: typeof flagsGet;
  };
}) {
  const api = deps.api ?? { tagTimeline: timelineForTag, bucketTimeline: timelineForBucket,
    assets: organizationAssets, flags: flagsGet };
  const [entries, setEntries] = createSignal<readonly OrganizationEntry[]>([]);
  const [orderedIds, setOrderedIds] = createSignal<readonly string[]>([]);
  const [entriesById, setEntriesById] = createSignal<ReadonlyMap<string, OrganizationEntry>>(new Map());
  const [items, setItems] = createSignal<ReadonlyMap<string, AssetItem>>(new Map());
  const [selection, setSelection] = createSignal<SelectionState>(EMPTY_SELECTION);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [scope, setScope] = createSignal("");
  let selectionScope = "";
  let generation = 0;
  let lastFailedRepositories = "";
  const reportedHydrationFailures = new Set<string>();
  const inFlight = new Set<string>();
  let pending = new Map<string, Set<number>>();
  let flushTimer: ReturnType<typeof setTimeout> | null = null;

  function idAt(index: number): string | null {
    const entry = entries()[index];
    return entry ? organizationPhotoId(entry.repositoryId, entry.assetId) : null;
  }
  function entryById(id: string): OrganizationEntry | undefined {
    return entriesById().get(id);
  }
  function itemById(id: string): AssetItem | null { return items().get(id) ?? null; }
  function absolute(entry: OrganizationEntry, item: AssetItem): string | null {
    const root = deps.repositories().find((repo) => repo.id === entry.repositoryId && repo.online)?.root;
    return root ? joinPath(root, item.relPath) : null;
  }
  function gridItem(id: string): GridItem | null {
    const entry = entryById(id), item = itemById(id);
    if (!entry || !item) return null;
    const path = absolute(entry, item);
    if (!path) return null;
    return {
      id, path, fileName: item.fileName, ext: item.ext,
      aspect: clampDisplayAspect(item.width ?? 0, item.height ?? 0),
      isRaw: item.isRaw, hasRaw: item.hasRaw, issueCount: item.issueCount,
      edited: item.edited, missing: item.missing,
      marks: { rating: item.rating, colorLabel: item.colorLabel, likeState: item.likeState,
        flag: null, locked: item.lockLevel > 0 },
    };
  }

  async function load(target: OrganizationSelection): Promise<void> {
    const mine = ++generation;
    if (flushTimer !== null) clearTimeout(flushTimer);
    flushTimer = null; pending = new Map(); inFlight.clear();
    reportedHydrationFailures.clear();
    const online = deps.repositories().filter((repo) => repo.online && repo.root !== null);
    const filter = deps.filter();
    const nextScope = JSON.stringify([target, online.map((repo) => [repo.id, repo.connection?.generation])]);
    if (selectionScope !== nextScope) setSelection(EMPTY_SELECTION);
    selectionScope = nextScope;
    setScope(nextScope);
    setEntries([]); setOrderedIds([]); setEntriesById(new Map()); setItems(new Map()); setError(null);
    if (target?.kind !== "tag" && target?.kind !== "bucket") return;
    setLoading(true);
    try {
      const settled = await Promise.allSettled(online.map(async (repo): Promise<OrganizationEntry[]> => {
        const localFilter: BrowseFilter = filter.flag ? { ...filter, flag: { ...filter.flag, ids: [] } } : filter;
        if (filter.flag) {
          const flags = await api.flags(repo.id);
          localFilter.flag = { mode: filter.flag.mode, ids: filter.flag.mode === "pick" ? flags.picks :
            filter.flag.mode === "reject" ? flags.rejects : [...flags.picks, ...flags.rejects] };
        }
        if (target.kind === "tag") {
          const timeline = await api.tagTimeline(repo.id, target.key, localFilter);
          return timeline.entries.map((entry) => ({ repositoryId: repo.id, assetId: entry.id,
            takenAt: entry.takenAt, relPath: entry.relPath }));
        }
        const timeline = await api.bucketTimeline(target.id, repo.id, localFilter);
        return timeline.entries.map((entry) => ({ repositoryId: repo.id, assetId: entry.id,
          takenAt: entry.takenAt, relPath: entry.relPath }));
      }));
      if (mine !== generation) return;
      const failed = settled.flatMap((result, index) => result.status === "rejected" ? [online[index].id] : []);
      const failureKey = [...failed].sort().join("\u0000");
      if (failureKey !== lastFailedRepositories) {
        lastFailedRepositories = failureKey;
        if (failed.length > 0) deps.onPartialFailure?.(failed);
      }
      const chunks = settled.flatMap((result) => result.status === "fulfilled" ? [result.value] : []);
      if (chunks.length === 0 && failed.length > 0) {
        const reason = settled.find((result) => result.status === "rejected");
        setError(reason?.status === "rejected" ? String(reason.reason) : "");
        return;
      }
      const ordered = chunks.flat().sort((a, b) => (b.takenAt ?? 0) - (a.takenAt ?? 0) ||
        a.repositoryId.localeCompare(b.repositoryId) || a.assetId - b.assetId);
      setEntries(ordered);
      const ids = ordered.map((entry) => organizationPhotoId(entry.repositoryId, entry.assetId));
      setOrderedIds(ids);
      setEntriesById(new Map(ordered.map((entry, index) => [ids[index], entry])));
      setSelection((before) => pruneSelection(before, ids));
    } catch (reason) { if (mine === generation) setError(String(reason)); }
    finally { if (mine === generation) setLoading(false); }
  }

  async function flushPending(): Promise<void> {
    flushTimer = null;
    const mine = generation;
    const byRepo = pending;
    pending = new Map();
    for (const [repositoryId, ids] of byRepo) {
      const all = [...ids];
      for (let index = 0; index < all.length; index += 500) {
        const batch = all.slice(index, index + 500);
        try {
          const loaded = await api.assets(repositoryId, batch);
          if (mine !== generation) return;
          setItems((before) => {
            const next = new Map(before);
            for (const item of loaded) next.set(organizationPhotoId(repositoryId, item.id), item);
            return next;
          });
          if (loaded.length > 0) setError(null);
        } catch (reason) {
          if (mine === generation) {
            if (!reportedHydrationFailures.has(repositoryId)) {
              reportedHydrationFailures.add(repositoryId);
              deps.onPartialFailure?.([repositoryId]);
            }
            if (items().size === 0) setError(String(reason));
          }
        }
        finally { if (mine === generation) for (const id of batch) inFlight.delete(organizationPhotoId(repositoryId, id)); }
      }
    }
  }

  function ensureRange(start: number, end: number): void {
    for (let index = Math.max(0, start); index < Math.min(end, entries().length); index += 1) {
      const entry = entries()[index];
      const key = organizationPhotoId(entry.repositoryId, entry.assetId);
      if (items().has(key) || inFlight.has(key)) continue;
      inFlight.add(key);
      const ids = pending.get(entry.repositoryId) ?? new Set<number>();
      ids.add(entry.assetId);
      pending.set(entry.repositoryId, ids);
    }
    if (pending.size > 0 && flushTimer === null) flushTimer = setTimeout(() => { void flushPending(); }, 0);
  }

  function ensureSelected(): void {
    for (const id of selection().ids) {
      const entry = entryById(id);
      if (!entry || items().has(id) || inFlight.has(id)) continue;
      inFlight.add(id);
      const ids = pending.get(entry.repositoryId) ?? new Set<number>();
      ids.add(entry.assetId);
      pending.set(entry.repositoryId, ids);
    }
    if (pending.size > 0 && flushTimer === null) flushTimer = setTimeout(() => { void flushPending(); }, 0);
  }

  const source: TilesSource = {
    count: () => entries().length,
    idAt,
    itemAt: (index) => { const id = idAt(index); return id ? gridItem(id) : null; },
    itemById: gridItem,
    ensureRange,
    aspectOf: (id) => { const item = itemById(id); return clampDisplayAspect(item?.width ?? 0, item?.height ?? 0); },
    naturalOf: (id) => { const item = itemById(id); return item?.width && item.height ? { width: item.width, height: item.height } : null; },
    ensureNatural: () => {},
    slices: () => undefined,
    status: () => error() ? "error" : loading() ? "loading" : "ready",
    error,
    reload: () => {},
    scopeKey: scope,
    selection: () => selection(),
    select: (id, mode) => setSelection((before) => applySelection(before, orderedIds(), id, mode)),
    setAnchor: (id) => setSelection((before) => focusSelection(before, id)),
    selectGroupRange: (start, count) => setSelection((before) => toggleGroupSelection(before, entries().slice(start, start + count).map((entry) => organizationPhotoId(entry.repositoryId, entry.assetId)))),
    clearSelection: () => setSelection(clearSelection()),
    infoMode: deps.infoMode,
    tileStep: deps.tileStep,
    setTileStep: deps.setTileStep,
    commitTileStep: deps.commitTileStep,
    thumb: deps.thumbs.get,
    requestThumb: deps.thumbs.request,
  };
  return { source, load, selection, entryById, itemById, entries, error, ensureSelected,
    selectAll: () => setSelection(selectAll(orderedIds())),
    clearSelection: () => setSelection(clearSelection()),
  };
}
