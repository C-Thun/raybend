/**
 * `LibrarySettingsDialog` —— 库设置（画布 `Dialog / 库设置` `A9vPG`）。
 *
 * M1 里**只有一项**：导入模版。三件事让它好用：
 *
 * 1. **变量 chip 可点** —— 插到光标处，不用记拼写；
 * 2. **实时预览** —— 示例照片会落到哪，边打字边看（校验在 Rust 侧，一份规则不重写两遍）；
 * 3. **保存前先校验** —— 模版坏了根本不写进库（否则下次导入才发现）。
 */

import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import * as db from "../../api/db.ts";
import type { RebuildProgress, RepositoryView, TemplatePreview } from "../../api/types.ts";
import { Button } from "../../components/ui/Button.tsx";
import type { ToastStore } from "../../components/ui/toast.ts";
import { IconCloudOff, IconRefresh } from "@tabler/icons-solidjs";
import { ConfirmDialog, Dialog } from "../../components/ui/Dialog.tsx";
import { Input } from "../../components/ui/Form.tsx";
import { locale, t } from "../../i18n/index.ts";
import { formatCount } from "../../lib/format.ts";
import { rebuildProgressMessage } from "./rebuild-progress.ts";

/** 可用的模版变量（与 Rust 的 `KNOWN_VARS` 一致；点一下插到光标处）。 */
const VARIABLES = [
  ":CYEAR",
  ":CMONTH",
  ":CDAY",
  ":CWEEK",
  ":FILENAME",
  ":BRAND",
  ":MODEL",
  ":SEQ000",
];

export interface LibrarySettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  repositoryId: string | null;
  repositoryName?: string;
  /**
   * 这个库**当前的状态行**（来自中央状态，`features/repositories/state.ts`）。
   *
   * 传进来而不是自己再存一份 —— 外面列表和这里读的是同一份数据，
   * 谁发现的离线都会同步（人类 2026-09-16：「一个地方变更了状态，
   * 所有挂在这套数据上的界面都会同步变更」）。
   */
  repository?: RepositoryView;
  onSaved?: (template: string) => void;
  /**
   * **发现它其实读不到**（读 `catalog.db` 失败）时调一次。
   *
   * 调用方把它转给中央状态的 `markOffline` —— 于是「弹窗里发现离线」会立刻
   * 反映到外面的库卡片上，不需要谁去挨个同步。
   */
  onStale?: (repositoryId: string) => void;
  /** 应用根层的统一提示队列；不传时仍会在弹窗内保留完成摘要。 */
  toast?: ToastStore;
}

export function LibrarySettingsDialog(props: LibrarySettingsDialogProps) {
  const [template, setTemplate] = createSignal("");
  const [preview, setPreview] = createSignal<TemplatePreview | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  let inputEl: HTMLInputElement | undefined;
  /*
   * 重建数据（人类 2026-09-19）：**花时间**（要重扫整个库），所以是「按钮 → 二级确认 → 跑」，
   * 跑的时候按钮转圈、别让用户以为界面死了。
   */
  const [counts, setCounts] = createSignal<[number, number] | null>(null);
  const [rebuildOpen, setRebuildOpen] = createSignal(false);
  const [rebuilding, setRebuilding] = createSignal(false);
  const [rebuildProgress, setRebuildProgress] = createSignal<RebuildProgress | null>(null);
  const [rebuilt, setRebuilt] = createSignal<string | null>(null);
  let stopRebuildProgress: (() => void) | null = null;

  onCleanup(() => stopRebuildProgress?.());

  // 打开时读一次当前模版
  createEffect(() => {
    if (!props.open) return;
    const id = props.repositoryId;
    if (id === null) return;
    setRebuildProgress(null);
    setRebuilt(null);
    setLoading(true);
    setError(null);
    /*
     * 两个计数**自己读一次**：外面的库卡片可能有（导入侧），也可能没有（浏览侧只拿得到 id）。
     * 读不到就是「—」—— 数字缺一个不该让整个弹窗打不开。
     */
    void db
      .repositoryCounts(id)
      .then(setCounts)
      .catch(() => setCounts(null));
    void db
      .repositorySettings(id)
      .then((settings) => {
        setTemplate(settings.importTemplate);
        return db.previewTemplate(settings.importTemplate);
      })
      .then(setPreview)
      .catch((caught: unknown) => {
        setError(caught instanceof Error ? caught.message : String(caught));
        // 读不到它的 catalog，基本就是离线了 —— 立刻把这条状态同步给所有界面
        props.onStale?.(id);
      })
      .finally(() => setLoading(false));
  });

  /** 改模版 → 顺手问一次预览（Rust 侧的纯函数命令，很快）。 */
  function onTemplateInput(value: string): void {
    setTemplate(value);
    void db
      .previewTemplate(value)
      .then(setPreview)
      .catch(() => setPreview(null));
  }

  /** 变量 chip：插到光标处（没聚焦就追加到末尾）。 */
  function insertVariable(name: string): void {
    const el = inputEl;
    const current = template();
    if (el === undefined) {
      onTemplateInput(current + name);
      return;
    }
    const start = el.selectionStart ?? current.length;
    const end = el.selectionEnd ?? start;
    const next = current.slice(0, start) + name + current.slice(end);
    onTemplateInput(next);
    // 光标落在插入的变量之后
    queueMicrotask(() => {
      el.focus();
      el.setSelectionRange(start + name.length, start + name.length);
    });
  }

  async function save(): Promise<void> {
    const id = props.repositoryId;
    if (id === null || saving()) return;
    setSaving(true);
    setError(null);
    try {
      const saved = await db.setRepositoryTemplate(id, template());
      props.onSaved?.(saved.importTemplate);
      props.onOpenChange(false);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setSaving(false);
    }
  }

  /** 真跑一次重建，把结果拼成一句人话（数字都在里面，不假装「已优化」）。 */
  async function runRebuild(): Promise<void> {
    const id = props.repositoryId;
    if (id === null || rebuilding()) return;
    setRebuildOpen(false);
    setRebuilding(true);
    setRebuilt(null);
    setRebuildProgress({ repositoryId: id, phase: "scan", done: 0, total: 0 });
    try {
      // 先订阅、后发命令：小库可能很快，反过来会漏掉第一批甚至全部事件。
      try {
        stopRebuildProgress?.();
        stopRebuildProgress = await db.onRebuildProgress((progress) => {
          if (progress.repositoryId !== id || props.repositoryId !== id) return;
          setRebuildProgress(progress.phase === "done" ? null : progress);
        });
      } catch {
        // 订阅失败不拦重建；按钮仍保持 loading，完成摘要仍由命令返回值给出。
        stopRebuildProgress = null;
      }
      const report = await db.rebuildRepository(id);
      const summary = t("repo.rebuild_done", {
        scanned: report.scanned,
        registered: report.registered,
        missing: report.missing,
        filled: report.metadataFilled,
        photos: report.photosCount,
        images: report.imagesCount,
      });
      // 运行期间允许关弹窗；若用户已经打开另一个库，不能把旧库结果写到新库面板里。
      if (props.repositoryId === id) {
        setCounts([report.photosCount, report.imagesCount]);
        setRebuilt(summary);
      }
      props.toast?.show({ tone: "success", message: summary });
    } catch (caught) {
      if (props.repositoryId === id) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    } finally {
      stopRebuildProgress?.();
      stopRebuildProgress = null;
      setRebuildProgress(null);
      setRebuilding(false);
    }
  }

  const rebuildProgressText = (): string | null => {
    const progress = rebuildProgress();
    if (progress === null) return null;
    const message = rebuildProgressMessage(progress);
    return "params" in message ? t(message.key, message.params) : t(message.key);
  };

  /** 相片数量：外面的视图优先（它刚更新过），没有就用自己读的 */
  const photosCount = (): number | null =>
    props.repository?.photosCount ?? counts()?.[0] ?? null;
  /** 图片数量（含 `_RAW`） */
  const imagesCount = (): number | null =>
    props.repository?.imagesCount ?? counts()?.[1] ?? null;

  /** 离线时不给保存：改了也写不进去（`repository_settings` 本身就会失败） */
  const offline = (): boolean => props.repository?.online === false;

  const canSave = () =>
    !saving() &&
    !offline() &&
    template().trim() !== "" &&
    (preview()?.ok ?? false);

  return (
    <Dialog
      open={props.open}
      onOpenChange={props.onOpenChange}
      title={t("repo.settings_title")}
      footer={
        <>
          <Button variant="secondary" onClick={() => props.onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={!canSave()}
            loading={saving()}
            onClick={() => void save()}
          >
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div class="flex flex-col gap-3">
        {/*
          离线：一行说明 + 保存按钮禁用。
          这条状态**不是这里自己判断的** —— 它来自中央状态里那一份（外面列表读的是同一份），
          所以「弹窗说离线、列表说在线」这种自相矛盾不会出现。
        */}
        <Show when={offline()}>
          <p class="flex items-start gap-1.5 text-fs-1 text-fg-2">
            <IconCloudOff size={14} class="mt-0.5 shrink-0" aria-hidden="true" />
            <span class="min-w-0">{t("repo.settings_offline")}</span>
          </p>
        </Show>
        <p class="text-fs-1 text-fg-2">
          {props.repositoryName === undefined
            ? t("repo.settings_hint")
            : t("repo.settings_hint_named", { name: props.repositoryName })}
        </p>

        {/*
          两个计数（人类 2026-09-19）：**相片数量**不含 `_RAW/`，**图片数量**含 ——
          卡片上只显示前者，这里两个都给（用户在这里才知道 `_RAW` 里还躺着多少）。
        */}
        <div class="flex flex-col gap-1 rounded-ui bg-surface-track p-2">
          <div class="flex items-center justify-between gap-2">
            <span class="text-fs-1 text-fg-2">{t("repo.counts_photos")}</span>
            <span class="text-fs-2 text-fg-1 tnum">
              {photosCount() === null ? "—" : formatCount(photosCount() ?? 0, locale())}
            </span>
          </div>
          <div class="flex items-center justify-between gap-2">
            <span class="text-fs-1 text-fg-2">{t("repo.counts_images")}</span>
            <span class="text-fs-2 text-fg-1 tnum">
              {imagesCount() === null ? "—" : formatCount(imagesCount() ?? 0, locale())}
            </span>
          </div>
          <p class="text-fs-0 text-fg-3">{t("repo.counts_hint")}</p>
        </div>

        {/* 重建数据：重扫整个库（文件对齐 + 元数据重读 + 计数重算） */}
        <div class="flex flex-col gap-1 rounded-ui bg-surface-track p-2">
          <div class="flex items-center justify-between gap-2">
            <div class="min-w-0">
              <p class="text-fs-1 text-fg-1">{t("repo.rebuild_title")}</p>
              <p class="text-fs-0 text-fg-3">{t("repo.rebuild_hint")}</p>
            </div>
            <Button
              variant="secondary"
              disabled={offline() || rebuilding()}
              loading={rebuilding()}
              onClick={() => setRebuildOpen(true)}
            >
              <IconRefresh size={14} aria-hidden="true" />
              {t("repo.rebuild_button")}
            </Button>
          </div>
          <Show when={rebuildProgressText()}>
            {(message) => (
              <p class="text-fs-0 text-brand" role="status" aria-live="polite">
                {message()}
              </p>
            )}
          </Show>
          <Show when={rebuilt()}>
            {(summary) => <p class="text-fs-0 text-fg-2">{summary()}</p>}
          </Show>
        </div>

        <label class="flex flex-col gap-1">
          <span class="text-fs-0 text-fg-2">{t("repo.template_label")}</span>
          <Input
            ref={(el: HTMLInputElement) => {
              inputEl = el;
            }}
            value={template()}
            disabled={loading()}
            onInput={(event) => onTemplateInput(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void save();
            }}
          />
        </label>

        {/* 变量 chip：点一下插到光标处 */}
        <div class="flex flex-wrap gap-1.5">
          <For each={VARIABLES}>
            {(name) => (
              <button
                type="button"
                class="cursor-pointer rounded-ui bg-surface-track px-1.5 py-0.5 text-fs-0 text-fg-2 hover:text-fg-1"
                onClick={() => insertVariable(name)}
              >
                {name}
              </button>
            )}
          </For>
        </div>

        {/* 预览（或错误）：示例照片会落到哪 */}
        <Show
          when={preview()?.ok}
          fallback={
            <Show when={preview()?.error ?? error()}>
              {(message) => <p class="text-fs-1 text-danger">{message()}</p>}
            </Show>
          }
        >
          <div class="flex flex-col gap-1 rounded-ui bg-surface-track p-2">
            <For each={preview()?.paths ?? []}>
              {(path) => <span class="text-fs-1 text-fg-3">{path}</span>}
            </For>
          </div>
        </Show>

        <Show when={(preview()?.warnings.length ?? 0) > 0}>
          <For each={preview()?.warnings ?? []}>
            {(warning) => <p class="text-fs-0 text-fg-3">{warning}</p>}
          </For>
        </Show>

        <Show when={error()}>
          {(message) => <p class="text-fs-1 text-danger">{message()}</p>}
        </Show>
      </div>

      {/*
        二级确认：**透明遮罩**（人类 2026-09-19）—— 一级窗口已经在压暗背景了，
        再叠一层半透就是越叠越黑。文案里把「要花时间」说清楚，别让用户以为卡住了。
      */}
      <ConfirmDialog
        open={rebuildOpen()}
        scrim={false}
        title={t("repo.rebuild_confirm_title")}
        message={t("repo.rebuild_confirm")}
        confirmLabel={t("repo.rebuild_confirm_ok")}
        onConfirm={() => void runRebuild()}
        onCancel={() => setRebuildOpen(false)}
      />
    </Dialog>
  );
}
