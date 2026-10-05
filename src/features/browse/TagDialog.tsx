/**
 * 标签弹窗（`memory/FUNCTION-BROWSE.md` §3.3、`design/browse.md` §2.6、画布帧 `Components / 标签弹窗`）。
 *
 * ```text
 * ┌ 标签 ───────────────────────────────┐
 * │ P1000156.JPG · 已有 3 个标签          │
 * │ ┌ 输入即搜（0.6s 防抖）─────────────┐ │
 * │ └───────────────────────────────────┘ │
 * │ ┌ 搜索结果 ──────────────────────────┐ │   ← 上下键选，回车第一个；不选而回车 = 创建
 * │ │ 婚礼 / wedding 外景 / 创建「wed」   │ │
 * │ └───────────────────────────────────┘ │
 * │ (婚礼 ×) (外景 ×) (朋友 ×)   ← 单张：每个标签右边带叉（点叉移除）
 * │                        [取消] [保存]   │
 * └───────────────────────────────────────┘
 * ```
 *
 * ## 两条硬规则
 *
 * 1. **单张可增可删、批量只能加**（人类的原话）。
 *    批量时标签右边**不给叉**，并且写一句「想移除请切回单张编辑」——
 *    少了叉却不说原因，人会以为界面坏了（画布上就画着这句提示）。
 * 2. **输入与提交分开**：这个弹窗是「编辑一份草稿，按保存才落库」——
 *    于是「加三个标签 + 摘一个」是**一步撤销**，而不是四次点错的来源。
 *    取消 = 整份草稿丢掉。
 *
 * ## 回车到底做什么（人类特意强调的那条）
 *
 * * 搜索结果里**选中的那一项**（上下键或鼠标指到）→ 挂它；
 * * **一个都没选** → 直接**创建新标签**，**即使列表里已经有匹配结果**
 *   （人的意思是「我就要这个写法」，不该被模糊匹配顶掉）。
 */

import {
  createEffect,
  createMemo,
  createSignal,
  For,
  onCleanup,
  Show,
  type JSX,
} from "solid-js";
import { IconPlus, IconSearch, IconBan } from "@tabler/icons-solidjs";

import { photoTagState, type PhotoTagState } from "../../api/organization.ts";
import { stageTagEdit, tagDraftRows, tagKey, type TagEdit } from "./tag-draft.ts";
import { tagEnsure, tagList } from "../../api/browse.ts";
import type { MarkResult, Tag } from "../../api/types.ts";
import { Button } from "../../components/ui/Button.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { t } from "../../i18n/index.ts";
import type { BrowseStore } from "./store.ts";

/** 输入即搜的防抖（人类定的是 **0.6 秒**，别改小） */
export const TAG_SEARCH_DEBOUNCE_MS = 600;
/** 一屏最多给多少条候选（词典可能很大，别一次倒出来） */
const SEARCH_LIMIT = 30;
/** 打开时先读一批常用的（顺带给「已有标签」查名字用） */
const DICTIONARY_LIMIT = 500;

export interface TagDialogProps {
  open: boolean;
  store: BrowseStore;
  /** 关窗（保存与取消都走它；取消时草稿直接丢掉） */
  onClose: () => void;
  /** 保存之后把结果交出去（位置留给 toast / 撤销：`specs/M2-W2-tail.md` 4.2） */
  onDone?: (result: MarkResult | null) => void;
}

export function TagDialog(props: TagDialogProps): JSX.Element {
  const [query, setQuery] = createSignal("");
  const [results, setResults] = createSignal<Tag[]>([]);
  const [dictionary, setDictionary] = createSignal<Tag[]>([]);
  const [highlight, setHighlight] = createSignal(-1);
  /** 草稿：待挂上的标签（新标签或搜索结果） */
  const [adding, setAdding] = createSignal<Tag[]>([]);
  /** 草稿：待摘掉的标签 id（只有单张模式会用到） */
  const [removing, setRemoving] = createSignal<readonly number[]>([]);
  const [busy, setBusy] = createSignal(false);
  const [sources, setSources] = createSignal<PhotoTagState | null>(null);
  const [edits, setEdits] = createSignal<TagEdit[]>([]);
  const [detail, setDetail] = createSignal<string | null>(null);
  const [sourceError, setSourceError] = createSignal<string | null>(null);
  const [sourceLoading, setSourceLoading] = createSignal(false);
  const [loadRevision, setLoadRevision] = createSignal(0);
  const [saveError, setSaveError] = createSignal<string | null>(null);
  const rows = createMemo(() => sources() ? tagDraftRows(sources()!, edits()) : []);
  const stage = (name: string, manual: boolean | null, masked: boolean | null) => setEdits((current) => stageTagEdit(current, { name, manual, masked }));
  let dialogEpoch = 0;
  let inputEl: HTMLInputElement | undefined;

  const selectedIds = () => props.store.selectedIds();
  const single = () => selectedIds().length === 1;

  /**
   * 这些照片**已有**的哪些标签（升序）。
   *
   * 单张 = 它的全部标签；批量 = **所有选中照片都有的那些**（交集）——
   * 交集才是「这一批共同已有」，也才不会让人以为某张缺标签。
   */
  const existingIds = createMemo<number[]>(() => {
    const ids = selectedIds();
    if (ids.length === 0) return [];
    const markings = props.store.markings();
    const sets = ids.map((id) => new Set(markings.get(id)?.tagIds ?? []));
    const first = sets[0];
    if (first === undefined) return [];
    const intersection = [...first].filter((tagId) => sets.every((set) => set.has(tagId)));
    return intersection.sort((a, b) => a - b);
  });

  const nameOf = (id: number): string => {
    const found =
      dictionary().find((tag) => tag.id === id) ?? results().find((tag) => tag.id === id);
    return found?.name ?? `#${id}`;
  };

  /** 已经挂上的（扣掉草稿里要摘的）+ 草稿里要加的 —— 这就是界面上的那把标签 */
  const shownTags = createMemo<{ id: number; name: string; staged: boolean }[]>(() => {
    if (single() && sources()) return rows().filter((row) => !row.masked).map((row, index) => ({ id: dictionary().find((tag) => tagKey(tag.name) === tagKey(row.name))?.id ?? -index - 1, name: row.name, staged: edits().some((edit) => tagKey(edit.name) === tagKey(row.name)) }));
    const out: { id: number; name: string; staged: boolean }[] = [];
    for (const id of existingIds()) {
      if (removing().includes(id)) continue;
      out.push({ id, name: nameOf(id), staged: false });
    }
    for (const tag of adding()) {
      if (out.some((item) => item.id === tag.id)) continue;
      out.push({ id: tag.id, name: tag.name, staged: true });
    }
    return out;
  });

  let searchRevision = 0;
  async function refreshSearch(text: string): Promise<void> {
    const revision = ++searchRevision;
    try {
      const found = await tagList(text, SEARCH_LIMIT);
      if (!props.open || revision !== searchRevision || text !== query()) return;
      setResults(found); setHighlight(-1);
    } catch (error) { if (props.open && revision === searchRevision) setSaveError(String(error)); }
  }

  createEffect(() => {
    loadRevision();
    const open = props.open;
    const repo = props.store.repositoryId();
    const ids = selectedIds();
    setSources(null); setEdits([]); setDetail(null); setSourceError(null); setSourceLoading(false);
    if (!open || ids.length !== 1 || !repo) return;
    let active = true;
    setSourceLoading(true);
    void photoTagState(repo, ids[0]!).then((value) => { if (active) setSources(value); })
      .catch((error: unknown) => { if (active) setSourceError(String(error)); })
      .finally(() => { if (active) setSourceLoading(false); });
    onCleanup(() => { active = false; });
  });

  // 打开：清草稿、读一份词典（给「已有标签」查名字）、聚焦输入
  createEffect(() => {
    dialogEpoch += 1;
    if (!props.open) return;
    setQuery("");
    setSaveError(null);
    setAdding([]);
    setRemoving([]);
    setHighlight(-1);
    setResults([]);
    const epoch = dialogEpoch;
    void (async () => {
      try {
        const found = await tagList("", DICTIONARY_LIMIT);
        if (!props.open || dialogEpoch !== epoch) return;
        setDictionary(found); await refreshSearch("");
      } catch (error) { if (props.open && dialogEpoch === epoch) setSaveError(String(error)); }
    })();
    // 输入框要拿到焦点，否则「输入即搜」根本没法开始
    queueMicrotask(() => inputEl?.focus());
  });

  // 输入即搜：**0.6 秒防抖**（每敲一个字都发一次 IPC 是浪费，也确实会抖）
  createEffect(() => {
    const text = query();
    if (!props.open) return;
    const timer = setTimeout(() => void refreshSearch(text), TAG_SEARCH_DEBOUNCE_MS);
    onCleanup(() => clearTimeout(timer));
  });

  /** 挂上一条草稿（已挂着的会先从「待摘」里撤回来 —— 点错了能反悔） */
  function stageTag(tag: Tag): void {
    if (single() && sources()) { stage(tag.name, true, null); setQuery(""); setHighlight(-1); inputEl?.focus(); return; }
    setRemoving((current) => current.filter((id) => id !== tag.id));
    setAdding((current) =>
      current.some((item) => item.id === tag.id) ||
      existingIds().includes(tag.id) ||
      current.some((item) => item.name === tag.name)
        ? current
        : [...current, tag],
    );
    setQuery("");
    setHighlight(-1);
    inputEl?.focus();
  }

  /** 单张：把已有的标签挂进「待摘」（再点一次撤回） */
  function stageRemove(id: number): void {
    if (!single()) return;
    setRemoving((current) =>
      current.includes(id) ? current.filter((item) => item !== id) : [...current, id],
    );
  }

  /** 回车：选中了就挂它，没选就**创建新标签**（即使已经有匹配结果） */
  async function commitInput(): Promise<void> {
    const epoch = dialogEpoch;
    const index = highlight();
    const list = results();
    if (index >= 0 && index < list.length) {
      stageTag(list[index]!);
      return;
    }
    const name = query().trim();
    if (name === "") return;
    // 词典里已有同名（折叠后相等）→ 直接当它，不必新建
    const folded = tagKey(name);
    const existing = dictionary().find((tag) => tagKey(tag.name) === folded);
    if (existing !== undefined) {
      stageTag(existing);
      return;
    }
    const created = await tagEnsure(name);
    if (created === null) throw new Error(t("ai.saveFailed"));
    if (!props.open || dialogEpoch !== epoch) return;
    setDictionary((current) => [created, ...current]);
    // 同步进 store 的词典：右栏「标签」那一段当场就能显示它（不必等下次刷新）
    props.store.rememberTag(created);
    stageTag(created);
  }

  function onInputKeyDown(event: KeyboardEvent): void {
    const list = results();
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setHighlight((current) => Math.min(list.length - 1, current + 1));
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      setHighlight((current) => Math.max(-1, current - 1));
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      void commitInput().catch((error: unknown) => setSaveError(String(error)));
    }
  }

  async function save(): Promise<void> {
    if (busy() || sourceLoading() || sourceError()) return;
    /*
     * **输入框里还有没提交的名字 ⇒ 先把它当一条标签**（人类 2026-09-19：
     * 「第一次点了保存没反应，后面好了」—— 真因就在这里：
     * 用户打完字直接点「保存」，而保存只提交「已挂上/已摘掉」的清单，
     * 输入框里的那串字既没挂上也没保存，弹窗一关看起来就是「点了没反应」）。
     *
     * 与回车同一个入口（`commitInput`），所以「回车新建」「点保存新建」结果一致。
     */
    setBusy(true);
    setSaveError(null);
    try {
    if (query().trim() !== "") await commitInput();
    const add = adding()
      .map((tag) => tag.id)
      .filter((id) => !existingIds().includes(id));
    const remove = single() ? [...removing()] : [];
    if (add.length === 0 && remove.length === 0 && edits().length === 0) {
      props.onClose();
      return;
    }
      const draft: TagEdit[] = single() && sources() ? edits() : [
        ...adding().map((tag) => ({ name: tag.name, manual: true, masked: null })),
        ...remove.map((id) => ({ name: nameOf(id), manual: false, masked: null })),
      ];
      const last = draft.length > 0 ? await props.store.mark({ kind: "photoTags", edits: draft }) : null;
      if (draft.length > 0 && last === null) throw new Error(t("ai.saveFailed"));
      props.onDone?.(last); props.onClose();
    } catch (error) { setSaveError(String(error)); }
    finally { setBusy(false); }
  }

  const title = () =>
    single()
      ? t("browse.tagsTitle")
      : t("browse.tagsTitleBatch").replace("{n}", String(selectedIds().length));

  const description = () =>
    single()
      ? t("browse.tagsDescSingle")
          .replace("{name}", props.store.selectedItems()[0]?.fileName ?? "")
          .replace("{n}", String(existingIds().length))
      : t("browse.tagsDescBatch");

  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => {
        if (!open && !busy()) props.onClose();
      }}
      title={title()}
      description={description()}
      footer={
        <>
          <Button variant="secondary" disabled={busy()} onClick={props.onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" disabled={busy() || sourceLoading() || sourceError() !== null} onClick={() => void save()}>
            {single() ? t("common.save") : t("browse.tagsAdd")}
          </Button>
        </>
      }
    >
      <div class="flex flex-col gap-3" data-tag-dialog={single() ? "single" : "batch"}>
        <Show when={sourceLoading()}><p class="text-fs-1 text-fg-3" role="status">{t("ai.loadingTags")}</p></Show>
        <Show when={sourceError()}>{(error) => <div class="space-y-2"><p class="text-fs-1 text-danger" role="alert">{error()}</p><Button variant="secondary" size="sm" onClick={() => setLoadRevision((n) => n + 1)}>{t("ai.retry")}</Button></div>}</Show>
        <Show when={saveError()}>{(error) => <p class="text-fs-1 text-danger" role="alert">{error()}</p>}</Show>
        <fieldset class="contents" disabled={busy() || sourceLoading() || sourceError() !== null}>
        {/* 输入即搜 */}
        <div class="flex h-8 items-center gap-2 rounded-ui bg-surface-bar px-2">
          <IconSearch size={14} class="shrink-0 text-fg-3" aria-hidden="true" />
          <input
            ref={inputEl}
            class="min-w-0 flex-1 bg-transparent text-fs-2 text-fg-1 outline-none placeholder:text-fg-3"
            placeholder={t("browse.tagsPlaceholder")}
            value={query()}
            aria-label={t("browse.tagsPlaceholder")}
            onInput={(event) => { setQuery(event.currentTarget.value); setHighlight(-1); setResults([]); }}
            onKeyDown={onInputKeyDown}
          />
        </div>

        {/* 搜索结果（上下键选、回车挂上；最后一行是「创建」） */}
        <div class="rounded-ui bg-surface-bar p-2" data-tag-results="open">
          <Show
            when={results().length > 0}
            fallback={<p class="text-fs-2 text-fg-3">{t("browse.tagsNoMatch")}</p>}
          >
            <ul class="flex max-h-40 flex-col gap-0.5 overflow-y-auto">
              <For each={results()}>
                {(tag, index) => (
                  <li>
                    <button
                      type="button"
                      data-tag-result={index()}
                      class={[
                        "flex w-full items-center gap-1.5 rounded-ui px-1.5 py-0.5 text-left text-fs-2",
                        index() === highlight()
                          ? "bg-state-selected text-fg-1"
                          : "text-fg-2 hover:bg-state-hover",
                      ].join(" ")}
                      onMouseEnter={() => setHighlight(index())}
                      onClick={() => stageTag(tag)}
                    >
                      <span class="min-w-0 flex-1 truncate">{tag.name}</span>
                      <Show when={existingIds().includes(tag.id)}>
                        <span class="shrink-0 text-fs-0 text-fg-3">{t("browse.tagsAlready")}</span>
                      </Show>
                    </button>
                  </li>
                )}
              </For>
            </ul>
          </Show>
          {/* 「创建」这一行只在输入非空时出现；回车不选任何结果时走的就是它 */}
          <Show when={query().trim() !== ""}>
            <button
              type="button"
              data-tag-create="open"
              class="mt-1 flex w-full items-center gap-1.5 rounded-ui px-1.5 py-0.5 text-left text-fs-2 text-brand hover:bg-state-hover"
              onClick={() => void commitInput().catch((error: unknown) => setSaveError(String(error)))}
            >
              <IconPlus size={12} aria-hidden="true" />
              {t("browse.tagsCreate").replace("{name}", query().trim())}
            </button>
          </Show>
        </div>

        {/* 袋里的标签（瀑布流）：单张带叉、批量不带 */}
        <div class="flex flex-wrap gap-1.5" data-tag-chips={shownTags().length}>
          <For each={shownTags()}>
            {(tag) => (
              <span
                class={[
                  "inline-flex h-6 items-center gap-1 rounded-full bg-surface-bar pl-2 text-fs-2",
                  tag.staged ? "text-brand" : "text-fg-1",
                  single() ? "pr-1" : "pr-2",
                ].join(" ")}
                data-tag-chip={tag.id}
                data-tag-staged={tag.staged ? "true" : undefined}
              >
                <button type="button" class="rounded-full px-1 hover:bg-state-hover" onClick={() => setDetail(tag.name)}>{tag.name}</button>
                <Show when={single() && sources()}><span class="pr-2 text-fs-0 text-fg-3">{t(rows().find((row) => tagKey(row.name) === tagKey(tag.name))?.manual ? rows().find((row) => tagKey(row.name) === tagKey(tag.name))?.ai ? "ai.both" : "ai.manual" : "ai.source")}</span></Show>
                <Show when={single() && !sources()}>
                  <button
                    type="button"
                    aria-label={t("browse.tagsRemove").replace("{name}", tag.name)}
                    class="flex h-4 w-4 items-center justify-center rounded-full text-fg-3 hover:bg-state-hover hover:text-fg-1"
                    onClick={() => {
                      if (tag.staged) {
                        setAdding((current) => current.filter((item) => item.id !== tag.id));
                      } else {
                        stageRemove(tag.id);
                      }
                    }}
                  >
                    <IconBan size={11} aria-hidden="true" />
                  </button>
                </Show>
              </span>
            )}
          </For>
        </div>

        <Show when={single() && sources() && detail()}>
          <div class="space-y-2 rounded-ui bg-surface-bar p-3">
            <p class="font-semibold text-fg-1">{detail()}</p>
            <Show when={rows().find((row) => tagKey(row.name) === tagKey(detail() ?? ""))?.manual}><p class="text-fs-1 text-fg-2">{t("ai.manual")}</p></Show>
            <Show when={sources()?.ai.some((name) => tagKey(name) === tagKey(detail() ?? ""))}>
              <p class="text-fs-1 text-fg-2">{t(sources()?.result?.origin === "sidecar" ? "ai.sidecar" : "ai.source")} · {t(sources()?.result?.valid ? "ai.valid" : "ai.invalid")}</p>
              <p class="break-all text-fs-0 text-fg-3">{sources()?.result?.modelSha256.slice(0, 12)} · {new Date(sources()?.result?.generatedAt ?? 0).toLocaleString()}</p>
            </Show>
            <div class="flex flex-wrap gap-2">
              <Button variant="secondary" size="sm" onClick={() => stage(detail()!, true, null)}>{t("ai.retain")}</Button>
              <Show when={rows().find((row) => tagKey(row.name) === tagKey(detail() ?? ""))?.manual}><Button variant="secondary" size="sm" onClick={() => stage(detail()!, false, null)}>{t("ai.removeManual")}</Button></Show>
              <Button variant="secondary" size="sm" icon={<IconBan size={14} />} onClick={() => { stage(detail()!, null, true); setDetail(null); }}>{t("ai.mask")}</Button>
            </div>
            <p class="text-fs-0 text-fg-3">{t("ai.removeHint")}</p>
          </div>
        </Show>
        <Show when={single() && sources()?.result?.valid === false && (sources()?.ai.length ?? 0) > 0}>
          <details class="rounded-ui bg-surface-bar p-3 text-fs-1 text-fg-2">
            <summary class="cursor-pointer">{t("ai.invalid")}</summary>
            <For each={sources()?.ai ?? []}>{(name) => <button type="button" class="mt-2 mr-2 rounded-ui px-1.5 hover:bg-state-hover" onClick={() => setDetail(name)}>{name}</button>}</For>
          </details>
        </Show>
        <Show when={single() && rows().some((row) => row.masked)}>
          <details class="rounded-ui bg-surface-bar p-3 text-fs-1 text-fg-2">
            <summary class="cursor-pointer">{t("ai.maskedTags")}</summary>
            <For each={rows().filter((row) => row.masked)}>{(row) => <div class="mt-2 flex items-center justify-between gap-2"><button type="button" class="text-left hover:text-fg-1" onClick={() => setDetail(row.name)}>{row.name}</button><Button variant="secondary" size="sm" onClick={() => stage(row.name, null, false)}>{t("ai.unmask")}</Button></div>}</For>
          </details>
        </Show>

        {/* 批量：说清楚为什么没有叉（画布上就写着这句） */}
        <Show when={!single()}>
          <p class="text-fs-0 text-fg-3">{t("browse.tagsBatchHint")}</p>
        </Show>
        </fieldset>
      </div>
    </Dialog>
  );
}
