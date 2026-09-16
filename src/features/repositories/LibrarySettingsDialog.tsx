/**
 * `LibrarySettingsDialog` —— 库设置（画布 `Dialog / 库设置` `A9vPG`）。
 *
 * M1 里**只有一项**：导入模版。三件事让它好用：
 *
 * 1. **变量 chip 可点** —— 插到光标处，不用记拼写；
 * 2. **实时预览** —— 示例照片会落到哪，边打字边看（校验在 Rust 侧，一份规则不重写两遍）；
 * 3. **保存前先校验** —— 模版坏了根本不写进库（否则下次导入才发现）。
 */

import { createEffect, createSignal, For, Show } from "solid-js";
import * as db from "../../api/db.ts";
import type { TemplatePreview } from "../../api/types.ts";
import type { RepositoryView } from "../../api/types.ts";
import { Button } from "../../components/ui/Button.tsx";
import { IconCloudOff } from "@tabler/icons-solidjs";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { Input } from "../../components/ui/Form.tsx";
import { t } from "../../i18n/index.ts";

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
}

export function LibrarySettingsDialog(props: LibrarySettingsDialogProps) {
  const [template, setTemplate] = createSignal("");
  const [preview, setPreview] = createSignal<TemplatePreview | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  let inputEl: HTMLInputElement | undefined;

  // 打开时读一次当前模版
  createEffect(() => {
    if (!props.open) return;
    const id = props.repositoryId;
    if (id === null) return;
    setLoading(true);
    setError(null);
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
    </Dialog>
  );
}
