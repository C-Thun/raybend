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

import { Show, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { uiReady } from "./api/window.ts";
import { t } from "./i18n/index.ts";
import * as db from "./api/db.ts";
import type { ExifData } from "./features/exif-strip/index.ts";
import { assetItemExif, toExifData } from "./features/exif-strip/index.ts";
import { createPhotoGridStore } from "./features/photo-grid/index.ts";
import { createAppearanceStore } from "./lib/appearance.ts";
import { createLayoutStore } from "./lib/layout-prefs.ts";
import { FlowBar } from "./shell/FlowBar.tsx";
import { createShellStore } from "./shell/store.ts";
import { TitleBar } from "./shell/TitleBar.tsx";
import { ToolsBar } from "./shell/ToolsBar.tsx";
import { createImportStore, ImportWorkspace } from "./workspaces/import/index.ts";
import { createToastStore, ToastHost, toastDisposer } from "./components/ui/Toast.tsx";
import { BrowseToolbar, createBrowseStore, TagDialog } from "./features/browse/index.ts";
import { browseDelete, browseFacets, browseMark, browseMarkings, browsePage, browseRedo, browseTimeline, browseUndo, flagsClear, flagsGet, flagsSet } from "./api/browse.ts";
import { BrowseWorkspace } from "./workspaces/browse/index.ts";

export default function App() {
  const shell = createShellStore();
  const appearance = createAppearanceStore();
  // 布局偏好（设备级）：左列宽度与左列内部的比例，拖拽结束落盘、下次启动还原
  const layout = createLayoutStore();
  /*
   * ⚠️ 比例**只在启动时读一次**（故意不放在 JSX 里，避开响应式跟踪）：
   * 如果让 `layout.prefs()` 参与渲染，就会形成回路 ——
   *   拖动/窗口缩放 → `onResizeEnd` 写 store → App 重渲染 → splitter 收到新的 `defaultSize`
   *   → Ark 重新布局 → 又一次 resize …… 真机症状就是「启动卡几秒、一最大化直接卡死」。
   * 比例本来就是**初始值**（Ark 只在首次渲染用它），所以读一次就够。
   */
  const initialLayout = layout.prefs();
  const importStore = createImportStore({ api: db });
  const grid = createPhotoGridStore({ api: db });
  /*
   * 浏览工作区的状态（`features/browse/store.ts`）。
   *
   * 与网格 store 一样在组装层创建：**当前工作流要决定渲染谁**，
   * 而 store 的生命周期不该跟着工作流开关走（切回来时窗口数据还在，不用重新加载）。
   */
  const browseStore = createBrowseStore({
    api: {
      page: browsePage,
      timeline: browseTimeline,
      facets: browseFacets,
      markings: browseMarkings,
      mark: browseMark,
      undo: browseUndo,
      redo: browseRedo,
      remove: browseDelete,
      flagsGet,
      flagsSet,
      flagsClear,
    },
  });

  /*
   * 启动闪屏的收尾（见 `src-tauri/src/lib.rs` 的 `ui_ready`）。
   *
   * 时机：**首屏挂载之后，并且等字体就绪**（限时 400ms）——
   * 主窗口是藏着的，所以在这之前把字换掉，用户看到的第一眼就是成品；
   * 但字体加载慢（或拿不到字体）也不能把闪屏拖住，所以给一个上限。
   * `requestAnimationFrame` 再推一帧：确保这一次 DOM 改动已经画出来了。
   *
   * 浏览器里（`pnpm dev`）`uiReady()` 直接返回，什么也不做。
   */
  onMount(() => {
    const fontsReady =
      typeof document !== "undefined" && "fonts" in document
        ? document.fonts.ready.catch(() => undefined)
        : Promise.resolve();
    void Promise.race([
      fontsReady,
      new Promise((resolve) => setTimeout(resolve, 400)),
    ]).then(() => {
      requestAnimationFrame(() => void uiReady());
    });
  });

  /*
   * flowbar 右侧的图片信息区（约定叫 **flowinfo**）——
   * **切到哪个 flow 就跟着哪个 flow 走**，那个 flow 没选中照片就清空
   * （人类 2026-09-19 定的口径；以前只认导入侧的选择，所以在浏览里选图它一动不动）。
   *
   * 两条路的取法不一样，但显示规则同源：
   *   - 导入：中列只给了路径 → 读一次 EXIF（异步）。只选一张才读，多选显示哪张都不对；
   *   - 浏览：列表项**本来就带着**这些字段 → 直接换形状，不用 IPC（本地应用能立刻给就别绕）。
   */
  const [importExif, setImportExif] = createSignal<ExifData | null>(null);
  createEffect(() => {
    const selected = grid.selectedIds();
    // 不在导入工作流就不读 —— 切回浏览时那块信息不该还留着上一张的
    if (shell.workflow() !== "import" || selected.size !== 1) {
      setImportExif(null);
      return;
    }
    const path = selected.values().next().value as string;
    let cancelled = false;
    void db
      .readFileExif(path)
      .then((file) => {
        if (!cancelled) setImportExif(toExifData(file));
      })
      .catch(() => {
        if (!cancelled) setImportExif(null);
      });
    onCleanup(() => {
      cancelled = true;
    });
  });

  /** flowinfo 的内容：当前 flow 的「当前照片」，没有就 `null`（空态） */
  const flowInfo = createMemo<ExifData | null>(() => {
    switch (shell.workflow()) {
      case "import":
        return importExif();
      case "browse": {
        const item = browseStore.anchorItem();
        return item === null ? null : assetItemExif(item);
      }
      default:
        // 编辑 / 导出还没开工：没有「当前照片」就显示空态
        return null;
    }
  });

  /**
   * 标签弹窗开着没有（`BROWSE.md` §3.3）。
   *
   * 状态住在**组装层**而不是工具条里：工具条在 `ToolsBar` 的插槽里，弹窗要挂在
   * 更外层（模态不该被条带的层叠上下文困住），而且它要读 browse store 的选择状态 ——
   * 两边都在这里汇合（`ARCHITECTURE.md` §2 的组合层职责）。
   */
  const [tagsOpen, setTagsOpen] = createSignal(false);
  /**
   * 提示通道（`components/ui/Toast.tsx`）：挂在**根层** —— 模态/条带都有自己的层叠上下文，
   * 提示要永远在最上面（`--z-toast`），所以由组装层建、往下传。
   */
  const toast = createToastStore();
  onCleanup(toastDisposer(toast));

  return (
    <div class="flex h-full w-full flex-col bg-surface-main text-fg-1">
      <TitleBar store={shell} appearance={appearance} />
      <FlowBar store={shell} exif={flowInfo()} />

      {/*
        批量排除（`DESIGN.md` §12.2 的**反转**语义）：没有选中项时禁用。
        选择状态来自照片网格 —— 外壳不认识照片，只认「有没有选」。
      */}
      <ToolsBar
        store={shell}
        hasSelection={shell.workflow() === "browse" ? browseStore.selectedCount() > 0 : grid.hasSelection()}
        // 批量排除是**反转**语义（DESIGN.md §12.2）：排除集合住在导入工作区，
        // 选中的照片清单来自网格 —— 外壳只负责把两边接起来
        onBatchExclude={() => importStore.toggleExcluded([...grid.selectedIds()])}
        // 插槽里到底有没有东西，由这里明说（理由见 ToolsBar 的 hasExtraTools）
        hasExtraTools={shell.workflow() === "browse"}
      >
        {/* 浏览模式的工具（标记系列 / 筛选开关 / 锁）由那个模块自己给 —— 见 ToolsBar 的说明 */}
        <Show when={shell.workflow() === "browse"}>
          <BrowseToolbar
            store={browseStore}
            toast={toast}
            onOpenTags={() => setTagsOpen(true)}
          />
        </Show>
      </ToolsBar>

      {/*
        工作区跟着工作流走（`AGENTS.md` §11.1）：导入 → 三列导入工作区；
        浏览 → 三列浏览工作区（库目录选择器 / 网格 / 信息栏）。
        编辑与导出还没做，落到导入那版（M1 的口径，切过去是空的）。
      */}
      {/*
        标签弹窗（`BROWSE.md` §3.3）：挂在**根层**，不推进 `ToolsBar` 的插槽 ——
        模态有自己的遮罩与层叠（`--z-modal`），放进条带里会被那一层的上下文困住。
      */}
      <TagDialog
        open={tagsOpen()}
        store={browseStore}
        onClose={() => setTagsOpen(false)}
        onDone={(result) => {
          // 标签改动也是「会进撤销栈」的动作：给一条带撤销的提示（与打标同一套口径）
          if (result === null || result.changed === 0) return;
          toast.show({
            tone: "success",
            message: t("browse.tagsSaved").replace("{n}", String(result.changed)),
            action: result.canUndo
              ? { label: t("browse.undo"), onAction: () => void browseStore.undo() }
              : undefined,
          });
        }}
      />

      {/* 提示（右上角、不阻塞、约 5 秒；带「撤销」的动作把撤销放在自己身上） */}
      <ToastHost store={toast} />

      <Show when={shell.workflow() === "browse"} fallback={
        <ImportWorkspace
          store={importStore}
          grid={grid}
          onRevealInLibrary={() => shell.setWorkflow("browse")}
          leftRatio={initialLayout.leftRatio}
          onLeftRatioChange={layout.setLeftRatio}
          recentRatio={initialLayout.recentRatio}
          onRecentRatioChange={layout.setRecentRatio}
        />
      }>
        <BrowseWorkspace
          store={browseStore}
          toast={toast}
          leftWidth={layout.prefs().browseLeftWidth}
          rightWidth={layout.prefs().browseRightWidth}
          onLeftWidthChange={(width) => layout.setBrowseLeftWidth(width)}
          onRightWidthChange={(width) => layout.setBrowseRightWidth(width)}
        />
      </Show>
    </div>
  );
}
