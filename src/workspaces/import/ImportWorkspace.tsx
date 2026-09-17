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

import { createEffect, createSignal, onMount, Show } from "solid-js";
import * as importCommands from "../../api/import.ts";
import { importPrecheck, onImportProgress, type ImportSource } from "../../api/import.ts";
import { locale, t } from "../../i18n/index.ts";
import { PhotoGrid, type PhotoGridStore } from "../../features/photo-grid/index.ts";
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
import { SplitHandleDots } from "../../components/ui/SplitHandle.tsx";
import { withTimeout } from "../../lib/timeout.ts";
import { timeoutMessage } from "../../i18n/index.ts";
import { LeftColumn } from "./LeftColumn.tsx";

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
  const [dragging, setDragging] = createSignal(false);

  /** 左列宽度的上下限（比例）：与 `lib/layout-prefs.ts` 的范围一致，再加一道像素下限 */
  const clampLeftRatio = (ratio: number): number =>
    Math.min(0.5, Math.max(0.12, ratio));

  function beginResize(event: PointerEvent & { currentTarget: HTMLDivElement }): void {
    const handle = event.currentTarget;
    const container = handle.parentElement;
    if (!container) return;
    const width = container.getBoundingClientRect().width;
    if (width <= 0) return;

    const startX = event.clientX;
    const startRatio = leftRatio();
    setDragging(true);
    try {
      handle.setPointerCapture(event.pointerId);
    } catch {
      // 合成事件（冒烟脚本）或异常指针 id 时可能拿不到捕获：
      // 拿不到也照样能用下面挂在元素上的监听拖 —— 不要因此把整个拖动搞崩
    }

    const onMove = (move: PointerEvent): void => {
      setLeftRatio(clampLeftRatio(startRatio + (move.clientX - startX) / width));
    };
    const finish = (): void => {
      handle.removeEventListener("pointermove", onMove);
      handle.removeEventListener("pointerup", finish);
      handle.removeEventListener("pointercancel", finish);
      setDragging(false);
      // **松手才落盘**（拖拽中间写存储既是浪费也会引起无谓的重渲染）
      props.onLeftRatioChange?.(leftRatio());
    };
    handle.addEventListener("pointermove", onMove);
    handle.addEventListener("pointerup", finish);
    handle.addEventListener("pointercancel", finish);
  }

  /** 键盘调整：左右方向键各 2%，立刻落盘（一次按键就是一次「结束」） */
  function nudgeLeftRatio(delta: number): void {
    const next = clampLeftRatio(leftRatio() + delta);
    setLeftRatio(next);
    props.onLeftRatioChange?.(next);
  }

  return (
    /*
     * 工作区 = 普通 flex 行 + **自写的宽度把手**（原则见 DESIGN.md §8.6：只给左边）。
     * 右列宽度固定（`--panel-w-right`），不参与拖拽。
     */
    <div class="flex min-h-0 flex-1">
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

      {/* 三点把手：拖它只改上面那个百分比 */}
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label={t("common.resize_left")}
        aria-valuenow={Math.round(leftRatio() * 100)}
        aria-valuemin={12}
        aria-valuemax={50}
        tabindex="0"
        class={[
          "group/split flex h-full w-2 shrink-0 cursor-col-resize items-center justify-center outline-none",
          dragging() ? "bg-state-selected" : "hover:bg-state-hover",
        ].join(" ")}
        onPointerDown={beginResize}
        onKeyDown={(event) => {
          if (event.key === "ArrowLeft") {
            event.preventDefault();
            nudgeLeftRatio(-0.02);
          } else if (event.key === "ArrowRight") {
            event.preventDefault();
            nudgeLeftRatio(0.02);
          }
        }}
      >
        <SplitHandleDots orientation="vertical" active={dragging()} />
      </div>

      {/* 中列 + 右列：右列宽度固定，不参与拖拽 */}
      <div class="flex min-h-0 min-w-0 flex-1">
              {/* ── 中列：照片网格 ─────────────────────────────── */}
              <main class="flex min-w-0 flex-1 flex-col bg-surface-bar">
                {/* 排除状态住在工作区 store（跨目录、跨源一份），网格只负责显示 */}
                <PhotoGrid store={grid} isExcluded={store.isExcluded} />
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
