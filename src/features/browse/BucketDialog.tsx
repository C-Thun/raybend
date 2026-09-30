/** 普通/自动桶共用草稿弹窗。保存前不写库；规则编辑也走同一组件。 */

import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { IconPlus, IconTrash } from "@tabler/icons-solidjs";

import type { RepositoryView } from "../../api/types.ts";
import type { PhotoBucket, RuleGroup, RuleSet, RulesPreview } from "../../api/organization.ts";
import { previewBucketRules } from "../../api/organization.ts";
import { Button } from "../../components/ui/Button.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { t } from "../../i18n/index.ts";
import { COLOR_VALUES, LOCK_LEVELS } from "../../lib/marking-state.ts";
import { emptyRuleDraft } from "./organization-rules.ts";

export interface BucketDialogProps {
  open: boolean;
  bucket?: PhotoBucket | null;
  seed?: RuleSet | null;
  repositories: readonly RepositoryView[];
  onClose: () => void;
  onSave: (name: string, rules: RuleSet) => Promise<void>;
}

const COLORS = COLOR_VALUES.map((value) => value ?? "none");
const LIKES = ["like", "dislike", "none"] as const;
const LOCKS = [LOCK_LEVELS.none, LOCK_LEVELS.noDelete, LOCK_LEVELS.noEdit] as const;

export function BucketDialog(props: BucketDialogProps) {
  const [name, setName] = createSignal("");
  const [rules, setRules] = createSignal<RuleSet>({ groups: [] });
  const [preview, setPreview] = createSignal<RulesPreview | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [saving, setSaving] = createSignal(false);
  let previewGeneration = 0;

  createEffect(() => {
    if (!props.open) return;
    setName(props.bucket?.name ?? "");
    setRules(structuredClone(props.bucket?.rules ?? props.seed ?? { groups: [] }));
    setError(null);
    setPreview(null);
  });

  function updateGroup(index: number, patch: Partial<RuleGroup>): void {
    setRules((current) => ({
      groups: current.groups.map((group, at) => at === index ? { ...group, ...patch } : group),
    }));
  }

  function toggle<T>(values: readonly T[] | undefined, value: T): T[] {
    const current = values ?? [];
    return current.includes(value) ? current.filter((item) => item !== value) : [...current, value];
  }

  createEffect(() => {
    if (!props.open) return;
    const draft = rules();
    const mine = ++previewGeneration;
    if (draft.groups.length === 0 || draft.groups.some((group) => group.repositoryIds.length === 0)) {
      setPreview(null);
      return;
    }
    const timer = setTimeout(() => {
      void previewBucketRules(draft).then((result) => {
        if (mine === previewGeneration) setPreview(result);
      }).catch((cause: unknown) => {
        if (mine === previewGeneration) setError(String(cause));
      });
    }, 300);
    onCleanup(() => clearTimeout(timer));
  });

  async function save(): Promise<void> {
    if (saving()) return;
    if (name().trim() === "") { setError(t("org.nameRequired")); return; }
    if (rules().groups.some((group) => group.repositoryIds.length === 0)) {
      setError(t("org.repositoryRequired")); return;
    }
    setSaving(true);
    setError(null);
    try { await props.onSave(name(), rules()); props.onClose(); }
    catch (cause) { setError(String(cause)); }
    finally { setSaving(false); }
  }

  return <Dialog open={props.open} onOpenChange={(open) => { if (!open) props.onClose(); }}
    title={props.bucket ? t("org.editBucket") : t("org.newBucket")}
    description={t("org.rulesDescription")} size="wide"
    footer={<>
      <Button variant="secondary" onClick={props.onClose}>{t("common.cancel")}</Button>
      <Button variant="primary" disabled={saving()} onClick={() => void save()}>
        {props.bucket ? t("common.save") : t("org.createBucket")}
      </Button>
    </>}
  >
    <div class="flex max-h-[70vh] flex-col gap-3 overflow-y-auto">
      <label class="flex flex-col gap-1 text-fs-1 text-fg-2">
        {t("org.bucketName")}
        <input class="h-8 rounded-ui bg-surface-bar px-2 text-fs-2 text-fg-1 outline-none focus:ring-1 focus:ring-brand"
          value={name()} onInput={(event) => setName(event.currentTarget.value)} maxLength={80} />
      </label>
      <For each={rules().groups}>
        {(group, index) => <div class="flex flex-col gap-2 rounded-ui bg-surface-bar p-3">
          <div class="flex items-center justify-between text-fs-2 font-semibold text-fg-1">
            <span>{t("org.ruleGroup").replace("{n}", String(index() + 1))}</span>
            <Button variant="ghost" icon={<IconTrash size={14} />} aria-label={t("org.removeRuleGroup")}
              onClick={() => setRules((current) => ({
                groups: current.groups.filter((_, at) => at !== index()),
              }))} />
          </div>
          <fieldset class="flex flex-wrap gap-2">
            <legend class="mb-1 text-fs-1 text-fg-2">{t("org.repositoryScope")}</legend>
            <For each={props.repositories}>
              {(repo) => <label class="flex items-center gap-1 text-fs-1 text-fg-1">
                <input type="checkbox" checked={group.repositoryIds.includes(repo.id)}
                  onChange={() => updateGroup(index(), {
                    repositoryIds: toggle(group.repositoryIds, repo.id),
                  })} />
                {repo.name}
              </label>}
            </For>
          </fieldset>
          <div class="grid grid-cols-2 gap-2">
            <label class="flex flex-col gap-1 text-fs-1 text-fg-2">
              {t("org.minRating")}
              <select class="h-8 rounded-ui bg-surface-layer px-2 text-fg-1"
                value={group.minRating ?? ""} onChange={(event) => updateGroup(index(), {
                  minRating: event.currentTarget.value === "" ? null : Number(event.currentTarget.value),
                })}>
                <option value="">{t("org.any")}</option>
                <For each={[1, 2, 3, 4, 5]}>{(value) =>
                  <option value={value}>{value}★ {t("org.andAbove")}</option>}</For>
              </select>
            </label>
            <label class="flex flex-col gap-1 text-fs-1 text-fg-2">
              {t("org.photoTags")}
              <input class="h-8 rounded-ui bg-surface-layer px-2 text-fg-1 outline-none"
                value={(group.tagKeys ?? []).join(", ")}
                onChange={(event) => updateGroup(index(), {
                  tagKeys: event.currentTarget.value.split(",").map((word) => word.trim()).filter(Boolean),
                })} placeholder={t("org.tagsPlaceholder")} />
            </label>
          </div>
          <fieldset class="flex flex-wrap gap-2">
            <legend class="mb-1 text-fs-1 text-fg-2">{t("org.colors")}</legend>
            <For each={COLORS}>{(value) =>
              <label class="flex items-center gap-1 text-fs-1 text-fg-1">
                <input type="checkbox" checked={(group.colors ?? []).includes(value)}
                  onChange={() => updateGroup(index(), { colors: toggle(group.colors, value) })} />
                {t(`org.color.${value}` as "org.color.red")}
              </label>}</For>
          </fieldset>
          <fieldset class="flex flex-wrap gap-2">
            <legend class="mb-1 text-fs-1 text-fg-2">{t("org.likes")}</legend>
            <For each={LIKES}>{(value) =>
              <label class="flex items-center gap-1 text-fs-1 text-fg-1">
                <input type="checkbox" checked={(group.likes ?? []).includes(value)}
                  onChange={() => updateGroup(index(), { likes: toggle(group.likes, value) })} />
                {t(`org.like.${value}` as "org.like.like")}
              </label>}</For>
          </fieldset>
          <fieldset class="flex flex-wrap gap-2">
            <legend class="mb-1 text-fs-1 text-fg-2">{t("org.locks")}</legend>
            <For each={LOCKS}>{(value) =>
              <label class="flex items-center gap-1 text-fs-1 text-fg-1">
                <input type="checkbox" checked={(group.locks ?? []).includes(value)}
                  onChange={() => updateGroup(index(), { locks: toggle(group.locks, value) })} />
                {t(`org.lock.${value}` as "org.lock.0")}
              </label>}</For>
          </fieldset>
          <Show when={group.repositoryIds.length > 0 &&
            group.minRating == null && !group.colors?.length && !group.likes?.length &&
            !group.locks?.length && !group.tagKeys?.length}>
            <p class="text-fs-1 text-fg-3">{t("org.allPhotosInScope")}</p>
          </Show>
        </div>}
      </For>
      <Button variant="ghost" icon={<IconPlus size={15} />} disabled={rules().groups.length >= 16}
        onClick={() => setRules((current) => ({
          groups: [...current.groups, ...emptyRuleDraft().groups],
        }))}>{t("org.addRuleGroup")}</Button>
      <Show when={preview()}>{(result) =>
        <p class="text-fs-1 text-fg-3">{t("org.previewCount").replace("{n}", String(result().matches))}</p>}
      </Show>
      <Show when={error()}>{(message) => <p role="alert" class="text-fs-1 text-danger">{message()}</p>}</Show>
    </div>
  </Dialog>;
}
