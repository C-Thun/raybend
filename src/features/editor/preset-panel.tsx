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
  IconPlayerPlay,
  IconStack2,
} from "@tabler/icons-solidjs";

import { Button } from "../../components/ui/Button.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { EasyDestroyButton } from "../../components/ui/EasyDestroy.tsx";
import { Tooltip } from "../../components/ui/Tooltip.tsx";
import { t, type MessageKey } from "../../i18n/index.ts";
import { trackPointerDrag } from "../../lib/pointer-drag.ts";
import {
  DEFAULT_DIRECTORY_ID,
  PRESET_GROUPS,
  DEFAULT_PRESET_GROUPS,
  isDirectoryNameTaken,
  isPresetNameTaken,
  resolveCreateDirectory,
  snapshotGroups,
  type PresetDirectory,
  type PresetGroup,
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

/**
 * 树区的最小高度：对齐「曲线页签在 RAW + 基础曲线块」时的内容高度
 * （CurveEditor 130 + 通道行 26 + 基础曲线块 78 + 间距）—— 切页签不伸缩
 * （`design/editor.md` §3.10；SOOC 下曲线较短时留白，可接受）。
 */
const TREE_MIN_HEIGHT = "224px";

export interface PresetPanelProps {
  store: EditorStore;
  /** 有没有可编辑的照片（空态下应用 / 新建预设禁用；目录管理不受影响） */
  enabled: boolean;
  /** 应用 / 新建后落库（与 LUT 选择的 `onSelect → commitDevelop` 同一条路） */
  onCommit?: () => void;
  applyPreset: (snapshot: PresetSnapshot) => Promise<void>;
  createDirectory: (name: string) => Promise<boolean>;
  createPreset: (name: string, directoryId: string, groups: readonly PresetGroup[]) => Promise<boolean>;
  deletePreset: (id: string) => Promise<void>;
  deleteDirectory: (id: string) => Promise<void>;
  movePresets: (ids: readonly string[], directoryId: string) => Promise<void>;
  class?: string;
}

export function PresetPanel(props: PresetPanelProps): JSX.Element {
  /* ── 新建目录弹窗 ─────────────────────────────────── */
  const [dirOpen, setDirOpen] = createSignal(false);
  const [dirDraft, setDirDraft] = createSignal("");
  const [dirDuplicate, setDirDuplicate] = createSignal(false);

  /* ── 新建预设弹窗 ─────────────────────────────────── */
  const [presetOpen, setPresetOpen] = createSignal(false);
  const [presetDraft, setPresetDraft] = createSignal("");
  const [presetGroups, setPresetGroups] = createSignal<PresetGroup[]>([...DEFAULT_PRESET_GROUPS]);
  const [presetDuplicate, setPresetDuplicate] = createSignal(false);
  const [busy, setBusy] = createSignal(false);

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
    if (name === "") return;
    if (isDirectoryNameTaken(name, directories())) {
      setDirDuplicate(true);
      return;
    }
    setBusy(true);
    try {
      if (await props.createDirectory(name)) setDirOpen(false);
      else setDirDuplicate(true);
    } finally {
      setBusy(false);
    }
  };

  /* ── 新建预设 ─────────────────────────────────────── */
  const submitPreset = async (): Promise<void> => {
    if (busy()) return;
    const name = presetDraft().trim();
    if (name === "" || presetGroups().length === 0) return;
    // 落点：选中的目录 → 选中预设所在目录 → default（specs §5.3）
    const directoryId = resolveCreateDirectory(selection(), directories(), presets());
    if (isPresetNameTaken(name, directoryId, presets())) {
      setPresetDuplicate(true);
      return;
    }
    setBusy(true);
    try {
      if (await props.createPreset(name, directoryId, presetGroups())) setPresetOpen(false);
      else setPresetDuplicate(true);
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
      {/* 工具行：应用（主色，选中单预设才可用）/ 新建目录 / 新建预设 */}
      <div class="flex shrink-0 items-center gap-1 pb-1.5">
        <Tooltip content={t("editor.preset.apply")}>
          {(triggerProps) => (
            <Button
              {...triggerProps()}
              variant="primary"
              disabled={!canApply()}
              aria-label={t("editor.preset.apply")}
              icon={<IconPlayerPlay size={14} />}
              onClick={applySelected}
            />
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
                setDirDuplicate(false);
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
                setPresetDuplicate(false);
                setPresetGroups([...DEFAULT_PRESET_GROUPS]);
                setPresetOpen(true);
              }}
            />
          )}
        </Tooltip>
      </div>

      {/* 可滚动的目录 / 预设树 */}
      <div
        ref={scrollHost}
        class="flex min-h-0 flex-col gap-1 overflow-y-auto pb-1"
        style={{ "min-height": TREE_MIN_HEIGHT }}
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
                    class="flex min-w-0 flex-1 items-center gap-1.5 rounded-ui text-left"
                    onClick={() => props.store.selectPresetDirectory(directory.id)}
                  >
                    <span class="min-w-0 flex-1 truncate text-fs-2 text-fg-1">
                      {dirLabel(directory)}
                    </span>
                    <span class="shrink-0 text-fs-0 tabular-nums text-fg-3">
                      {t("editor.preset.count").replace("{n}", String(entries().length))}
                    </span>
                  </button>
                  {/* 目录清空后才能删；默认目录永远不可删（specs §5.3） */}
                  <Show when={directory.id !== DEFAULT_DIRECTORY_ID && entries().length === 0 && !dragging()}>
                    <EasyDestroyButton
                      class="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
                      label={t("editor.preset.removeDir")}
                      confirmTitle={t("editor.preset.removeDirTitle")}
                      confirmMessage={t("editor.preset.removeDirConfirm").replace("{name}", dirLabel(directory))}
                      onRemove={() => props.deleteDirectory(directory.id)}
                    />
                  </Show>
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
                                  onPointerDown={(event) => startRowDrag(event, preset)}
                                >
                                  <IconBookmark size={14} class="shrink-0 text-fg-2" aria-hidden="true" />
                                  <span class="min-w-0 flex-1 truncate text-fs-2 text-fg-1">
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
              setDirDuplicate(false);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.isComposing) {
                event.preventDefault();
                void submitDirectory();
              }
            }}
          />
        </label>
        <Show when={dirDuplicate()}>
          <p data-preset-dir-duplicate class="mt-1.5 text-fs-0 text-danger">
            {t("editor.preset.dirExists")}
          </p>
        </Show>
      </Dialog>

      {/* 新建预设：名称 + 大类 chips（默认全选；照导入 LUT 弹窗的 chip 语言） */}
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
              setPresetDuplicate(false);
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
        <Show when={presetDuplicate()}>
          <p data-preset-duplicate class="mt-1.5 text-fs-0 text-danger">
            {t("editor.preset.exists")}
          </p>
        </Show>
      </Dialog>
    </div>
  );
}
