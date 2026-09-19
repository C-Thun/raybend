/**
 * **命令注册表**（`plans/M2-W3.md` §2.1/§3）—— 命令面板、标题栏菜单、快捷键分发器
 * 三者共用的唯一事实来源。
 *
 * ## 这一层做什么、不做什么
 *
 * * **做**：把「一件能做的事」描述成数据 —— id / 文案 key / 分组 / 作用域 / 默认键 /
 *   可用性（`when`、`enabled`）/ 怎么执行（`run`）。
 * * **不做**：不认识任何 store、不认识任何 feature。所有动作由**组装层**
 *   （`App.tsx`）通过 `CommandDeps` 注入 —— 这样 `features/commands/` 不会
 *   import 别的 feature（`lint:arch` 的硬规矩），命令也就能被单测替身驱动。
 *
 * ## 三个判定字段的分工（别混）
 *
 * | 字段 | 回答的问题 | 谁在用 |
 * | --- | --- | --- |
 * | `scope` | 这条命令属于哪个**面**（`global` / `tiles` / `viewer`） | 冲突检测（静态） |
 * | `when()` | **此刻**适不适用（在导入流？在看图？有选中？） | 分发器（决定接不接这个键）、面板（显不显示） |
 * | `enabled()` | 现在**能不能执行**（没有选中时「删除」不可执行） | 菜单（暗着但可见）、面板（暗着不可点） |
 *
 * `scope` 只有三个值是有意的：`tiles` 同时覆盖导入网格与浏览网格（键位与语义本来一样），
 * `viewer` 覆盖单张看图与对比（同一时刻只挂载一个）。
 */

import { infoKeyApplies } from "../../components/ui/tile-info.ts";
import type { TileInfoMode } from "../../lib/display-prefs.ts";
import type { ViewerActions } from "../../components/ui/viewer/actions.ts";
import type { CommandSpec } from "../../lib/commands.ts";

/**
 * 工作流 id（结构与 `shell/flow.ts` 的 `WorkflowId` **一致**）。
 *
 * 为什么不直接 import 那个类型：`features/` 只能向下依赖，而 `shell/` 是**上层**
 * （`lint:arch` 会拦）。组装层把 `shell` 的 workflow 传进来，两边按**结构**对齐 ——
 * 一旦哪边加了工作流，`App.tsx` 那处赋值会当场报错（这比共享一个类型更早暴露漂移）。
 */
export type CommandFlow = "import" | "browse" | "edit" | "export";
import { TILE_SIZE_STEPS } from "../../lib/tile-flow.ts";

/** 组装层要注入的全部能力（**只有函数**：命令不认识 store） */
/**
 * 标记意图（结构与 `features/browse/mark-actions.ts` 的 `MarkIntent` 一致）。
 *
 * 为什么不直接 import 那个类型：`features/` 之间**不许互相 import**（`lint:arch`）。
 * 组装层把两边接上 —— 结构相同就够了（TS 按结构比较）。
 */
export type CommandMarkIntent =
  | { kind: "rating"; value: number }
  | { kind: "color"; value: string | null }
  | { kind: "like"; value: "like" | "dislike" }
  | { kind: "lock"; value: number };

export interface CommandDeps {
  /* ── 外壳 ───────────────────────────────────────── */
  flow: () => CommandFlow;
  setFlow: (flow: CommandFlow) => void;

  /* ── 外观 / 语言 ─────────────────────────────────── */
  theme: () => "dark" | "light";
  toggleTheme: () => void;
  density: () => "compact" | "loose";
  setDensity: (density: "compact" | "loose") => void;
  toggleLocale: () => void;

  /* ── 窗口 ───────────────────────────────────────── */
  window: {
    available: () => boolean;
    minimize: () => void;
    toggleMaximize: () => void;
    close: () => void;
  };

  /* ── 弹窗 ───────────────────────────────────────── */
  openPalette: () => void;
  openShortcuts: () => void;
  openAbout: () => void;
  openTags: () => void;
  openLibrarySettings: () => void;
  openNewRepository: () => void;

  /* ── 网格显示（两个工作区共用一份偏好） ────────────── */
  display: {
    byTime: () => boolean;
    setByTime: (value: boolean) => void;
    infoMode: () => TileInfoMode;
    cycleInfo: () => void;
    tileStep: () => number;
    setTileStep: (index: number) => void;
    commitTileStep: () => void;
  };

  /* ── 看图（当前挂载的那个视图） ───────────────────── */
  viewer: {
    viewing: () => boolean;
    comparing: () => boolean;
    filmVisible: () => boolean;
    actions: () => ViewerActions | null;
  };

  /* ── 浏览 ───────────────────────────────────────── */
  browse: {
    repositoryId: () => string | null;
    undo: () => void;
    redo: () => void;
    canUndo: () => boolean;
    canRedo: () => boolean;
    hasSelection: () => boolean;
    selectedCount: () => number;
    selectAll: () => void;
    clearSelection: () => void;
    mark: (action: CommandMarkIntent) => void;
    setFlag: (value: "pick" | "reject" | null) => void;
    filterMode: () => boolean;
    toggleFilter: () => void;
    clearFilter: () => void;
    sortKey: () => string;
    setSortKey: (key: string) => void;
    toggleSortDirection: () => void;
    /** 打开删除确认（工作区负责确认与失败清单） */
    requestDelete: () => void;
    /** 网格里移动「当前那张」（←/→） */
    moveFocus: (delta: -1 | 1) => void;
    /** 进看图（锚点那张） */
    openViewer: () => void;
    /** `Tab` 三态循环 */
    cycleChrome: () => void;
    /** 对比态：胶片带只显示参与对比的图 */
    toggleCompareStrip: () => void;
  };

  /* ── 导入 ───────────────────────────────────────── */
  import: {
    hasSelection: () => boolean;
    selectedCount: () => number;
    selectAll: () => void;
    excludeSelected: () => void;
    openViewer: () => void;
    cycleChrome: () => void;
  };
}

/** 排序键（浏览侧；与 `BrowseSort["key"]` 同一套值） */
const SORT_KEYS = ["takenAt", "importedAt", "fileName", "rating", "camera"] as const;

/** 色标（与工具条同一套值） */
const COLOR_VALUES = ["red", "yellow", "green", "cyan", "blue", "purple"] as const;

/**
 * 造出全部命令。
 *
 * **顺序即命令面板的默认顺序**（空查询时按它排；同分也按它稳定排序）——
 * 所以这里的分组顺序是**人工编排**的：文件 → 编辑 → 视图 → 看图 → 标记 → 导航 → 导入。
 */
export function createCommandRegistry(deps: CommandDeps): CommandSpec[] {
  const inTiles = (): boolean => !deps.viewer.viewing() && deps.flow() !== "edit" && deps.flow() !== "export";
  const inBrowse = (): boolean => inTiles() && deps.flow() === "browse";
  const inImport = (): boolean => inTiles() && deps.flow() === "import";

  /** 加一条命令（统一填 `when` 默认值，省得每条都写） */
  const spec = (command: CommandSpec): CommandSpec => command;

  return [
    /* ══ 文件 ══════════════════════════════════════════ */
    spec({
      id: "file.newRepository",
      titleKey: "cmd.file.newRepository",
      group: "file",
      menu: "file",
      scope: "global",
      run: () => deps.openNewRepository(),
    }),
    spec({
      id: "file.openRepository",
      titleKey: "cmd.file.openRepository",
      group: "file",
      menu: "file",
      scope: "global",
      run: () => deps.setFlow("browse"),
    }),
    spec({
      id: "file.repositorySettings",
      titleKey: "cmd.file.repositorySettings",
      group: "file",
      menu: "file",
      scope: "global",
      enabled: () => deps.browse.repositoryId() !== null,
      run: () => deps.openLibrarySettings(),
    }),
    spec({
      id: "file.import",
      titleKey: "cmd.file.import",
      group: "file",
      menu: "file",
      scope: "global",
      run: () => deps.setFlow("import"),
    }),
    spec({
      id: "file.exit",
      titleKey: "cmd.file.exit",
      group: "file",
      menu: "file",
      scope: "global",
      dangerous: true,
      enabled: () => deps.window.available(),
      run: () => deps.window.close(),
    }),

    /* ══ 编辑 ══════════════════════════════════════════ */
    spec({
      id: "edit.undo",
      titleKey: "cmd.edit.undo",
      group: "edit",
      menu: "edit",
      scope: "tiles",
      defaultKey: "Mod+Z",
      when: inBrowse,
      enabled: () => deps.browse.canUndo(),
      run: () => deps.browse.undo(),
    }),
    spec({
      id: "edit.redo",
      titleKey: "cmd.edit.redo",
      group: "edit",
      menu: "edit",
      scope: "tiles",
      defaultKey: "Mod+Shift+Z",
      when: inBrowse,
      enabled: () => deps.browse.canRedo(),
      run: () => deps.browse.redo(),
    }),
    spec({
      id: "edit.selectAll",
      titleKey: "cmd.edit.selectAll",
      group: "edit",
      menu: "edit",
      scope: "tiles",
      defaultKey: "Mod+A",
      when: inTiles,
      run: () => (deps.flow() === "import" ? deps.import.selectAll() : deps.browse.selectAll()),
    }),
    spec({
      id: "edit.clearSelection",
      titleKey: "cmd.edit.clearSelection",
      group: "edit",
      menu: "edit",
      scope: "tiles",
      defaultKey: "Esc",
      when: inBrowse,
      enabled: () => deps.browse.hasSelection(),
      run: () => deps.browse.clearSelection(),
    }),
    spec({
      id: "edit.tags",
      titleKey: "cmd.edit.tags",
      group: "edit",
      menu: "edit",
      scope: "tiles",
      when: inBrowse,
      enabled: () => deps.browse.hasSelection(),
      run: () => deps.openTags(),
    }),
    spec({
      id: "edit.delete",
      titleKey: "cmd.edit.delete",
      group: "edit",
      menu: "edit",
      scope: "tiles",
      defaultKey: "Delete",
      dangerous: true,
      when: inBrowse,
      enabled: () => deps.browse.hasSelection(),
      run: () => deps.browse.requestDelete(),
    }),
    spec({
      id: "nav.movePrev",
      titleKey: "cmd.nav.movePrev",
      group: "navigate",
      scope: "tiles",
      defaultKey: "ArrowLeft",
      when: inBrowse,
      run: () => deps.browse.moveFocus(-1),
    }),
    spec({
      id: "nav.moveNext",
      titleKey: "cmd.nav.moveNext",
      group: "navigate",
      scope: "tiles",
      defaultKey: "ArrowRight",
      when: inBrowse,
      run: () => deps.browse.moveFocus(1),
    }),

    /* ══ 视图 ══════════════════════════════════════════ */
    spec({
      id: "view.flow.import",
      titleKey: "cmd.view.flow.import",
      group: "view",
      menu: "view",
      scope: "global",
      defaultKey: "Mod+1",
      enabled: () => deps.flow() !== "import",
      run: () => deps.setFlow("import"),
    }),
    spec({
      id: "view.flow.browse",
      titleKey: "cmd.view.flow.browse",
      group: "view",
      menu: "view",
      scope: "global",
      defaultKey: "Mod+2",
      enabled: () => deps.flow() !== "browse",
      run: () => deps.setFlow("browse"),
    }),
    spec({
      id: "view.flow.edit",
      titleKey: "cmd.view.flow.edit",
      group: "view",
      menu: "view",
      scope: "global",
      defaultKey: "Mod+3",
      enabled: () => deps.flow() !== "edit",
      run: () => deps.setFlow("edit"),
    }),
    spec({
      id: "view.flow.export",
      titleKey: "cmd.view.flow.export",
      group: "view",
      menu: "view",
      scope: "global",
      defaultKey: "Mod+4",
      enabled: () => deps.flow() !== "export",
      run: () => deps.setFlow("export"),
    }),
    spec({
      id: "view.theme.toggle",
      titleKey: "cmd.view.theme.toggle",
      group: "view",
      menu: "view",
      scope: "global",
      run: () => deps.toggleTheme(),
    }),
    spec({
      id: "view.density.compact",
      titleKey: "cmd.view.density.compact",
      group: "view",
      menu: "view",
      scope: "global",
      enabled: () => deps.density() !== "compact",
      run: () => deps.setDensity("compact"),
    }),
    spec({
      id: "view.density.loose",
      titleKey: "cmd.view.density.loose",
      group: "view",
      menu: "view",
      scope: "global",
      enabled: () => deps.density() !== "loose",
      run: () => deps.setDensity("loose"),
    }),
    spec({
      id: "view.filter.toggle",
      titleKey: "cmd.view.filter.toggle",
      group: "view",
      menu: "view",
      scope: "tiles",
      when: inBrowse,
      run: () => deps.browse.toggleFilter(),
    }),
    spec({
      id: "view.filter.clear",
      titleKey: "cmd.view.filter.clear",
      group: "view",
      menu: "view",
      scope: "tiles",
      when: inBrowse,
      enabled: () => deps.browse.filterMode(),
      run: () => deps.browse.clearFilter(),
    }),
    spec({
      id: "view.tiles.byTime",
      titleKey: "cmd.view.tiles.byTime",
      group: "view",
      menu: "view",
      scope: "tiles",
      when: inTiles,
      run: () => deps.display.setByTime(!deps.display.byTime()),
    }),
    spec({
      id: "view.tiles.info",
      titleKey: "cmd.view.tiles.info",
      group: "view",
      menu: "view",
      scope: "tiles",
      defaultKey: "i",
      when: () =>
        inTiles() ||
        infoKeyApplies({ viewing: deps.viewer.viewing(), filmVisible: deps.viewer.filmVisible() }),
      run: () => deps.display.cycleInfo(),
    }),
    spec({
      id: "view.tiles.zoomIn",
      titleKey: "cmd.view.tiles.zoomIn",
      group: "view",
      menu: "view",
      scope: "tiles",
      when: inTiles,
      enabled: () => deps.display.tileStep() < TILE_SIZE_STEPS.length - 1,
      run: () => {
        deps.display.setTileStep(deps.display.tileStep() + 1);
        deps.display.commitTileStep();
      },
    }),
    spec({
      id: "view.tiles.zoomOut",
      titleKey: "cmd.view.tiles.zoomOut",
      group: "view",
      menu: "view",
      scope: "tiles",
      when: inTiles,
      enabled: () => deps.display.tileStep() > 0,
      run: () => {
        deps.display.setTileStep(deps.display.tileStep() - 1);
        deps.display.commitTileStep();
      },
    }),
    ...SORT_KEYS.map((key) =>
      spec({
        id: `view.sort.${key}`,
        titleKey: `cmd.view.sort.${key}`,
        group: "view",
        menu: "view",
        scope: "tiles",
        when: inBrowse,
        enabled: () => deps.browse.sortKey() !== key,
        run: () => deps.browse.setSortKey(key),
      }),
    ),
    spec({
      id: "view.sort.direction",
      titleKey: "cmd.view.sort.direction",
      group: "view",
      menu: "view",
      scope: "tiles",
      when: inBrowse,
      run: () => deps.browse.toggleSortDirection(),
    }),
    spec({
      id: "view.chrome.cycle",
      titleKey: "cmd.view.chrome.cycle",
      group: "view",
      menu: "view",
      scope: "viewer",
      defaultKey: "Tab",
      when: () => deps.viewer.viewing(),
      run: () => (deps.flow() === "import" ? deps.import.cycleChrome() : deps.browse.cycleChrome()),
    }),

    /* ══ 窗口 ══════════════════════════════════════════ */
    spec({
      id: "window.minimize",
      titleKey: "cmd.window.minimize",
      group: "window",
      menu: "window",
      scope: "global",
      enabled: () => deps.window.available(),
      run: () => deps.window.minimize(),
    }),
    spec({
      id: "window.toggleMaximize",
      titleKey: "cmd.window.toggleMaximize",
      group: "window",
      menu: "window",
      scope: "global",
      enabled: () => deps.window.available(),
      run: () => deps.window.toggleMaximize(),
    }),
    spec({
      id: "window.close",
      titleKey: "cmd.window.close",
      group: "window",
      menu: "window",
      scope: "global",
      dangerous: true,
      enabled: () => deps.window.available(),
      run: () => deps.window.close(),
    }),

    /* ══ 帮助 ══════════════════════════════════════════ */
    spec({
      id: "help.palette",
      titleKey: "cmd.help.palette",
      group: "help",
      menu: "help",
      scope: "global",
      defaultKey: "Mod+K",
      run: () => deps.openPalette(),
    }),
    spec({
      id: "help.shortcuts",
      titleKey: "cmd.help.shortcuts",
      group: "help",
      menu: "help",
      scope: "global",
      defaultKey: "Mod+,",
      run: () => deps.openShortcuts(),
    }),
    spec({
      id: "help.language.toggle",
      titleKey: "cmd.help.language.toggle",
      group: "help",
      menu: "help",
      scope: "global",
      run: () => deps.toggleLocale(),
    }),
    spec({
      id: "help.about",
      titleKey: "cmd.help.about",
      group: "help",
      menu: "help",
      scope: "global",
      run: () => deps.openAbout(),
    }),

    /* ══ 看图 ══════════════════════════════════════════ */
    spec({
      id: "viewer.close",
      titleKey: "cmd.viewer.close",
      group: "viewer",
      scope: "viewer",
      defaultKey: "Esc",
      when: () => deps.viewer.viewing(),
      run: () => deps.viewer.actions()?.close(),
    }),
    spec({
      id: "viewer.prev",
      titleKey: "cmd.viewer.prev",
      group: "viewer",
      scope: "viewer",
      defaultKey: "ArrowLeft",
      when: () => deps.viewer.viewing(),
      run: () => deps.viewer.actions()?.prev(),
    }),
    spec({
      id: "viewer.next",
      titleKey: "cmd.viewer.next",
      group: "viewer",
      scope: "viewer",
      defaultKey: "ArrowRight",
      when: () => deps.viewer.viewing(),
      run: () => deps.viewer.actions()?.next(),
    }),
    spec({
      id: "viewer.zoomIn",
      titleKey: "cmd.viewer.zoomIn",
      group: "viewer",
      scope: "viewer",
      defaultKey: "=",
      when: () => deps.viewer.viewing(),
      run: () => deps.viewer.actions()?.zoomIn(),
    }),
    spec({
      id: "viewer.zoomOut",
      titleKey: "cmd.viewer.zoomOut",
      group: "viewer",
      scope: "viewer",
      defaultKey: "-",
      when: () => deps.viewer.viewing(),
      run: () => deps.viewer.actions()?.zoomOut(),
    }),
    spec({
      id: "viewer.fit",
      titleKey: "cmd.viewer.fit",
      group: "viewer",
      scope: "viewer",
      defaultKey: "0",
      when: () => deps.viewer.viewing(),
      run: () => deps.viewer.actions()?.toggleFit(),
    }),
    spec({
      id: "viewer.actual",
      titleKey: "cmd.viewer.actual",
      group: "viewer",
      scope: "viewer",
      defaultKey: "1",
      when: () => deps.viewer.viewing(),
      run: () => deps.viewer.actions()?.actual(),
    }),
    spec({
      id: "viewer.compareOnly",
      titleKey: "cmd.viewer.compareOnly",
      group: "viewer",
      scope: "viewer",
      defaultKey: "Enter",
      when: () => deps.viewer.comparing(),
      run: () => deps.browse.toggleCompareStrip(),
    }),

    /* ══ 标记（浏览） ═══════════════════════════════════ */
    ...[0, 1, 2, 3, 4, 5].map((star) =>
      spec({
        id: `mark.rating.${star}`,
        titleKey: `cmd.mark.rating.${star}`,
        group: "mark",
        scope: "tiles",
        defaultKey: String(star),
        when: inBrowse,
        enabled: () => deps.browse.hasSelection(),
        run: () => deps.browse.mark({ kind: "rating", value: star }),
      }),
    ),
    spec({
      id: "mark.flag.pick",
      titleKey: "cmd.mark.flag.pick",
      group: "mark",
      scope: "tiles",
      defaultKey: "P",
      when: inBrowse,
      enabled: () => deps.browse.hasSelection(),
      run: () => deps.browse.setFlag("pick"),
    }),
    spec({
      id: "mark.flag.reject",
      titleKey: "cmd.mark.flag.reject",
      group: "mark",
      scope: "tiles",
      defaultKey: "X",
      when: inBrowse,
      enabled: () => deps.browse.hasSelection(),
      run: () => deps.browse.setFlag("reject"),
    }),
    spec({
      id: "mark.flag.clear",
      titleKey: "cmd.mark.flag.clear",
      group: "mark",
      scope: "tiles",
      defaultKey: "U",
      when: inBrowse,
      run: () => deps.browse.setFlag(null),
    }),
    spec({
      id: "mark.like",
      titleKey: "cmd.mark.like",
      group: "mark",
      scope: "tiles",
      when: inBrowse,
      enabled: () => deps.browse.hasSelection(),
      run: () => deps.browse.mark({ kind: "like", value: "like" }),
    }),
    spec({
      id: "mark.dislike",
      titleKey: "cmd.mark.dislike",
      group: "mark",
      scope: "tiles",
      when: inBrowse,
      enabled: () => deps.browse.hasSelection(),
      run: () => deps.browse.mark({ kind: "like", value: "dislike" }),
    }),
    ...COLOR_VALUES.map((color) =>
      spec({
        id: `mark.color.${color}`,
        titleKey: `cmd.mark.color.${color}`,
        group: "mark",
        scope: "tiles",
        when: inBrowse,
        enabled: () => deps.browse.hasSelection(),
        run: () => deps.browse.mark({ kind: "color", value: color }),
      }),
    ),
    ...[1, 2].map((level) =>
      spec({
        id: `mark.lock.${level}`,
        titleKey: `cmd.mark.lock.${level}`,
        group: "mark",
        scope: "tiles",
        when: inBrowse,
        enabled: () => deps.browse.hasSelection(),
        run: () => deps.browse.mark({ kind: "lock", value: level }),
      }),
    ),

    /* ══ 导航 ══════════════════════════════════════════ */
    spec({
      id: "nav.open",
      titleKey: "cmd.nav.open",
      group: "navigate",
      scope: "tiles",
      when: inTiles,
      enabled: () => (deps.flow() === "import" ? deps.import.hasSelection() : deps.browse.hasSelection()),
      run: () => (deps.flow() === "import" ? deps.import.openViewer() : deps.browse.openViewer()),
    }),

    /* ══ 导入 ══════════════════════════════════════════ */
    spec({
      id: "import.excludeSelected",
      titleKey: "cmd.import.excludeSelected",
      group: "import",
      scope: "tiles",
      when: inImport,
      enabled: () => deps.import.hasSelection(),
      run: () => deps.import.excludeSelected(),
    }),
  ];
}
