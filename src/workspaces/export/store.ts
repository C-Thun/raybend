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
  queueProgress,
  readPresets,
  serializePresets,
  variantKey,
  visibleVariants,
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

export const EXPORT_PRESETS_KEY = "export.presets.v1";
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
  let sequence = 0;
  let disposed = false;
  let writeChain: Promise<void> = Promise.resolve();
  const inflight = new Map<number, Promise<void>>();
  const blank = (): ExportPreset => ({
    id: "new",
    name: "",
    format: "jpeg",
    quality: 90,
    maxEdge: 0,
    directory: "",
    template: ":FILENAME",
  });
  const [draft, setDraft] = createSignal<ExportPreset>(blank());
  const [validation, setValidation] = createSignal<{
    errors: Record<string, string>;
    warnings: string[];
  }>({ errors: {}, warnings: [] });
  const selectedPreset = () =>
    presets().find((p) => p.id === preferences.value().selectedPreset) ?? null;
  const dirty = () =>
    selectedPreset() === null ||
    JSON.stringify(draft()) !== JSON.stringify(selectedPreset());
  const listFor = (asset: number) =>
    visibleVariants(variants().get(asset) ?? [], preferences.value().scope);
  const refsFor = (assets: readonly number[]) =>
    assets.flatMap((asset) => listFor(asset));
  const orderedKeys = (assets: readonly number[]) =>
    repository() === null
      ? []
      : refsFor(assets).map((variant) =>
          variantKey(repository()!, variant.reference),
        );
  const ready = deps
    .getSetting(EXPORT_PRESETS_KEY)
    .then((raw) => {
      if (disposed) return;
      const loaded = readPresets(raw);
      setPresets(loaded);
      const current = selectedPreset();
      if (current !== null) setDraft({ ...current });
    })
    .catch((e: unknown) => setError(String(e)))
    .finally(() => setLoading(false));
  function context(repo: string | null, scope: string): void {
    const key = JSON.stringify([repo, scope]);
    if (key === contextKey) return;
    contextKey = key;
    revision++;
    selectionRevision++;
    batch(() => {
      setRepository(repo);
      setVariants(new Map());
      setSelection(clearSelection());
      setError(null);
    });
    inflight.clear();
  }
  async function ensure(assets: readonly number[]): Promise<void> {
    const repo = repository();
    if (repo === null || disposed) return;
    const wanted = [...new Set(assets)].filter(
      (asset) => !variants().has(asset) && !inflight.has(asset),
    );
    const prior = assets
      .map((asset) => inflight.get(asset))
      .filter((p): p is Promise<void> => p !== undefined);
    const ticket = revision;
    for (let start = 0; start < wanted.length; start += 128) {
      const ids = wanted.slice(start, start + 128);
      const task = deps
        .variants(repo, ids)
        .then((result) => {
          if (ticket !== revision || disposed) return;
          setVariants((old) => {
            const next = new Map(old);
            for (const id of ids) next.set(id, []);
            for (const item of result)
              if (ids.includes(item.assetId))
                next.set(item.assetId, item.variants);
            return next;
          });
          setSelection((old) => {
            const allowed = new Set(
              ids.flatMap((id) =>
                listFor(id).map((v) => variantKey(repo, v.reference)),
              ),
            );
            const removed = new Set(ids);
            const next = new Set(
              [...old.ids].filter(
                (key) =>
                  !removed.has(
                    (JSON.parse(key) as [string, number, string])[1],
                  ) || allowed.has(key),
              ),
            );
            return {
              ids: next,
              anchor:
                old.anchor !== null &&
                (!removed.has(
                  (JSON.parse(old.anchor) as [string, number, string])[1],
                ) ||
                  allowed.has(old.anchor))
                  ? old.anchor
                  : null,
            };
          });
        })
        .finally(() => {
          if (ticket === revision) for (const id of ids) inflight.delete(id);
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
      const existing = presets().find((p) => p.name === value.name);
      const saved = {
        ...value,
        id: existing?.id ?? deps.id?.() ?? globalThis.crypto.randomUUID(),
      };
      const next =
        existing === undefined
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
      setValidation({ errors: {}, warnings: result.warnings });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  function choosePreset(id: string | null): void {
    preferences.update({ selectedPreset: id });
    setDraft({ ...(presets().find((p) => p.id === id) ?? blank()) });
    setValidation({ errors: {}, warnings: [] });
  }
  async function enqueue(root: string): Promise<void> {
    const preset = selectedPreset();
    const repo = repository();
    if (
      preset === null ||
      repo === null ||
      busy() ||
      selection().ids.size === 0
    )
      return;
    const refs = [...variants().values()]
      .flatMap((items) => items)
      .filter((v) => selection().ids.has(variantKey(repo, v.reference)))
      .map((v) => v.reference);
    if (refs.length === 0) return;
    setBusy(true);
    setError(null);
    const ticket = revision;
    try {
      const snapshots: VariantSnapshot[] = [];
      for (let i = 0; i < refs.length; i += 128)
        snapshots.push(...(await deps.snapshots(repo, refs.slice(i, i + 128))));
      if (ticket !== revision || disposed) return;
      if (snapshots.length !== refs.length)
        throw new Error(t("export.error.incomplete"));
      setQueues((old) => {
        const next = new Map(old);
        const entries = [...(old.get(preset.id) ?? [])];
        const seen = new Set(
          entries.map((item) =>
            JSON.stringify([
              item.repositoryId,
              item.snapshot.reference,
              item.snapshot.profileHash,
            ]),
          ),
        );
        for (const snapshot of snapshots) {
          const identity = JSON.stringify([
            repo,
            snapshot.reference,
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
    } catch (e) {
      if (ticket === revision) setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  return {
    preferences,
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
      setDraft((old) => ({ ...old, ...patch }));
      setValidation({ errors: presetErrors(draft()), warnings: [] });
    },
    clear() {
      selectionRevision++;
      setSelection(clearSelection());
    },
    cycleScope() {
      const scopes = ["all", "edited", "sooc"] as const;
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
      revision++;
      inflight.clear();
      setVariants((old) => {
        const next = new Map(old);
        for (const id of assets) next.delete(id);
        return next;
      });
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
      setEnabled(new Set<string>());
    },
    reset() {
      setEnabled(new Set<string>());
      setQueues(new Map());
    },
    dispose() {
      disposed = true;
      revision++;
      inflight.clear();
    },
  };
}
export type ExportStore = ReturnType<typeof createExportStore>;
