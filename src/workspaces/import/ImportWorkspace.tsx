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

import { createEffect, onMount } from "solid-js";
import { PhotoGrid, type PhotoGridStore } from "../../features/photo-grid/index.ts";
import { t } from "../../i18n/index.ts";
import type { ImportStore } from "./store.ts";
import { LeftColumn } from "./LeftColumn.tsx";

export interface ImportWorkspaceProps {
  store: ImportStore;
  /** 照片网格的状态（在组装层创建，因为外壳的 `toolsbar` 也要读它的选择） */
  grid: PhotoGridStore;
}

export function ImportWorkspace(props: ImportWorkspaceProps) {
  const store = props.store;
  const grid = props.grid;

  onMount(() => {
    // 三个列表各拉一次；互不依赖，失败各自记状态（不阻塞其它面板）
    void store.reloadRecent();
    void store.reloadVolumes();
    void store.reloadRepositories();
    // 网格的偏好（档位 / 按时间 / 时间片阈值）从设置里读回
    void grid.hydrate();
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

      {/* ── 右列：库（C 批）───────────────────────────── */}
      <aside class="flex w-panel-w-right shrink-0 flex-col bg-surface-main p-panel-pad">
        <p class="text-fs-1 tracking-wide text-fg-2 uppercase">
          {t("repo.title")}
        </p>
      </aside>
    </div>
  );
}
