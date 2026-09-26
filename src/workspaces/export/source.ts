/** 导出数据适配，PhotoGrid/Tile/分组行模型仍唯一。 */
import { createMemo, createEffect, on, onCleanup } from "solid-js";
import type {
  TilesSource,
  GridItem,
} from "../../components/ui/tiles/source.ts";
import type { RowSlice } from "../../components/ui/tiles/rows.ts";
import type { ThumbEntry } from "../../components/ui/thumb-queue.ts";
import type { ExportQueueItem } from "../../lib/export-model.ts";
import { variantKey, variantAssetId, mainVariant, admitsPhoto, orderedIssues } from "../../lib/export-model.ts";
import type { ExportStore } from "./store.ts";
export function issueExtraHeight(count: number, cellSize: number): number {
  if(count<=0)return 0;
  const small=Math.max(1,(cellSize-4)/2);
  return 4 + Math.ceil(Math.min(count,6)/2)*(small+4) + (count>6?20:0);
}
export function exportGallerySource(
  base: TilesSource,
  store: ExportStore,
  thumbs?: { get(key:string):ThumbEntry;request(key:string):void },
): TilesSource {
  const order = createMemo(() =>
    Array.from({ length: base.count() }, (_, i) => i).filter((i) => {
      const id = Number(base.idAt?.(i) ?? base.itemAt(i)?.id);
      return (
        store.preferences.value().scope === "all" ||
        !store.variants().has(id) ||
        admitsPhoto(store.listFor(id), store.preferences.value().scope)
      );
    }),
  );
  const allAssets = createMemo(() =>
    Array.from({ length: base.count() }, (_, i) =>
      Number(base.idAt?.(i) ?? base.itemAt(i)?.id),
    ).filter((id) => Number.isSafeInteger(id) && id > 0),
  );
  const visibleAssets=()=>order().map(i=>Number(base.idAt?.(i)??base.itemAt(i)?.id)).filter(id=>Number.isSafeInteger(id)&&id>0);
  let disposed=false;onCleanup(()=>{disposed=true;});
  createEffect(on(() => JSON.stringify([base.scopeKey(),allAssets(),store.preferences.value().scope]), () => {
    const ids=store.preferences.value().scope==="all" ? allAssets().slice(0,128) : allAssets();
    const key=base.scopeKey();
    void (async()=>{for(let start=0;start<ids.length && !disposed && key===base.scopeKey();start+=128)await store.ensure(ids.slice(start,start+128));})().catch(store.reportError);
  }));
  const adapt = (item: GridItem|null): GridItem|null => {
    if(item===null)return null;
    const main=mainVariant(store.listFor(Number(item.id)));
    if(!main)return null;
    return {...item,path:JSON.stringify([store.repository(),main.reference,main.profileHash]),
      fileName: `${item.fileName} · ${main.name}`, selectionFrame:true,
      selectionLocked:store.locked(main),disabled:store.locked(main),aspect:1};
  };
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
    status: () => {
      if(base.status()==="loading")return "loading";
      const required=store.preferences.value().scope==="all"?allAssets().slice(0,128):allAssets();
      if(required.some(id=>!store.variants().has(id)))return store.error()?"error":"loading";
      return base.status();
    },
    count: () => order().length,
    idAt: id,
    itemAt: (i) => {
      const index = at(i);
      return index === undefined ? null : adapt(base.itemAt(index));
    },
    extraHeight: (i, size) => {
      const asset = Number(id(i));
      return issueExtraHeight(
        store.variants().has(asset) ? orderedIssues(store.listFor(asset),()=>false).length : 0,
        size,
      );
    },
    selection: createMemo(() => {
      const ids = new Set<string>();
      for (const asset of allAssets()) {
        const main=mainVariant(store.listFor(asset));
        if(main && store.selection().ids.has(variantKey(store.repository()??"",main.reference)))ids.add(String(asset));
      }
      const anchor = store.selection().anchor;
      const anchorAsset = anchor === null ? null : variantAssetId(anchor);
      return {
        ids,
        anchor: anchorAsset === null ? null : String(anchorAsset),
      };
    }),
    thumb: thumbs?.get ?? base.thumb,
    requestThumb: thumbs?.request ?? base.requestThumb,
    aspectOf: () => 1,
    naturalOf: () => null,
    ensureNatural: () => {},
    itemById: (key) => adapt(base.itemById(key)),
    select: (key, mode) => {
      const main=mainVariant(store.listFor(Number(key)));
      if(main)void store.selectIssue(main.reference,mode,visibleAssets()).catch(store.reportError);
    },
    /*
     * 日组 / 时间片那颗药丸 = **整段开关**（`BROWSE.md` §5.2.2）：全选中 → 全取消，否则 → 全开。
     *
     * `store.group` **不带第二参**就是这条开关（内部走共享的 `toggleGroupSelection`）；
     * `additive = false` 那条分支是「**替换成只有这一组**」，给 `selectAllActive`（Mod+A）用 ——
     * 不是给药丸用的，所以**这里不传第三个参数**。
     * （曾有一句 `additive = true`，它既是死参数又会被误读成「只加不减」——已删。）
     */
    selectGroupRange: (start, count) => {
      const ids = order()
        .slice(start, start + count)
        .map((i) => Number(base.idAt?.(i) ?? base.itemAt(i)?.id));
      void store.group(ids).catch(store.reportError);
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
    clearSelection: () => {store.focusArea("gallery");store.clear();},
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
    selectionFrame:true,selectionLocked:["running","done","skipped"].includes(item.status),disabled:["running","done","skipped"].includes(item.status),
  });
  return {
    count: () => entries().length,
    extraHeight: i => entries()[i]?.status === "failed" ? 68 : 0,
    invertedCtrl: true,
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
    selection: store.queueSelection,
    select: store.selectQueue,
    setAnchor: () => {},
    selectGroupRange: () => {},
    clearSelection: () => {store.focusArea("queue");store.clear();},
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
