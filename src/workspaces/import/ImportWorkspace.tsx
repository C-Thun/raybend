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
import { LeftColumn } from "./LeftColumn.tsx";

export interface ImportWorkspaceProps {
  store: ImportStore;
  /** 照片网格的状态（在组装层创建，因为外壳的 `toolsbar` 也要读它的选择） */
  grid: PhotoGridStore;
}

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
      const precheck = await importPrecheck(repository.id, sources());
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

  // 左列选中哪个目录，中列就跟着换（选中是**一个**状态，跨面板同步）
  createEffect(() => grid.setSourceDir(store.selectedDir()));

  return (
    <div class="flex min-h-0 flex-1">
      <aside class="flex w-panel-w-left shrink-0 flex-col bg-surface-main p-panel-pad">
        <LeftColumn store={store} />
      </aside>

      {/* ── 中列：照片网格 ─────────────────────────────── */}
      <main class="flex min-w-0 flex-1 flex-col bg-surface-bar">
        <PhotoGrid store={grid} />
      </main>

      {/* ── 右列：库 ─────────────────────────────────── */}
      <aside class="flex w-panel-w-right shrink-0 flex-col gap-2 bg-surface-main p-panel-pad">
        <p class="text-fs-1 tracking-wide text-fg-2 uppercase">
          {t("repo.title")}
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
          locale={groupingLocale()}
          class="min-h-0 flex-1"
        />

        <RepositoryFooter
          checkedDirs={store.checkedDirs()}
          photoCount={store.checkedPhotoCount()}
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

        <ImportProgressDialog store={importStore} />

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
  );
}
