/**
 * 应用组装（`ARCHITECTURE.md` §1 的最上层）。
 *
 * 结构就是设计稿的三行 + 工作区：
 *   titlebar   (bar)   —— 沉浸式，接管窗口拖动与三键
 *   flowbar    (main)  —— 工作流 + EXIF + 吸附
 *   toolsbar   (main)  —— 随工作流装配，**无内容时整行不存在**
 *   workspace          —— 两侧 main / 中央 bar
 *
 * 两条状态在这里创建、往下传（`ARCHITECTURE.md` §3 的状态归属）：
 *   - **外壳状态**（当前工作流、菜单可见性）→ `shell/store.ts`
 *   - **外观状态**（主题、密度）→ `lib/appearance.ts`（建店时就会落到 `<html>` 上）
 *
 * 工作区目前仍是占位：三列内容属于 M1-5，届时这里换成 `src/workspaces/import/`。
 * 现在就把外壳接上真东西，是因为外壳的显隐规则（toolsbar 跟随工作流）**必须现在就能看到** ——
 * 等到 M1-5 再验证，问题会混在照片网格里，分不清是谁的。
 */

import * as db from "./api/db.ts";
import { createAppearanceStore } from "./lib/appearance.ts";
import { FlowBar } from "./shell/FlowBar.tsx";
import { createShellStore } from "./shell/store.ts";
import { TitleBar } from "./shell/TitleBar.tsx";
import { ToolsBar } from "./shell/ToolsBar.tsx";
import { createImportStore, ImportWorkspace } from "./workspaces/import/index.ts";

export default function App() {
  const shell = createShellStore();
  const appearance = createAppearanceStore();
  /*
   * 导入工作区的共享状态（`ARCHITECTURE.md` §3）：创建在组装层、往下传。
   * 只有它能同时被工作区（三列）与外壳的 `toolsbar`（批量排除）用到 ——
   * 照片选择状态横跨这两层，所以不能藏在任何一个 feature 里。
   */
  const importStore = createImportStore({ api: db });

  return (
    <div class="flex h-full w-full flex-col bg-surface-main text-fg-1">
      <TitleBar store={shell} appearance={appearance} />

      {/* EXIF 数据来自 M1-3 的扫描管线；在此之前一律是空态 */}
      <FlowBar store={shell} exif={null} />

      {/*
        批量排除（DESIGN.md §12.2 的**反转**语义）：M1-4 阶段还没有照片可选，
        所以它一直是禁用态 —— 这是**正确**的表现，不是没做完。
        M1-5 会把 `hasSelection` 与回调接到照片网格上。
      */}
      <ToolsBar store={shell} hasSelection={false} />

      {/* ── workspace（导入工作区；其余工作流仍待实现）────────── */}
      <ImportWorkspace store={importStore} />

    </div>
  );
}
