/**
 * 应用组装（`ARCHITECTURE.md` §1 的最上层）。
 *
 * 结构就是设计稿的三行 + 工作区：
 *   titlebar   (bar)   —— 沉浸式，接管窗口拖动与三键
 *   flowbar    (main)  —— 工作流 + EXIF + 吸附
 *   toolsbar   (main)  —— 随工作流装配，**无内容时整行不存在**
 *   workspace          —— 导入工作区三列
 *
 * 状态都在这一层创建、往下传（`ARCHITECTURE.md` §3 的状态归属）：
 *   - **外壳状态**（当前工作流、菜单可见性）→ `shell/store.ts`
 *   - **外观状态**（主题、密度）→ `lib/appearance.ts`（建店时就会落到 `<html>` 上）
 *   - **导入工作区的共享状态** → `workspaces/import/store.ts`
 *   - **照片网格**（选择 / 排除 / 照片清单）→ `features/photo-grid/store.ts`
 *
 * 为什么网格的 store 也在这里创建、而不是藏在工作区里：**照片选择横跨外壳与工作区** ——
 * 外壳 `toolsbar` 上的「批量排除」要用它，工作区里的网格要写它。
 * 放进任何一边都会让另一边去钻内部实现。
 */

import { createEffect, createSignal, onCleanup } from "solid-js";
import * as db from "./api/db.ts";
import type { ExifData } from "./features/exif-strip/index.ts";
import { toExifData } from "./features/exif-strip/index.ts";
import { createPhotoGridStore } from "./features/photo-grid/index.ts";
import { createAppearanceStore } from "./lib/appearance.ts";
import { FlowBar } from "./shell/FlowBar.tsx";
import { createShellStore } from "./shell/store.ts";
import { TitleBar } from "./shell/TitleBar.tsx";
import { ToolsBar } from "./shell/ToolsBar.tsx";
import { createImportStore, ImportWorkspace } from "./workspaces/import/index.ts";

export default function App() {
  const shell = createShellStore();
  const appearance = createAppearanceStore();
  const importStore = createImportStore({ api: db });
  const grid = createPhotoGridStore({ api: db });

  /*
   * flowbar 的图片信息区：**只选了一张**时才去读它的 EXIF。
   * 多选时不显示（显示哪一张都不对），没有选择时是空态。
   */
  const [exif, setExif] = createSignal<ExifData | null>(null);
  createEffect(() => {
    const selected = grid.selectedIds();
    if (selected.size !== 1) {
      setExif(null);
      return;
    }
    const path = selected.values().next().value as string;
    let cancelled = false;
    void db
      .readFileExif(path)
      .then((file) => {
        if (!cancelled) setExif(toExifData(file));
      })
      .catch(() => {
        if (!cancelled) setExif(null);
      });
    onCleanup(() => {
      cancelled = true;
    });
  });

  return (
    <div class="flex h-full w-full flex-col bg-surface-main text-fg-1">
      <TitleBar store={shell} appearance={appearance} />
      <FlowBar store={shell} exif={exif()} />

      {/*
        批量排除（`DESIGN.md` §12.2 的**反转**语义）：没有选中项时禁用。
        选择状态来自照片网格 —— 外壳不认识照片，只认「有没有选」。
      */}
      <ToolsBar
        store={shell}
        hasSelection={grid.hasSelection()}
        onBatchExclude={grid.toggleExcludedSelected}
      />

      <ImportWorkspace store={importStore} grid={grid} />
    </div>
  );
}
