/** 浏览左栏的标签/相片桶面板；库目录仍由原 BrowseLeftColumn 渲染。 */

import { createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import { IconBan, IconDots, IconFolder, IconLock, IconPhoto, IconPlus, IconSearch, IconSparkles, IconTag } from "@tabler/icons-solidjs";

import type { RepositoryView, Tag } from "../../api/types.ts";
import type { PhotoBucket } from "../../api/organization.ts";
import { addPhotosToBucket, directoriesForTag, directoryTagCounts, listPhotoBuckets, pinPhotoBucket, deletePhotoBucket, pausePhotoBucket, setDirectoryTag, syncPhotoTags } from "../../api/organization.ts";
import { parsePhotoDrag, PHOTO_DRAG_TYPE } from "./organization-drag.ts";
import { tagList } from "../../api/browse.ts";
import { TreeNode } from "../../components/ui/TreeNode.tsx";
import { Menu } from "../../components/ui/Menu.tsx";
import { Button } from "../../components/ui/Button.tsx";
import { ConfirmDialog } from "../../components/ui/Dialog.tsx";
import { t } from "../../i18n/index.ts";

export type OrganizationSelection =
  | { kind: "tag"; key: string }
  | { kind: "bucket"; id: number }
  | { kind: "directory"; repositoryId: string; path: string }
  | null;

export interface OrganizationPanelProps {
  mode: "tags" | "buckets";
  refreshKey?: number;
  repositories: readonly RepositoryView[];
  selection: OrganizationSelection;
  onSelectTag: (tag: Tag) => void;
  onSelectBucket: (bucket: PhotoBucket) => void;
  onSelectDirectory: (repositoryId: string, path: string) => void;
  onNewBucket: () => void;
  onEditBucket: (bucket: PhotoBucket) => void;
  onError: (error: unknown) => void;
  onChanged?: () => void;
}

export function OrganizationPanel(props: OrganizationPanelProps) {
  const tagKey = (name: string) => name.normalize("NFC").toLowerCase();
  const [search, setSearch] = createSignal("");
  const [tags, setTags] = createSignal<readonly Tag[]>([]);
  const [buckets, setBuckets] = createSignal<readonly PhotoBucket[]>([]);
  const [expanded, setExpanded] = createSignal<ReadonlySet<string>>(new Set());
  const [children, setChildren] = createSignal<ReadonlyMap<string, readonly { repositoryId: string; path: string }[]>>(new Map());
  const [directoryCounts, setDirectoryCounts] = createSignal<ReadonlyMap<string, number>>(new Map());
  const [directoryCountsKnown, setDirectoryCountsKnown] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [deleteTarget, setDeleteTarget] = createSignal<PhotoBucket | null>(null);
  let tagGeneration = 0;

  async function refreshTags(query: string): Promise<void> {
    const mine = ++tagGeneration;
    try {
      const result = await tagList(query, 500);
      if (mine === tagGeneration) setTags(result);
    } catch (error) {
      if (mine === tagGeneration) props.onError(error);
    }
  }

  async function refreshBuckets(): Promise<void> {
    try { setBuckets(await listPhotoBuckets()); }
    catch (error) { props.onError(error); }
  }

  createEffect(on(() => [props.mode, props.repositories.filter((repo) => repo.online).map((repo) => repo.id).join("|")] as const,
    ([mode, ids]) => {
      if (mode !== "tags") return;
      void Promise.allSettled(ids.split("|").filter(Boolean).map(syncPhotoTags)).then((results) => {
        for (const result of results) if (result.status === "rejected") props.onError(result.reason);
        void refreshTags(search());
      });
    }));
  createEffect(on(() => [props.mode, search(), props.refreshKey] as const, ([mode, query]) => {
    if (mode === "tags") void refreshTags(query);
    else void refreshBuckets();
  }));
  createEffect(on(() => [props.mode, props.refreshKey,
    props.repositories.filter((repo) => repo.online).map((repo) => repo.id).join("|")] as const,
  ([mode, , ids]) => {
    if (mode !== "tags") return;
    let current = true;
    setDirectoryCountsKnown(false);
    void Promise.allSettled(ids.split("|").filter(Boolean).map(directoryTagCounts)).then((results) => {
      if (!current) return;
      const counts = new Map<string, number>();
      for (const result of results) {
        if (result.status === "rejected") { props.onError(result.reason); continue; }
        for (const [key, count] of Object.entries(result.value)) counts.set(key, (counts.get(key) ?? 0) + count);
      }
      setDirectoryCounts(counts);
      setDirectoryCountsKnown(results.every((result) => result.status === "fulfilled"));
    });
    onCleanup(() => { current = false; });
  }));

  async function expandTag(tag: Tag): Promise<void> {
    const key = tagKey(tag.name);
    if (expanded().has(key)) {
      setExpanded((previous) => { const next = new Set(previous); next.delete(key); return next; });
      return;
    }
    const mounted = props.repositories.filter((repo) => repo.online && repo.root !== null);
    try {
      const all = await Promise.all(mounted.map(async (repo) => {
        const directories: { repositoryId: string; path: string }[] = [];
        for (let offset = 0; ; offset += 500) {
          const page = await directoriesForTag(repo.id, key, offset, 500);
          directories.push(...page.map((path) => ({ repositoryId: repo.id, path })));
          if (page.length < 500) break;
        }
        return directories;
      }));
      setChildren((previous) => new Map(previous).set(key, all.flat()));
      setExpanded((previous) => new Set(previous).add(key));
    } catch (error) { props.onError(error); }
  }

  async function bucketAction(bucket: PhotoBucket, action: string): Promise<void> {
    if (action === "rules") { props.onEditBucket(bucket); return; }
    if (action === "delete") { setDeleteTarget(bucket); return; }
    setBusy(true);
    try {
      if (action === "pin") await pinPhotoBucket(bucket.id, !bucket.pinned);
      if (action === "pause") await pausePhotoBucket(bucket.id, !bucket.paused);
      await refreshBuckets();
      props.onChanged?.();
    } catch (error) { props.onError(error); }
    finally { setBusy(false); }
  }

  async function confirmDelete(): Promise<void> {
    const target = deleteTarget();
    if (!target || busy()) return;
    setBusy(true);
    try {
      await deletePhotoBucket(target.id);
      setDeleteTarget(null);
      await refreshBuckets();
      props.onChanged?.();
    } catch (error) { props.onError(error); }
    finally { setBusy(false); }
  }

  return (
    <div class="flex min-h-0 flex-1 flex-col gap-2 overflow-hidden bg-surface-main p-panel-pad">
      <ConfirmDialog open={deleteTarget() !== null} title={t("org.deleteBucket")}
        message={t("org.deleteBucketConfirm").replace("{name}", deleteTarget()?.name ?? "")}
        confirmLabel={t("org.deleteBucket")}
        onCancel={() => setDeleteTarget(null)} onConfirm={() => void confirmDelete()} />
      <div class="flex h-8 shrink-0 items-center justify-between">
        <h2 class="text-fs-2 font-semibold text-fg-1">
          {props.mode === "tags" ? t("org.tags") : t("org.buckets")}
        </h2>
        <Show when={props.mode === "buckets"}>
          <Button variant="ghost" icon={<IconPlus size={16} />} aria-label={t("org.newBucket")} onClick={props.onNewBucket} />
        </Show>
      </div>
      <Show when={props.mode === "tags"} fallback={
        <div class="min-h-0 flex-1 overflow-y-auto" role="listbox" aria-label={t("org.buckets")}>
          <For each={buckets()} fallback={<p class="p-2 text-fs-1 text-fg-3">{t("org.emptyBuckets")}</p>}>
            {(bucket) => (
              <div class={[
                "mb-2 flex h-18 items-center gap-2 rounded-ui px-2 text-fg-2 hover:bg-state-hover",
                props.selection?.kind === "bucket" && props.selection.id === bucket.id ? "bg-state-selected text-fg-1" : "bg-surface-bar",
              ].join(" ")}
                role="option" aria-selected={props.selection?.kind === "bucket" && props.selection.id === bucket.id}
                tabindex="0" onClick={() => props.onSelectBucket(bucket)}
                onKeyDown={(event) => { if (event.key === "Enter") props.onSelectBucket(bucket); }}
                onDragOver={(event) => {
                  if (event.dataTransfer?.types.includes(PHOTO_DRAG_TYPE)) {
                    event.preventDefault(); event.dataTransfer.dropEffect = "copy";
                  }
                }}
                onDrop={(event) => {
                  const photos = parsePhotoDrag(event.dataTransfer?.getData(PHOTO_DRAG_TYPE) ?? "");
                  if (photos.length === 0) return;
                  event.preventDefault();
                  void addPhotosToBucket(bucket.id, photos).then(async () => {
                    await refreshBuckets(); props.onChanged?.();
                  }).catch(props.onError);
                }}
              >
                <span class="relative flex size-12 shrink-0 items-center justify-center" aria-hidden="true">
                  <span class="absolute left-2 top-0 size-9 rounded-ui bg-surface-layer" />
                  <span class="absolute bottom-0 left-0 flex size-9 items-center justify-center rounded-ui bg-surface-main text-fg-3">
                    <IconPhoto size={18} />
                  </span>
                </span>
                <span class="min-w-0 flex-1">
                  <span class="block truncate text-fs-2 font-semibold text-fg-1">{bucket.name}</span>
                  <span class="block truncate text-fs-0 text-fg-3">
                    {bucket.memberCount} {t("org.photos")}
                    {bucket.rules.groups.length > 0 ? ` · ${bucket.rules.groups.length} ${t("org.ruleGroups")}` : ""}
                  </span>
                </span>
                <span class="flex shrink-0 flex-col items-center gap-1 text-fg-3">
                  <Show when={bucket.pinned}><IconLock size={14} class="text-brand" /></Show>
                  <Show when={bucket.rules.groups.length > 0}><IconSparkles size={14} class="text-brand-2" /></Show>
                  <span onClick={(event) => event.stopPropagation()}>
                  <Menu
                    label={t("org.bucketActions")}
                    items={[
                      { value: "rules", label: t("org.editRules") },
                      { value: "pin", label: bucket.pinned ? t("org.unpin") : t("org.pin") },
                      ...(bucket.rules.groups.length > 0 ? [{ value: "pause", label: bucket.paused ? t("org.resume") : t("org.pause") }] : []),
                      { value: "delete", label: t("org.deleteBucket"), disabled: bucket.pinned || busy(), separatorBefore: true },
                    ]}
                    onSelect={(value) => void bucketAction(bucket, value)}
                  >
                    {(trigger) => <button {...trigger} type="button" aria-label={t("org.bucketActions")}
                      class="rounded-ui p-0.5 hover:bg-state-hover">
                      <IconDots size={16} />
                    </button>}
                  </Menu>
                  </span>
                </span>
              </div>
            )}
          </For>
        </div>
      }>
        <label class="flex h-8 shrink-0 items-center gap-1 rounded-ui bg-surface-bar px-2 text-fg-3">
          <IconSearch size={15} />
          <input class="min-w-0 flex-1 bg-transparent text-fs-1 text-fg-1 outline-none"
            value={search()} onInput={(event) => setSearch(event.currentTarget.value)}
            placeholder={t("org.searchTags")} aria-label={t("org.searchTags")} />
        </label>
        <div class="min-h-0 flex-1 overflow-y-auto" role="tree" aria-label={t("org.tags")}>
          <For each={tags()} fallback={<p class="p-2 text-fs-1 text-fg-3">{t("org.noTags")}</p>}>
            {(tag) => {
              const key = () => tagKey(tag.name);
              const dirs = () => children().get(key()) ?? [];
              return <>
                <TreeNode label={tag.name} depth={0} icon={<IconTag size={15} />}
                  selected={props.selection?.kind === "tag" && props.selection.key === key()}
                  hasChildren={dirs().length > 0 || (directoryCounts().get(key()) ?? 0) > 0 || !directoryCountsKnown() && !children().has(key())}
                  expanded={expanded().has(key())}
                  onClick={() => props.onSelectTag(tag)}
                  onToggleExpand={() => void expandTag(tag)}
                />
                <Show when={expanded().has(key())}>
                  <For each={dirs()}>
                    {(directory) => {
                      const repo = () => props.repositories.find((item) => item.id === directory.repositoryId);
                      return <TreeNode
                        label={directory.path}
                        labelNode={<span class="truncate">{repo()?.name ?? directory.repositoryId} / {directory.path.split("/").pop()}</span>}
                        depth={1} icon={<IconFolder size={15} />}
                        selected={props.selection?.kind === "directory" &&
                          props.selection.repositoryId === directory.repositoryId &&
                          props.selection.path === directory.path}
                        trailing={<button type="button" aria-label={t("org.removeDirectoryTag")}
                          class="rounded-ui p-0.5 text-fg-3 hover:bg-state-hover hover:text-fg-1"
                          onClick={(event) => {
                            event.stopPropagation();
                            void setDirectoryTag(directory.repositoryId, directory.path, tag.name, false)
                              .then(() => {
                                setChildren((previous) => new Map(previous).set(key(),
                                  (previous.get(key()) ?? []).filter((item) => item.repositoryId !== directory.repositoryId || item.path !== directory.path)));
                                props.onChanged?.();
                              }).catch(props.onError);
                          }}><IconBan size={14} /></button>}
                        onClick={() => props.onSelectDirectory(directory.repositoryId, directory.path)}
                      />;
                    }}
                  </For>
                </Show>
              </>;
            }}
          </For>
        </div>
      </Show>
    </div>
  );
}
