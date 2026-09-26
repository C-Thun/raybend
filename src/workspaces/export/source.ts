/** 导出数据适配，PhotoGrid/Tile/分组行模型仍唯一。 */
import { createMemo } from "solid-js";
import type {
  TilesSource,
  GridItem,
} from "../../components/ui/tiles/source.ts";
import type { RowSlice } from "../../components/ui/tiles/rows.ts";
import type { ThumbEntry } from "../../components/ui/thumb-queue.ts";
import type { ExportQueueItem } from "../../lib/export-model.ts";
import { variantKey } from "../../lib/export-model.ts";
import type { ExportStore } from "./store.ts";
export const ISSUE_CHIP_WIDTH = 56;
export const ISSUE_CHIP_HEIGHT = 74;
export function issueExtraHeight(count: number, cellSize: number): number {
  return count === 0
    ? 0
    : 4 +
        Math.ceil(
          count /
            Math.max(1, Math.floor((cellSize + 4) / (ISSUE_CHIP_WIDTH + 4))),
        ) *
          ISSUE_CHIP_HEIGHT;
}
export function exportGallerySource(
  base: TilesSource,
  store: ExportStore,
): TilesSource {
  const order = createMemo(() =>
    Array.from({ length: base.count() }, (_, i) => i).filter((i) => {
      const id = Number(base.idAt?.(i) ?? base.itemAt(i)?.id);
      return (
        store.preferences.value().scope === "all" ||
        !store.variants().has(id) ||
        store.listFor(id).length > 0
      );
    }),
  );
  const allAssets = createMemo(() =>
    Array.from({ length: base.count() }, (_, i) =>
      Number(base.idAt?.(i) ?? base.itemAt(i)?.id),
    ).filter((id) => Number.isSafeInteger(id) && id > 0),
  );
  const at = (i: number) => order()[i];
  const id = (i: number) => {
    const index = at(i);
    return index === undefined
      ? null
      : (base.idAt?.(index) ?? base.itemAt(index)?.id ?? null);
  };
  return {
    ...base,
    invertedCtrl: true,
    count: () => order().length,
    idAt: id,
    itemAt: (i) => {
      const index = at(i);
      return index === undefined ? null : base.itemAt(index);
    },
    extraHeight: (i, size) => {
      const asset = Number(id(i));
      return issueExtraHeight(
        store.variants().has(asset) ? store.listFor(asset).length : 1,
        size,
      );
    },
    selection: createMemo(() => {
      const ids = new Set<string>();
      for (const asset of allAssets()) {
        const variants = store.listFor(asset);
        if (
          variants.length > 0 &&
          variants.every((variant) =>
            store
              .selection()
              .ids.has(variantKey(store.repository() ?? "", variant.reference)),
          )
        )
          ids.add(String(asset));
      }
      const anchor = store.selection().anchor;
      return {
        ids,
        anchor:
          anchor === null
            ? null
            : String((JSON.parse(anchor) as [string, number, string])[1]),
      };
    }),
    select: (key, mode) => {
      void store
        .selectAssets([Number(key)], mode, allAssets())
        .catch(store.reportError);
    },
    selectGroupRange: (start, count, additive = true) => {
      const ids = order()
        .slice(start, start + count)
        .map((i) => Number(base.idAt?.(i) ?? base.itemAt(i)?.id));
      void store.group(ids, additive).catch(store.reportError);
    },
    setAnchor: (asset) => {
      const variant = store
        .listFor(Number(asset))
        .find((v) =>
          store
            .selection()
            .ids.has(variantKey(store.repository() ?? "", v.reference)),
        );
      if (variant !== undefined) store.focusIssue(variant.reference);
    },
    clearSelection: store.clear,
    ensureRange: async (start, end) => {
      const indices = order().slice(start, end);
      if (indices.length === 0) return;
      await base.ensureRange?.(Math.min(...indices), Math.max(...indices) + 1);
      await store.ensure(
        indices
          .map((i) => Number(base.idAt?.(i) ?? base.itemAt(i)?.id))
          .filter((asset) => asset > 0),
      );
    },
    slices: () => {
      const slices = base.slices();
      if (slices === undefined) return undefined;
      const indices = new Set(order());
      let at = 0;
      const out: RowSlice[] = [];
      for (const slice of slices) {
        let count = 0;
        for (let i = slice.start; i < slice.start + slice.count; i++)
          if (indices.has(i)) count++;
        if (count === 0) continue;
        out.push({ ...slice, start: at, count });
        at += count;
      }
      for (const slice of out) {
        const day = out.filter((s) => s.dayId === slice.dayId);
        slice.dayStart = day[0]!.start;
        slice.dayCount = day.reduce((n, s) => n + s.count, 0);
      }
      return out;
    },
  };
}
export function exportQueueSource(
  store: ExportStore,
  thumbs: { get(key: string): ThumbEntry; request(key: string): void },
): TilesSource {
  const entries = () =>
    store.queues().get(store.selectedPreset()?.id ?? "") ?? [];
  const toItem = (item: ExportQueueItem): GridItem => ({
    id: item.id,
    path: queueImageKey(item),
    fileName: `${item.snapshot.relPath.split("/").pop() ?? ""} · ${item.snapshot.name}`,
    ext: item.preset.format,
  });
  return {
    count: () => entries().length,
    extraHeight: () => 20,
    idAt: (i) => entries()[i]?.id ?? null,
    itemAt: (i) => (entries()[i] === undefined ? null : toItem(entries()[i]!)),
    itemById: (id) => {
      const item = entries().find((item) => item.id === id);
      return item === undefined ? null : toItem(item);
    },
    aspectOf: () => 1,
    naturalOf: () => null,
    ensureNatural: () => {},
    slices: () => undefined,
    status: () => "ready",
    error: () => null,
    reload: () => {},
    scopeKey: () => `queue:${store.selectedPreset()?.id ?? ""}`,
    selection: () => ({ ids: new Set(), anchor: null }),
    select: () => {},
    setAnchor: () => {},
    selectGroupRange: () => {},
    clearSelection: () => {},
    infoMode: () => "marks-name",
    tileStep: () => store.preferences.value().queueStep,
    setTileStep: (queueStep) => store.preferences.update({ queueStep }, false),
    commitTileStep: () => store.preferences.commit(),
    thumb: thumbs.get,
    requestThumb: thumbs.request,
  };
}
export function queueImageKey(item: ExportQueueItem): string {
  return JSON.stringify([
    item.repositoryId,
    item.snapshot.reference,
    item.snapshot.profileHash,
  ]);
}
