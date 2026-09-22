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
import { uiReady, tauriWindowHandle } from "./api/window.ts";
import { isTauriRuntime } from "./api/tauri-env.ts";
import type { BrowseSort } from "./api/types.ts";
import { t, timeoutMessage } from "./i18n/index.ts";
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
import { browseActions } from "./features/browse/actions.ts";
import { applyMarkIntent } from "./features/browse/mark-actions.ts";
import { importActions } from "./features/import/actions.ts";
import { viewerActions } from "./components/ui/viewer/actions.ts";
import {
  CommandPalette,
  ShortcutSettingsDialog,
  createCommandDispatcher,
  createCommandRegistry,
  type CommandDeps,
} from "./features/commands/index.ts";
import { chordOf, type CommandSpec } from "./lib/commands.ts";
import { rememberCommand, shortcutOverrides } from "./lib/shortcuts.ts";
import {
  browseDisplayByTime,
  browseDisplayInfoMode,
  browseDisplayTileStep,
  commitBrowseDisplayTileStep,
  commitImportDisplayTileStep,
  importDisplayByTime,
  importDisplayInfoMode,
  importDisplayTileStep,
  setBrowseDisplayByTime,
  setBrowseDisplayTileStep,
  setImportDisplayByTime,
  setImportDisplayTileStep,
} from "./lib/display-prefs.ts";
import {
  cycleBrowseTileInfo,
  toggleImportTileInfo,
} from "./components/ui/tile-info.ts";
import { locale, nextLocale, setLocale } from "./i18n/index.ts";
import { LibrarySettingsDialog } from "./features/repositories/index.ts";
import { browseDelete, browseFacets, browseMark, browseMarkings, browsePage, browseRedo, browseTimeline, browseUndo, flagsClear, flagsGet, flagsSet, tagList } from "./api/browse.ts";
import { BrowseWorkspace } from "./workspaces/browse/index.ts";
import {
  applyNotice,
  MigrationGate,
  NO_MIGRATIONS,
  type MigrationMap,
} from "./features/migration/index.ts";
import { readBrowseSession, writeBrowseSession } from "./lib/browse-session.ts";
import { withTimeout } from "./lib/timeout.ts";
import { createFilmStripPreferenceStore } from "./lib/film-strip-prefs.ts";

const STARTUP_REPOSITORIES_TIMEOUT_MS = 15_000;

export default function App() {
  const shell = createShellStore();
  const appearance = createAppearanceStore();
  // 布局偏好（设备级）：左列宽度与左列内部的比例，拖拽结束落盘、下次启动还原
  const layout = createLayoutStore();
  const filmStripPrefs = createFilmStripPreferenceStore({
    getSetting: db.getSetting,
    setSetting: db.setSetting,
    keys: {
      import: db.SETTING_KEYS.importFilmStripStep,
      browse: db.SETTING_KEYS.browseFilmStripStep,
    },
    onError: (error, scope, operation) => {
      console.error(`[filmstrip] ${scope} ${operation} failed`, error); // i18n-exempt: 控制台诊断
    },
  });
  const filmStripPrefsReady = filmStripPrefs.load();
  onCleanup(() => filmStripPrefs.dispose());
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
   * 而 store 的生命周期不该跟着工作流开关走（切回来时滚动位置与选择还在）。
   * 但「数据还是新的吗」**不跟着 store 跨工作流** —— 每次进浏览都要重读一遍，
   * 见 `BrowseWorkspace` 的 `onMount`。
   */
  const browseStore = createBrowseStore({
    /*
     * 补读元信息的口子：老库里 `assets.width/height` 可能是 NULL（2026-09-18 之前的导入
     * 不写 EXIF），那批照片的 tile 比例与对比尺寸都靠它兜底。
     */
    metaEnsure: db.dirMetaEnsure,
    api: {
      page: browsePage,
      timeline: browseTimeline,
      facets: browseFacets,
      markings: browseMarkings,
      tagList,
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
   * 启动落点（人类 2026-09-20）：只要登记过库就进 browse；完全没有库才进 import。
   * 选库优先级 = 上次会话明确记录 → `lastOpenedAt` 最新 → 列表第一项；目录只在库也匹配时恢复。
   *
   * 这是设备级导航记忆，不进 catalog：catalog 跟着库移动，而「这台电脑上次看到哪里」属于 app。
   * 本地存储若损坏/路径过期，`readBrowseSession` 会丢掉非法值，browse 仍能落在库选择器上。
   */
  const [startupResolved, setStartupResolved] = createSignal(false);
  const startupReady = withTimeout(
    db.listRepositories(),
    STARTUP_REPOSITORIES_TIMEOUT_MS,
    timeoutMessage("startup.timeout.repositories", STARTUP_REPOSITORIES_TIMEOUT_MS),
  )
    .then((repositories) => {
      if (repositories.length === 0) {
        shell.setWorkflow("import");
        return;
      }

      shell.setWorkflow("browse");
      const saved = readBrowseSession();
      const preferred =
        repositories.find((repository) => repository.id === saved.repositoryId) ??
        [...repositories].sort(
          (a, b) => (b.lastOpenedAt ?? Number.MIN_SAFE_INTEGER) - (a.lastOpenedAt ?? Number.MIN_SAFE_INTEGER),
        )[0];
      browseStore.setRepository(preferred.id);
      if (saved.repositoryId === preferred.id && saved.scopePath !== null) {
        browseStore.setScope(saved.scopePath);
      }
    })
    .catch((error: unknown) => {
      // 启动读库失败不能被误判为「没有库」；保留默认 import，让界面可用，同时留诊断证据。
      console.error("[startup] 读取库注册表失败", error); // i18n-exempt: 控制台诊断
    })
    .finally(() => setStartupResolved(true));

  createEffect(() => {
    if (!startupResolved()) return;
    writeBrowseSession({
      repositoryId: browseStore.repositoryId(),
      scopePath: browseStore.scopePath(),
    });
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
  /*
   * 数据库升级的阻塞遮罩（人类 2026-09-19）：外壳只做**订阅**这件事，
   * 「哪些库正在升级、显示哪一条」是 `features/migration/notice.ts` 的纯逻辑。
   *
   * 为什么挂在组装层：升级可能在启动时就发生（`app.db`），也可能在用户第一次点库时
   * 才发生（`catalog.db` —— 人类要求「只在真正用到时才检查 + 升级」）。
   * 遮罩要盖住**整个窗口**，所以它属于最外层，不属于某个工作区。
   */
  const [migrations, setMigrations] = createSignal<MigrationMap>(NO_MIGRATIONS);
  void db.onMigrationNotice((notice) => {
    setMigrations((previous) => applyNotice(previous, notice));
  });

  onMount(() => {
    const fontsReady =
      typeof document !== "undefined" && "fonts" in document
        ? document.fonts.ready.catch(() => undefined)
        : Promise.resolve();
    void Promise.all([
      startupReady,
      filmStripPrefsReady,
      Promise.race([
        fontsReady,
        new Promise((resolve) => setTimeout(resolve, 400)),
      ]),
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
   * 库设置弹窗（齿轮）：**导入侧与浏览侧共用同一个弹窗**，由组装层持有。
   *
   * 为什么放这里：导入侧的齿轮长在 `RepositoryList` 里（那边自己持有弹窗），
   * 而浏览侧的库卡片在 `BrowseLeftColumn` 里 —— 两个工作区都放一份弹窗状态就是两份真相。
   * 浏览侧从这里开；导入侧维持原样（它还要把「模版已保存」回写给列表）。
   */
  const [librarySettingsId, setLibrarySettingsId] = createSignal<string | null>(null);
  /**
   * 提示通道（`components/ui/Toast.tsx`）：挂在**根层** —— 模态/条带都有自己的层叠上下文，
   * 提示要永远在最上面（`--z-toast`），所以由组装层建、往下传。
   */
  const toast = createToastStore();
  onCleanup(toastDisposer(toast));

  /* ── 命令体系（`features/commands/`）的组装（`plans/M2-W3.md` §2.1）── */

  const [paletteOpen, setPaletteOpen] = createSignal(false);
  const [shortcutsOpen, setShortcutsOpen] = createSignal(false);
  const [aboutOpen, setAboutOpen] = createSignal(false);
  /** 「新建库」的请求计数：导入工作区看到它变就开弹窗（弹窗状态住在那个工作区） */
  const [newRepositoryRequest, setNewRepositoryRequest] = createSignal(0);

  /** 找一份窗口句柄（浏览器里是 `null`：那三条命令静默不做） */
  type BrowseSortKey = NonNullable<BrowseSort["key"]>;
  const withWindow = async (act: (handle: {
    minimize: () => Promise<void>;
    toggleMaximize: () => Promise<void>;
    close: () => Promise<void>;
  }) => Promise<void>): Promise<void> => {
    const handle = await tauriWindowHandle();
    if (handle !== null) await act(handle);
  };

  /** 当前挂载的工作区的动作槽（同一时刻只有一个） */
  const activeWorkspaceActions = () => browseActions() ?? importActions();
  const viewing = (): boolean => activeWorkspaceActions()?.viewing() ?? false;
  const filmVisible = (): boolean => activeWorkspaceActions()?.filmVisible() ?? false;
  const importFlow = (): boolean => shell.workflow() === "import";

  const commandDeps: CommandDeps = {
    flow: shell.workflow,
    setFlow: shell.setWorkflow,
    theme: appearance.theme,
    toggleTheme: appearance.toggleTheme,
    density: appearance.density,
    setDensity: appearance.setDensity,
    toggleLocale: () => setLocale(nextLocale(locale())),
    window: {
      available: isTauriRuntime,
      minimize: () => void withWindow((handle) => handle.minimize()),
      toggleMaximize: () => void withWindow((handle) => handle.toggleMaximize()),
      close: () => void withWindow((handle) => handle.close()),
    },
    openPalette: () => setPaletteOpen(true),
    openShortcuts: () => setShortcutsOpen(true),
    openAbout: () => setAboutOpen(true),
    openTags: () => setTagsOpen(true),
    openLibrarySettings: () => {
      const id = browseStore.repositoryId();
      if (id !== null) setLibrarySettingsId(id);
    },
    openNewRepository: () => setNewRepositoryRequest((count) => count + 1),
    display: {
      byTime: () => (importFlow() ? importDisplayByTime() : browseDisplayByTime()),
      setByTime: (value) =>
        importFlow() ? setImportDisplayByTime(value) : setBrowseDisplayByTime(value),
      infoMode: () => (importFlow() ? importDisplayInfoMode() : browseDisplayInfoMode()),
      cycleInfo: () => (importFlow() ? toggleImportTileInfo() : cycleBrowseTileInfo()),
      tileStep: () => (importFlow() ? importDisplayTileStep() : browseDisplayTileStep()),
      setTileStep: (value) =>
        importFlow() ? setImportDisplayTileStep(value) : setBrowseDisplayTileStep(value),
      commitTileStep: () =>
        importFlow() ? commitImportDisplayTileStep() : commitBrowseDisplayTileStep(),
    },
    viewer: {
      viewing,
      comparing: () =>
        shell.workflow() === "import"
          ? (importActions()?.comparing() ?? false)
          : (browseActions()?.comparing() ?? false),
      filmVisible,
      actions: viewerActions,
    },
    browse: {
      repositoryId: browseStore.repositoryId,
      undo: () => void browseStore.undo(),
      redo: () => void browseStore.redo(),
      canUndo: () => browseStore.undoState().canUndo,
      canRedo: () => browseStore.undoState().canRedo,
      hasSelection: () => browseStore.selectedCount() > 0,
      selectedCount: browseStore.selectedCount,
      selectAll: () => browseStore.selectAll(),
      clearSelection: () => browseStore.clearSelection(),
      // **与工具条同一份实现**（`features/browse/mark-actions.ts`）：筛选态改条件、标记态打标
      mark: (action) => void applyMarkIntent(browseStore, action, toast),
      setFlag: (value) => void browseStore.setFlag(browseStore.selectedIds(), value),
      filterMode: browseStore.filterMode,
      toggleFilter: () => browseStore.setFilterMode(!browseStore.filterMode()),
      clearFilter: () => browseStore.setFilterMode(false),
      sortKey: () => browseStore.sort().key ?? "takenAt",
      setSortKey: (key) =>
        browseStore.setSort({ ...browseStore.sort(), key: key as BrowseSortKey }),
      toggleSortDirection: () =>
        browseStore.setSort({ ...browseStore.sort(), desc: !browseStore.sort().desc }),
      requestDelete: () => browseActions()?.requestDelete(),
      moveFocus: (delta) => browseActions()?.moveFocus(delta),
      openViewer: () => browseActions()?.openViewer(),
      cycleChrome: () => browseActions()?.cycleChrome(),
      toggleCompareStrip: () => browseActions()?.toggleCompareStrip(),
    },
    import: {
      hasSelection: () => grid.hasSelection(),
      selectedCount: () => grid.selectedIds().size,
      selectAll: () => grid.selectAll(),
      excludeSelected: () => importStore.toggleExcluded([...grid.selectedIds()]),
      openViewer: () => importActions()?.openViewer(),
      cycleChrome: () => importActions()?.cycleChrome(),
      toggleCompareStrip: () => importActions()?.toggleCompareStrip(),
    },
  };

  const commands = createCommandRegistry(commandDeps);

  /** 跑一条命令（**唯一入口**：菜单、命令面板、以后可能的别处都走它） */
  const runCommand = (command: CommandSpec): void => {
    rememberCommand(command.id);
    void command.run();
  };

  const dispatcher = createCommandDispatcher({
    commands: () => commands,
    overrides: shortcutOverrides,
    blocked: () =>
      paletteOpen() ||
      shortcutsOpen() ||
      aboutOpen() ||
      document.querySelector('[role="dialog"]') !== null,
    onRun: (command) => {
      // 「打开面板」这类命令会把面板开开关关，别让分发器的日志把它们写成递归
      if (command.id !== "help.palette") setPaletteOpen(false);
    },
  });
  onMount(() => {
    dispatcher.attach();
    onCleanup(() => dispatcher.dispose());
  });

  /** 菜单项的键位提示（菜单与面板读同一份覆盖表） */
  const menuShortcut = (command: CommandSpec): string | undefined =>
    chordOf(command, shortcutOverrides()) ?? undefined;
  void menuShortcut;

  return (
    <div class="flex h-full w-full flex-col bg-surface-main text-fg-1">
      <TitleBar
        store={shell}
        appearance={appearance}
        commands={commands}
        onRun={runCommand}
        aboutOpen={aboutOpen()}
        onAboutOpenChange={setAboutOpen}
      />
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

      {/*
        库设置（齿轮）：浏览侧的那条路。弹窗自带「两个计数 + 重建数据」，
        关掉之后浏览工作区下次进目录时会重新同步计数。
      */}
      <LibrarySettingsDialog
        open={librarySettingsId() !== null}
        repositoryId={librarySettingsId()}
        toast={toast}
        onOpenChange={(open) => {
          if (!open) setLibrarySettingsId(null);
        }}
        onSaved={() => {
          // 改了模版：库列表那份缓存也要跟着刷新（它显示的是模版与计数）
          void browseStore.reload();
        }}
      />

      {/* 提示（右上角、不阻塞、约 5 秒；带「撤销」的动作把撤销放在自己身上） */}
      <ToastHost store={toast} />

      {/*
        命令面板与快捷键设置（`plans/M2-W3.md`）：都挂在**根层** ——
        它们自己带遮罩与层叠，不能困在条带或工作区的上下文里。
        面板的 `onRun` 走 `runCommand`（与菜单、分发器同一个入口）。
      */}
      <CommandPalette
        open={paletteOpen()}
        onOpenChange={setPaletteOpen}
        commands={commands}
        onRun={(command) => {
          setPaletteOpen(false);
          runCommand(command);
        }}
      />
      <ShortcutSettingsDialog
        open={shortcutsOpen()}
        onOpenChange={setShortcutsOpen}
        commands={commands}
        onSaved={() => toast.show({ tone: "success", message: t("shortcuts.saved") })}
      />

      {/* 数据库升级：全窗口阻塞遮罩（不给出口 —— 升级是原子操作，只能等） */}
      <MigrationGate notices={migrations()} />

      <Show when={shell.workflow() === "browse"} fallback={
        <ImportWorkspace
          store={importStore}
          grid={grid}
          toast={toast}
          onRevealInLibrary={() => shell.setWorkflow("browse")}
          leftRatio={initialLayout.leftRatio}
          onLeftRatioChange={layout.setLeftRatio}
          recentRatio={initialLayout.recentRatio}
          onRecentRatioChange={layout.setRecentRatio}
          filmStripStep={filmStripPrefs.step("import")}
          onFilmStripStepChange={(step) => filmStripPrefs.setStep("import", step)}
          /* 命令面板里的「新建库…」靠它打开导入侧的弹窗（状态住在那个工作区） */
          openCreateRequest={newRepositoryRequest()}
        />
      }>
        <BrowseWorkspace
          store={browseStore}
          onOpenLibrarySettings={(id) => setLibrarySettingsId(id)}
          toast={toast}
          leftWidth={layout.prefs().browseLeftWidth}
          onLeftWidthChange={(width) => layout.setBrowseLeftWidth(width)}
          filmStripStep={filmStripPrefs.step("browse")}
          onFilmStripStepChange={(step) => filmStripPrefs.setStep("browse", step)}
        />
      </Show>
    </div>
  );
}
