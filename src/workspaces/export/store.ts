import { batch, createSignal } from "solid-js";
import {
  applySelection,
  clearSelection,
  toggleGroupSelection,
  focusSelection,
  selectAll,
  type SelectionState,
} from "../../lib/selection.ts";
import {
  DEFAULT_EXPORT_TEMPLATE,
  queueProgress, queueFinished,
  readPresets,
  serializePresets,
  variantKey,
  variantAssetId,
  visibleVariants,
  mainVariant, orderedIssues, admitsPhoto,
  type AssetVariants,
  type ExportPreset,
  type ExportQueueItem,
  type VariantRef,
  type VariantSnapshot,
  type VariantSummary,
} from "../../lib/export-model.ts";
import { createExportPreferences } from "../../lib/export-prefs.ts";
import { t } from "../../i18n/index.ts";
import { presetErrors } from "../../lib/export-model.ts";

import type {ExportQueueAction,ExportQueueView} from "../../api/export.ts";

export const EXPORT_PRESETS_KEY = "export.presets.v3";
export const PREVIOUS_EXPORT_PRESETS_KEY = "export.presets.v2";
export const LEGACY_EXPORT_PRESETS_KEY = "export.presets.v1";
export interface ExportStoreDeps {
  getSetting(key: string): Promise<string | null>;
  setSetting(key: string, value: string): Promise<void>;
  variants(
    repositoryId: string,
    assets: readonly number[],
  ): Promise<AssetVariants[]>;
  snapshots(
    repositoryId: string,
    refs: readonly VariantRef[],
  ): Promise<VariantSnapshot[]>;
  validate(
    preset: ExportPreset,
  ): Promise<{ errors: Record<string, string>; warnings: string[] }>;
  runtime?(action:ExportQueueAction,payload?:Record<string,unknown>):Promise<ExportQueueView>;
  subscribe?(handler:(view:ExportQueueView)=>void):Promise<()=>void>;
  id?(): string;
  preferences?: ReturnType<typeof createExportPreferences>;
}
export function createExportStore(deps: ExportStoreDeps) {
  const preferences = deps.preferences ?? createExportPreferences();
  const [presets, setPresets] = createSignal<readonly ExportPreset[]>([]);
  const [queues, setQueues] = createSignal<
    ReadonlyMap<string, readonly ExportQueueItem[]>
  >(new Map());
  const [enabled, setEnabled] = createSignal<ReadonlySet<string>>(new Set());
  const [selection, setSelection] =
    createSignal<SelectionState>(clearSelection());
  const [activeArea, setActiveArea] = createSignal<"gallery"|"queue">("gallery");
  const [queueSelection, setQueueSelection] = createSignal<SelectionState>(clearSelection());
  const [variants, setVariants] = createSignal<
    ReadonlyMap<number, readonly VariantSummary[]>
  >(new Map());
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [loading, setLoading] = createSignal(true);
  const [repository, setRepository] = createSignal<string | null>(null);
  let contextKey = "";
  let revision = 0;
  let selectionRevision = 0;
  let queueRevision = 0;
  let sequence = 0;
  let runtimeRevision=-1,generation=0,minimumGeneration=0;
  const [resetting,setResetting]=createSignal(false);
  let offRuntime:(()=>void)|undefined;
  const [limitOpen,setLimitOpen]=createSignal(false);
  let disposed = false;
  let preserveDragSelection = false;
  let writeChain: Promise<void> = Promise.resolve();
  const inflight = new Map<number, Promise<void>>();
  const variantRevisions = new Map<number, number>();
  const staleVariants = new Set<number>();
  const blank = (): ExportPreset => ({
    id: "new",
    name: "",
    format: "webp",
    quality: 90,
    maxEdge: 2048,
    sizeMode: "original",
    percent: 50,
    directory: "",
    template: DEFAULT_EXPORT_TEMPLATE,
    existingFile: "append",
  });
  const [draft, setDraft] = createSignal<ExportPreset>(blank());
  const [validation, setValidation] = createSignal<{
    errors: Record<string, string>;
    warnings: string[];
  }>({ errors: {}, warnings: [] });
  const selectedPreset = () =>
    presets().find((p) => p.id === preferences.value().selectedPreset) ?? null;
  const [confirmedName, setConfirmedName] = createSignal("");
  const [nameChecking, setNameChecking] = createSignal(false);
  let nameTimer: ReturnType<typeof setTimeout> | undefined;
  function confirmName(name: string): void {
    clearTimeout(nameTimer);
    nameTimer = undefined;
    batch(() => { setConfirmedName(name.trim().normalize("NFC")); setNameChecking(false); });
  }
  const matchingPreset = (name = confirmedName()) => {
    const normalized = name.trim().normalize("NFC");
    return presets().find(p => p.name === normalized) ?? null;
  };
  const dirty = () =>
    selectedPreset() === null ||
    JSON.stringify(draft()) !== JSON.stringify(selectedPreset());
  const canRun = () => {
    const preset = selectedPreset();
    return preset !== null && (enabled().has(preset.id) ||
      (!dirty() && !busy() && !resetting() && !loading() && !nameChecking() && Object.keys(validation().errors).length === 0));
  };
  const emptyVariants: readonly VariantSummary[] = [];
  const visibleCache = new WeakMap<readonly VariantSummary[], readonly VariantSummary[]>();
  const smallCache = new Map<number, {list: readonly VariantSummary[]; promoted: string; result: readonly VariantSummary[]}>();
  const listFor = (asset: number): readonly VariantSummary[] => {
    const values = variants().get(asset) ?? emptyVariants;
    let visible = visibleCache.get(values);
    if (!visible) {visible=visibleVariants(values,"all");visibleCache.set(values,visible);}
    return visible;
  };
  const queueItems = () => queues().get(selectedPreset()?.id ?? "") ?? [];
  const queueState = (ref: VariantRef, hash?: string|null) => queueItems().find(item =>
    item.repositoryId === repository() && item.snapshot.reference.assetId === ref.assetId &&
    (hash ? item.snapshot.profileHash === hash : item.snapshot.reference.variant === ref.variant));
  const locked = (v: VariantSummary) => ["running","done","skipped"].includes(queueState(v.reference,v.profileHash)?.status ?? "");
  const pruneLockedSelection = () => {
    if (preserveDragSelection) return;
    const lockedKeys=new Set([...variants().values()].flat().filter(locked).map(v=>variantKey(repository()??"",v.reference)));
    setSelection(old=>({ids:new Set([...old.ids].filter(key=>!lockedKeys.has(key))),anchor:old.anchor!==null&&lockedKeys.has(old.anchor)?null:old.anchor}));
  };
  const smallFor = (asset: number): readonly VariantSummary[] => {
    const list=listFor(asset);
    const promoted=new Set(list.filter(v=>selection().ids.has(variantKey(repository()??"",v.reference)) || queueState(v.reference,v.profileHash)!==undefined).map(v=>v.reference.variant));
    const signature=JSON.stringify([...promoted]), old=smallCache.get(asset);
    if(old?.list===list && old.promoted===signature)return old.result;
    const result=orderedIssues(list,v=>promoted.has(v.reference.variant));
    smallCache.set(asset,{list,promoted:signature,result});return result;
  };
  const refsFor = (assets: readonly number[]) => assets.flatMap(asset => {
    const main=mainVariant(listFor(asset));return [...(main?[main]:[]),...smallFor(asset)];
  });
  const activeSelection = () => activeArea()==="gallery"?selection():queueSelection();
  const selectedVariants = () => [...variants().values()].flat().filter(v => selection().ids.has(variantKey(repository()??"",v.reference)));
  const chosenVariants = () => selectedVariants().filter(v => !locked(v));
  const canEnqueue = () => activeArea()==="gallery" && selectedPreset()!==null && !busy() && !resetting() && chosenVariants().some(v => queueState(v.reference,v.profileHash)===undefined);
  const removable = () => queueItems().filter(item => ["pending","failed"].includes(item.status) && (activeArea()==="queue" ? queueSelection().ids.has(item.id) : chosenVariants().some(v => queueState(v.reference,v.profileHash)?.id===item.id)));
  const canRemove = () => !busy() && !resetting() && removable().length>0;
  const orderedKeys = (assets: readonly number[]) =>
    repository() === null
      ? []
      : refsFor(assets.filter(asset=>admitsPhoto(listFor(asset),preferences.value().scope))).filter(v=>!locked(v)).map((variant) =>
          variantKey(repository()!, variant.reference),
        );
  function applyRuntime(view:ExportQueueView):void {
    if(disposed||view.generation<minimumGeneration||view.revision<=runtimeRevision)return;
    runtimeRevision=view.revision;generation=view.generation;
    batch(()=>{setQueues(new Map(Object.entries(view.queues)));setEnabled(new Set(view.enabled));pruneLockedSelection();
      const eligible=new Set(queueItems().filter(i=>i.status!=="running" && !queueFinished(i.status)).map(i=>i.id));
      setQueueSelection(old=>({ids:new Set([...old.ids].filter(id=>eligible.has(id))),anchor:old.anchor!==null&&eligible.has(old.anchor)?old.anchor:null}));
    });
  }
  async function runtime(action:ExportQueueAction,payload:Record<string,unknown>={}):Promise<void>{
    if(!deps.runtime)return;
    try{applyRuntime(await deps.runtime(action,payload));}catch(e){if(!disposed){setError(String(e));if(action==="reset"){minimumGeneration=generation;runtimeRevision=-1;await runtime("status");}}}
  }
  const runtimeReady=deps.subscribe?deps.subscribe(applyRuntime).then(off=>{if(disposed)off();else offRuntime=off;}).then(()=>runtime("status")):Promise.resolve();
  const ready = runtimeReady.catch(e=>setError(String(e))).then(()=>deps
    .getSetting(EXPORT_PRESETS_KEY)
    .then(async (stored) => {
      if (disposed) return;
      const previous = stored === null ? await deps.getSetting(PREVIOUS_EXPORT_PRESETS_KEY) : null;
      const legacy = stored === null && previous === null ? await deps.getSetting(LEGACY_EXPORT_PRESETS_KEY) : null;
      const loaded = readPresets(stored ?? previous ?? legacy);
      // Explicit one-time device-setting migration; an empty v3 also marks completion.
      if (stored === null) await deps.setSetting(EXPORT_PRESETS_KEY, serializePresets(loaded));
      if (disposed) return;
      setPresets(loaded);
      const current = selectedPreset();
      if (current !== null) {setDraft({ ...current });confirmName(current.name);}
    })
    .catch((e: unknown) => setError(String(e)))
    .finally(() => setLoading(false)));
  function context(repo: string | null, scope: string): void {
    const key = JSON.stringify([repo, scope]);
    if (key === contextKey) return;
    contextKey = key;
    preserveDragSelection = false;
    revision++;
    selectionRevision++;
    batch(() => {
      setRepository(repo);
      setVariants(new Map());
      setSelection(clearSelection());
      setError(null);
    });
    inflight.clear();
    variantRevisions.clear();
    staleVariants.clear();
    smallCache.clear();
  }
  async function ensure(assets: readonly number[]): Promise<void> {
    const repo = repository();
    if (repo === null || disposed) return;
    const wanted = [...new Set(assets)].filter(
      (asset) => (!variants().has(asset) || staleVariants.has(asset)) && !inflight.has(asset),
    );
    const prior = assets
      .map((asset) => inflight.get(asset))
      .filter((p): p is Promise<void> => p !== undefined);
    const ticket = revision;
    for (let start = 0; start < wanted.length; start += 128) {
      const ids = wanted.slice(start, start + 128);
      const versions = new Map(ids.map(id => [id, variantRevisions.get(id) ?? 0]));
      const task = deps
        .variants(repo, ids)
        .then((result) => {
          if (ticket !== revision || disposed) return;
          const accepted = ids.filter(id => versions.get(id) === (variantRevisions.get(id) ?? 0));
          setVariants((old) => {
            const next = new Map(old);
            let changed = false;
            for (const id of accepted) {
              const previous = old.get(id);
              const incoming = result.find(item => item.assetId === id)?.variants ?? [];
              const values = incoming.map(value => {
                const prior = previous?.find(v => v.reference.variant === value.reference.variant);
                return prior && JSON.stringify(prior) === JSON.stringify(value) ? prior : value;
              });
              if (!previous || previous.length !== values.length || values.some((v, i) => v !== previous[i])) {
                next.set(id, values);
                changed = true;
              }
              staleVariants.delete(id);
            }
            return changed ? next : old;
          });
          setSelection((old) => {
            const allowed = new Set(
              accepted.flatMap((id) =>
                listFor(id).map((v) => variantKey(repo, v.reference)),
              ),
            );
            const removed = new Set(accepted);
            // 这一批刷新之后这个键还成不成立？——「刷新过的资产」且「该变体已消失」⇒ 剔掉。
            // 键解析不出来时**保留**：不因解析失败丢用户已选的东西（旧写法在这里会直接抛）。
            const dropped = (key: string): boolean => {
              const asset = variantAssetId(key);
              return asset !== null && removed.has(asset) && !allowed.has(key);
            };
            return {
              ids: new Set([...old.ids].filter((key) => !dropped(key))),
              anchor:
                old.anchor !== null && !dropped(old.anchor) ? old.anchor : null,
            };
          });
        })
        .finally(() => {
          if (ticket === revision) for (const id of ids) if (inflight.get(id) === task) inflight.delete(id);
        });
      for (const id of ids) inflight.set(id, task);
      prior.push(task);
    }
    try {
      await Promise.all(prior);
    } catch (e) {
      if (ticket === revision) setError(String(e));
      throw e;
    }
  }
  async function selectIssue(
    ref: VariantRef,
    mode: "replace" | "toggle" | "range",
    assets: readonly number[],
  ): Promise<void> {
    const repo = repository();
    if (repo === null) return;
    setActiveArea("gallery");
    const variant=listFor(ref.assetId).find(v=>v.reference.variant===ref.variant);
    if(variant && locked(variant))return;
    preserveDragSelection = false;
    const ticket = revision;
    const gesture = ++selectionRevision;
    if (mode === "range") await ensure(assets);
    if (ticket !== revision || gesture !== selectionRevision || disposed)
      return;
    setSelection((previous) =>
      applySelection(
        previous,
        orderedKeys(assets),
        variantKey(repo, ref),
        mode,
      ),
    );
  }
  async function selectAssets(
    assets: readonly number[],
    mode: "replace" | "toggle" | "range",
    order: readonly number[],
  ): Promise<void> {
    preserveDragSelection = false;
    const ticket = revision;
    const gesture = ++selectionRevision;
    const oldAnchor = selection().anchor;
    await ensure(mode === "range" ? order : assets);
    if (ticket !== revision || gesture !== selectionRevision || disposed)
      return;
    const keys = orderedKeys(assets);
    if (keys.length === 0) return;
    if (mode === "range")
      setSelection((previous) =>
        applySelection(
          { ...previous, anchor: oldAnchor },
          orderedKeys(order),
          keys[keys.length - 1]!,
          "range",
        ),
      );
    else if (mode === "toggle")
      setSelection((previous) => toggleGroupSelection(previous, keys));
    else setSelection({ ids: new Set(keys), anchor: keys[0]! });
  }
  async function group(
    assets: readonly number[],
    additive = true,
  ): Promise<void> {
    preserveDragSelection = false;
    const ticket = revision;
    const gesture = ++selectionRevision;
    await ensure(assets);
    if (ticket !== revision || gesture !== selectionRevision || disposed)
      return;
    setSelection((old) =>
      additive
        ? toggleGroupSelection(old, orderedKeys(assets))
        : selectAll(orderedKeys(assets)),
    );
  }
  async function check(
    candidate = draft(),
  ): Promise<{ errors: Record<string, string>; warnings: string[] }> {
    const result = await deps.validate(candidate);
    if (!disposed && JSON.stringify(candidate) === JSON.stringify(draft()))
      setValidation(result);
    return result;
  }
  async function save(): Promise<void> {
    if (busy() || loading()) return;
    const value = { ...draft(), name: draft().name.trim().normalize("NFC") };
    const local = presetErrors(value);
    if (Object.keys(local).length > 0) {
      setValidation({ errors: local, warnings: [] });
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await check(value);
      if (Object.keys(result.errors).length > 0) {
        setValidation(result);
        return;
      }
      const existing = matchingPreset(value.name);
      const saved = {
        ...value,
        id: existing?.id ?? deps.id?.() ?? globalThis.crypto.randomUUID(),
      };
      const next =
        existing === null
          ? [...presets(), saved]
          : presets().map((p) => (p.id === saved.id ? saved : p));
      // 只有预设载荷进入持久化；写失败不把 UI 假装成已保存。
      writeChain = writeChain
        .catch(() => {})
        .then(() =>
          deps.setSetting(EXPORT_PRESETS_KEY, serializePresets(next)),
        );
      await writeChain;
      if (disposed) return;
      setPresets(next);
      preferences.update({ selectedPreset: saved.id });
      setDraft({ ...saved });
      confirmName(saved.name);
      setValidation({ errors: {}, warnings: result.warnings });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  function choosePreset(id: string | null): void {
    preferences.update({ selectedPreset: id });
    setQueueSelection(clearSelection());
    pruneLockedSelection();
    setDraft({ ...(presets().find((p) => p.id === id) ?? blank()) });
    confirmName(draft().name);
    setValidation({ errors: {}, warnings: [] });
  }
  async function enqueue(root: string, options: {presetId?:string;references?:readonly VariantRef[];preserveSelection?:boolean} = {}): Promise<number> {
    const preset = options.presetId === undefined ? selectedPreset() : presets().find(p=>p.id===options.presetId)??null;
    const repo = repository();
    if (
      preset === null ||
      repo === null ||
      busy() ||
      selection().ids.size === 0
    )
      return 0;
    const candidates = options.presetId === undefined ? chosenVariants() : selectedVariants();
    const targetQueue = () => queues().get(preset.id) ?? [];
    const requested = options.references && new Set(options.references.map(r=>variantKey(repo,r)));
    const queued = new Set(targetQueue().map(i=>JSON.stringify([i.repositoryId,i.snapshot.reference.assetId,i.snapshot.profileHash])));
    const refs = candidates.filter(v => (!requested || requested.has(variantKey(repo,v.reference))) &&
      !queued.has(JSON.stringify([repo,v.reference.assetId,v.profileHash]))).map(v=>v.reference);
    if (refs.length === 0) return 0;
    const before=targetQueue().length;
    if (options.preserveSelection) preserveDragSelection = true;
    setBusy(true);
    setError(null);
    const ticket = revision, queueTicket=queueRevision, capturedGeneration=generation;
    try {
      const snapshots: VariantSnapshot[] = [];
      for (let i = 0; i < refs.length; i += 128)
        snapshots.push(...(await deps.snapshots(repo, refs.slice(i, i + 128))));
      if (ticket !== revision || queueTicket !== queueRevision || disposed) return 0;
      if (snapshots.length !== refs.length)
        throw new Error(t("export.error.incomplete"));
      if(deps.runtime){
        const entries=snapshots.map(snapshot=>({id:"",repositoryId:repo,root,snapshot,preset:{...preset},status:"pending",error:null,sequence:0,output:null}));
        for(let i=0;i<entries.length;i+=128){
          if(ticket!==revision||queueTicket!==queueRevision||disposed)return 0;
          await runtime("enqueue",{generation:capturedGeneration,items:entries.slice(i,i+128)});
        }
        return Math.max(0,targetQueue().length-before);
      }
      setQueues((old) => {
        const next = new Map(old);
        const entries = [...(old.get(preset.id) ?? [])];
        const seen = new Set(
          entries.map((item) =>
            JSON.stringify([
              item.repositoryId,
              item.snapshot.reference.assetId,
              item.snapshot.profileHash,
            ]),
          ),
        );
        for (const snapshot of snapshots) {
          const identity = JSON.stringify([
            repo,
            snapshot.reference.assetId,
            snapshot.profileHash,
          ]);
          if (seen.has(identity)) continue;
          seen.add(identity);
          sequence++;
          entries.unshift({
            id: `${preset.id}:${sequence}`,
            repositoryId: repo,
            root,
            snapshot: structuredClone(snapshot),
            preset: { ...preset },
            status: "pending",
            error: null,
            sequence,
          });
        }
        next.set(preset.id, entries);
        return next;
      });
      return Math.max(0,targetQueue().length-before);
    } catch (e) {
      if (ticket === revision) setError(String(e));
      return 0;
    } finally {
      setBusy(false);
    }
  }
  return {
    preferences, syncRuntime:()=>void runtime("status"), applyRuntime, limitOpen, setLimitOpen,
    canRun,
    toggleRun(){const p=selectedPreset();if(!p || !canRun())return;const on=!enabled().has(p.id);if(on&&enabled().size>=4){setLimitOpen(true);return;}void runtime(on?"enable":"disable",{presetId:p.id});},
    retry(ids:readonly string[]){void runtime("retry",{ids:[...ids]});},
    /** 暂不启用：预设文件交换，无工作区/命令入口。 */
    async importPresets(raw:string){
      const incoming=readPresets(raw);if(incoming.length===0){setError(t("export.invalidPresets"));return;}
      if(busy())return;setBusy(true);
      try{const next=[...presets()];for(const p of incoming){const existing=next.findIndex(i=>i.name===p.name);const value={...p,id:existing>=0?next[existing].id:(deps.id?.()??globalThis.crypto.randomUUID())};
          if(existing>=0)next[existing]=value;else next.push(value);}
        await deps.setSetting(EXPORT_PRESETS_KEY,serializePresets(next));setPresets(next);
        // Imported paths belong to another machine; validate here, never auto-enable.
        const first=next.find(p=>p.name===incoming[0].name)!;choosePreset(first.id);await check(first);
      }catch(e){setError(String(e));}finally{setBusy(false);}
    },
    selectedVariants, activeArea, activeSelection, queueSelection, queueItems, queueState, locked, smallFor, canEnqueue, canRemove,
    focusArea(area: "gallery"|"queue") {setActiveArea(area);},
    selectQueue(id: string, mode: "replace"|"toggle"|"range") {
      setActiveArea("queue");
      const eligible=queueItems().filter(i=>i.status!=="running" && !queueFinished(i.status)).map(i=>i.id);
      if(eligible.includes(id))setQueueSelection(old=>applySelection(old,eligible,id,mode));
    },
    selectAllActive(assets: readonly number[]) {
      if(activeArea()==="queue")setQueueSelection(selectAll(queueItems().filter(i=>i.status!=="running" && !queueFinished(i.status)).map(i=>i.id)));
      else void group(assets,false);
    },
    removeSelected() {
      const ids=new Set(removable().map(i=>i.id));const preset=selectedPreset();if(!preset||ids.size===0)return;
      if(deps.runtime){void runtime("remove",{ids:[...ids]});return;}
      setQueues(old=>new Map(old).set(preset.id,(old.get(preset.id)??[]).filter(i=>!ids.has(i.id))));
      setQueueSelection(old=>({ids:new Set([...old.ids].filter(id=>!ids.has(id))),anchor:old.anchor!==null&&ids.has(old.anchor)?null:old.anchor}));
    },
    setQueueStatus(id:string,status:ExportQueueItem["status"],error:string|null=null) {
      setQueues(old=>new Map([...old].map(([key,items])=>[key,items.map(i=>i.id===id?{...i,status,error}:i)])));
      pruneLockedSelection();
      if(status==="running"||queueFinished(status))setQueueSelection(old=>({ids:new Set([...old.ids].filter(key=>key!==id)),anchor:old.anchor===id?null:old.anchor}));
    },
    presets,
    queues,
    enabled,
    selection,
    variants,
    error,
    busy,
    loading,
    ready,
    draft,
    validation,
    selectedPreset,
    matchingPreset,
    nameChecking,
    canSave: () => !busy() && !loading() && !nameChecking() && Object.keys(validation().errors).length === 0,
    dirty,
    repository,
    context,
    ensure,
    listFor,
    refsFor,
    orderedKeys,
    selectIssue,
    focusIssue(ref: VariantRef) {
      const repo = repository();
      if (repo !== null)
        setSelection((old) => focusSelection(old, variantKey(repo, ref)));
    },
    selectAssets,
    group,
    save,
    check,
    choosePreset,
    enqueue,
    edit(patch: Partial<ExportPreset>) {
      setDraft((old) => {
        const next = {...old,...patch};
        if (patch.sizeMode !== undefined) {
          if (!Number.isInteger(next.maxEdge) || next.maxEdge < 1 || next.maxEdge > 65535) next.maxEdge = 2048;
          if (!Number.isInteger(next.percent) || next.percent < 1 || next.percent > 100) next.percent = 50;
        }
        return next;
      });
      if (patch.name !== undefined) {
        clearTimeout(nameTimer);
        const name = draft().name.trim().normalize("NFC");
        if (name === confirmedName()) confirmName(name);
        else {
          setNameChecking(true);
          nameTimer = setTimeout(() => { if (!disposed) confirmName(name); }, 1500);
        }
      }
      setValidation({ errors: presetErrors(draft()), warnings: [] });
    },
    clear() {
      preserveDragSelection=false;
      selectionRevision++;
      if(activeArea()==="queue")setQueueSelection(clearSelection());else setSelection(clearSelection());
    },
    cycleScope() {
      const scopes = ["all", "issues", "edited"] as const;
      preferences.update({
        scope: scopes[(scopes.indexOf(preferences.value().scope) + 1) % 3],
      });
      selectionRevision++;
      setSelection(clearSelection());
    },
    reportError(e: unknown) {
      setError(String(e));
    },
    invalidate(assets: readonly number[]) {
      // Keep visible data until its replacement arrives. Invalidate only these assets;
      // unrelated in-flight batches remain valid and cannot be stranded on scope return.
      for (const id of assets) {
        variantRevisions.set(id, (variantRevisions.get(id) ?? 0) + 1);
        staleVariants.add(id);
        inflight.delete(id);
      }
      return ensure(assets).catch(() => {});
    },
    progress(id: string) {
      return queueProgress(queues().get(id) ?? []);
    },
    processing() {
      return [...queues().values()].some(
        (items) => queueProgress(items).processing,
      );
    },
    stopAll() {
      if(deps.runtime){void runtime("stop");return;}
      setEnabled(new Set<string>());
    },
    reset() {
      preserveDragSelection=false;
      queueRevision++;
      selectionRevision++;
      if(deps.runtime){minimumGeneration=generation+1;setResetting(true);void runtime("reset").finally(()=>setResetting(false));}
      setEnabled(new Set<string>());
      setQueues(new Map());
      setSelection(clearSelection());setQueueSelection(clearSelection());
    },
    dispose() {
      disposed = true;offRuntime?.();clearTimeout(nameTimer);
      revision++;
      inflight.clear();
    },
  };
}
export type ExportStore = ReturnType<typeof createExportStore>;
