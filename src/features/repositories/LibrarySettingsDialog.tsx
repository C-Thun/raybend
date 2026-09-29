/**
 * `LibrarySettingsDialog` —— 库位置、数量、重建与导入模版。
 * W1 画稿 `ZhVcs`：离线仍可管理登记位置，仅 catalog 操作需要连接。
 * 模版校验与预览沿用 Rust 同一套规则；位置与连接事实由中央状态提供。
 */

import { createEffect, createMemo, createSignal, For, on, onCleanup, Show } from "solid-js";
import * as db from "../../api/db.ts";
import type { RebuildProgress, RepositoryView, TemplatePreview } from "../../api/types.ts";
import { Button } from "../../components/ui/Button.tsx";
import type { ToastStore } from "../../components/ui/toast.ts";
import { IconBan, IconCloudOff, IconRefresh } from "@tabler/icons-solidjs";
import { ConfirmDialog, Dialog } from "../../components/ui/Dialog.tsx";
import { Input } from "../../components/ui/Form.tsx";
import { locale, t } from "../../i18n/index.ts";
import { formatCount } from "../../lib/format.ts";
import { pickDirectory } from "../../api/dialog.ts";
import { EasyCopy } from "../../components/ui/EasyCopy.tsx";
import { samePath } from "../../lib/tree.ts";
import { locationErrorKey } from "../../i18n/repository-feedback.ts";
import type { RepositoryStateStore } from "./state.ts";
import { createLocationController } from "./location-controller.ts";
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
  repositories?: RepositoryStateStore;
  locateRequest?: number;
  releaseRequest?: number;
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

  const [locationBusy, setLocationBusy] = createSignal(false);
  const [locationError, setLocationError] = createSignal<unknown | null>(null);
  const [removePath, setRemovePath] = createSignal<string | null>(null);
  const [usePath, setUsePath] = createSignal<string | null>(null);
  const [releaseOpen, setReleaseOpen] = createSignal(false);
  const released = () => props.repository?.connection?.state === "released";
  const releasing = () => props.repository?.connection?.state === "releasing";
  async function manageLocation(path: string | null): Promise<void> {
    const id = props.repositoryId; if (id === null || changingLocation()) return;
    const epoch=dialogEpoch; setLocationBusy(true); setLocationError(null);
    try {
      const view = path === null ? await (props.repositories?.release(id) ?? db.releaseRepository(id)) : await (props.repositories?.useLocation(id,path) ?? db.useRepositoryLocation(id,path));
      if (epoch === dialogEpoch) props.repositories?.upsert(view);
    } catch (caught) { if (epoch === dialogEpoch) setLocationError(caught); }
    finally { if (epoch === dialogEpoch) setLocationBusy(false); }
  }
  let dialogEpoch = 0;
  const locations = createLocationController({
    target: () => props.open ? props.repositoryId : null,
    pick: () => pickDirectory({ title: t("repo.locations.choose") }),
    add: (id, path) => props.repositories ? props.repositories.addLocation(id, path) : db.addRepositoryLocation(id, path),
    remove: (id, path) => props.repositories ? props.repositories.removeLocation(id, path) : db.removeRepositoryLocation(id, path),
    busy: setLocationBusy,
    error: setLocationError,
  });
  createEffect(() => {
    const open = props.open, id = props.repositoryId;
    void open; void id;
    dialogEpoch++;
    locations.reset(); setRemovePath(null); setUsePath(null); setReleaseOpen(false); setSaving(false);
  });
  createEffect(on(() => props.locateRequest ?? 0, (request, previous) => {
    if (request > 0 && request !== previous && props.open) void locations.choose();
  }, { defer: true }));
  createEffect(on(() => props.releaseRequest ?? 0, (value, previous) => { if (value > 0 && value !== previous && props.open) setReleaseOpen(true); }, {defer:true}));
  onCleanup(locations.reset);
  const changingLocation = () => locationBusy() || (props.repositoryId !== null && props.repositories?.isChangingLocation(props.repositoryId) === true);
  const currentPath = (path: string) => props.repository?.online === true && !!props.repository.root && samePath(path, props.repository.root);

  let draftId: string | null = null, dirty = false;
  const settingsContext = createMemo(() => props.open && props.repositoryId !== null ? `${props.repositoryId}:${props.repository?.online === true}` : null);
  createEffect(on(settingsContext, context => {
    if (context === null) { draftId = null; return; }
    const id = props.repositoryId!;
    let stale = false;
    onCleanup(() => { stale = true; });
    const current = () => !stale && props.open && props.repositoryId === id;
    if (draftId !== id) {
      draftId = id; dirty = false;
      setTemplate(props.repository?.importTemplate ?? ""); setPreview(null); setCounts(null);
      setRebuildProgress(null); setRebuilt(null); setError(null);
    }
    void db.repositoryCounts(id).then(value => { if (current()) setCounts(value); }).catch(() => {});
    if (props.repository?.online !== true) { setLoading(false); setError(null); return; }
    setLoading(true); setError(null);
    void db.repositorySettings(id).then(async settings => {
      if (!current()) return;
      if (!dirty) setTemplate(settings.importTemplate);
      const value = await db.previewTemplate(template()).catch(() => null);
      if (current()) setPreview(value);
    }).catch(() => {
      if (current()) { setError(t("repo.location_error.connection_lost")); props.onStale?.(id); }
    }).finally(() => { if (current()) setLoading(false); });
  }));

  /** 改模版 → 顺手问一次预览（Rust 侧的纯函数命令，很快）。 */
  function onTemplateInput(value: string): void {
    dirty = true;
    setTemplate(value);
    const id = props.repositoryId;
    void db
      .previewTemplate(value)
      .then(result => { if (props.open && props.repositoryId === id && template() === value) setPreview(result); })
      .catch(() => { if (props.open && props.repositoryId === id && template() === value) setPreview(null); });
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
    if (id === null || !canSave()) return;
    const epoch = dialogEpoch;
    const current = () => epoch === dialogEpoch && props.open && props.repositoryId === id;
    setSaving(true);
    setError(null);
    try {
      const saved = await db.setRepositoryTemplate(id, template());
      if (!current()) return;
      props.onSaved?.(saved.importTemplate);
      props.onOpenChange(false);
    } catch (caught) {
      if (current()) setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      if (current()) setSaving(false);
    }
  }

  /** 真跑一次重建，把结果拼成一句人话（数字都在里面，不假装「已优化」）。 */
  async function runRebuild(): Promise<void> {
    const id = props.repositoryId;
    if (id === null || offline() || rebuilding()) return;
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
  const offline = (): boolean => props.repository?.online !== true;

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
      class="max-h-[calc(100dvh-2rem)] overflow-y-auto"
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
        <div class="flex flex-col gap-2 rounded-ui bg-surface-track p-3" data-repository-locations>
          <p class="text-fs-2 text-fg-1">{t("repo.locations.title")}</p>
          <p class="text-fs-1 text-fg-2">{t("repo.locations.hint")}</p>
          <Show when={offline()}><p class="flex items-center gap-1.5 text-fs-1 text-fg-2"><IconCloudOff size={14} />{t("repo.settings_offline")}</p></Show>
          <For each={props.repository?.paths ?? []} fallback={<p class="text-fs-1 text-fg-3">{t("repo.locations.empty")}</p>}>
            {position => <div class="flex min-w-0 items-center gap-2" data-repository-position={position.path}>
              <div class="min-w-0 flex-1">
                <EasyCopy value={position.path} class="max-w-full"><span class="min-w-0 break-all text-fs-1" title={position.path}>{position.path}</span></EasyCopy>
                <p class="text-fs-0 text-fg-3">{t(currentPath(position.path) ? "repo.locations.current" : position.status === "offline" ? "repo.locations.missing" : position.status === "online" ? "repo.locations.available" : "repo.locations.unknown")}</p>
              </div>
              <Show when={!currentPath(position.path) && !releasing()}>
                <Button variant="secondary" size="sm" disabled={changingLocation()} onClick={() => setUsePath(position.path)}>{t("repo.locations.use")}</Button>
              </Show>
              <span title={currentPath(position.path) ? t("repo.locations.in_use") : t("repo.locations.remove")}>
                <Button variant="ghost" size="sm" disabled={changingLocation() || currentPath(position.path)} aria-label={t("repo.locations.remove")}
                  onClick={event => { if (event.shiftKey) void locations.remove(position.path); else setRemovePath(position.path); }}>
                  <IconBan size={14} />
                </Button>
              </span>
            </div>}
          </For>
          <div class="flex gap-2">
            <Button variant="primary" loading={changingLocation()} onClick={() => void locations.choose()} data-repository-locate>
              {t(offline() ? "repo.locations.locate" : "repo.locations.add")}
            </Button>
            <Button variant="secondary" disabled={releasing() || changingLocation() || (props.repositoryId !== null && props.repositories?.isRemounting(props.repositoryId) === true)}
              onClick={() => { const id = props.repositoryId; if (id !== null) { if (props.repositories) void props.repositories.remount(id); else props.onStale?.(id); } }}>
              {t("repo.remount")}
            </Button>
          </div>
          <Show when={locationError() !== null}><p role="status" class="text-fs-1 text-fg-2">{t(locationErrorKey(locationError()))}</p></Show>
        </div>

        <div class="flex items-center gap-3 rounded-ui bg-surface-track p-3" data-repository-release>
          <p class="min-w-0 flex-1 text-fs-1 text-fg-2">{t(releasing() ? "repo.connection.releasing" : released() ? "repo.connection.released" : "repo.release_hint")}</p>
          <Show when={!released()}><Button variant="secondary" loading={releasing()} disabled={changingLocation() || releasing()} onClick={() => setReleaseOpen(true)}>{t("repo.release")}</Button></Show>
        </div>

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
            disabled={loading() || offline()}
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
                disabled={offline() || loading()}
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
            <Show when={!offline() && (preview()?.error ?? error())}>
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

        <Show when={!offline() && error()}>
          {(message) => <p class="text-fs-1 text-danger">{message()}</p>}
        </Show>
      </div>

      <ConfirmDialog open={usePath() !== null} scrim={false} title={t("repo.locations.use")}
        message={t("repo.locations.use_confirm", {path:usePath() ?? ""})} confirmLabel={t("common.confirm")}
        onCancel={() => setUsePath(null)} onConfirm={() => { const path=usePath(); setUsePath(null); if (path !== null) void manageLocation(path); }} />
      <ConfirmDialog open={releaseOpen()} scrim={false} title={t("repo.release")} message={t("repo.release_confirm")} confirmLabel={t("repo.release")}
        onCancel={() => setReleaseOpen(false)} onConfirm={() => { setReleaseOpen(false); void manageLocation(null); }} />
      <ConfirmDialog open={removePath() !== null} scrim={false}
        title={t("repo.locations.remove_title")} message={t("repo.locations.remove_confirm", { path: removePath() ?? "" })}
        confirmLabel={t("repo.locations.remove")} onCancel={() => setRemovePath(null)}
        onConfirm={() => { const path = removePath(); setRemovePath(null); if (path !== null) void locations.remove(path); }} />

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
