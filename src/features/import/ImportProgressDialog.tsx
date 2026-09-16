/**
 * `ImportProgressDialog` —— 导入进度（画布 `Dialog / 导入进度` 三帧）。
 *
 * ```text
 * ┌──────────────────────────────────────────────┐
 * │ 导入中                                        │
 * │ 3 个目录 · 共 120 张 —— 已导入的会留在库里      │
 * │ 扫描 › 规划 › 导入 › 缩略图                    │  ← 阶段条
 * │ ████████████░░░░░░  68%                       │
 * │ 已导入 82   跳过 3   失败 1                    │
 * │ ┌ P1040733.ORF ────────────────────────────┐ │  ← 当前项：证明「它没卡住」
 * │ │ photos/2026-08-15/_RAW/MYP0733.ORF       │ │
 * │ └──────────────────────────────────────────┘ │
 * │                        [暂停] [取消]          │
 * └──────────────────────────────────────────────┘
 * ```
 *
 * 三条口径（`REPOSITORY.md` §4.4 与设计稿一致）：
 *
 * * **扫描/规划阶段没有总数** —— 进度条走不确定态，不编百分比；
 * * **取消要说清「已导入的保留」**，并且要二次确认（关弹窗 = 取消）；
 * * **结束时给「在库中查看这些照片」按钮**，不自动跳工作流。
 */

import { createSignal, For, Show } from "solid-js";
import { t } from "../../i18n/index.ts";
import { pickSaveFile } from "../../api/dialog.ts";
import { Button } from "../../components/ui/Button.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import type { ImportStage, ImportStore } from "./index.ts";

/** 阶段顺序（阶段条按它渲染）。 */
const STAGES: ImportStage[] = ["scan", "plan", "import", "thumbs"];

export interface ImportProgressDialogProps {
  store: ImportStore;
  /** 结束后点「在库中查看这些照片」（切到浏览并选中这批；M1-6 先只切工作流）。 */
  onRevealInLibrary?: () => void;
}

export function ImportProgressDialog(props: ImportProgressDialogProps) {
  const store = props.store;
  const [confirmingCancel, setConfirmingCancel] = createSignal(false);
  const [exportedTo, setExportedTo] = createSignal<string | null>(null);

  const stageIndex = () => STAGES.indexOf(store.stage());

  const requestCancel = () => {
    // 还在跑：先问一次（已导入的部分保留，这句要写在确认里）
    if (!store.finished()) {
      setConfirmingCancel(true);
      return;
    }
    props.store.dismiss();
  };

  async function doCancel(): Promise<void> {
    setConfirmingCancel(false);
    await store.cancel();
  }

  async function exportErrors(): Promise<void> {
    const path = await pickSaveFile({
      title: t("import.export_errors"),
      defaultName: "raybend-import-errors.json",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (path === null) return;
    const written = await store.exportErrors(path);
    setExportedTo(written > 0 ? path : null);
  }

  return (
    <Dialog
      open={store.open()}
      onOpenChange={(open) => {
        if (!open) requestCancel();
      }}
      title={store.finished() ? t("import.done_title") : t("import.title")}
      footer={
        <>
          <Show when={store.errors().length > 0}>
            <Button variant="secondary" onClick={() => void exportErrors()}>
              {t("import.export_errors")}
            </Button>
          </Show>
          <Show
            when={store.finished()}
            fallback={
              <>
                <Show
                  when={store.state() === "paused"}
                  fallback={
                    <Button
                      variant="secondary"
                      disabled={store.busy()}
                      onClick={() => void store.pause()}
                    >
                      {t("import.pause")}
                    </Button>
                  }
                >
                  <Button
                    variant="secondary"
                    disabled={store.busy()}
                    onClick={() => void store.resume()}
                  >
                    {t("import.resume")}
                  </Button>
                </Show>
                <Button variant="secondary" onClick={requestCancel}>
                  {t("import.cancel_button")}
                </Button>
              </>
            }
          >
            <Button variant="secondary" onClick={() => store.dismiss()}>
              {t("common.close")}
            </Button>
            <Show when={store.counts().imported > 0 && props.onRevealInLibrary}>
              <Button variant="primary" onClick={() => props.onRevealInLibrary?.()}>
                {t("import.reveal")}
              </Button>
            </Show>
          </Show>
        </>
      }
    >
      <div class="flex flex-col gap-3">
        {/* 没有后端 / 开始失败：说清原因，别显示一个永远转圈的进度条 */}
        <Show when={store.error()}>
          {(message) => <p class="text-fs-1 text-danger">{message()}</p>}
        </Show>

        <Show when={store.progress()}>
          <p class="text-fs-1 text-fg-2">{summaryLine(store)}</p>

          {/* 阶段条 */}
          <div class="flex items-center gap-1.5">
            <For each={STAGES}>
              {(stage, index) => (
                <>
                  <Show when={index() > 0}>
                    <span class="text-fs-0 text-fg-3" aria-hidden="true">
                      ›
                    </span>
                  </Show>
                  <span
                    class={
                      index() === stageIndex()
                        ? "text-fs-1 text-brand"
                        : "text-fs-1 text-fg-3"
                    }
                  >
                    {t(`import.stage.${stage}`)}
                  </span>
                </>
              )}
            </For>
          </div>

          {/* 进度条：有总数才给确定值 */}
          <div class="h-1.5 w-full overflow-hidden rounded-ui bg-surface-track">
            <Show when={store.percent() !== null}>
              <div
                class="h-full rounded-ui bg-brand transition-[width] duration-200"
                style={{ width: `${store.percent() ?? 0}%` }}
              />
            </Show>
          </div>

          <div class="flex items-center gap-4">
            <span class="text-fs-1 text-fg-1">
              {t("import.imported")} {store.counts().imported}
            </span>
            <span class="text-fs-1 text-fg-2">
              {t("import.skipped")} {store.counts().skipped}
            </span>
            <Show when={store.counts().duplicates > 0}>
              <span class="text-fs-0 text-fg-3">
                （{t("import.duplicates")} {store.counts().duplicates}）
              </span>
            </Show>
            <span
              class={
                store.counts().failed > 0
                  ? "text-fs-1 text-danger"
                  : "text-fs-1 text-fg-2"
              }
            >
              {t("import.failed")} {store.counts().failed}
            </span>
          </div>

          <Show when={store.runLabel()}>
            {(label) => <p class="text-fs-0 text-fg-3">{label()}</p>}
          </Show>

          <Show when={store.currentLabel()}>
            {(current) => (
              <div class="flex flex-col gap-0.5 rounded-ui bg-surface-track px-2 py-1.5">
                <span class="truncate text-fs-1 text-fg-1">{current()}</span>
                <span class="text-fs-0 text-fg-3">
                  {t("import.keep_partial")}
                </span>
              </div>
            )}
          </Show>

          {/* 错误清单（有失败才出现；只显示尾巴） */}
          <Show when={store.errors().length > 0}>
            <div class="flex max-h-40 flex-col gap-1 overflow-auto rounded-ui bg-surface-track p-2">
              <For each={store.errors()}>
                {(error) => (
                  <div class="flex flex-col gap-0.5">
                    <span class="truncate text-fs-1 text-fg-1">
                      {error.target ?? error.source}
                    </span>
                    <span class="text-fs-1 text-danger">{error.reason}</span>
                  </div>
                )}
              </For>
            </div>
            <Show when={store.progress()!.errorsTotal > store.errors().length}>
              <p class="text-fs-0 text-fg-3">
                {t("import.errors_truncated", {
                  shown: store.errors().length,
                  total: store.progress()!.errorsTotal,
                })}
              </p>
            </Show>
          </Show>

          <Show when={store.runNote()}>
            {(note) => <p class="text-fs-0 text-fg-3">{note()}</p>}
          </Show>
          <Show when={exportedTo()}>
            {(path) => (
              <p class="text-fs-0 text-fg-3">
                {t("import.exported_to", { path: path() })}
              </p>
            )}
          </Show>
        </Show>
      </div>

      {/* 取消的二次确认（画布上没有单独帧：与「新建库」的确认同一套口径） */}
      <Show when={confirmingCancel()}>
        <div class="fixed inset-0 z-50 flex items-center justify-center bg-scrim">
          <div class="flex w-96 flex-col gap-3 rounded-ui bg-surface-layer p-4">
            <p class="text-fs-2 text-fg-1">{t("import.cancel_confirm")}</p>
            <p class="text-fs-1 text-fg-2">{t("import.keep_partial")}</p>
            <div class="flex justify-end gap-2">
              <Button variant="secondary" onClick={() => setConfirmingCancel(false)}>
                {t("import.keep_running")}
              </Button>
              <Button variant="primary" onClick={() => void doCancel()}>
                {t("import.cancel_button")}
              </Button>
            </div>
          </div>
        </div>
      </Show>
    </Dialog>
  );
}

/** 头部那一句：目录数 + 总数 + 「已导入的保留」的承诺。 */
function summaryLine(store: ImportStore): string {
  const progress = store.progress();
  if (progress === null) return t("import.preparing");
  const runs = progress.runs.length;
  return t("import.summary", {
    dirs: runs,
    total: progress.total > 0 ? progress.total : progress.runs.reduce((n, r) => n + r.scanned, 0),
  });
}

/** 阶段名（画布上的四段：扫描 / 规划 / 导入 / 缩略图）。 */
export function stageLabel(stage: ImportStage): string {
  return t(`import.stage.${stage}`);
}
