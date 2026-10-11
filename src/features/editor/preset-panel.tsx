/**
 * 编辑右栏第 3 组的「预设」页签（`design/editor.md` §3.10、`specs/editor-presets.md` §5.3）。
 *
 * ```text
 * [▶ 应用] [＋目录] [＋预设]        ← 工具行：三个图标按钮 + Tooltip（照 LUT 标题行）
 * ▼ 默认 (3)                        ← 目录行：chevron 折叠 + 名称 + 计数；空目录才行尾删除
 *    □ 柔和胶片                      ← 预设行：点选（Shift 多选）；悬浮行尾删除 + 信息气泡
 *    □ 街头黑金             ⃠
 * ▶ 人像 (2)
 * ```
 *
 * 拖拽复用 `lib/pointer-drag.ts`（阈值 + 越窗取消）；拖起后**所有目录临时折叠**、
 * 释放后还原（展开记录从未被改，`dragging` 一关就自动回去），再展开移入的目录。
 */

import { For, Show, createSignal, onCleanup, type JSX } from "solid-js";
import { Portal } from "solid-js/web";
import {
  IconBookmark,
  IconBookmarkPlus,
  IconChevronDown,
  IconChevronRight,
  IconFolderPlus,
  IconPencil,
  IconPlayerPlay,
  IconStack2,
} from "@tabler/icons-solidjs";

import { Button } from "../../components/ui/Button.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { EasyDestroyButton } from "../../components/ui/EasyDestroy.tsx";
import { IconButton } from "../../components/ui/Button.tsx";
import { Tooltip } from "../../components/ui/Tooltip.tsx";
import { t, type MessageKey } from "../../i18n/index.ts";
import { selectAllOnMount } from "../../lib/dom-focus.ts";
import { trackPointerDrag } from "../../lib/pointer-drag.ts";
import {
  DEFAULT_DIRECTORY_ID,
  PRESET_GROUPS,
  PRESET_NAME_MAX,
  DEFAULT_PRESET_GROUPS,
  isDirectoryNameTaken,
  isPresetNameTaken,
  resolveCreateDirectory,
  snapshotGroups,
  validatePresetName,
  type PresetMutationOutcome,
  type PresetDirectory,
  type PresetGroup,
  type PresetNameError,
  type PresetNameKind,
  type PresetRecord,
  type PresetSnapshot,
} from "../../lib/presets.ts";
import type { EditorStore } from "./store.ts";

/** 大类 → 页签文案 key（与右栏第 2 组四个页签共用语言，curve/lut 预设自带）。 */
const GROUP_LABEL_KEY: Record<PresetGroup, MessageKey> = {
  tone: "editor.preset.group.tone",
  color: "editor.preset.group.color",
  detail: "editor.preset.group.detail",
  lens: "editor.preset.group.lens",
  curve: "editor.preset.group.curve",
  lut: "editor.preset.group.lut",
  colorManagement: "editor.colorManagement.title",
};

/*
 * 树区高度由第 3 组等高容器给定（`panels.tsx` `data-editor-advanced-body`，
 * 高度 = 曲线页签的自然高度）：这里 `flex-1` 填充、内部滚动，
 * 预设再多也不把右栏撑长（审计 2026-10-04）。
 */
export interface PresetPanelProps {
  store: EditorStore;
  /** 有没有可编辑的照片（空态下应用 / 新建预设禁用；目录管理不受影响） */
  enabled: boolean;
  /** 应用 / 新建后落库（与 LUT 选择的 `onSelect → confirmEdit` 同一条路） */
  onConfirm?: () => void;
  applyPreset: (snapshot: PresetSnapshot) => Promise<void>;
  createDirectory: (name: string) => Promise<PresetMutationOutcome>;
  createPreset: (name: string, directoryId: string, groups: readonly PresetGroup[]) => Promise<PresetMutationOutcome>;
  renameDirectory: (id: string, name: string) => Promise<PresetMutationOutcome>;
  deletePreset: (id: string) => Promise<void>;
  deleteDirectory: (id: string) => Promise<void>;
  movePresets: (ids: readonly string[], directoryId: string) => Promise<void>;
  class?: string;
}

export function PresetPanel(props: PresetPanelProps): JSX.Element {
  /* ── 新建目录弹窗 ─────────────────────────────────── */
  const [dirOpen, setDirOpen] = createSignal(false);
  const [dirDraft, setDirDraft] = createSignal("");
  const [dirError, setDirError] = createSignal<string | null>(null);

  /* 重命名目录弹窗（默认目录不可改名，不会打开） */
  const [renameDirTarget, setRenameDirTarget] = createSignal<PresetDirectory | null>(null);
  const [renameDirDraft, setRenameDirDraft] = createSignal("");
  const [renameDirError, setRenameDirError] = createSignal<string | null>(null);

  /* ── 新建预设弹窗 ─────────────────────────────────── */
  const [presetOpen, setPresetOpen] = createSignal(false);
  const [presetDraft, setPresetDraft] = createSignal("");
  const [presetGroups, setPresetGroups] = createSignal<PresetGroup[]>([...DEFAULT_PRESET_GROUPS]);
  const [presetError, setPresetError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  /** 前端名称校验的原因文案（与后端同一口径，审计 2026-10-04）。 */
  const nameErrorText = (error: PresetNameError, kind: PresetNameKind): string =>
    error === "tooLong"
      ? t("editor.preset.nameTooLong").replace("{n}", String(PRESET_NAME_MAX[kind]))
      : t("editor.preset.nameInvalid");

  /* ── 拖拽（多选整组移动 + 边缘自动滚动）────────────── */
  const [dragging, setDragging] = createSignal(false);
  const [dragPoint, setDragPoint] = createSignal<{ x: number; y: number } | null>(null);
  const [dropTarget, setDropTarget] = createSignal<string | null>(null);
  const [dragLabel, setDragLabel] = createSignal("");
  let scrollHost: HTMLDivElement | undefined;
  let dragFrame = 0;
  let cancelDrag: (() => void) | null = null;
  let suppressDragClick = false;

  const onDragKeyDown = (event: KeyboardEvent): void => {
    if (event.key !== "Escape" || cancelDrag === null) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    cancelDrag();
  };

  const directories = () => props.store.presetDirectories();
  const presets = () => props.store.presets();
  const selection = () => props.store.presetSelection();

  const dirLabel = (directory: PresetDirectory): string =>
    directory.id === DEFAULT_DIRECTORY_ID ? t("editor.preset.defaultDir") : directory.name;

  const entriesOf = (directoryId: string): PresetRecord[] =>
    presets().filter((preset) => preset.directoryId === directoryId);

  const selectedSingle = (): PresetRecord | null => {
    const current = selection();
    if (current?.kind !== "presets" || current.ids.length !== 1) return null;
    return presets().find((preset) => preset.id === current.ids[0]) ?? null;
  };
  const canApply = (): boolean => props.enabled && selectedSingle() !== null;

  /** 悬浮信息：这个预设包含了哪些大类（specs §5.3，照抄全系统 tooltip）。 */
  const containsLabel = (snapshot: PresetSnapshot): string => {
    const groups = snapshotGroups(snapshot);
    if (groups.length === 0) return t("editor.preset.containsNone");
    return t("editor.preset.contains")
      .replace("{groups}", groups.map((group) => t(GROUP_LABEL_KEY[group])).join(" · "));
  };

  const applySelected = (): void => {
    const record = selectedSingle();
    if (record === null) return;
    void props.applyPreset(record.payload);
  };

  /* ── 新建目录 ─────────────────────────────────────── */
  const submitDirectory = async (): Promise<void> => {
    if (busy()) return;
    const name = dirDraft().trim();
    const problem = validatePresetName(name, "directory");
    if (problem !== null) {
      setDirError(nameErrorText(problem, "directory"));
      return;
    }
    if (isDirectoryNameTaken(name, directories())) {
      setDirError(t("editor.preset.dirExists"));
      return;
    }
    setBusy(true);
    try {
      const outcome = await props.createDirectory(name);
      if (outcome.ok) setDirOpen(false);
      else setDirError(outcome.message);
    } finally {
      setBusy(false);
    }
  };

  /* ── 新建预设 ─────────────────────────────────────── */
  /* ── 重命名目录 ───────────────────────────────── */
  const openRenameDirectory = (directory: PresetDirectory): void => {
    setRenameDirTarget(directory);
    setRenameDirDraft(directory.name);
    setRenameDirError(null);
  };

  const closeRenameDirectory = (): void => {
    setRenameDirTarget(null);
    setRenameDirDraft("");
    setRenameDirError(null);
  };

  const submitRenameDirectory = async (): Promise<void> => {
    const target = renameDirTarget();
    if (busy() || target === null) return;
    const name = renameDirDraft().trim();
    const problem = validatePresetName(name, "directory");
    if (problem !== null) {
      setRenameDirError(nameErrorText(problem, "directory"));
      return;
    }
    // 名字没变：不当成一次改名（也免得白跑一趟后端）
    if (name === target.name) {
      closeRenameDirectory();
      return;
    }
    if (isDirectoryNameTaken(
      name,
      directories().filter((directory) => directory.id !== target.id),
    )) {
      setRenameDirError(t("editor.preset.dirExists"));
      return;
    }
    setBusy(true);
    try {
      const outcome = await props.renameDirectory(target.id, name);
      if (outcome.ok) closeRenameDirectory();
      else setRenameDirError(outcome.message);
    } finally {
      setBusy(false);
    }
  };

  const submitPreset = async (): Promise<void> => {
    if (busy()) return;
    const name = presetDraft().trim();
    if (name === "" || presetGroups().length === 0) return;
    const problem = validatePresetName(name, "preset");
    if (problem !== null) {
      setPresetError(nameErrorText(problem, "preset"));
      return;
    }
    // 落点：选中的目录 → 选中预设所在目录 → default（specs §5.3）
    const directoryId = resolveCreateDirectory(selection(), directories(), presets());
    if (isPresetNameTaken(name, directoryId, presets())) {
      setPresetError(t("editor.preset.exists"));
      return;
    }
    setBusy(true);
    try {
      const outcome = await props.createPreset(name, directoryId, presetGroups());
      if (outcome.ok) setPresetOpen(false);
      else setPresetError(outcome.message);
    } finally {
      setBusy(false);
    }
  };

  /* ── 拖拽实现 ─────────────────────────────────────── */
  const updateTarget = (point: { x: number; y: number }): void => {
    if (scrollHost === undefined) return;
    const headers = scrollHost.querySelectorAll<HTMLElement>("[data-preset-dir-header]");
    let target: string | null = null;
    for (const element of headers) {
      const rect = element.getBoundingClientRect();
      if (point.x >= rect.left && point.x <= rect.right && point.y >= rect.top - 4 && point.y <= rect.bottom + 4) {
        target = element.dataset.presetDirHeader ?? null;
        break;
      }
    }
    setDropTarget(target);
  };

  /** 接近滚动区上/下缘且没到头 → 持续步进滚动（指针停住也继续滚）。 */
  const tickAutoScroll = (): void => {
    const point = dragPoint();
    if (scrollHost !== undefined && point !== null) {
      const rect = scrollHost.getBoundingClientRect();
      const edge = 24;
      let dy = 0;
      if (point.y < rect.top + edge && scrollHost.scrollTop > 0) dy = -8;
      else if (point.y > rect.bottom - edge
        && scrollHost.scrollTop + scrollHost.clientHeight < scrollHost.scrollHeight) dy = 8;
      if (dy !== 0) {
        scrollHost.scrollTop += dy;
        updateTarget(point);
      }
    }
    dragFrame = requestAnimationFrame(tickAutoScroll);
  };

  const startRowDrag = (event: PointerEvent, preset: PresetRecord): void => {
    if (event.button !== 0) return;
    cancelDrag?.();
    // 多选时拖动选中成员 = 整组移动；否则只拖这一个
    const current = selection();
    const ids = current?.kind === "presets" && current.ids.includes(preset.id)
      ? [...current.ids]
      : [preset.id];
    const names = ids
      .map((id) => presets().find((entry) => entry.id === id)?.name ?? "")
      .filter((name) => name !== "");
    cancelDrag = trackPointerDrag(event, {
      threshold: 4,
      capture: true,
      cancelOutsideWindow: true,
      start: (point) => {
        setDragging(true);
        setDragPoint(point);
        setDragLabel(names.length > 1 ? `${names[0]} +${names.length - 1}` : (names[0] ?? ""));
        dragFrame = requestAnimationFrame(tickAutoScroll);
      },
      move: (point) => {
        setDragPoint(point);
        updateTarget(point);
      },
      end: (_point, cancelled, started) => {
        cancelDrag = null;
        window.removeEventListener("keydown", onDragKeyDown, true);
        if (dragFrame !== 0) cancelAnimationFrame(dragFrame);
        dragFrame = 0;
        const target = dropTarget();
        setDragging(false);
        setDragPoint(null);
        setDropTarget(null);
        // Pointer capture may dispatch a click to the source button after drop.
        // It is a drag completion, not a request to replace the multi-selection.
        if (started) {
          suppressDragClick = true;
          window.setTimeout(() => { suppressDragClick = false; }, 0);
        }
        if (!started || cancelled || target === null) return;
        const moving = ids.filter(
          (id) => presets().find((entry) => entry.id === id)?.directoryId !== target,
        );
        if (moving.length === 0) return;
        void props.movePresets(moving, target);
      },
    });
    window.addEventListener("keydown", onDragKeyDown, true);
  };
  onCleanup(() => {
    cancelDrag?.();
    window.removeEventListener("keydown", onDragKeyDown, true);
    if (dragFrame !== 0) cancelAnimationFrame(dragFrame);
  });

  /* ── 渲染 ─────────────────────────────────────────── */
  return (
    <div
      data-editor-preset-panel
      class={["flex min-h-0 flex-col", props.class ?? ""].filter(Boolean).join(" ")}
    >
      {/* 工具行：应用 / 新建目录 / 新建预设（三个同款 ghost 图标按钮） */}
      <div class="flex shrink-0 items-center gap-1 pb-1.5">
        {/*
         * 应用按钮**不做主色常亮**（崔总 2026-10-10：常亮会让人误以为“还有东西要确认”）。
         * 禁用时按钮自己 `pointer-events-none`，所以 Tooltip 的触发器挂在外层 span 上，
         * 那样悬停才收得到 —— 顺手把“为什么不能用”说清楚。
         */}
        <Tooltip content={canApply() ? t("editor.preset.apply") : t("editor.preset.applyDisabled")}>
          {(triggerProps) => (
            <span {...triggerProps()} class="inline-flex">
              <Button
                variant="ghost"
                disabled={!canApply()}
                aria-label={t("editor.preset.apply")}
                icon={<IconPlayerPlay size={14} />}
                onClick={applySelected}
              />
            </span>
          )}
        </Tooltip>
        <Tooltip content={t("editor.preset.newDirectory")}>
          {(triggerProps) => (
            <Button
              {...triggerProps()}
              variant="ghost"
              aria-label={t("editor.preset.newDirectory")}
              icon={<IconFolderPlus size={14} />}
              onClick={() => {
                setDirDraft("");
                setDirError(null);
                setDirOpen(true);
              }}
            />
          )}
        </Tooltip>
        <Tooltip content={t("editor.preset.newPreset")}>
          {(triggerProps) => (
            <Button
              {...triggerProps()}
              variant="ghost"
              disabled={!props.enabled}
              aria-label={t("editor.preset.newPreset")}
              icon={<IconBookmarkPlus size={14} />}
              onClick={() => {
                setPresetDraft("");
                setPresetError(null);
                setPresetGroups([...DEFAULT_PRESET_GROUPS]);
                setPresetOpen(true);
              }}
            />
          )}
        </Tooltip>
      </div>

      {/* 可滚动的目录 / 预设树：父容器已限定高度，这里 flex 填充并自滚 */}
      <div
        ref={scrollHost}
        data-preset-tree
        class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto pb-1"
      >
        <For each={directories()}>
          {(directory) => {
            const entries = () => entriesOf(directory.id);
            const dirSelected = () => {
              const current = selection();
              return current?.kind === "directory" && current.id === directory.id;
            };
            // 拖动中一律显示折叠（展开记录没动，`dragging` 一关自动还原）
            const expanded = () => !dragging() && props.store.presetExpanded(directory.id);
            return (
              <div class="flex flex-col" data-preset-directory={directory.id}>
                <div
                  data-preset-dir-header={directory.id}
                  class="group flex h-row-h items-center gap-0.5 rounded-ui pl-0.5 pr-1.5"
                  classList={{
                    "bg-state-selected": dirSelected(),
                    "bg-surface-bar": !dirSelected(),
                    "ring-1 ring-brand": dragging() && dropTarget() === directory.id,
                  }}
                >
                  <button
                    type="button"
                    class="flex h-row-h w-5 shrink-0 items-center justify-center rounded-ui hover:bg-state-hover"
                    aria-expanded={expanded()}
                    aria-label={t("editor.preset.toggleDirectory").replace("{name}", dirLabel(directory))}
                    onClick={() => props.store.togglePresetDirectory(directory.id)}
                  >
                    <Show
                      when={expanded()}
                      fallback={<IconChevronRight size={12} class="text-fg-3" />}
                    >
                      <IconChevronDown size={12} class="text-fg-3" />
                    </Show>
                  </button>
                  <button
                    type="button"
                    class="flex min-w-0 flex-1 items-center rounded-ui text-left"
                    onClick={() => props.store.selectPresetDirectory(directory.id)}
                  >
                    {/* 名称超长在右缘渐隐（不是省略号）：与全局收尾纪律一致 */}
                    <span class="min-w-0 flex-1 overflow-hidden whitespace-nowrap fade-x-end text-fs-2 text-fg-1">
                      {dirLabel(directory)}
                    </span>
                  </button>
                  {/*
                   * 最右动作位**固定占宽**（崔总 2026-10-10）：默认目录不可删 / 不可改名，
                   * 也照样留着空位 —— 各行按钮横坐标一致，目录之间不跳。
                   * 空目录 = 删除；有预设 = 改名（默认目录两者都不显示）。
                   */}
                  <span class="flex size-remove-hit shrink-0 items-center justify-center">
                    <Show when={!dragging() && directory.id !== DEFAULT_DIRECTORY_ID}>
                      <Show
                        when={entries().length === 0}
                        fallback={
                          <IconButton
                            size="hit"
                            class="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
                            label={t("editor.preset.renameDir")}
                            onClick={() => openRenameDirectory(directory)}
                          >
                            <IconPencil size="var(--remove-icon)" aria-hidden="true" />
                          </IconButton>
                        }
                      >
                        <EasyDestroyButton
                          class="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
                          label={t("editor.preset.removeDir")}
                          confirmTitle={t("editor.preset.removeDirTitle")}
                          confirmMessage={t("editor.preset.removeDirConfirm").replace("{name}", dirLabel(directory))}
                          onRemove={() => props.deleteDirectory(directory.id)}
                        />
                      </Show>
                    </Show>
                  </span>
                </div>

                <Show when={expanded()}>
                  <Show
                    when={entries().length > 0}
                    fallback={
                      <p class="px-6 py-2 text-fs-0 text-fg-3" data-preset-directory-empty>
                        {t("editor.preset.emptyDirectory")}
                      </p>
                    }
                  >
                    <For each={entries()}>
                      {(preset) => {
                        const selected = () => {
                          const current = selection();
                          return current?.kind === "presets" && current.ids.includes(preset.id);
                        };
                        return (
                          <div
                            class="group flex h-row-h items-center gap-0.5 rounded-ui pl-0.5 pr-1.5"
                            classList={{
                              "bg-state-selected": selected(),
                              "hover:bg-state-hover": !selected(),
                            }}
                            data-preset-row={preset.id}
                          >
                            <Tooltip content={containsLabel(preset.payload)} openDelay={300}>
                              {(triggerProps) => (
                                <button
                                  type="button"
                                  {...triggerProps()}
                                  class="flex min-w-0 flex-1 items-center gap-1.5 rounded-ui text-left"
                                  onClick={(event) => {
                                    if (suppressDragClick) {
                                      suppressDragClick = false;
                                      event.preventDefault();
                                      return;
                                    }
                                    props.store.selectPreset(preset.id, event);
                                  }}
                                  onDblClick={() => {
                                    // 双击 = 直接应用这一条（崔总 2026-10-10）；与工具栏「应用」同一条路
                                    // （都走 `props.applyPreset` → 落库 confirm）
                                    if (props.enabled && !suppressDragClick) {
                                      void props.applyPreset(preset.payload);
                                    }
                                  }}
                                  onPointerDown={(event) => startRowDrag(event, preset)}
                                >
                                  <IconBookmark size={14} class="shrink-0 text-fg-2" aria-hidden="true" />
                                  <span class="min-w-0 flex-1 overflow-hidden whitespace-nowrap fade-x-end text-fs-2 text-fg-1">
                                    {preset.name}
                                  </span>
                                </button>
                              )}
                            </Tooltip>
                            <Show when={!dragging()}>
                              <EasyDestroyButton
                                class="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
                                label={t("editor.preset.remove")}
                                confirmTitle={t("editor.preset.removeTitle")}
                                confirmMessage={t("editor.preset.removeConfirm").replace("{name}", preset.name)}
                                onRemove={() => props.deletePreset(preset.id)}
                              />
                            </Show>
                          </div>
                        );
                      }}
                    </For>
                  </Show>
                </Show>
              </div>
            );
          }}
        </For>

        <Show when={presets().length === 0}>
          <div class="flex flex-1 flex-col items-center justify-center gap-2 px-4 py-8 text-center" data-preset-empty>
            <IconStack2 size={48} stroke-width={1} class="text-fg-3 opacity-60" aria-hidden="true" />
            <p class="text-fs-2 text-fg-2">{t("editor.preset.empty")}</p>
            <p class="text-fs-0 text-fg-3">{t("editor.preset.emptyHint")}</p>
          </div>
        </Show>
      </div>

      {/* 拖动幽灵：跟随指针的行快照（浮层底 + 主色描边，照画稿 nxMA2） */}
      <Show when={dragPoint() !== null}>
        <Portal>
          <div
            class="pointer-events-none fixed z-50 flex h-row-h items-center gap-1.5 rounded-ui bg-surface-layer px-2 text-fs-2 text-fg-1 ring-1 ring-brand"
            style={{
              left: `${dragPoint()!.x + 12}px`,
              top: `${dragPoint()!.y + 8}px`,
            }}
            data-preset-drag-ghost
          >
            <IconBookmark size={14} class="text-fg-2" aria-hidden="true" />
            <span>{dragLabel()}</span>
          </div>
        </Portal>
      </Show>

      {/* 新建目录：小模态（照 LUT 新建分类） */}
      <Dialog
        open={dirOpen()}
        onOpenChange={setDirOpen}
        title={t("editor.preset.newDirTitle")}
        footer={
          <>
            <Button variant="secondary" onClick={() => setDirOpen(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              disabled={busy() || dirDraft().trim() === ""}
              onClick={() => void submitDirectory()}
            >
              {t("common.confirm")}
            </Button>
          </>
        }
      >
        <label class="flex flex-col gap-1.5">
          <span class="text-fs-2 text-fg-2">{t("editor.preset.dirName")}</span>
          <input
            data-preset-dir-input
            class="h-row-h rounded-ui bg-surface-track px-2 text-fs-2 text-fg-1 outline-none focus-visible:ring-1 focus-visible:ring-focus-ring"
            placeholder={t("editor.preset.dirPlaceholder")}
            value={dirDraft()}
            autofocus
            onInput={(event) => {
              setDirDraft(event.currentTarget.value);
              setDirError(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.isComposing) {
                event.preventDefault();
                void submitDirectory();
              }
            }}
          />
        </label>
        <Show when={dirError()}>
          <p data-preset-dir-error class="mt-1.5 text-fs-0 text-danger">
            {dirError()}
          </p>
        </Show>
      </Dialog>

      {/* 重命名目录：与新建同形；打开就聚焦并**全选**原名（直接重打） */}
      <Dialog
        open={renameDirTarget() !== null}
        onOpenChange={(open) => { if (!open) closeRenameDirectory(); }}
        title={t("editor.preset.renameDirTitle")}
        footer={
          <>
            <Button variant="secondary" onClick={closeRenameDirectory}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              disabled={busy() || renameDirDraft().trim() === ""}
              onClick={() => void submitRenameDirectory()}
            >
              {t("common.save")}
            </Button>
          </>
        }
      >
        <label class="flex flex-col gap-1.5">
          <span class="text-fs-2 text-fg-2">{t("editor.preset.dirName")}</span>
          <input
            data-preset-dir-rename-input
            class="h-row-h rounded-ui bg-surface-track px-2 text-fs-2 text-fg-1 outline-none focus-visible:ring-1 focus-visible:ring-focus-ring"
            placeholder={t("editor.preset.dirPlaceholder")}
            value={renameDirDraft()}
            autofocus
            ref={selectAllOnMount}
            onInput={(event) => {
              setRenameDirDraft(event.currentTarget.value);
              setRenameDirError(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.isComposing) {
                event.preventDefault();
                void submitRenameDirectory();
              }
            }}
          />
        </label>
        <Show when={renameDirError()}>
          <p data-preset-dir-rename-error class="mt-1.5 text-fs-0 text-danger">
            {renameDirError()}
          </p>
        </Show>
      </Dialog>

      {/* 新建预设：名称 + 大类 chips（默认不含镜头 / 色彩管理；照导入 LUT 弹窗的 chip 语言） */}
      <Dialog
        open={presetOpen()}
        onOpenChange={setPresetOpen}
        title={t("editor.preset.newPresetTitle")}
        footer={
          <>
            <Button variant="secondary" onClick={() => setPresetOpen(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              disabled={busy() || presetDraft().trim() === "" || presetGroups().length === 0}
              onClick={() => void submitPreset()}
            >
              {t("common.save")}
            </Button>
          </>
        }
      >
        <label class="flex flex-col gap-1.5">
          <span class="text-fs-2 text-fg-2">{t("editor.preset.name")}</span>
          <input
            data-preset-name-input
            class="h-row-h rounded-ui bg-surface-track px-2 text-fs-2 text-fg-1 outline-none focus-visible:ring-1 focus-visible:ring-focus-ring"
            placeholder={t("editor.preset.namePlaceholder")}
            value={presetDraft()}
            autofocus
            onInput={(event) => {
              setPresetDraft(event.currentTarget.value);
              setPresetError(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.isComposing) {
                event.preventDefault();
                void submitPreset();
              }
            }}
          />
        </label>
        <div class="mt-2 flex flex-col gap-1.5">
          <span class="text-fs-2 text-fg-2">{t("editor.preset.includeGroups")}</span>
          <div class="flex flex-wrap gap-1.5" data-preset-group-chips>
            <For each={PRESET_GROUPS}>
              {(group) => {
                const on = () => presetGroups().includes(group);
                return (
                  <button
                    type="button"
                    class="flex h-6.5 items-center rounded-ui px-2.5 text-fs-1"
                    classList={{
                      "bg-state-selected font-semibold text-fg-1": on(),
                      "bg-surface-bar text-fg-2": !on(),
                    }}
                    disabled={group === "colorManagement" && props.store.colorState() === null}
                    aria-pressed={on()}
                    onClick={() => {
                      setPresetGroups((current) =>
                        current.includes(group)
                          ? current.filter((entry) => entry !== group)
                          : [...current, group],
                      );
                    }}
                  >
                    {t(GROUP_LABEL_KEY[group])}
                  </button>
                );
              }}
            </For>
          </div>
          <p class="text-fs-0 text-fg-3">{t("editor.preset.keepHint")}</p>
        </div>
        <Show when={presetError()}>
          <p data-preset-error class="mt-1.5 text-fs-0 text-danger">
            {presetError()}
          </p>
        </Show>
      </Dialog>
    </div>
  );
}
