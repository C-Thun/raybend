import { clickMode } from "../../lib/selection.ts";
/**
 * `PhotoGrid` —— **全项目唯一的照片网格**（`design/main.md` §3.2）。
 *
 * 导入与浏览两侧都用它；两侧的差异（分页 / 洞 / 显示序置换 / 标记 / 排除）
 * 全部由**数据源**（`TilesSource`，见 `components/ui/tiles/source.ts`）吸收，
 * 网格自己不知道「这是导入还是浏览」。
 *
 * 组成：`VirtualGrid`（只渲染可见行）+ `ui/tiles/rows.ts`（行模型）+ 缩略图队列 + Tile。
 *
 * 四件事值得说明：
 *
 * 1. **tile 尺寸由 JS 拥有**（`DESIGN.md` §12.6）：档位 → `--tile-cell` 写在容器上，
 *    `Tile` 读它画画面区；换行数学用 `computeTileFlow`（行模型那边有单测）。
 * 2. **缩略图只在行被渲染时才请求**：虚拟化已经把「可见」算好了，
 *    `TileCell` 在自己的 `createEffect` 里请求，滚出视口就不再管它。
 * 3. **看图是覆盖层**：网格不卸载 —— 退出时光标、选中与**滚动位置**原地不动
 *    （人类 2026-09-17 报过「进看图再退出回到列表开头」，根因就是卸载）。
 * 4. **看图关掉要把焦点要回来**（组件自己盯 `viewer.state().active`）：否则焦点掉到 `<body>`，
 *    回车/方向键再也到不了网格（人类 2026-09-19 报的「Esc 后回车进不去」）。
 */

import {
  createEffect,
  createMemo,
  createSignal,
  For,
  on,
  onCleanup,
  onMount,
  Show,
  untrack,
  type JSX,
} from "solid-js";
import { IconCalendar } from "@tabler/icons-solidjs";

import { Tile } from "../../components/ui/Tile.tsx";
import { VirtualGrid } from "../../components/ui/VirtualGrid.tsx";
import { createTokenPx } from "../../components/ui/tokens.ts";
import { measureScrollbarWidth } from "../../lib/scrollbar.ts";
import { useTilesFitChannel } from "../../components/ui/tiles/fit.ts";
import {
  createViewerStore,
  photosFromSource,
  Viewer,
  type ViewerPhoto,
  type ViewerStore,
} from "../../components/ui/viewer/index.ts";
import { locale, t } from "../../i18n/index.ts";
import { formatDayLabel, formatTimeRange } from "../../lib/datetime.ts";
import { formatCount, type GroupingLocale } from "../../lib/format.ts";
import {
  tileSizeSteps,
  canFitRow,
  computeTileFlow,
  fitTileSizeToRow,
  nextTilePresetPosition,
  nextIndexForArrow,
  tilePositionForSize,
  tileSizeAt,
} from "../../lib/tile-flow.ts";
import { rowTop } from "../../lib/virtual-window.ts";
import { isColorLabel, type ColorLabel } from "../../lib/color-labels.ts";
import { getThumbBytes, getViewImage } from "../../api/db.ts";
import {
  buildGridRows,
  retainGridRows,
  DAY_HEADER,
  SLICE_HEADER,
  type GridRowModel,
} from "../../components/ui/tiles/rows.ts";
import type { GridItem, GridStatus, TilesSource } from "../../components/ui/tiles/source.ts";

export interface PhotoGridProps {
  /** 在现有照片格下附加业务内容，仍复用同一网格、Tile 和虚拟行。 */
  cellExtra?: (item: GridItem, slot: number) => JSX.Element;
  listMode?: boolean;
  cellOverlay?: (item: GridItem) => JSX.Element;
  /** Enter 可交给当前工作流命令；双击仍可接明确的稿查看入口。 */
  activate?: (id: string) => void;
  commandEnter?: boolean;

  /** 数据源（导入侧 `importSource()`、浏览侧 `browseSource()`） */
  source: TilesSource;
  /**
   * 看图件。**不传就自建** —— 浏览侧与胶片带/右栏共用工作区那一份
   * （看图的倍率、当前那张在三个视图之间必须一致）。
   */
  viewer?: ViewerStore;
  /**
   * 内容指纹（浏览侧传「库 + 范围 + 筛选」）。变了就把**参考照片**钉回原来的高度，
   * 换筛选时窗口内容不动（人类 2026-09-19）。
   */
  pinsKey?: string;
  /** 用户在网格里点了一下（浏览侧用它收起展开的库列表） */
  onInteract?: () => void;
  /** 点了第几格（显示序下标）：浏览侧据此记住「当前那张」，键盘导航从它接着走 */
  onFocusIndex?: (index: number) => void;
  /**
   * 打开看图的**前一刻**回调（浏览侧要在这里复位四态 `chrome`、收起库列表）。
   * 网格自己不认识那些概念 —— 它只负责「打开前打个招呼」。
   */
  onOpeningViewer?: () => void;
  /**
   * 「打开看图」的**请求计数**（命令面板 / 快捷键发的；每次 +1 就按当前选中开）。
   *
   * 为什么走计数而不是暴露 `openViewer`：打开看图时要拿「显示序里的完整清单」——
   * 那是网格（与数据源适配器）知道的事。计数式请求让命令复用**同一段代码**，
   * 不用在工作区里再拼一份「该给看图什么」的清单（`AGENTS.md` §2.12）。
   */
  openRequest?: number;
  /**
   * 把这一张（按 id）滚进视野 —— 键盘 `←`/`→` 换了「当前那张」之后调。
   * 已经看得见时一个像素都不动（数学在 `lib/virtual-window.ts` 的 `rowScrollTop`）。
   */
  focusId?: string;
  /**
   * 方向键是否在网格里移动「当前那张」。
   *
   * 导入侧：是（网格自己接键盘）。浏览侧：**不是** —— 那边有一套全局的
   * 「事件 → 意图」映射（`features/browse/keys.ts`），方向键由工作区统一处理，
   * 网格再插一手就会两边同时动。
   */
  movesWithArrowKeys?: boolean;
  /**
   * 空态 / 加载 / 错误的水印。两侧文案不同（空目录 vs 没选库），所以由调用方给。
   * 不传 = 网格只在有数据时渲染（调用方自己管空态）。
   */
  watermark?: (info: {
    status: GridStatus;
    error: string | null;
    count: number;
  }) => JSX.Element | null;
  class?: string;
}

/** 库里的色标字符串 → `Tile` 认的联合类型（认不出就是没有） */
function asColorLabel(value: string | null | undefined): ColorLabel | null {
  return isColorLabel(value) ? value : null;
}

export function PhotoGrid(props: PhotoGridProps): JSX.Element {
  const source = props.source;
  let container: HTMLDivElement | undefined;
  const [width, setWidth] = createSignal(0);

  /*
   * 格子间距读 `--tile-gap` 而**不是 `--gap`**（人类 2026-09-23 定：tiles 不参与密度调节）。
   * `--gap` 是密度令牌（3↔6），读它的话切一次松紧就要全网格重算换行 + 重建可见行；
   * 照片一多就是秒级卡顿。tile 内部（`--tile-pad` / `--tile-bar-h`）同理已固定。
   */
  const gap = createTokenPx("--tile-gap", 3);
  /**
   * 右侧**滚动条占位**（人类 2026-09-23 定：不做自适应，**留死**）。
   *
   * 网格的滚动容器（`[data-virtual-scroller]`）用的是**经典滚动条**（它**占布局宽度**）。
   * 而量宽度的是它的**外层容器**（`px-2 py-2` 那个 div）：`clientWidth` 把滚动条那一竖条
   * 也算进去，于是「铺满」算出来的格子会仲到滚动条底下被切（2026-09-23 人类报的那一条）。
   *
   * 所以：**不管有没有滚动条，右边一律留出滚动条宽度**。代价是没有滚动条时右边多一条空带 ——
   * 而那条空带与边距长得一样（轨道本来就是透明的，§6 无边线），换来的是「尺寸不随滚动条
   * 有无而跳」与「永远不会被滚动条切」。左边那个 8px 边距保留（人类：「左边有边距」）。
   *
   * ⚠️ 宽度是**实测**的（`measureScrollbarWidth()`），不是读 `--scrollbar-w` 令牌：
   * `* { scrollbar-width: thin }` 在 Chromium 里盖过了 `::-webkit-scrollbar { width: … }`，
   * 真实生效的宽度（本机 10px）与令牌（8px）**不是一个数** —— 按令牌留就会差 2px、
   * 最后一列仍被切掉一条边（真机量到过）。
   */
  const scrollbar = measureScrollbarWidth();

  onMount(() => {
    if (!container) return;
    const measure = (): void => {
      const style = getComputedStyle(container as HTMLDivElement);
      const padding =
        Number.parseFloat(style.paddingLeft) + Number.parseFloat(style.paddingRight);
      setWidth(Math.max(0, (container?.clientWidth ?? 0) - padding - scrollbar));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(container);
    onCleanup(() => observer.disconnect());
  });

  const fitRequest = useTilesFitChannel();
  const sizeBounds = () => fitRequest?.sizeBounds();
  const cellWidth = () => tileSizeAt(source.tileStep(), sizeBounds());
  const flow = () =>
    computeTileFlow({ containerWidth: width(), cellWidth: cellWidth(), gap: gap() });
  /**
   * 列数（只在**真的换了列数**时才通知下游）。
   *
   * 为什么单独包一层：`flow()` 每次宽度变化都返回新对象，而 `rows()` 是 O(照片数)
   * 的行模型构建 —— 拖左栏 / 切密度时宽度连续变，直接读 `flow().columns` 会让
   * 每一像素都重建全部行（大库下就是卡顿的来源）。`createMemo` 默认 `===` 比较，
   * 列数没变就不往下游发。
   */
  const columns = createMemo(() => props.listMode ? 1 : flow().columns);

  /** 当前「铺满一行」算出来的格宽（给请求处理与可用性读数共用，不写两遍公式） */
  const fittedCellWidth = (): number =>
    fitTileSizeToRow({
      containerWidth: width(),
      cellWidth: cellWidth(),
      gap: gap(),
    });

  /*
   * 回填「现在能不能铺满」（人类 2026-09-23）：算出来的格宽超过最大档就不能 ——
   * 状态条据此把按钮**禁用**，而不是让人按一下发现没反应。
   * 依赖只有容器宽 / 当前档 / 间距，与 `available` 本身无关，不会自激。
   */
  createEffect(() => {
    if (fitRequest === undefined) return;
    fitRequest.setAvailable(props.listMode!==true && width() > 0 && canFitRow({
      containerWidth: width(),
      cellWidth: cellWidth(),
      gap: gap(),
    }, sizeBounds()));
  });

  createEffect(
    on(
      () => fitRequest?.request() ?? 0,
      (request) => {
        if (request <= 0 || width() <= 0) return;
        const fitted = fittedCellWidth();
        /*
         * 超过最大档就**什么都不做**（按钮此时已经是禁用态，这里是第二道闸）：
         * 夹到最大档看着像「按了一半」，而铺不满是用户能一眼看出来的。
         */
        const steps = tileSizeSteps(sizeBounds());
        if (!Number.isFinite(fitted) || fitted < steps[0]! || fitted > steps[steps.length-1]!) return;
        source.setTileStep(tilePositionForSize(fitted, sizeBounds()));
        source.commitTileStep();
      },
    ),
  );

  /*
   * ⚠️ **必须 memo**：`buildGridRows` 每次都返回**新数组 + 新行对象**，
   * 而 `VirtualGrid` 的 `<For>` 按引用去重 —— 不 memo 的话，任何一次重算
   * （哪怕只是选中变了）都会把整片行 DOM 重建一遍。
   * 后果很具体（真机冒烟抓到的）：
   *   * 同一次事件里连点两张（Ctrl 多选）时，第二次点的是**已经被替换掉的**节点 ⇒ 没反应；
   *   * 点一下再双击进看图，双击落在旧节点上 ⇒ 看图打不开。
   * 依赖只有「格子数 / 列数 / 档位 / 分组」这几样，选中与否不进这个 memo。
   */
  const rows = createMemo<GridRowModel[]>((previous) =>
    retainGridRows(previous ?? [], buildGridRows({
      count: source.count(),
      columns: columns(),
      cellSize: cellWidth(),
      ...(source.extraHeight === undefined || props.listMode===true ? {} : {extraHeight: (index: number) => source.extraHeight!(index, cellWidth())}),
      ...(source.slices() === undefined ? {} : { slices: source.slices() }),
    })),
  );

  /*
   * 看图：**状态与视图都在 viewer 模块**，这里只负责「谁触发打开」。
   * 浏览侧传进来的是与胶片带共用的那一份（倍率、当前那张必须一致）。
   */
  const ownsViewer = props.viewer === undefined;
  const ownViewer = props.viewer === undefined
    ? createViewerStore({
        loadScreen: (path) => getViewImage(path, "screen"),
        loadThumb: (path) => getThumbBytes(path, "grid"),
      })
    : null;
  const viewer = props.viewer ?? ownViewer!;

  /**
   * 网格里的照片（按显示序）→ 查看器要的形态。
   *
   * 转换住在 `components/ui/viewer/photos.ts`：编辑工作区没有网格，但同样要胶片带，
   * 两边共用同一份（`AGENTS.md` §2.12）。这里只留一个两行的调用点。
   */
  const viewerPhotos = (): ViewerPhoto[] => photosFromSource(source);

  function openViewer(byId: string): void {
    if (props.activate !== undefined) { props.activate(byId); return; }
    props.onOpeningViewer?.();
    const list = viewerPhotos();
    const at = list.findIndex((photo) => photo.id === byId);
    if (at >= 0) viewer.show(list, at);
  }

  /*
   * ── 键盘 ──
   *
   * 回车进看图（**多选也走这条**：选中 ≥ 2 张时进看图就是对比态，
   * 对比是选择状态的派生值）。方向键只有 `movesWithArrowKeys` 时才接。
   */
  function onGridKeyDown(event: KeyboardEvent): void {
    if (viewer.state().active) return;
    if (
      props.movesWithArrowKeys === true &&
      (event.key === "ArrowLeft" ||
        event.key === "ArrowRight" ||
        event.key === "ArrowUp" ||
        event.key === "ArrowDown")
    ) {
      const selection = source.selection();
      const current = [...selection.ids][0];
      if (current === undefined) return;
      const next = nextIndexForArrow({
        from: Number.parseInt(current, 10) >= 0 && source.itemById(current) !== null
          ? indexOf(current)
          : -1,
        count: source.count(),
        columns: columns(),
        key: event.key,
      });
      if (next === null) return;
      const target = source.itemAt(next);
      if (target === null) return;
      event.preventDefault();
      source.select(target.id, "replace");
      queueMicrotask(() => {
        const node = container?.querySelector<HTMLElement>(
          '[role="option"][aria-selected="true"]',
        );
        node?.focus();
      });
      return;
    }
    if (event.key !== "Enter" || props.commandEnter === true) return;
    const selection = source.selection();
    if (selection.ids.size === 0) return;
    const anchor = selection.anchor;
    const target =
      anchor !== null && selection.ids.has(anchor) ? anchor : [...selection.ids][0];
    if (target === undefined) return;
    event.preventDefault();
    // 这个回车已经被我们用掉了：不让它继续冒泡到 window ——
    // 否则刚打开的看图件会在同一个事件里又收到一次「回车＝退出」，闪一下就关
    event.stopPropagation();
    openViewer(target);
  }

  /** 某个 id 现在落在显示序的第几格（键盘导航用） */
  const indexOf = (id: string): number => {
    for (let index = 0; index < source.count(); index += 1) {
      if (source.itemAt(index)?.id === id) return index;
    }
    return -1;
  };

  /*
   * 「打开看图」的外部请求（命令面板 / 快捷键）：与网格里按回车**同一条路** ——
   * 目标 = 锚点（它在选中集里就用它），否则选中集里的第一张。
   */
  createEffect(
    on(
      () => props.openRequest ?? 0,
      (request) => {
        if (request <= 0) return;
        const selection = source.selection();
        const anchor = selection.anchor;
        const target =
          anchor !== null && selection.ids.has(anchor) ? anchor : [...selection.ids][0];
        if (target !== undefined) openViewer(target);
      },
    ),
  );

  /*
   * ══ 锚定：换筛选时让窗口内容不动（人类 2026-09-19）══
   *
   * 1. **持续记录参考照片**：可见范围变化时把「第一条可见行里的第一个格子」
   *    连同它离容器顶边的像素距离存下来（普通对象，不参与响应式）；
   * 2. **指纹一变**：把那份快照记成待钉目标；新数据铺好后，在新列表里找同一张
   *    （找不到就退回锚点），把它滚回原来的高度。
   *
   * 找不到那张时**什么都不做** —— 硬滚到「第 N 行」在筛掉一大半时会把用户甩到别处。
   */
  let lastSeen: { id: string; offsetPx: number } | null = null;
  const [pendingPin, setPendingPin] = createSignal<
    { id: string; offsetPx: number; key: string } | null
  >(null);
  const [scrollRequest, setScrollRequest] = createSignal<
    { row: number; offsetPx: number; key: string } | null
  >(null);

  const rowOfId = (id: string): number => {
    const list = rows();
    for (let index = 0; index < list.length; index += 1) {
      const row = list[index];
      if (row === undefined || row.kind !== "tiles") continue;
      for (const slot of row.slots) {
        if (source.itemAt(slot)?.id === id) return index;
      }
    }
    return -1;
  };

  function rememberReference(startRow: number): void {
    const scroller = container?.querySelector<HTMLElement>("[data-virtual-scroller]");
    if (scroller === null || scroller === undefined) return;
    const row = rows()[startRow];
    if (row === undefined || row.kind !== "tiles") return;
    for (const slot of row.slots) {
      const item = source.itemAt(slot);
      if (item === null) continue;
      lastSeen = { id: item.id, offsetPx: rowTop(rows(), startRow) - scroller.scrollTop };
      return;
    }
  }

  createEffect<string | undefined>((previous) => {
    const key = props.pinsKey;
    if (previous !== undefined && key !== previous && lastSeen !== null) {
      setPendingPin({ ...lastSeen, key: `${previous}→${key}` });
    }
    return key;
  });

  createEffect(() => {
    const pin = pendingPin();
    if (pin === null) return;
    if (source.count() === 0) {
      setPendingPin(null);
      return;
    }
    const anchor = source.selection().anchor;
    let row = rowOfId(pin.id);
    if (row < 0 && anchor !== null) row = rowOfId(anchor);
    if (row < 0) {
      setPendingPin(null);
      return;
    }
    setScrollRequest({ row, offsetPx: pin.offsetPx, key: pin.key });
    setPendingPin(null);
  });

  /*
   * ── 看图关掉后把焦点还给网格 ──
   *
   * 落到哪：优先选中的那张 tile，没有就任意一张，再没有就容器自己（临时给 `tabindex="-1"`）。
   * **重试几帧**：关掉看图会触发一次数据重载，虚拟列表的行元素是整块换掉的 ——
   * 第一帧 focus 上的 tile 下一帧可能已经不在了。判据收紧成「**焦点丢了才补**」：
   * 焦点已在网格里、或用户点到了别处（工具条、输入框），就不再插手。
   */
  function focusTiles(attempt = 0): void {
    if (container === undefined) return;
    const selected =
      container.querySelector<HTMLElement>('[role="option"][aria-selected="true"]') ??
      container.querySelector<HTMLElement>('[role="option"]');
    if (selected !== null) {
      selected.focus();
    } else {
      container.setAttribute("tabindex", "-1");
      container.focus();
    }
    if (attempt >= 6) return;
    requestAnimationFrame(() => {
      if (container === undefined) return;
      const active = document.activeElement;
      if (active !== null && active !== document.body) return;
      focusTiles(attempt + 1);
    });
  }

  /** 「当前那张」落在哪一行（虚拟列表按行滚动） */
  const focusRow = (): number | undefined => {
    const id = props.focusId;
    if (id === undefined) return undefined;
    const row = rowOfId(id);
    return row < 0 ? undefined : row;
  };

  /*
   * 看图关掉 ⇒ 把焦点还给网格（`BROWSE.md` §5.4 的键盘接续）。
   *
   * **盯的就是看图的开关本身**（人类 2026-09-20 统一）：看图的 store 就在这个组件
   * 手里（`viewer`），所以这条不需要外面接线 —— 以前是工作区把「关了」变成计数器
   * 传进来，于是导入侧要接一遍、浏览侧要接一遍（两份一模一样的逻辑）。
   *
   * 看图的关闭路径很多（`Esc`、对比态的「返回」、关闭按钮），但它们最后都落到
   * `viewer.state().active` 这一个状态上 —— 只做一次、只有一份实现。
   * 焦点不抢回来，回车/方向键就再也到不了网格（人类 2026-09-19 报的「Esc 后回车进不去」）。
   */
  createEffect<boolean | undefined>((wasActive) => {
    const active = viewer.state().active;
    if (wasActive === true && !active) queueMicrotask(() => focusTiles());
    return active;
  });

  /*
   * ── 按需取数 + 补读真实宽高 ──
   *
   * 虚拟化已经算好「看到哪几行」，网格把它换成显示序区间交给数据源：
   * * 浏览侧按页取（`ensureRange`）；
   * * 两侧都要补读**可见那些**的真实宽高 —— 老库的 `width/height` 可能是 NULL，
   *   而 tile 的比例、看图的拖动边界都要它。
   */
  function onVisibleRange(start: number, end: number): void {
    rememberReference(start);
    const list = rows();
    let from = Number.POSITIVE_INFINITY;
    let to = Number.NEGATIVE_INFINITY;
    for (let at = Math.max(0, start); at < Math.min(end, list.length); at += 1) {
      const row = list[at];
      if (row === undefined || row.kind !== "tiles") continue;
      for (const slot of row.slots) {
        if (slot < from) from = slot;
        if (slot > to) to = slot;
      }
    }
    if (!Number.isFinite(from)) return;
    void source.ensureRange?.(from, to + 1);
    const entries: { id: string; path: string }[] = [];
    for (let slot = from; slot <= to; slot += 1) {
      const item = source.itemAt(slot);
      if (item === null) continue;
      entries.push({ id: item.id, path: item.path });
    }
    void source.ensureNatural(entries);
  }

  const groupingLocale = (): GroupingLocale => (locale() === "en-US" ? "en-US" : "zh-CN");

  const watermark = (): JSX.Element | null =>
    props.watermark?.({
      status: source.status(),
      error: source.error(),
      count: source.count(),
    }) ?? null;

  return (
    <div
      ref={container}
      class={["flex min-h-0 flex-1 flex-col bg-surface-bar px-2 py-2", props.class ?? ""]
        .filter(Boolean)
        .join(" ")}
      onKeyDown={onGridKeyDown}
      style={{ "--tile-cell": `${cellWidth()}px` }}
    >
      {/*
        水印（空态 / 加载 / 错误）与网格**互斥**：`keyed` 不能删 —— 非 keyed 的 `Show`
        比的是真值，而这里的 `when` 是元素本身（2026-09-18 冒烟抓到的那个
        「真值不变就不重渲染」）。有数据时水印为 `null`，网格照画。
      */}
      <Show when={watermark()} keyed>
        {(node) => node}
      </Show>
      <Show when={watermark() === null}>
        <VirtualGrid
          rows={rows()}
          overscan={2}
          resetKey={source.scopeKey()}
          scrollTo={scrollRequest()}
          {...(focusRow() === undefined ? {} : { focusRow: focusRow() })}
          onVisibleRange={onVisibleRange}
          /* Ctrl+滚轮调档位（两侧同一个手势）；松手那次由 commitTileStep 落盘 */
          onZoomWheel={(step) => {
            source.setTileStep(nextTilePresetPosition(source.tileStep(), step > 0 ? 1 : -1, sizeBounds()));
            source.commitTileStep();
          }}
          onBackgroundClick={() => source.clearSelection()}
          renderRow={(row) =>
            row.kind === "tiles" ? (
              <TileRow
                commandEnter={props.commandEnter}
                source={source}
                row={row}
                gap={gap()}
                onOpen={openViewer}
                listMode={props.listMode}
            cellExtra={props.cellExtra}
                cellOverlay={props.cellOverlay}
                {...(props.onInteract === undefined ? {} : { onInteract: props.onInteract })}
                {...(props.onFocusIndex === undefined ? {} : { onFocusIndex: props.onFocusIndex })}
              />
            ) : (
              <GroupHeader source={source} row={row} locale={groupingLocale()} />
            )
          }
        />
      </Show>

      {/*
        看图是覆盖层，不是把网格换掉：网格一直挂着，滚动位置留得住。
        ⚠️ **只在「网格自己建的 viewer」时渲染**：浏览侧传进来的是工作区那一份，
        而工作区还要在它上面叠**对比视图**（CompareView）与胶片带 —— 网格再渲染一份
        就会同时冒出两个看图件（真机冒烟抓到的：「对比态下不该同时出现单张看图件」、
        「不该有多组返回控件（实测 2）」）。
      */}
      <Show when={ownsViewer && viewer.state().active}>
        <Viewer store={viewer} class="z-10" />
      </Show>
    </div>
  );
}

/** 一行 tile */
function TileRow(props: {
  commandEnter?: boolean;
  source: TilesSource;
  row: GridRowModel & { kind: "tiles" };
  gap: number;
  onOpen: (id: string) => void;
  cellExtra?: (item: GridItem, slot: number) => JSX.Element;
  listMode?: boolean;
  cellOverlay?: (item: GridItem) => JSX.Element;
  onInteract?: () => void;
  onFocusIndex?: (index: number) => void;
}): JSX.Element {
  return (
    <div
      class="flex items-start"
      style={{ gap: `${props.gap}px`, height: `${props.row.height}px` }}
    >
      {/*
        用 `For`（不是 `.map()`）：**每一格要有自己的响应式作用域**。
        实测（真机冒烟）：`.map()` 铺出来的格子，`selected` 这个绑定在选中变化后
        不再重跑 —— 点完两张，DOM 上 `aria-selected` 还是 false（数据层是对的）。
        换成 `For` 后每格是独立的 owner，绑定按各自的依赖更新。
      */}
      <For each={props.row.slots}>
        {(slot) => (
          <TileCell
            commandEnter={props.commandEnter}
            source={props.source}
            slot={slot}
            onOpen={props.onOpen}
            listMode={props.listMode}
            cellExtra={props.cellExtra}
            cellOverlay={props.cellOverlay}
            {...(props.onInteract === undefined ? {} : { onInteract: props.onInteract })}
            {...(props.onFocusIndex === undefined ? {} : { onFocusIndex: props.onFocusIndex })}
          />
        )}
      </For>
    </div>
  );
}

/** 一个格子（负责请求自己的缩略图） */
function TileCell(props: {
  commandEnter?: boolean;
  source: TilesSource;
  slot: number;
  onOpen: (id: string) => void;
  cellExtra?: (item: GridItem, slot: number) => JSX.Element;
  listMode?: boolean;
  cellOverlay?: (item: GridItem) => JSX.Element;
  onInteract?: () => void;
  onFocusIndex?: (index: number) => void;
}): JSX.Element {
  // 被渲染（= 可见）时才请求 —— 虚拟化保证了这一点
  let requestedPath: string | null = null;
  createEffect(() => {
    const item = props.source.itemAt(props.slot);
    // A same-range disk refresh can invalidate a page without scrolling. Reread
    // visible stale/missing pages even when VirtualGrid's index range is unchanged.
    untrack(() => { void props.source.ensureRange?.(props.slot, props.slot + 1); });
    if (item !== null) {
      const idle = props.source.thumb(item.imageKey ?? item.path).status === "idle";
      if (requestedPath !== (item.imageKey ?? item.path) || idle) {
        requestedPath = item.imageKey ?? item.path;
        props.source.requestThumb(item.imageKey ?? item.path);
      }
    }
  });

  const item = () => props.source.itemAt(props.slot);
  const id = () => item()?.id ?? "";
  const thumb = () => props.source.thumb(item()?.imageKey ?? item()?.path ?? "");
  /*
   * 选中态**直接在 JSX 里读信号**（这里只留一个语义化的名字）：
   * `selection()` 是 store 的信号，`item()` 也是 —— 两个信号一变，
   * `Tile` 的 `aria-selected` / 底色就该跟着变。
   */
  const isSelected = (): boolean => {
    const it = item();
    return it !== null && props.source.selection().ids.has(it.id);
  };
  /** RAW 角标：展示的就是 RAW → `RAW`；位图 + RAW → `+RAW`；否则不显示 */
  const fitChannel = useTilesFitChannel();
  const tileSize = () => tileSizeAt(props.source.tileStep(), fitChannel?.sizeBounds());
  const rawMode = (): "raw" | "plus" | undefined => {
    const it = item();
    if (it === null) return undefined;
    if (it.isRaw === true) return "raw";
    if (it.hasRaw === true) return "plus";
    return undefined;
  };

  return (
    <div
      data-grid-item={id()}
      class={props.listMode?"relative flex w-full items-center gap-3":"relative"}
      // **正方外框**：边长就是尺寸档。行高恒定才有得拖（见 Tile 的模块注释）
      style={{ width: props.listMode?"100%":"var(--tile-cell)", height: `${tileSize() + (props.listMode?0:props.source.extraHeight?.(props.slot, tileSize()) ?? 0)}px` }}
    >
      <div class="relative shrink-0" style={{height: "var(--tile-cell)",width:"var(--tile-cell)"}}>
      <Tile
        keyboardActivate={props.commandEnter !== true}
        info={props.source.infoMode()}
        /*
         * **库内**上下文：顶部那条标记信息条（星标/色标/旗标）只在库内照片上出现。
         * 这一条 2026-09-20 才补上 —— 之前 `Tile` 里的 `inLibrary()` 判定一直没人满足，
         * 于是顶部条从来没渲染过（人类报「顶部信息条没了」）。
         */
        context="library"
        label={item()?.fileName ?? ""}
        tag={item()?.ext?.toUpperCase() ?? undefined}
        raw={rawMode()}
        aspect={props.source.aspectOf(id())}
        // 小尺寸档（96/120/144）星标退化成「一颗星 + 数字」
        compact={tileSize() <= 144}
        src={thumb().url ?? undefined}
        selected={isSelected()}
        selectionFrame={item()?.selectionFrame}
        selectionLocked={item()?.selectionLocked}
        disabled={item()?.disabled}
        loading={item() === null || thumb().status === "loading" || thumb().status === "idle"}
        excluded={item()?.excluded === true}
        rating={item()?.marks?.rating ?? 0}
        /*
         * 色标值来自数据库（可能有历史脏数据），认不出来的当没有 ——
         * 与右栏/工具条同一个判据（`lib/color-labels.ts`）。
         */
        colorLabel={asColorLabel(item()?.marks?.colorLabel)}
        flag={item()?.marks?.flag ?? null}
        // 赞/踩（人类 2026-09-20）：只认三态里那两个值，认不出的当没有
        like={
          item()?.marks?.likeState === "like"
            ? "like"
            : item()?.marks?.likeState === "dislike"
              ? "dislike"
              : null
        }
        locked={item()?.marks?.locked === true}
        /*
         * 双击进看图走 `Tile` 自己的 `onActivate`：**它内部把 `onDblClick` 占住了**
         * （`Tile.tsx` 的 `onDblClick={() => local.onActivate?.()}`），
         * 外面再传 `onDblClick` 会被它覆盖掉 —— 老代码为此在外面包了一层，
         * 那是绕过；这里按它设计的接口接（回车/空格激活也一起覆盖）。
         */
        onActivate={() => {
          const it = item();
          if (it !== null) props.onOpen(it.id);
        }}
        onClick={(event) => {
          const it = item();
          if (it === null) return;
          const mode = clickMode(event, props.source.invertedCtrl);
          props.source.select(it.id, mode);
          props.onInteract?.();
          props.onFocusIndex?.(props.slot);
        }}
      />
      <Show when={item() === null ? null : id()} keyed>{(_id) => untrack(() => props.cellOverlay?.(item()!))}</Show>
      </div>
      <Show when={item() === null ? null : id()} keyed>{(_id) => untrack(() => props.cellExtra?.(item()!, props.slot))}</Show>
    </div>
  );
}

/** 分组标题行（日 / 时间片 / 未知时间） */
function GroupHeader(props: {
  source: TilesSource;
  row: GridRowModel & { kind: "group" };
  locale: GroupingLocale;
}): JSX.Element {
  const label = (): string => {
    const row = props.row;
    if (row.unknown) return t("grid.unknown_time");
    if (row.level === "day") return formatDayLabel(row.dayId, props.locale);
    if (row.startMs === null || row.endMs === null) return row.dayId;
    return formatTimeRange(row.startMs, row.endMs, row.offsetMinutes, props.locale);
  };

  const isDay = (): boolean => props.row.level === "day";

  return (
    /*
     * 留白只加在**上方**（行高里已经含了它）：标题贴着自己这一组、与上一组拉开。
     * 层级靠三样一起表达：留白（20 vs 10）、字号字重（14 semibold vs 13 normal）、
     * 颜色（`fg-1` vs `fg-2`）—— 这一块本来就没有横线也没有色块。
     */
    <div
      class="flex items-center gap-2"
      style={{
        height: `${props.row.height}px`,
        // 留白**从同一个事实源里推导**（行高 − 内容高）—— 见 ui/tiles/rows.ts
        "padding-top": `${
          props.row.height - (isDay() ? DAY_HEADER.contentHeight : SLICE_HEADER.contentHeight)
        }px`,
      }}
    >
      <Show when={isDay()}>
        <IconCalendar size={14} class="shrink-0 text-fg-1" aria-hidden="true" />
      </Show>
      <span
        class={[
          "truncate",
          isDay() ? "text-fs-2 font-semibold text-fg-1" : "text-fs-1 text-fg-2",
        ].join(" ")}
      >
        {label()}
      </span>
      <span class="shrink-0 text-fs-0 text-fg-3 tnum">
        {t("grid.count", { n: formatCount(props.row.count, props.locale) })}
      </span>
      {/* 药丸（日组 `$state-selected` 高 20 / 时间片 `$state-hover` 高 18） */}
      <button
        type="button"
        class={[
          "flex shrink-0 cursor-pointer items-center rounded-ui px-1.5 text-fs-0",
          isDay()
            ? "h-5 bg-state-selected text-fg-2 hover:text-fg-1"
            : "h-4.5 bg-state-hover text-fg-3 hover:text-fg-1",
        ].join(" ")}
        onClick={() => props.source.selectGroupRange(props.row.start, props.row.count)}
      >
        {isDay() ? t("grid.select_all_day") : t("grid.select_all_range")}
      </button>
    </div>
  );
}
