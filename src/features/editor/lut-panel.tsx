/**
 * 编辑左栏：**LUT 面板**（`design/editor.md` §3.1、`prompts/editor.pd`）。
 *
 * ```text
 * LUT                          [+分类] [导入]
 * ├ Travel (3)     ▼          ← 点标题展开；展开时其他分类自动收起；再点全部收起
 * │  ┌────┐ ┌────┐            ← 一行 2 个 tile：上面预览、下面名字
 * │  │预览│ │预览│
 * │  └────┘ └────┘
 * │  名字    名字
 * └ Portrait (0)
 * ```
 *
 * M3-W6c：分类和条目写入 `app.db`；导入后按 4:3 WebP 封面展示，
 * 选中 LUT 与应用开关随 latest 编辑栈保存。
 *
 * 面板宽度**固定 280、不可拖**（`.pd`：「占据整个left，不可调宽度」）——
 * 所以这里没有把手，也不需要 `SplitHandle`。
 */

import { For, Show, createSignal, createEffect, onCleanup, type JSX } from "solid-js";
import {
  IconChevronDown,
  IconChevronRight,
  IconFolderPlus,
  IconPalette,
  IconUpload,
} from "@tabler/icons-solidjs";

import { Button } from "../../components/ui/Button.tsx";
import { EasyDestroyButton } from "../../components/ui/EasyDestroy.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { Tooltip } from "../../components/ui/Tooltip.tsx";
import { t } from "../../i18n/index.ts";
import { lutCount, visibleLutCategories, type LutEntry } from "../../lib/lut-library.ts";
import { getLutCover } from "../../api/lut.ts";
import type { EditorStore } from "./store.ts";

/** 面板固定宽（`.pd` 只说「不可调宽」，没给数值 —— 见 `design/editor.md` §7 待决项 #3）。 */
export const LUT_PANEL_WIDTH = 280;

export interface LutPanelProps {
  store: EditorStore;
  class?: string;
  onCreateCategory?: (name: string) => Promise<boolean>;
  onImport?: (categoryId: string) => Promise<string | null>;
  onSelect?: (id: string) => void;
  onToggle?: () => void;
  onHide?: (id: string) => Promise<void>;
}

export function LutPanel(props: LutPanelProps): JSX.Element {
  const [creating, setCreating] = createSignal(false);
  const [draftName, setDraftName] = createSignal("");
  const [duplicate, setDuplicate] = createSignal(false);
  const [importOpen, setImportOpen] = createSignal(false);
  const [importCategory, setImportCategory] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [importSummary, setImportSummary] = createSignal<string | null>(null);

  const categories = () => visibleLutCategories(props.store.lutCategories());
  const total = () => lutCount(categories());
  const selectedMissing = () => {
    const id = props.store.lutId();
    return id !== null && props.store.lutCategories().flatMap((category) => category.entries).find((entry) => entry.id === id)?.available !== true;
  };

  function openCreate(): void {
    setDraftName("");
    setDuplicate(false);
    setCreating(true);
  }

  async function submitCreate(): Promise<void> {
    if (busy()) return;
    setBusy(true);
    try {
      const created = props.onCreateCategory === undefined
        ? props.store.addCategory(draftName()) : await props.onCreateCategory(draftName());
      if (!created) { setDuplicate(true); return; }
      setCreating(false);
    } finally { setBusy(false); }
  }

  return (
    <div
      data-editor-lut-panel
      class={["flex min-h-0 flex-1 flex-col bg-surface-main", props.class ?? ""]
        .filter(Boolean)
        .join(" ")}
      style={{ width: `${LUT_PANEL_WIDTH}px` }}
    >
      {/*
        标题行：「LUT」+ 行尾两个图标按钮（新建分类 / 导入 LUT）。
        文字按钮会和 LUT 名挤在一起，所以用图标 + Tooltip（`design/editor.md` §3.1）。
      */}
      <div class="flex shrink-0 items-center gap-1 px-panel-pad pt-panel-pad pb-1.5">
        <span class="flex-1 text-fs-3 font-semibold text-fg-1">
          {t("editor.lut.title")}
        </span>
        <Tooltip content={t("editor.lut.newCategory")}>
          {(triggerProps) => (
            <Button
              {...triggerProps()}
              variant="ghost"
              aria-label={t("editor.lut.newCategory")}
              icon={<IconFolderPlus size={14} />}
              onClick={openCreate}
            />
          )}
        </Tooltip>
        <Tooltip content={t("editor.lut.import")}>
          {(triggerProps) => (
            <Button {...triggerProps()} variant="ghost" data-lut-import
              aria-label={t("editor.lut.import")} icon={<IconUpload size={14} />}
              onClick={() => { setImportCategory(categories()[0]?.id ?? ""); setImportOpen(true); }} />
          )}
        </Tooltip>
      </div>

      <div class="flex shrink-0 items-center justify-between px-panel-pad pb-1 text-fs-1 text-fg-2">
        <span>{t("editor.lut.apply")}</span>
        <input type="checkbox" aria-label={t("editor.lut.apply")}
          checked={props.store.lutEnabled()} disabled={props.store.lutId() === null}
          onChange={(event) => { props.store.setLutEnabled(event.currentTarget.checked); props.onToggle?.(); }} />
      </div>
      <Show when={importSummary()}><p role="status" class="px-panel-pad pb-2 text-fs-0 text-fg-2">{importSummary()}</p></Show>
      <Show when={selectedMissing()}><p class="px-panel-pad pb-1 text-fs-0 text-danger">{t("editor.lut.selectedMissing")}</p></Show>
      <div class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto px-panel-pad pb-panel-pad">
        <Show
          when={categories().length > 0}
          fallback={
            <div
              class="flex flex-1 flex-col items-center justify-center gap-2 px-4 text-center"
              data-lut-empty
            >
              <IconPalette size={48} stroke-width={1} class="text-fg-3 opacity-60" aria-hidden="true" />
              <p class="text-fs-2 text-fg-2">{t("editor.lut.empty")}</p>
              <p class="text-fs-0 text-fg-3">{t("editor.lut.emptyHint")}</p>
            </div>
          }
        >
          <For each={categories()}>
            {(category) => {
              const expanded = (): boolean => props.store.expandedCategory() === category.id;
              return (
                <div data-lut-category={category.id} class="flex flex-col">
                  {/* 分类标题行：chevron + 名字 + 数量 */}
                  <button
                    type="button"
                    class="flex h-row-h w-full items-center gap-1.5 rounded-ui px-1.5 text-left hover:bg-state-hover"
                    aria-expanded={expanded()}
                    onClick={() => props.store.toggleCategory(category.id)}
                  >
                    <Show
                      when={expanded()}
                      fallback={<IconChevronRight size={12} class="shrink-0 text-fg-3" />}
                    >
                      <IconChevronDown size={12} class="shrink-0 text-fg-3" />
                    </Show>
                    <span class="min-w-0 flex-1 truncate text-fs-2 text-fg-1">
                      {category.name}
                    </span>
                    <span class="shrink-0 text-fs-0 tabular-nums text-fg-3">
                      {t("editor.lut.count").replace("{n}", String(category.entries.length))}
                    </span>
                  </button>

                  <Show when={expanded()}>
                    <Show
                      when={category.entries.length > 0}
                      fallback={
                        <p class="px-1.5 py-2 text-fs-0 text-fg-3">
                          {t("editor.lut.emptyCategory")}
                        </p>
                      }
                    >
                      <div class="grid grid-cols-2 gap-2 px-1.5 py-1.5">
                        <For each={category.entries}>
                          {(entry) => <LutTile entry={entry}
                            selected={props.store.lutId() === entry.id}
                            enabled={props.store.lutEnabled()}
                            onSelect={() => props.onSelect?.(entry.id)}
                            onHide={() => props.onHide?.(entry.id)} />}
                        </For>
                      </div>
                    </Show>
                  </Show>
                </div>
              );
            }}
          </For>

        </Show>
      </div>

      <Dialog open={importOpen()} onOpenChange={setImportOpen} title={t("editor.lut.import")}
        footer={<><Button variant="secondary" onClick={() => setImportOpen(false)}>{t("common.cancel")}</Button>
          <Button variant="primary" disabled={busy() || importCategory() === ""}
            onClick={async () => { setBusy(true); setImportSummary(null);
              try { setImportSummary(await props.onImport?.(importCategory()) ?? null); }
              finally { setBusy(false); setImportOpen(false); } }}>
            {t("editor.lut.import")}</Button></>}>
        <label class="flex flex-col gap-2 text-fs-1 text-fg-2">{t("editor.lut.category")}
          <select class="rounded-ui bg-surface-track px-2 py-1.5 text-fg-1"
            value={importCategory()} onChange={(event) => setImportCategory(event.currentTarget.value)}>
            <For each={categories()}>{(category) => <option value={category.id}>{category.name}</option>}</For>
          </select>
        </label>
      </Dialog>

      {/* 库里一个 LUT 都没有时的说明（分类建了但空着时也有用） */}
      <Show when={categories().length > 0 && total() === 0}>
        <p class="shrink-0 px-panel-pad pb-panel-pad text-fs-0 text-fg-3">
          {t("editor.lut.emptyHint")}
        </p>
      </Show>

      {/* 新建分类：一个小模态（与全系统的弹窗同一套 —— 取消在左、确认在右） */}
      <Dialog
        open={creating()}
        onOpenChange={(open) => setCreating(open)}
        title={t("editor.lut.newCategoryTitle")}
        footer={
          <>
            <Button variant="secondary" onClick={() => setCreating(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              disabled={draftName().trim() === ""}
              onClick={submitCreate}
            >
              {t("common.confirm")}
            </Button>
          </>
        }
      >
        <label class="flex flex-col gap-1.5">
          <span class="text-fs-2 text-fg-2">{t("editor.lut.newCategory")}</span>
          <input
            data-lut-category-input
            class="h-row-h rounded-ui bg-surface-track px-2 text-fs-2 text-fg-1 outline-none focus-visible:ring-1 focus-visible:ring-focus-ring"
            placeholder={t("editor.lut.categoryPlaceholder")}
            value={draftName()}
            autofocus
            onInput={(event) => {
              setDraftName(event.currentTarget.value);
              setDuplicate(false);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                submitCreate();
              }
            }}
          />
        </label>
        <Show when={duplicate()}>
          <p data-lut-category-duplicate class="mt-1.5 text-fs-0 text-danger">
            {t("editor.lut.categoryExists")}
          </p>
        </Show>
      </Dialog>
    </div>
  );
}

function LutTile(props: { entry: LutEntry; selected: boolean; enabled: boolean; onSelect: () => void; onHide: () => void | Promise<void> }): JSX.Element {
  const [cover, setCover] = createSignal<string | null>(null);
  createEffect(() => {
    const id = props.entry.id;
    let active = true;
    void getLutCover(id).then((bytes) => {
      if (!active || bytes === null) return;
      const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "image/webp" }));
      setCover((previous) => { if (previous !== null) URL.revokeObjectURL(previous); return url; });
    }).catch(() => undefined);
    onCleanup(() => { active = false; const previous = cover(); if (previous !== null) URL.revokeObjectURL(previous); });
  });
  return <div class="group min-w-0">
    <button type="button" disabled={props.entry.available === false} onClick={props.onSelect}
      class="relative block aspect-[4/3] w-full overflow-hidden rounded-ui bg-surface-bar disabled:opacity-50"
      classList={{ "ring-1 ring-brand": props.selected && props.enabled }}>
      <Show when={cover()}>{(url) => <img src={url()} alt="" class="h-full w-full object-cover" />}</Show>
      <Show when={props.selected && props.enabled}><span class="absolute right-1 top-1 rounded-full bg-brand px-1 text-fg-on-brand">✓</span></Show>
    </button>
    <div class="flex items-center gap-0.5">
      <span class="min-w-0 flex-1 truncate text-center text-fs-0 text-fg-2" title={props.entry.name}>{props.entry.name}</span>
      <EasyDestroyButton class="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
        label={t("editor.lut.remove")}
        confirmTitle={t("editor.lut.removeTitle")} confirmLabel={t("editor.lut.removeAction")}
        confirmMessage={t("editor.lut.removeConfirm").replace("{name}", props.entry.name)}
        onRemove={props.onHide} />
    </div>
    <Show when={props.entry.available === false}><span class="text-fs-0 text-danger">{t("editor.lut.missing")}</span></Show>
  </div>;
}
