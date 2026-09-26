/** 导出组装层：共用库目录列、PhotoGrid、Tile 和双 TilesShell；队列状态由 App 持有。 */
import {
  createEffect,
  createMemo,
  createSignal,
  For,
  onCleanup,
  onMount,
  Show,
  untrack,
} from "solid-js";
import {
  IconFolderOpen,
  IconList,
  IconPhoto,
  IconUpload,
} from "@tabler/icons-solidjs";
import {
  BrowseLeftColumn,
  type BrowseStore,
} from "../../features/browse/index.ts";
import { browseSource } from "../../features/browse/grid-source.ts";
import { PhotoGrid } from "../../features/photo-grid/PhotoGrid.tsx";
import { TilesShell } from "../../components/ui/tiles/TilesShell.tsx";
import { Tile } from "../../components/ui/Tile.tsx";
import { Button, IconButton } from "../../components/ui/Button.tsx";
import { Input } from "../../components/ui/Form.tsx";
import { Slider } from "../../components/ui/Slider.tsx";
import { SegmentedControl } from "../../components/ui/SegmentedControl.tsx";
import { SplitStack } from "../../components/ui/SplitStack.tsx";
import { SplitHandle } from "../../components/ui/SplitHandle.tsx";
import { ConfirmDialog } from "../../components/ui/Dialog.tsx";
import { StateWatermark } from "../../components/ui/StateWatermark.tsx";
import { createThumbQueue } from "../../components/ui/thumb-queue.ts";
import {
  getThumbBytes,
  listRepositories,
  onCatalogChanged,
  remountRepository,
} from "../../api/db.ts";
import { getExportVariantImage } from "../../api/export.ts";
import { openFullscreen } from "../../api/fullscreen.ts";
import { pickDirectory } from "../../api/dialog.ts";
import type { RepositoryView } from "../../api/types.ts";
import type { SelectedFileMetadata } from "../../features/exif-strip/index.ts";
import { nudgeWidth, resizeWidth } from "../../lib/column-resize.ts";
import { LAYOUT_BOUNDS } from "../../lib/layout-prefs.ts";
import { clickMode } from "../../lib/selection.ts";
import { joinPath } from "../../lib/paths.ts";
import {
  variantKey,
  type ExportFormat,
  type VariantSummary,
} from "../../lib/export-model.ts";
import { t } from "../../i18n/index.ts";
import { registerExportActions } from "./actions.ts";
import {
  exportGallerySource,
  exportQueueSource,
  queueImageKey,
  ISSUE_CHIP_WIDTH,
  ISSUE_CHIP_HEIGHT,
} from "./source.ts";
import type { ExportStore } from "./store.ts";

export interface ExportWorkspaceProps {
  store: ExportStore;
  browse: BrowseStore;
  selectedMetadata: SelectedFileMetadata;
  leftWidth: number;
  onLeftWidthChange(width: number): void;
  onOpenLibrarySettings(id: string): void;
}
export function ExportWorkspace(props: ExportWorkspaceProps) {
  const store = props.store;
  const prefs = store.preferences;
  const [repositories, setRepositories] = createSignal<RepositoryView[]>([]);
  const [reposLoading, setReposLoading] = createSignal(true);
  const [reposError, setReposError] = createSignal<string | null>(null);
  const [libsExpanded, setLibsExpanded] = createSignal(false);
  const [leftWidth, setLeftWidth] = createSignal(props.leftWidth);
  let dragStart = 0;
  const [resetOpen, setResetOpen] = createSignal(false);
  const root = () =>
    repositories().find((r) => r.id === props.browse.repositoryId())?.root ??
    null;
  const photoThumbs = createThumbQueue({
    load: (path) => getThumbBytes(path, "grid"),
  });
  // variant 图与队列图共用一个限流/LRU 队列，队列取入队快照。
  const issueThumbs = createThumbQueue({
    load: async (key) => {
      const [repo, reference, hash] = JSON.parse(key) as [
        string,
        { assetId: number; variant: string },
        string | null,
      ];
      const captured = [...store.queues().values()]
        .flat()
        .find((item) => queueImageKey(item) === key)?.snapshot;
      return getExportVariantImage(
        repo,
        reference,
        "strip",
        captured?.profileHash === hash ? captured : undefined,
      );
    },
  });
  const base = browseSource({
    store: props.browse,
    root,
    thumbs: photoThumbs,
    tileStep: () => prefs.value().topStep,
    setTileStep: (topStep) => prefs.update({ topStep }, false),
    commitTileStep: () => prefs.commit(),
    grouped: () => prefs.value().grouped,
    infoMode: () => prefs.value().info,
  });
  const assets = createMemo(() =>
    Array.from({ length: base.count() }, (_, i) =>
      Number(base.idAt?.(i)),
    ).filter((id) => id > 0),
  );
  const gallery = exportGallerySource(base, store);
  const queue = exportQueueSource(store, issueThumbs);
  const currentIssue = createMemo(
    () =>
      store
        .refsFor(assets())
        .find(
          (v) =>
            variantKey(store.repository() ?? "", v.reference) ===
            store.selection().anchor,
        ) ?? null,
  );
  const fullscreenTarget = () => {
    const current = currentIssue(),
      repo = store.repository(),
      dir = root();
    if (current === null || repo === null || dir === null) return null;
    return {
      items: [
        {
          id: variantKey(repo, current.reference),
          path: joinPath(dir, current.relPath),
          fileName: current.name,
          exportVariant: { repositoryId: repo, reference: current.reference },
        },
      ],
      index: 0,
    };
  };
  const showIssue = (variant: VariantSummary) => {
    const repo = store.repository(),
      dir = root();
    if (repo === null || dir === null) return;
    void openFullscreen(
      [
        {
          id: variantKey(repo, variant.reference),
          path: joinPath(dir, variant.relPath),
          fileName: variant.name,
          exportVariant: { repositoryId: repo, reference: variant.reference },
        },
      ],
      0,
    ).catch(store.reportError);
  };
  const refreshRepos = async () => {
    setReposLoading(true);
    try {
      setRepositories(await listRepositories());
      setReposError(null);
    } catch (e) {
      setReposError(String(e));
    } finally {
      setReposLoading(false);
    }
  };
  createEffect(() => {
    const repo = props.browse.repositoryId(),
      scope = props.browse.scopePath();
    store.context(repo, scope ?? "");
    photoThumbs.clear();
    issueThumbs.clear();
  });
  createEffect(() => {
    const v = currentIssue(),
      dir = root();
    props.selectedMetadata.select(
      v === null || dir === null ? null : joinPath(dir, v.relPath),
    );
  });
  onMount(() => {
    void refreshRepos();
    store.invalidate([...store.variants().keys()]);
    void props.browse.refresh().catch(store.reportError);
    registerExportActions({
      viewing: () => false,
      filmVisible: () => false,
      fullscreenTarget,
      enqueue: () => {
        const dir = root();
        if (dir !== null) void store.enqueue(dir);
      },
      selectAll: () =>
        void store.group(assets(), false).catch(store.reportError),
      requestReset: () => setResetOpen(true),
    });
    let dead = false;
    let off: (() => void) | undefined;
    void onCatalogChanged((change) => {
      if (change.repositoryId !== store.repository()) return;
      store.invalidate([...store.variants().keys()]);
      for (const path of change.paths)
        if (photoThumbs.get(path).status !== "idle") photoThumbs.refresh(path);
      issueThumbs.clear();
      void refreshRepos();
    }).then((unsub) => {
      if (dead) unsub();
      else off = unsub;
    });
    onCleanup(() => {
      dead = true;
      off?.();
    });
  });
  onCleanup(() => {
    registerExportActions(null);
    photoThumbs.clear();
    issueThumbs.clear();
    props.selectedMetadata.select(null);
  });
  let checkTimer: ReturnType<typeof setTimeout> | undefined;
  createEffect(() => {
    const candidate = store.draft();
    clearTimeout(checkTimer);
    if (!candidate.name || !candidate.directory) return;
    checkTimer = setTimeout(
      () => void store.check(candidate).catch(store.reportError),
      250,
    );
  });
  onCleanup(() => clearTimeout(checkTimer));
  const cycleInfo = () => {
    const info = ["off", "marks", "marks-name"] as const;
    prefs.update({ info: info[(info.indexOf(prefs.value().info) + 1) % 3] });
  };
  const bar = (bottom = false) => ({
    count: bottom ? queue.count() : gallery.count(),
    countLabel: bottom
      ? t("export.queueCount", {
          remaining: store.progress(store.selectedPreset()?.id ?? "").remaining,
          total: queue.count(),
        })
      : undefined,
    selectedCount: bottom ? 0 : store.selection().ids.size,
    byTime: prefs.value().grouped,
    onByTimeChange: (grouped: boolean) => prefs.update({ grouped }),
    infoMode: prefs.value().info,
    onInfoToggle: cycleInfo,
    tileStep: bottom ? prefs.value().queueStep : prefs.value().topStep,
    onTileStepChange: (step: number) =>
      prefs.update(bottom ? { queueStep: step } : { topStep: step }, false),
    onTileStepCommit: () => prefs.commit(),
    compactControls: bottom,
    centerContent: bottom ? (
      <span class="flex min-w-0 items-center gap-2">
        <span class="truncate rounded-ui bg-state-selected px-2 text-fs-1 text-fg-1">
          {store.selectedPreset()?.name ?? t("export.pickPreset")}
        </span>
        <IconButton
          label={t("export.listMode")}
          selected={prefs.value().queueList}
          onClick={() => prefs.update({ queueList: !prefs.value().queueList })}
        >
          <IconList size={14} />
        </IconButton>
      </span>
    ) : undefined,
    label: bottom
      ? (store.selectedPreset()?.name ?? t("export.pickPreset"))
      : props.browse.scopePath(),
  });
  const galleryEmpty = () => (
    <StateWatermark
      animate={props.browse.loading()}
      icon={<IconPhoto size={64} />}
      text={
        store.repository() === null
          ? t("browse.noRepository")
          : props.browse.scopePath() === null
            ? t("browse.pickDirectory")
            : props.browse.loading()
              ? t("browse.loading")
              : t("export.noIssues")
      }
    />
  );
  function IssueStrip(p: { assetId: number }) {
    return (
      <div data-export-issues class="flex flex-wrap content-start gap-1 pt-1">
        <For each={store.listFor(p.assetId)}>
          {(variant) => {
            const key = () =>
              JSON.stringify([
                store.repository(),
                variant.reference,
                variant.profileHash,
              ]);
            createEffect(() => issueThumbs.request(key()));
            return (
              <div
                style={{
                  width: `${ISSUE_CHIP_WIDTH}px`,
                  height: `${ISSUE_CHIP_HEIGHT - 4}px`,
                }}
                title={variant.name}
              >
                <div style={{ height: `${ISSUE_CHIP_WIDTH}px` }}>
                  <Tile
                    label={variant.name}
                    src={issueThumbs.get(key()).url}
                    loading={issueThumbs.get(key()).status === "loading"}
                    selected={store
                      .selection()
                      .ids.has(
                        variantKey(store.repository() ?? "", variant.reference),
                      )}
                    onClick={(event) => {
                      event.stopPropagation();
                      void store
                        .selectIssue(
                          variant.reference,
                          clickMode(event, true),
                          assets(),
                        )
                        .catch(store.reportError);
                    }}
                    onActivate={() => showIssue(variant)}
                  />
                </div>
                <div class="truncate text-center text-fs-0 text-fg-2">
                  {variant.name}
                </div>
              </div>
            );
          }}
        </For>
        <Show when={!store.variants().has(p.assetId)}>
          <span class="text-fs-0 text-fg-3">{t("common.loading")}</span>
        </Show>
      </div>
    );
  }
  const queueOverlay = (id: string) => {
    const item = () =>
      store
        .queues()
        .get(store.selectedPreset()?.id ?? "")
        ?.find((item) => item.id === id);
    return (
      <span class="pointer-events-none absolute left-1 top-1 rounded-ui bg-surface-layer px-1 text-fs-0 text-fg-2">
        {t(
          item()?.status === "done"
            ? "export.status.done"
            : item()?.status === "running"
              ? "export.status.running"
              : item()?.status === "failed"
                ? "export.status.failed"
                : "export.status.pending",
        )}
      </span>
    );
  };
  const top = (
    <TilesShell bar={bar()}>
      <PhotoGrid
        source={gallery}
        commandEnter
        cellExtra={(item) => <IssueStrip assetId={Number(item.id)} />}
        activate={(id) => {
          const v = store.listFor(Number(id))[0];
          if (v !== undefined) showIssue(v);
        }}
        watermark={galleryEmpty}
        onInteract={() => setLibsExpanded(false)}
      />
    </TilesShell>
  );
  const bottom = (
    <TilesShell bar={bar(true)}>
      <PhotoGrid
        source={queue}
        listMode={prefs.value().queueList}
        cellExtra={(item) => {
          const entry = store
            .queues()
            .get(store.selectedPreset()?.id ?? "")
            ?.find((q) => q.id === item.id);
          return (
            <div
              class="min-w-0 truncate text-fs-1 text-fg-2"
              title={entry?.snapshot.name}
            >
              {entry?.snapshot.name}
            </div>
          );
        }}
        commandEnter
        cellOverlay={(item) => queueOverlay(item.id)}
        activate={() => {}}
        watermark={() => (
          <StateWatermark
            icon={<IconUpload size={64} />}
            text={
              store.selectedPreset() === null
                ? t("export.pickPreset")
                : t("export.emptyQueue")
            }
          />
        )}
      />
    </TilesShell>
  );
  const ratio = untrack(() => prefs.value().ratio);
  const fieldError = (field: string) => store.validation().errors[field];
  return (
    <div data-export-workspace class="flex min-h-0 flex-1 overflow-hidden">
      <div
        data-export-left
        class="flex min-h-0 shrink-0 flex-col"
        style={{ width: `${leftWidth()}px` }}
      >
        <BrowseLeftColumn
          store={props.browse}
          repositories={repositories()}
          reposLoading={reposLoading()}
          reposError={reposError()}
          libsExpanded={libsExpanded()}
          onExpandLibs={() => setLibsExpanded(true)}
          onCollapseLibs={() => setLibsExpanded(false)}
          onOpenSettings={props.onOpenLibrarySettings}
          onRemount={(id) =>
            void remountRepository(id)
              .then(refreshRepos)
              .catch(store.reportError)
          }
        />
      </div>
      <SplitHandle
        orientation="vertical"
        aria-label={t("common.resize_left")}
        tabindex="0"
        onDragStart={() => {
          dragStart = leftWidth();
        }}
        onDrag={(dx) =>
          setLeftWidth(
            resizeWidth({
              start: dragStart,
              dx,
              bounds: LAYOUT_BOUNDS.browseLeftWidth,
            }),
          )
        }
        onDragEnd={() => props.onLeftWidthChange(leftWidth())}
        onKeyDown={(event) => {
          if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
          event.preventDefault();
          const width = nudgeWidth(
            leftWidth(),
            event.key === "ArrowRight" ? 8 : -8,
            LAYOUT_BOUNDS.browseLeftWidth,
          );
          setLeftWidth(width);
          props.onLeftWidthChange(width);
        }}
      />
      <div
        data-export-mid
        class="flex min-h-0 min-w-0 flex-1 flex-col bg-surface-bar"
      >
        <Show when={store.error() || props.browse.error()}>
          <p
            role="alert"
            class="shrink-0 break-words p-2 text-fs-1 text-danger"
          >
            {store.error() ?? props.browse.error()}
          </p>
        </Show>
        <SplitStack
          class="flex-1"
          segments={[
            {
              id: t("export.gallery"),
              defaultSize: ratio * 100,
              minSize: 20,
              content: top,
            },
            {
              id: t("export.queue"),
              defaultSize: (1 - ratio) * 100,
              minSize: 20,
              content: bottom,
            },
          ]}
          onResizeEnd={(sizes) =>
            prefs.update({ ratio: (sizes[0] ?? 58) / 100 })
          }
        />
      </div>
      <aside
        data-export-right
        class="flex w-panel-w-right min-h-0 shrink-0 flex-col gap-3 bg-surface-main p-panel-pad"
      >
        <div class="flex shrink-0 items-center justify-between">
          <span class="text-fs-2 text-fg-2">{t("export.presets")}</span>
          <Button onClick={() => store.choosePreset(null)}>
            {t("export.new")}
          </Button>
        </div>
        <div data-export-preset-list class="max-h-36 shrink-0 overflow-auto">
          <For each={store.presets()}>
            {(preset) => (
              <button
                type="button"
                class={`mb-1 flex w-full items-center gap-2 rounded-ui p-2 text-left text-fs-1 ${store.selectedPreset()?.id === preset.id ? "bg-state-selected" : "bg-surface-track hover:bg-state-hover"}`}
                onClick={() => store.choosePreset(preset.id)}
              >
                <span class="min-w-0 flex-1">
                  <span class="block truncate text-fg-1">{preset.name}</span>
                  <span class="block truncate text-fs-0 text-fg-3">
                    {preset.format.toUpperCase()} ·{" "}
                    {preset.maxEdge === 0
                      ? t("export.originalSize")
                      : preset.maxEdge + "px"}
                  </span>
                </span>
                <Show when={store.progress(preset.id).total > 0}>
                  <span class="shrink-0 text-fg-2 tnum">
                    {store.progress(preset.id).remaining}/
                    {store.progress(preset.id).total}
                  </span>
                </Show>
              </button>
            )}
          </For>
        </div>
        <form
          data-export-form
          class="flex min-h-0 flex-1 flex-col gap-2 overflow-auto"
          onSubmit={(event) => {
            event.preventDefault();
            void store.save();
          }}
        >
          <span class="text-fs-2 text-fg-2">
            {store.selectedPreset() === null
              ? t("export.newPreset")
              : t("export.settings")}
          </span>
          <label class="flex flex-col gap-1 text-fs-1 text-fg-2">
            {t("export.name")}
            <Input
              value={store.draft().name}
              invalid={!!fieldError("name")}
              onInput={(event) =>
                store.edit({ name: event.currentTarget.value })
              }
            />
          </label>
          <div class="text-fs-1 text-fg-2">{t("export.format")}</div>
          <SegmentedControl
            label={t("export.format")}
            value={store.draft().format}
            options={(
              ["jpeg", "tiff", "png", "webp", "avif"] as ExportFormat[]
            ).map((value) => ({ value, label: value.toUpperCase() }))}
            onValueChange={(format) => store.edit({ format })}
          />
          <div class="flex justify-between text-fs-1 text-fg-2">
            <span>{t("export.quality")}</span>
            <span class="tnum">{store.draft().quality}</span>
          </div>
          <Slider
            value={store.draft().quality}
            min={1}
            max={100}
            label={t("export.quality")}
            onValueChange={(quality) => store.edit({ quality })}
            disabled={
              store.draft().format === "png" || store.draft().format === "tiff"
            }
          />
          <label class="flex flex-col gap-1 text-fs-1 text-fg-2">
            {t("export.maxEdge")}
            <Input
              type="number"
              min="0"
              max="65535"
              value={store.draft().maxEdge}
              invalid={!!fieldError("maxEdge")}
              onInput={(event) =>
                store.edit({
                  maxEdge:
                    event.currentTarget.value === ""
                      ? NaN
                      : Number(event.currentTarget.value),
                })
              }
            />
          </label>
          <label class="flex flex-col gap-1 text-fs-1 text-fg-2">
            {t("export.directory")}
            <span class="flex gap-1">
              <Input
                class="min-w-0 flex-1"
                value={store.draft().directory}
                invalid={!!fieldError("directory")}
                onInput={(event) =>
                  store.edit({ directory: event.currentTarget.value })
                }
              />
              <IconButton
                label={t("export.pickDirectory")}
                onClick={() =>
                  void pickDirectory({ title: t("export.pickDirectory") })
                    .then((dir) => {
                      if (dir !== null) store.edit({ directory: dir });
                    })
                    .catch(store.reportError)
                }
              >
                <IconFolderOpen size={16} />
              </IconButton>
            </span>
          </label>
          <label class="flex flex-col gap-1 text-fs-1 text-fg-2">
            {t("export.template")}
            <Input
              value={store.draft().template}
              invalid={!!fieldError("template")}
              onInput={(event) =>
                store.edit({ template: event.currentTarget.value })
              }
            />
          </label>
          <p class="text-fs-0 text-fg-3">{t("export.templateHelp")}</p>
          <For each={Object.entries(store.validation().errors)}>
            {([field, message]) => (
              <p role="alert" class="break-words text-fs-0 text-danger">
                {t("export.invalidField", {
                  field: t(
                    field === "name"
                      ? "export.name"
                      : field === "directory"
                        ? "export.directory"
                        : field === "template"
                          ? "export.template"
                          : field === "maxEdge"
                            ? "export.maxEdge"
                            : field === "quality"
                              ? "export.quality"
                              : "export.format",
                  ),
                  message: message === field ? t("export.invalid") : message,
                })}
              </p>
            )}
          </For>
          <For each={store.validation().warnings}>
            {(message) => <p class="text-fs-0 text-fg-3">{message}</p>}
          </For>
          <Button
            type="submit"
            variant={
              store
                .presets()
                .some(
                  (p) => p.name === store.draft().name.trim().normalize("NFC"),
                )
                ? "secondary"
                : "primary"
            }
            disabled={
              store.busy() ||
              store.loading() ||
              Object.keys(store.validation().errors).length > 0
            }
            loading={store.busy()}
          >
            {t(
              store
                .presets()
                .some(
                  (p) => p.name === store.draft().name.trim().normalize("NFC"),
                )
                ? "export.update"
                : "export.add",
            )}
          </Button>
        </form>
        <div class="flex shrink-0 flex-col gap-1">
          <Button
            variant="primary"
            disabled
            title={t("export.executionPending")}
          >
            {t("export.start")}
          </Button>
          <span class="text-center text-fs-0 text-fg-3">
            {t("export.executionPending")}
          </span>
        </div>
      </aside>
      <ConfirmDialog
        open={resetOpen()}
        onCancel={() => setResetOpen(false)}
        title={t("export.reset")}
        message={t("export.resetConfirm")}
        onConfirm={() => {
          store.reset();
          issueThumbs.clear();
          setResetOpen(false);
        }}
      />
    </div>
  );
}
