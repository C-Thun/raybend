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
 * 本波（M3-W1）的边界，写在最显眼处：
 *
 * * **分类是真实且持久的**（设备级偏好，`lib/editor-prefs.ts`）—— 建了就一直在；
 * * **LUT 文件导入在 W4**：`导入 LUT` 按钮**禁用并说明**，不假装能点；
 * * tile 预览要「SOOC 缩略图经该 LUT 处理」（`.pd` 的 TODO），所以 W1 只画空态。
 *
 * 面板宽度**固定 280、不可拖**（`.pd`：「占据整个left，不可调宽度」）——
 * 所以这里没有把手，也不需要 `SplitHandle`。
 */

import { For, Show, createSignal, type JSX } from "solid-js";
import {
  IconChevronDown,
  IconChevronRight,
  IconFolderPlus,
  IconPalette,
  IconUpload,
} from "@tabler/icons-solidjs";

import { Button } from "../../components/ui/Button.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { Tooltip } from "../../components/ui/Tooltip.tsx";
import { t } from "../../i18n/index.ts";
import { lutCount } from "../../lib/lut-library.ts";
import type { EditorStore } from "./store.ts";
import { PendingNote } from "./parts.tsx";

/** 面板固定宽（`.pd` 只说「不可调宽」，没给数值 —— 见 `design/editor.md` §7 待决项 #3）。 */
export const LUT_PANEL_WIDTH = 280;

export interface LutPanelProps {
  store: EditorStore;
  class?: string;
}

export function LutPanel(props: LutPanelProps): JSX.Element {
  const [creating, setCreating] = createSignal(false);
  const [draftName, setDraftName] = createSignal("");
  const [duplicate, setDuplicate] = createSignal(false);

  const categories = () => props.store.lutCategories();
  const total = () => lutCount(categories());

  function openCreate(): void {
    setDraftName("");
    setDuplicate(false);
    setCreating(true);
  }

  function submitCreate(): void {
    const created = props.store.addCategory(draftName());
    if (!created) {
      setDuplicate(true);
      return;
    }
    setCreating(false);
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
        {/*
          导入 LUT：W4 才接线 —— **禁用 + 说明**，不做「点了没反应」。
          Tooltip 里写明在哪个波次接入，评审时一眼能分清「没画对」还是「还没接」。
        */}
        <Tooltip content={t("editor.lut.importLater")}>
          {(triggerProps) => (
            <Button
              {...triggerProps()}
              variant="ghost"
              disabled
              data-lut-import
              aria-label={t("editor.lut.import")}
              icon={<IconUpload size={14} />}
            />
          )}
        </Tooltip>
      </div>

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
                      {/* 一行 2 个 tile（`.pd`）；tile 内容在 W4 才有（要 SOOC 缩略图经 LUT 处理） */}
                      <div class="grid grid-cols-2 gap-2 px-1.5 py-1.5">
                        <For each={category.entries}>
                          {(entry) => (
                            <div class="flex flex-col gap-1">
                              <div class="aspect-[4/3] rounded-ui bg-surface-bar" />
                              <span class="truncate text-fs-0 text-fg-2">{entry.name}</span>
                            </div>
                          )}
                        </For>
                      </div>
                    </Show>
                  </Show>
                </div>
              );
            }}
          </For>
          <PendingNote class="mt-2" text={t("editor.lut.importLater")} />
        </Show>
      </div>

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
