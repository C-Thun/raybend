/**
 * 导入工作区：三列（`design/main.md` §3）。
 *
 * ```text
 * ┌────────────┬──────────────────────────────┬──────────────┐
 * │ 左列 320   │ 中列（弹性，surface-bar）     │ 右列 300     │
 * │ 最近       │ 照片网格 + 底部控制条         │ 库列表       │
 * │ 来源树     │                              │ + 新建库     │
 * │ 已选目录   │                              │ 导入按钮     │
 * └────────────┴──────────────────────────────┴──────────────┘
 * ```
 *
 * 这是**组装层**：把 feature 与工作区共享状态接起来（`ARCHITECTURE.md` §2），
 * 自己不做业务判断。初始数据（最近目录 / 驱动器 / 库列表）在这里拉一次 ——
 * 属于工作区自己的生命周期，跟着它挂载与卸载。
 *
 * 三列的宽度用 `$surface-main`（左右）与 `$surface-bar`（中）拉出层次：
 * 两种主题下中央都形成聚焦（`design/main.md` §3）。
 */

import {
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  Show,
  type JSX,
} from "solid-js";
import * as importCommands from "../../api/import.ts";
import { importPrecheck, onImportProgress, type ImportSource } from "../../api/import.ts";
import { locale, t } from "../../i18n/index.ts";
import { fileNameOf } from "../../lib/format.ts";
import {
  importSource,
  PhotoGrid,
  type PhotoGridStore,
} from "../../features/photo-grid/index.ts";
import { StateWatermark } from "../../components/ui/StateWatermark.tsx";
import { IconAlertTriangle, IconFolderOpen, IconPhoto, IconPhotoOff } from "@tabler/icons-solidjs";
import { TilesShell } from "../../components/ui/tiles/index.ts";
import {
  CompareView,
  createViewerStore,
  FilmStrip,
  Viewer,
  type ViewerPhoto,
} from "../../components/ui/viewer/index.ts";
import { createThumbQueue } from "../../components/ui/thumb-queue.ts";
import { getThumbBytes, getViewImage } from "../../api/db.ts";
import { chromeShowsFilm, nextChrome, type ViewerChrome } from "../../lib/viewer-chrome.ts";
import { shouldHandleKey } from "../../lib/viewer-keys.ts";
import { cycleTileInfo, infoKeyApplies } from "../../components/ui/tile-info.ts";
import { compareIds } from "../../lib/viewer-compare.ts";
import {
  createImportStore,
  ImportProgressDialog,
} from "../../features/import/index.ts";
import {
  CreateRepositoryDialog,
  RepositoryFooter,
  RepositoryList,
} from "../../features/repositories/index.ts";
import type { ImportStore } from "./store.ts";
import { SplitHandle } from "../../components/ui/SplitHandle.tsx";
import { withTimeout } from "../../lib/timeout.ts";
import { timeoutMessage } from "../../i18n/index.ts";
import { LeftColumn } from "./LeftColumn.tsx";
import type { ToastStore } from "../../components/ui/toast.ts";

export interface ImportWorkspaceProps {
  store: ImportStore;
  /** 照片网格的状态（在组装层创建，因为外壳的 `toolsbar` 也要读它的选择） */
  grid: PhotoGridStore;
  /**
   * 「在库中查看这些照片」—— 切到浏览工作流。
   *
   * 由组装层给（工作流是外壳的状态，工作区不该自己去动它）；
   * 不给就不显示这个按钮（M1-6 之前的那版就是这样）。
   */
  onRevealInLibrary?: () => void;
  /** 左列宽度比例（0–1；来自 `lib/layout-prefs.ts`，只在首次渲染生效） */
  leftRatio?: number;
  /** 左列拖拽结束 —— 交给布局偏好店落盘 */
  onLeftRatioChange?: (ratio: number) => void;
  /** 左列里「最近」段的高度比例（0–1） */
  recentRatio?: number;
  onRecentRatioChange?: (ratio: number) => void;
  /** 与浏览工作区共用的根层提示队列。 */
  toast?: ToastStore;
}

/** 空间预检的时限：与导入命令同量级（15 秒只可能是「后端挂了」）。 */
const PRECHECK_TIMEOUT_MS = 15_000;

/** 字节数 → 「1.2 GB」这种人话（预检提示用）。 */
function formatGiB(bytes: number): string {
  const gb = bytes / (1024 * 1024 * 1024);
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  return `${Math.round(bytes / (1024 * 1024))} MB`;
}

export function ImportWorkspace(props: ImportWorkspaceProps) {
  const store = props.store;
  const grid = props.grid;
  const [creating, setCreating] = createSignal(false);
  const groupingLocale = () => (locale() === "en-US" ? "en-US" : "zh-CN");

  // 导入：进度弹窗的状态机（api 从 `src/api/` 注入，浏览器里自动降级）
  const importStore = createImportStore({
    api: {
      start: importCommands.importStart,
      pause: importCommands.importPause,
      resume: importCommands.importResume,
      cancel: importCommands.importCancel,
      status: importCommands.importStatus,
      exportErrors: importCommands.importErrorsExport,
      subscribe: onImportProgress,
    },
  });
  // 空间预检的结论：偏紧就先问一句再开工（`plans/M1-6.md` §3.3）
  const [spaceWarning, setSpaceWarning] = createSignal<string | null>(null);

  /** 已勾选目录 → 每个目录带自己的「包含子目录」开关。 */
  const sources = (): ImportSource[] =>
    store.checkedDirs().map((dir) => ({
      path: dir.path,
      includeSubdirs: dir.includeSubdirs,
    }));

  /** 真正开始导入（`avoidDuplicates` 用右列那个复选框的状态）。 */
  async function startImport(): Promise<void> {
    const repository = store.selectedRepository();
    if (repository === null || sources().length === 0) return;
    try {
      // 预检也限时：后端要是挂了，这一句同样会永远不回来（按钮就一直转）
      const precheck = await withTimeout(
        importPrecheck(repository.id, sources()),
        PRECHECK_TIMEOUT_MS,
        timeoutMessage("import.timeout.precheck", PRECHECK_TIMEOUT_MS),
      );
      if (precheck.tight) {
        setSpaceWarning(
          t("import.space_warning", {
            needed: formatGiB(precheck.neededBytes),
            free: formatGiB(precheck.freeBytes ?? 0),
          }),
        );
        return;
      }
    } catch {
      // 预检失败不该拦住导入（例如网络盘读不到可用空间）
    }
    await beginImport(repository.id);
  }

  async function beginImport(repositoryId: string): Promise<void> {
    setSpaceWarning(null);
    await importStore.begin({
      repositoryId,
      sources: sources(),
      avoidDuplicates: store.avoidDuplicates(),
      // 排除清单只带**落在已勾选目录里**的那些：用户排过、随后又把那个目录取消勾选的，
      // 不该再跟着这批走（排除本身还留在内存里，再勾回来就还在）
      excluded: store.excludedForImport(),
    });
  }

  onMount(() => {
    // 三个列表各拉一次；互不依赖，失败各自记状态（不阻塞其它面板）
    void store.reloadRecent();
    void store.reloadVolumes();
    void store.reloadRepositories();
    // 网格的偏好（档位 / 按时间 / 时间片阈值）从设置里读回
    void grid.hydrate();
    // 导入偏好（避免重复导入）从设置里读回
    void store.hydratePreferences();
  });

  /*
   * 导入结束后刷新右列：库卡片的照片数变了。
   * （中列网格列的是**来源**目录，导入不改它，所以不用动。）
   */
  createEffect(() => {
    if (importStore.finished()) void store.reloadRepositories();
  });

  // 左列选中哪个目录，中列就跟着换（选中是**一个**状态，跨面板同步）
  createEffect(() => grid.setSourceDir(store.selectedDir()));

  /*
   * ── 左列宽度：**自己写的把手**，不用 Ark 的 Splitter ──────────────────────
   *
   * 为什么不用 Ark（2026-09-16 真机事故的结论）：外层横向 splitter 套内层竖向 splitter
   * （左列内部那几段）时，拖外层会让内层不断收到 ResizeObserver 回调 ——
   * 内层量到的是**过渡中的尺寸**（症状：「目录树很矮、下面还空着一块」），
   * 极端情况下两边互相触发就是**卡死**（症状：「一拖就死」）。
   *
   * 这里的把手只做一件事：pointermove → 改一个百分比。没有 observer、没有嵌套，
   * 行为完全可预期；可访问性靠 `role="separator"` + 左右方向键自己补。
   */
  const [leftRatio, setLeftRatio] = createSignal(props.leftRatio ?? 0.22);
  /** 拖动起点：比例 + 容器宽度（拖动期间窗口尺寸不会变，起点量一次就够） */
  let leftDragStart = 0;
  let leftDragWidth = 0;
  /** 量容器宽度用（把手与它同一行） */
  let rowEl: HTMLDivElement | undefined;

  /*
   * ── 看图（tiles / film / view 三态）────────────────────────────────────
   *
   * 人类 2026-09-19：import 与 browse 的**中列**要能用同一套东西 ——
   * 同一份 `chrome.ts`（三态循环）、同一个 `Viewer` / `FilmStrip` / `CompareView`。
   * tiles 仍是默认态，且**排版口径一点没动**（还是 `PhotoGrid` + `lib/tile-flow.ts`）。
   */
  const viewer = createViewerStore({
    loadScreen: (path) => getViewImage(path, "screen"),
    loadThumb: (path) => getThumbBytes(path, "grid"),
  });
  const [chrome, setChrome] = createSignal<ViewerChrome>("default");

  /**
   * 看图用的照片列表：与中列**同一份数据、同一个顺序**（`grid.items()`）。
   * `naturalOf` 给的是真实宽高（头部缓存那份），对比与缩放都靠它，
   * 缺了也不慌 —— `ViewerPhoto.natural` 本来就是可选。
   */
  const viewerPhotos = createMemo<ViewerPhoto[]>(() =>
    grid.items().map((item) => {
      const natural = grid.naturalOf(item.path);
      return {
        id: item.path,
        path: item.path,
        fileName: item.fileName,
        ...(natural === null ? {} : { natural }),
      };
    }),
  );

  /**
   * 对比：与 browse **同一条规则**（`lib/viewer-compare.ts`）——
   * 选中 ≥ 2 张就是对比态，超过 4 张只对比**最近选中的 4 张**（锚点必含）。
   */
  const comparePhotoIds = createMemo<string[]>(() =>
    compareIds(grid.selectedIds(), viewerPhotos().map((photo) => photo.id), grid.selection().anchor),
  );
  const comparing = (): boolean => viewer.state().active && comparePhotoIds().length >= 2;
  /** 对比态要显示的那几张（按对比顺序） */
  const comparePhotos = createMemo<ViewerPhoto[]>(() => {
    const byId = new Map(viewerPhotos().map((photo) => [photo.id, photo]));
    return comparePhotoIds().flatMap((id) => {
      const photo = byId.get(id);
      return photo === undefined ? [] : [photo];
    });
  });

  /*
   * 进对比就把**对比那几张**的宽高补齐（人类 2026-09-19 报的拉伸/拖动不对的根因）：
   * 网格只为可见 tile 读过元数据，别的照片 `naturalOf` 是 null → 基准比例算不出来。
   * 只补对比集（通常 ≤4 张），不去碰整个目录 —— 别为一次对比读上千个文件头。
   */
  createEffect(() => {
    const ids = comparePhotoIds();
    if (ids.length >= 2) void grid.ensureNatural(ids);
  });

  /** 胶片带的缩略图队列（网格那份藏在 `PhotoGrid` 里没对外暴露，所以这里自建一份） */
  const filmThumbs = createThumbQueue({
    load: async (path) => {
      const bytes = await getThumbBytes(path, "grid");
      return bytes ?? null;
    },
  });

  /*
   * 键盘：与 browse **同一套语义**，并复用它的 `shouldHandleKey` 守卫
   * （输入框里按 Tab 不该被我们吃掉）。
   *   Tab   → 三态循环（film+左右 / film only / view only）
   *   Enter → tiles 里进看图（从锚点那张开始）
   *   Esc   → 退出看图并把外壳复位
   */
  onMount(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (!shouldHandleKey(event.target, false)) return;
      const active = viewer.state().active;

      if (event.key === "Tab") {
        if (!active) return;
        event.preventDefault();
        setChrome(nextChrome(chrome()));
        return;
      }
      if (event.key === "Escape") {
        if (!active) return;
        event.preventDefault();
        setChrome("default");
        viewer.close();
        return;
      }
      // `i`：切 tiles 的「信息」档位 —— **只在 tiles / film 下生效**（纯看图态不接，人类 2026-09-19）
      if (event.key === "i" || event.key === "I") {
        if (!infoKeyApplies({ viewing: active, filmVisible: chromeShowsFilm(chrome()) })) return;
        event.preventDefault();
        cycleTileInfo();
        return;
      }
      if (event.key === "Enter" && !active) {
        const photos = viewerPhotos();
        if (photos.length === 0) return;
        event.preventDefault();
        const anchor = grid.selection().anchor;
        const at = anchor === null ? 0 : photos.findIndex((photo) => photo.id === anchor);
        viewer.show(photos, at < 0 ? 0 : at);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    onCleanup(() => window.removeEventListener("keydown", onKeyDown));
  });

  /** 左列宽度的上下限（比例）：与 `lib/layout-prefs.ts` 的范围一致，再加一道像素下限 */
  const clampLeftRatio = (ratio: number): number =>
    Math.min(0.5, Math.max(0.12, ratio));

  /*
   * 拖动只做一件事：pointermove（已按帧节流）→ 换一个比例。
   * 指针捕获、`touch-action`、拖动期禁选、rAF 节流都在 `SplitHandle` 里 ——
   * 全项目只有它一个把手实现（2026-09-19 统一：以前这里、浏览列宽、Ark 的 Splitter 是三套）。
   */
  function beginLeftDrag(): void {
    leftDragWidth = rowEl?.getBoundingClientRect().width ?? 0;
    leftDragStart = leftRatio();
  }

  /** 键盘调整：左右方向键各 2%，立刻落盘（一次按键就是一次「结束」） */
  function nudgeLeftRatio(delta: number): void {
    const next = clampLeftRatio(leftRatio() + delta);
    setLeftRatio(next);
    props.onLeftRatioChange?.(next);
  }

  /** 网格数据源（适配器：把导入 store 包成网格契约） */
  const gridSource = createMemo(() => importSource(grid, { isExcluded: store.isExcluded }));

  /** 空态 / 加载 / 错误的水印（文案是导入侧的，所以由工作区给） */
  function gridWatermark(): JSX.Element | null {
    if (grid.dir() === null) {
      return (
        <StateWatermark
          icon={<IconFolderOpen size={64} stroke-width={1} />}
          text={t("grid.pick_dir")}
        />
      );
    }
    if (grid.status() === "error") {
      return (
        <StateWatermark
          tone="error"
          icon={<IconAlertTriangle size={64} stroke-width={1} />}
          text={t("grid.load_error", { message: grid.error() ?? "" })}
          action={{ label: t("common.retry"), run: grid.reload }}
        />
      );
    }
    if (grid.status() === "idle" || grid.status() === "loading") {
      return (
        <StateWatermark
          animate
          icon={<IconPhoto size={64} stroke-width={1} />}
          text={t("grid.loading_dir")}
        />
      );
    }
    if (grid.displayItems().length === 0) {
      return (
        <StateWatermark
          icon={<IconPhotoOff size={64} stroke-width={1} />}
          text={t("grid.empty_dir")}
        />
      );
    }
    return null;
  }

  return (
    /*
     * 工作区 = 普通 flex 行 + **自写的宽度把手**（原则见 DESIGN.md §8.6：只给左边）。
     * 右列宽度固定（`--panel-w-right`），不参与拖拽。
     */
    <div ref={rowEl} class="flex min-h-0 flex-1">
      <aside
        class="flex min-h-0 shrink-0 flex-col bg-surface-main p-panel-pad"
        // 用百分比而不是像素：窗口大小变了之后比例仍然对（与持久化的口径一致）
        style={{ "flex-basis": `${leftRatio() * 100}%`, "min-width": "220px" }}
      >
        <LeftColumn
          store={store}
          {...(props.recentRatio === undefined
            ? {}
            : { recentRatio: props.recentRatio })}
          {...(props.onRecentRatioChange === undefined
            ? {}
            : { onRecentRatioChange: props.onRecentRatioChange })}
        />
      </aside>

      {/* 三点把手：拖它只改左列宽度比例（全项目唯一的把手组件） */}
      <SplitHandle
        orientation="vertical"
        aria-label={t("common.resize_left")}
        aria-valuenow={Math.round(leftRatio() * 100)}
        aria-valuemin={12}
        aria-valuemax={50}
        tabindex="0"
        onDragStart={beginLeftDrag}
        onDrag={(dx) => {
          if (leftDragWidth <= 0) return;
          setLeftRatio(clampLeftRatio(leftDragStart + dx / leftDragWidth));
        }}
        onDragEnd={() => props.onLeftRatioChange?.(leftRatio())}
        onKeyDown={(event) => {
          if (event.key === "ArrowLeft") {
            event.preventDefault();
            nudgeLeftRatio(-0.02);
          } else if (event.key === "ArrowRight") {
            event.preventDefault();
            nudgeLeftRatio(0.02);
          }
        }}
      />

      {/* 中列 + 右列：右列宽度固定，不参与拖拽 */}
      <div class="flex min-h-0 min-w-0 flex-1">
              {/* ── 中列：照片网格 ─────────────────────────────── */}
              <main class="relative flex min-w-0 flex-1 flex-col bg-surface-bar">
                {/*
                  tiles = **网格 + 下面那条状态条**（人类 2026-09-19：业务上不可分割）。
                  两侧用的是同一个 `TilesShell` + `TilesControlBar`，差异只走显式配置：
                  导入侧现在不传 `sort`（以后想开就传同一份 props，不用改组件）。

                  看图时不给 `bar`（那条位置让给胶片带与看图件）——
                  与浏览侧口径一致：看图态下**没有** tiles 的状态条。
                */}
                <TilesShell
                  bar={
                    viewer.state().active
                      ? null
                      : {
                          count: grid.items().length,
                          selectedCount: grid.selectedCount(),
                          dir: grid.dir(),
                          // 「当前那张」= 选择锚点（最后一次点的）
                          fileName: fileNameOf(grid.selection().anchor ?? ""),
                          byTime: grid.byTime(),
                          onByTimeChange: grid.setByTime,
                          tileStep: grid.tileStep(),
                          onTileStepChange: grid.setTileStep,
                          onTileStepCommit: grid.commitTileStep,
                          locale: groupingLocale(),
                          loadingTimes: grid.loadingTimes(),
                        }
                  }
                >
                <Show
                  when={viewer.state().active}
                  fallback={
                    /*
                     * 网格是全项目唯一那份（`PhotoGrid`）；导入侧的差异通过**数据源适配器**
                     * 传进去（排除状态住在工作区 store：跨目录、跨源一份，网格只负责显示）。
                     */
                    <PhotoGrid source={gridSource()} watermark={() => gridWatermark()} />
                  }
                >
                  <Show
                    when={comparing()}
                    fallback={
                      <Viewer
                        store={viewer}
                        class="z-10"
                        onClose={() => {
                          // 退回 tiles 时左右栏必定回来（与 browse 同一条规矩）；
                          // **关 store 归 Viewer 自己做** —— 这里只管外壳状态
                          setChrome("default");
                        }}
                      />
                    }
                  >
                    <CompareView
                      photos={comparePhotos()}
                      selectedCount={grid.selectedCount()}
                      store={viewer}
                      /* 对比态的返回（左上角那颗）：与单张看图同一条规矩 */
                      onClose={() => setChrome("default")}
                      onFocus={(photo) => {
                        const at = viewer.state().photos.findIndex((item) => item.id === photo.id);
                        if (at >= 0) viewer.focus(at);
                      }}
                      class="z-10"
                    />
                  </Show>
                </Show>
                </TilesShell>

                {/* 胶片带：仅看图态可见；`view only`（第③态）整条收起 */}
                <Show when={viewer.state().active && chromeShowsFilm(chrome())}>
                  <FilmStrip
                    viewer={viewer}
                    selectedIds={grid.selectedIds()}
                    /* 选择语义全在网格 store 里（`clickItem` 与 tiles 点一下是同一条路） */
                    onSelect={(id, mode) => grid.clickItem(id, mode)}
                    thumbs={filmThumbs}
                    class="shrink-0"
                  />
                </Show>
              </main>

              {/* ── 右列：库（固定宽，不可拖）─────────────────── */}
              <aside class="flex w-panel-w-right shrink-0 flex-col gap-2 bg-surface-main p-panel-pad">
        <p class="text-fs-1 tracking-wide text-fg-2 uppercase">
          {/* 面板标题说清是「导入到哪个库」—— 单说「库」太模糊 */}
          {t("import.dest_library")}
        </p>

        <RepositoryList
          repositories={store.repositories()}
          status={store.repositoriesStatus()}
          error={store.repositoriesError()}
          selectedId={store.selectedRepositoryId()}
          remountingId={store.remountingId()}
          remountErrors={store.remountErrors()}
          onSelect={store.selectRepository}
          onRemount={(id) => void store.remount(id)}
          onCreate={() => setCreating(true)}
          onRetry={() => void store.reloadRepositories()}
          onTemplateSaved={store.applyRepositoryTemplate}
          onRepositoryStale={store.markRepositoryOffline}
          locale={groupingLocale()}
          class="min-h-0 flex-1"
          {...(props.toast === undefined ? {} : { toast: props.toast })}
        />

        <RepositoryFooter
          checkedDirs={store.checkedDirs()}
          photoCount={store.checkedPhotoCount()}
          excludedCount={store.excludedInChecked()}
          hasRepository={store.selectedRepository() !== null}
          repositoryOnline={store.selectedRepository()?.online ?? false}
          avoidDuplicates={store.avoidDuplicates()}
          onAvoidDuplicatesChange={store.setAvoidDuplicates}
          onImport={() => void startImport()}
          locale={groupingLocale()}
        />

        {/* 空间偏紧：问一句再开工（不做成弹窗套弹窗，就一行） */}
        <Show when={spaceWarning()}>
          {(message) => (
            <div class="flex flex-col gap-1 rounded-ui bg-surface-track p-2">
              <p class="text-fs-1 text-danger">{message()}</p>
              <div class="flex justify-end gap-2">
                <button
                  type="button"
                  class="cursor-pointer text-fs-1 text-fg-2 underline"
                  onClick={() => setSpaceWarning(null)}
                >
                  {t("common.cancel")}
                </button>
                <button
                  type="button"
                  class="cursor-pointer text-fs-1 text-brand"
                  onClick={() => {
                    const repository = store.selectedRepository();
                    if (repository !== null) void beginImport(repository.id);
                  }}
                >
                  {t("import.space_continue")}
                </button>
              </div>
            </div>
          )}
        </Show>

        <ImportProgressDialog
          store={importStore}
          {...(props.onRevealInLibrary === undefined
            ? {}
            : { onRevealInLibrary: props.onRevealInLibrary })}
        />

        <CreateRepositoryDialog
          open={creating()}
          onOpenChange={setCreating}
          onCreated={(view) => {
            store.upsertRepository(view);
            store.selectRepository(view.id);
          }}
                />
      </aside>
      </div>
    </div>
  );
}
