/** 选中/旗标照片共用的加桶入口；创建与加入走同一份照片身份。 */
import { createEffect, createSignal, For, Show } from "solid-js";
import { addPhotosToBucket, createPhotoBucket, listPhotoBuckets, type AssetIdentity, type PhotoBucket } from "../../api/organization.ts";
import { Button } from "../../components/ui/Button.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { t } from "../../i18n/index.ts";

export function BucketPickerDialog(props: {
  open: boolean;
  photos: readonly AssetIdentity[];
  onClose: () => void;
  onDone: (count: number) => void;
}) {
  const [buckets, setBuckets] = createSignal<PhotoBucket[]>([]);
  const [name, setName] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  createEffect(() => {
    if (!props.open) return;
    setName(""); setError(null);
    void listPhotoBuckets().then(setBuckets).catch((reason: unknown) => setError(String(reason)));
  });
  async function add(bucketId?: number): Promise<void> {
    if (busy() || props.photos.length === 0) return;
    setBusy(true); setError(null);
    try {
      const id = bucketId ?? (await createPhotoBucket(name().trim())).id;
      const count = await addPhotosToBucket(id, [...props.photos]);
      props.onDone(count);
      props.onClose();
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }
  return <Dialog open={props.open} onOpenChange={(open) => { if (!open) props.onClose(); }} title={t("org.chooseBucket")}
    description={t("org.addCount").replace("{n}", String(props.photos.length))}>
    <div class="flex max-h-64 flex-col gap-1 overflow-y-auto">
      <For each={buckets()} fallback={<p class="text-fg-3">{t("org.emptyBuckets")}</p>}>
        {(bucket) => <button type="button" disabled={busy()} class="rounded-ui bg-surface-bar px-3 py-2 text-left hover:bg-state-hover disabled:opacity-50"
          onClick={() => void add(bucket.id)}>{bucket.name}</button>}
      </For>
    </div>
    <div class="mt-3 flex gap-2">
      <input class="min-w-0 flex-1 rounded-ui bg-surface-bar px-2 text-fg-1" value={name()}
        onInput={(event) => setName(event.currentTarget.value)} placeholder={t("org.bucketName")} aria-label={t("org.bucketName")} />
      <Button variant="primary" disabled={busy() || !name().trim()} onClick={() => void add()}>{t("org.createAndAdd")}</Button>
    </div>
    <Show when={error()}><p role="alert" class="mt-2 text-danger">{error()}</p></Show>
  </Dialog>;
}
