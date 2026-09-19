/**
 * 浏览网格：库里照片的虚拟化网格（`BROWSE.md` §5、`design/browse.md` §2.3）。
 *
 * 与导入网格（`features/photo-grid/`）的关键差别是**数据来源**：
 * 那边是「扫一个目录拿到全量清单」，这边是「按页从库里取」——
 * 所以行只记下标区间，具体内容渲染时用 `store.itemAt(index)` 现取，
 * 取到 `null` 就是「还没加载」（显示占位 tile，不是空）。
 *
 * 三件事在这里落地：
 * * **按需取数**：靠 `VirtualGrid` 的 `onVisibleRange` 回调 →
 *   `store.ensureRange(start, end)`（只补缺的页，不重复请求）；
 * * **点击语义**（`BROWSE.md` §5.2）：无修饰键 = 只选这张；`Ctrl` = 反选；
 *   `Shift` = 从锚点区间翻转（翻转语义在 `lib/selection.ts` 里，有测试）；
 * * **标记显示**：星标 / 色标 / 旗标 / 锁都交给 `Tile` 的既有槽位
 *   （`context="library"` 才会渲染标记区）。
 */

import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import {
  IconAlertTriangle,
  IconAlbumOff,
  IconFolder,
  IconPhoto,
  IconPhotoOff,
} from "@tabler/icons-solidjs";

import { getThumbBytes } from "../../api/db.ts";
import { StateWatermark } from "../../components/ui/StateWatermark.tsx";
import { Tile, type TileColorLabel } from "../../components/ui/Tile.tsx";
import { isColorLabel } from "../../lib/color-labels.ts";
import { infoMode } from "../../components/ui/tile-info.ts";
import type { ViewerPhoto } from "../../components/ui/viewer/index.ts";
import { VirtualGrid } from "../../components/ui/VirtualGrid.tsx";
import { createTokenPx } from "../../components/ui/tokens.ts";
import { createThumbQueue, type ThumbQueue } from "../../components/ui/thumb-queue.ts";
import { clickMode } from "../../lib/selection.ts";
import { t } from "../../i18n/index.ts";
import {
  computeTileFlow,
  clampTileStepIndex,
  clampDisplayAspect,
  DEFAULT_TILE_STEP_INDEX,
  tileSizeAt,
} from "../../lib/tile-flow.ts";
import { joinPath } from "../../lib/paths.ts";
import { rowTop } from "../../lib/virtual-window.ts";
import { buildBrowseRows, browseGroups, sliceOrder, type BrowseRowModel } from "./rows.ts";
import type { BrowseStore } from "./store.ts";

/**
 * 把库里的色标字符串收紧成 `Tile` 认的联合类型。
 *
 * **不用类型断言糊过去**：值来自数据库（可能有历史脏数据、或者将来加了新颜色），
 * 认不出来的就当没有 —— 比让界面显示一个不存在的颜色强。
 */
/* 合法性判断走共享表（`lib/color-labels.ts`）：加色标只改那一处 */

function asColorLabel(value: string | null | undefined): TileColorLabel | null {
  return isColorLabel(value) ? value : null;
}

export interface BrowseGridProps {
  store: BrowseStore;
  /** 库根目录（缩略图要绝对路径）。 */
  root: string | null;
  /**
   * **与胶片带共用**的缩略图队列（`plans/M2-W2.md` 2.1）。
   *
   * 由工作区建、传给两边：网格里已经缓存的照片，胶片带里立刻就有。
   * 不传就自己建一个（组件独立可用，`dev/` 陈列室与单测不必关心这件事）。
   */
  thumbs?: ThumbQueue;
  /** 网格滚动位置的复位键（换库/换目录时用）。 */
  resetKey?: string;
  class?: string;
  /** 尺寸档位（受控；控制条改它）。 */
  tileStep?: number;
  /** 尺寸档位变化（Ctrl+滚轮用；由工作区落盘） */
  onTileStepChange?: (step: number) => void;
  /** 按时间分组（受控）。 */
  grouped?: boolean;
  /**
   * 用户在网格里真的点了一下（选中某张照片）。
   *
   * 展开的库列表靠它自动收起（`BROWSE.md` §4.2 的三个条件之一：
   * 「从用户体感来说就是在 browse mid 里随便点了一下」）。
   */
  onInteract?: () => void;
  /** 用户点了第几张（**全列表下标**）：工作区据此记住「当前那张」，键盘导航从它接着走 */
  onFocusIndex?: (index: number) => void;
  /**
   * 把这一张（**全列表下标**）滚进视野 —— 键盘 `←`/`→` 移动「当前那张」之后调。
   * 不传 = 不管滚动。
   */
  focusIndex?: number;
  /**
   * 「看图刚刚关掉了」的**计数器**（每次关 +1）。
   *
   * 网格要它**只为一件事**：把焦点要回来（见下面那个 effect）——否则焦点掉到 `body`，
   * 回车/方向键再也到不了网格（人类 2026-09-19 报的「Esc 退出后回车进不去」就是这个）。
   *
   * 为什么是计数器而不是「看图是否开着」：看图有**多条关闭路径**
   * （`Esc`、对比态的「返回」、工具条上的关闭），只盯「开着→关掉」的跃变会漏掉
   * 「关闭时网格根本没在跑那个跃变」的情形 —— 实测踩过（真机冒烟里
   * 对比态点返回后焦点就没人管了）。这里改成「外面每关一次就报一次」，
   * 与谁关的、走哪条路都无关。
   */
  focusNudge?: number;
  /**
   * 当前筛选条件的**指纹**（`JSON.stringify(filter)` + 模式）。
   *
   * 网格用它做一件事：**换筛选时让窗口内容不动**（人类 2026-09-19）——
   * 指纹一变就把「参考照片」钉回它原来离容器顶边的位置。
   */
  filterKey?: string;
  /**
   * 要进看图（双击某张、或在选中一张时回车）。
   *
   * 传的是**显示顺序下的完整清单**与当前那张的位置 —— 「下一张」要按用户看到的顺序走
   * （含按时间分组后的片内顺序），所以顺序在这一层算好（显示层），别人不重复推导。
   */
  onOpenViewer?: (photos: ViewerPhoto[], index: number) => void;
}

export function BrowseGrid(props: BrowseGridProps) {
  const store = props.store;
  let container: HTMLDivElement | undefined;
  const [width, setWidth] = createSignal(0);
  const step = () => clampTileStepIndex(props.tileStep ?? DEFAULT_TILE_STEP_INDEX);
  const gap = createTokenPx("--gap", 4);

  onMount(() => {
    if (!container) return;
    const measure = (): void => {
      const style = getComputedStyle(container as HTMLDivElement);
      const padding =
        Number.parseFloat(style.paddingLeft) + Number.parseFloat(style.paddingRight);
      setWidth(Math.max(0, (container?.clientWidth ?? 0) - padding));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(container);
    onCleanup(() => observer.disconnect());
  });

  const cellWidth = () => tileSizeAt(step());
  const flow = () =>
    computeTileFlow({ containerWidth: width(), cellWidth: cellWidth(), gap: gap() });

  /** 分组边界（只有「按时间」模式下才有）。 */
  const groups = createMemo(() => (props.grouped ? browseGroups(store.timeline()) : undefined));

  /*
   * 片内按文件名自然序 → 交给 store 当作**显示序**。
   *
   * 为什么要绕 store 一道：分页来自后端的时间序，而界面要按名字排 ——
   * 两套顺序各说各话就会出现「取下来的页与屏幕上那一格错位」（显示错照片）。
   * 把映射交给 store，`itemAt` / `ensureRange` 两个边界自己换算，
   * 于是界面、选区、键盘导航都只用一套下标（详见 store 的 `setDisplayOrder`）。
   */
  /**
   * 显示序 → 数据下标（片内按文件名自然序）。**这是显示层的东西**：
   * 数据层（store）只按查询顺序给数据，不参与分组与排序（人类 2026-09-18 定的边界）。
   */
  const order = createMemo(() => {
    const list = groups();
    return list ? sliceOrder(store.timeline(), list) : undefined;
  });

  /** 当前可见顺序的 id（区间选择要用 —— 用户看到的顺序才是「顺序」）。 */
  const visibleIds = (): string[] => {
    const ids: string[] = [];
    for (const row of rows()) {
      if (row.kind !== "tiles") continue;
      for (const slot of row.slots) {
        const item = store.itemAt(slot);
        if (item !== null) ids.push(String(item.id));
      }
    }
    return ids;
  };

  const rows = createMemo<BrowseRowModel[]>(() => {
    return buildBrowseRows({
      total: store.total(),
      columns: flow().columns,
      cellSize: cellWidth(),
      groups: groups(),
      gap: gap(),
      order: order(),
    });
  });

  /**
   * 显示顺序下的全部已加载照片（看图要用）。
   *
   * 只含**已加载**的格子：看图是「从这一屏往下翻」，没取回来的页不在列表里
   * （滚动时会把它们取回来，所以不会卡在首页）。
   */
  const orderedPhotos = (): ViewerPhoto[] => {
    const out: ViewerPhoto[] = [];
    for (const row of rows()) {
      if (row.kind !== "tiles") continue;
      for (const slot of row.slots) {
        const item = store.itemAt(slot);
        if (item === null) continue;
        const path = absPath(item.relPath);
        if (path === null) continue;
        /*
         * 真实宽高：**先问 store**（它认得补读回来的那些），再退回数据库清单里那两列。
         * 看图靠它算拖动边界、对比靠它算基准比例（RAW 尤其要紧，见 viewer/store.ts）。
         */
        const natural = store.naturalOf(item.id);
        out.push({
          id: String(item.id),
          path,
          fileName: item.fileName,
          ...(natural === null ? {} : { natural }),
          // 看图态的底部状态栏要显示这些（省一次为了四个数问后端的往返）
          marks: {
            rating: item.rating,
            colorLabel: item.colorLabel,
            likeState: item.likeState,
            lockLevel: item.lockLevel,
          },
          flag: store.picks().has(item.id)
            ? "pick"
            : store.rejects().has(item.id)
              ? "reject"
              : null,
        });
      }
    }
    return out;
  };

  /** 双击 / 回车进看图。 */
  function openViewerAt(id: number): void {
    const list = orderedPhotos();
    const at = list.findIndex((photo) => photo.id === String(id));
    if (at >= 0) props.onOpenViewer?.(list, at);
  }

  /**
   * 看图关掉时把焦点还给网格。
   *
   * 为什么必须还：回车进看图的监听挂在**网格容器**上（`onKeyDown`），
   * 而看图件关闭时它是被卸载的 —— 焦点会掉回 `<body>`，之后按回车**事件根本到不了网格**。
   * 界面上照片看着还是选中的（状态在 store 里没丢），但回车没反应，
   * 人类 2026-09-19 报的「Esc 退出后按 Enter 进不去」就是这一条。
   *
   * 落到哪：优先**锚点那张 tile**（用户看到的「当前这张」），没有就落到任意一张 tile，
   * 再没有就落到容器本身（容器临时给 `tabindex="-1"`，不然不可聚焦）。
   */
  /**
   * 把焦点还回「当前那张」tile。
   *
   * 落到哪：优先**锚点那张**（用户看到的「当前这张」），没有就落到任意一张 tile，
   * 再没有就落到容器本身（容器临时给 `tabindex="-1"`，不然不可聚焦）。
   *
   * 为什么要**重试几帧**：看图关掉会触发一次数据重载（标记/分页），
   * 虚拟列表的行元素是整块换掉的 —— 第一帧刚 focus 上的那张 tile
   * 下一帧可能已经不在 DOM 里了，焦点于是又掉回 `body`（真机冒烟实测）。
   * 判据收紧成「焦点**丢了**才补」：只要此刻焦点在网格里，或用户已经点到别处
   * （工具条、输入框……），就不再插手。
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
      const lost = active === null || active === document.body;
      if (!lost) return;
      focusTiles(attempt + 1);
    });
  }

  createEffect<number | undefined>((seen) => {
    const nudge = props.focusNudge ?? 0;
    if (seen !== undefined && nudge !== seen) queueMicrotask(() => focusTiles());
    return nudge;
  });

  /*
   * ══ 锚定：换筛选时让窗口内容不动（人类 2026-09-19）══
   *
   * 问题：筛选把一批照片去掉之后，列表只剩剩下的那些 —— 原来在屏幕上方的照片全没了，
   * 于是「视线里那张」会突然跳到别处（用户失去方位感）。
   *
   * 做法（两条）：
   *   1. **持续记录参考照片**：滚动/取数时把「第一条可见行里的第一个 tile」连同
   *      它离容器顶边的像素距离存下来（`lastSeen`）—— 这是个普通对象，不参与响应式；
   *   2. **筛选指纹一变**：把刚才那份快照记成 `pendingPin`；等新数据铺好之后，
   *      在**新列表**里找同一张照片（找不到就退回锚点那张），把它滚回原来的高度。
   *
   * 找不到那张照片时**什么都不做**（保持浏览器的滚动位置）——
   * 硬滚到「第 N 行」在筛掉一大半的情况下会把用户甩到别的地方，比不动更糟。
   */
  let lastSeen: { id: number; offsetPx: number } | null = null;
  const [pendingPin, setPendingPin] = createSignal<
    { id: number; offsetPx: number; key: string } | null
  >(null);
  const [scrollRequest, setScrollRequest] = createSignal<
    { row: number; offsetPx: number; key: string } | null
  >(null);

  /** 某个 id 现在落在第几行（`-1` = 不在当前已加载的列表里） */
  const rowOfId = (id: number): number => {
    const list = rows();
    for (let index = 0; index < list.length; index += 1) {
      const row = list[index];
      if (row === undefined || row.kind !== "tiles") continue;
      for (const slot of row.slots) {
        if (store.itemAt(slot)?.id === id) return index;
      }
    }
    return -1;
  };

  /** 记一次「参考照片」（可见范围变化时调） */
  function rememberReference(startRow: number): void {
    const scroller = container?.querySelector<HTMLElement>("[data-virtual-scroller]");
    if (scroller === null || scroller === undefined) return;
    const row = rows()[startRow];
    if (row === undefined || row.kind !== "tiles") return;
    const firstSlot = row.slots.find((slot) => store.itemAt(slot) !== null);
    if (firstSlot === undefined) return;
    const item = store.itemAt(firstSlot);
    if (item === null) return;
    lastSeen = { id: item.id, offsetPx: rowTop(rows(), startRow) - scroller.scrollTop };
  }

  // 筛选指纹变了 ⇒ 记下待钉的目标（用**变化前**记下的那份快照）
  createEffect<string | undefined>((previous) => {
    const key = props.filterKey;
    if (previous !== undefined && key !== previous && lastSeen !== null) {
      setPendingPin({ ...lastSeen, key: `${previous}→${key}` });
    }
    return key;
  });

  // 新数据铺好之后（`rows()` 变了）执行钉位；数据还没回来就等下一轮
  createEffect(() => {
    const pin = pendingPin();
    if (pin === null) return;
    if (store.total() === 0) {
      setPendingPin(null);
      return;
    }
    const anchor = store.selection().anchor;
    let row = rowOfId(pin.id);
    if (row < 0 && anchor !== null) row = rowOfId(Number(anchor));
    if (row < 0) {
      // 参考照片被筛掉了、锚点也不在：**什么都不做**（见上面第 2 条的说明）
      setPendingPin(null);
      return;
    }
    setScrollRequest({ row, offsetPx: pin.offsetPx, key: pin.key });
    setPendingPin(null);
  });

  /** 「当前那张」落在哪一行（虚拟列表按行渲染，滚动得按行来） */
  const focusRow = (): number | undefined => {
    const index = props.focusIndex;
    if (index === undefined) return undefined;
    return rows().findIndex((row) => row.kind === "tiles" && row.slots.includes(index));
  };

  /**
   * 网格里的键盘：只接「回车进看图」（方向键与区间选本来就在 tile 的点击语义里）。
   *
   * **多选也走这里**（人类 2026-09-19 报的「tiles 下多选按回车进不了对比」）：
   * 选中 ≥ 2 张时进看图就是**对比态**（对比是选择状态的派生值，见工作区的 `comparing()`），
   * 所以「多选 + 回车」不需要另写一条路径 —— 只要别在这里把多选挡掉。
   * 进看图的那一张取**锚点**（最后点中的），没锚点就取第一张。
   */
  function onGridKeyDown(event: KeyboardEvent): void {
    if (event.key !== "Enter") return;
    const selected = store.selectedIds();
    if (selected.length === 0) return;
    const anchor = store.selection().anchor;
    const target = anchor !== null && selected.includes(Number(anchor)) ? anchor : selected[0];
    if (target === undefined) return;
    event.preventDefault();
    // **这个回车已经被我们用掉了**：不让它继续冒泡到 window ——
    // 否则刚打开的看图件会在同一个事件里又收到一次「回车＝退出」，闪一下就关
    // （2026-09-18 冒烟实测：active 在同一个 tick 里变回 false）。
    event.stopPropagation();
    openViewerAt(Number(target));
  }

  /** 行 → 下标区间（按需取数用）。 */
  const rowIndexRange = (start: number, end: number): [number, number] => {
    const list = rows();
    if (list.length === 0) return [0, 0];
    const from = Math.max(0, Math.min(start, list.length - 1));
    const to = Math.max(0, Math.min(end - 1, list.length - 1));
    // 可见行的**数据下标**可能是散的（显示序是置换）→ 取 min..max 的超集即可，
    // 多取几张无所谓：页是按数据序取的，多出来的格子本来就该在内存里。
    let lo = Number.POSITIVE_INFINITY;
    let hi = Number.NEGATIVE_INFINITY;
    for (let at = from; at <= to; at += 1) {
      const row = list[at];
      if (row === undefined) continue;
      // 分组标题行没有格子，但它覆盖的**显示位次区间**也要算进去（否则区间选择会漏数据）
      const slots =
        row.kind === "tiles"
          ? row.slots
          : Array.from({ length: row.count }, (_unused, index) =>
              order()?.[row.start + index] ?? row.start + index,
            );
      for (const slot of slots) {
        if (slot < lo) lo = slot;
        if (slot > hi) hi = slot;
      }
    }
    if (!Number.isFinite(lo)) return [0, 0];
    return [lo, hi + 1];
  };

  /*
   * 缩略图队列：与导入网格共用同一个实现（`components/ui/thumb-queue.ts`）。
   * 浏览的路径是「库根 + 库内相对路径」；换库时清空（否则会串图）。
   */
  /** 没人给队列时自己建一个（自己建的才由自己回收 —— 共用的那个归工作区管） */
  const ownThumbs = props.thumbs === undefined
    ? createThumbQueue({
        load: async (path) => {
          const bytes = await getThumbBytes(path, "grid");
          return bytes ?? null;
        },
      })
    : null;
  const thumbs = props.thumbs ?? ownThumbs!;
  createEffect(() => {
    const root = props.root;
    if (root === null) return;
    thumbs.clear();
  });
  onCleanup(() => ownThumbs?.clear());

  /**
   * RAW 角标的显示模式（`Tile` 的 `raw`）：展示的是 RAW → `"raw"`；
   * 展示的是位图、但同一张还有 RAW → `"plus"`（角标写 `+RAW`）；其余不显示。
   */
  const rawMode = (
    item: { isRaw?: boolean; hasRaw?: boolean } | undefined,
  ): "raw" | "plus" | undefined => {
    if (item === undefined) return undefined;
    if (item.isRaw === true) return "raw";
    if (item.hasRaw === true) return "plus";
    return undefined;
  };

  /** 库里照片的绝对路径（拼接规则在 `lib/paths.ts`，全项目一份）。 */
  const absPath = (relPath: string): string | null => {
    const root = props.root;
    return root === null ? null : joinPath(root, relPath);
  };

  /*
   * 没有内容可画时的那块水印（`null` = 该画网格）。
   *
   * 四个条件**按优先级**排 —— 顺序错了会出现「载入中却报空」这类假状态：
   *   1. 还没选库（左列会告诉用户去哪建库，网格这边只做背景陈述）
   *   2. **选了库但还没选目录**（人类 2026-09-18 定：点库不铺整库照片，要再点一个目录）
   *   3. 出错且什么都没拿到（有数据时不清屏：报错还要能接着看图）
   *   4. 首次加载中（`total() === 0` 才算「首次」：翻页不置 loading，见 store 的说明）
   *   5. 真的没有照片
   */
  const watermark = () => {
    if (store.repositoryId() === null) {
      return (
        <StateWatermark
          icon={<IconAlbumOff size={64} stroke-width={1} />}
          text={t("browse.noRepository")}
        />
      );
    }
    if (store.scopePath() === null) {
      return (
        <StateWatermark
          icon={<IconFolder size={64} stroke-width={1} />}
          text={t("browse.pickDirectory")}
        />
      );
    }
    if (store.error() !== null && store.total() === 0) {
      return (
        <StateWatermark
          tone="error"
          icon={<IconAlertTriangle size={64} stroke-width={1} />}
          text={t("browse.load_error", { message: store.error() ?? "" })}
          action={{ label: t("common.retry"), run: () => void store.reload() }}
        />
      );
    }
    if (store.loading() && store.total() === 0) {
      return (
        <StateWatermark
          animate
          icon={<IconPhoto size={64} stroke-width={1} />}
          text={t("browse.loading")}
        />
      );
    }
    if (store.total() === 0) {
      return (
        <StateWatermark
          icon={<IconPhotoOff size={64} stroke-width={1} />}
          text={t("browse.empty_lib")}
        />
      );
    }
    return null;
  };

  return (
    <div
      ref={container}
      class={["flex min-h-0 flex-1 flex-col overflow-hidden px-2 py-2", props.class ?? ""]
        .filter(Boolean)
        .join(" ")}
      // 回车进看图（方向键/区间选在 tile 自己的点击语义里，不在这里重复实现）
      onKeyDown={onGridKeyDown}
    >
      {/*
        ⚠️ **`keyed` 不能删**（2026-09-18 实测踩到）：`Show` 在非 `keyed` 模式下
        比的是**真值**（`!a === !b`），而这里的 `when` 是**水印元素本身** ——
        两个不同的水印（如「还没有库」→「挑一个目录」）都是真值，于是它认为「没变」、
        不重渲染，界面上就永远停在第一个水印上。`keyed` 改成按对象比较，才能换掉。
        （同类先例：`components/ui/EasyCopy.tsx` 的 `popKey`。判据：所有 "两个非空值互换" 的
        水印切换都靠它。）
      */}
      <Show when={watermark()} keyed fallback={<VirtualGrid
        rows={rows()}
        resetKey={props.resetKey}
        /* Ctrl+滚轮调档位（与导入网格同一个手势，实现也在 VirtualGrid 里） */
        onZoomWheel={(delta) => props.onTileStepChange?.(clampTileStepIndex(step() + delta))}
        /* 点空白（没落在 tile 上）= 取消选择（人类 2026-09-19） */
        onBackgroundClick={() => store.clearSelection()}
        focusRow={focusRow()}
        scrollTo={scrollRequest()}
        onVisibleRange={(start, end) => {
          rememberReference(start);
          const [from, to] = rowIndexRange(start, end);
          /*
           * 取数据 + **顺手把这一段的真实宽高补齐**。
           *
           * 为什么要补：`assets.width/height` 是导入时写的，老库那批是 NULL
           * （见 `store/backfill.rs` 的文件头）→ tile 比例退回占位、对比报「没读到尺寸」。
           * 只补**可见这一段**（与导入侧「为可见 tile 读元信息」同一条纪律：
           * 不为一次滚动去读上千个文件头）。
           */
          void store.ensureRange(from, to + 1).then(() => {
            const entries: { id: number; path: string }[] = [];
            for (let at = from; at <= to; at += 1) {
              const item = store.itemAt(at);
              if (item === null) continue;
              const path = absPath(item.relPath);
              if (path !== null) entries.push({ id: item.id, path });
            }
            void store.ensureNatural(entries);
          });
        }}
        renderRow={(row) => {
          if (row.kind === "group") {
            return (
              <div
                class="flex items-baseline gap-2 px-1"
                style={{ height: `${row.height}px` }}
              >
                <span class="text-fs-2 font-semibold text-fg-2">
                  {row.unknown ? t("grid.unknown_time") : row.label}
                </span>
                <span class="text-fs-0 text-fg-3">{row.count}</span>
              </div>
            );
          }
          return (
            <div class="flex" style={{ gap: `${gap()}px` }}>
              <For each={row.slots}>
                {(index) => {
                  const item = () => store.itemAt(index);
                  /**
                   * 这张照片的旗标。
                   *
                   * ⚠️ 必须写成**一次函数调用里先判空再取值**，不能写成
                   * `item() === null ? null : store.picks().has(item()!.id) ? …` ——
                   * Solid 的编译会把那个三元拆成**两次独立的重算**：判空那次的结果会被缓存
                   * （「非空」），而分支里的 `item().id` 是**新读的** ⇒ 数据在两次之间变成 null 时
                   * 直接读着 null 崩（2026-09-19 冒烟实测：删完照片后
                   * `TypeError: Cannot read properties of null (reading 'id')`，
                   * 整个网格渲染跟着坏掉）。
                   */
                  const flagOf = (): "pick" | "reject" | null => {
                    const it = item();
                    if (it === null) return null;
                    if (store.picks().has(it.id)) return "pick";
                    if (store.rejects().has(it.id)) return "reject";
                    return null;
                  };
                  const path = () => {
                    const it = item();
                    return it === null ? null : absPath(it.relPath);
                  };
                  const url = () => {
                    const p = path();
                    if (p === null) return null;
                    thumbs.request(p);
                    const entry = thumbs.get(p);
                    return entry.status === "ready" ? entry.url : null;
                  };

                  /*
                   * 这张的真实宽高（tile 比例要用）：**先问 store**，
                   * 因为老库那批 `width/height` 是 NULL，补读回来的那份只有 store 知道。
                   */
                  const natural = (): { width: number; height: number } | null => {
                    const it = item();
                    return it === null ? null : store.naturalOf(it.id);
                  };

                  return (
                    /*
                     * 双击进看图挂在**外面这层**：`Tile` 自己会接管 `onClick`（它有一套激活语义），
                     * 实测把 `onDblClick` 传给它**不会被挂到 DOM 上**（2026-09-18 冒烟抓到的）。
                     * 导入网格也是这么做的（`PhotoGrid` 的 `onOpen`），两边保持一致。
                     *
                     * `contents` = 这层不产生盒子，网格布局与「Tile 直接当 flex 子项」完全一致。
                     */
                    <div
                      class="contents"
                      onDblClick={() => {
                        const it = item();
                        if (it !== null) openViewerAt(it.id);
                      }}
                    >
                      <Tile
                      info={infoMode()}
                      selected={
                        item() !== null &&
                        store.selection().ids.has(String(item()?.id))
                      }
                      loading={item() === null}
                      context="library"
                      label={item()?.fileName ?? ""}
                      /* 文件名那条＝**主名 + 扩展名**（与导入侧同一个表达，别在这里放 RAW） */
                      tag={item()?.ext?.toUpperCase() ?? undefined}
                      /*
                       * RAW 角标（人类 2026-09-19）：与导入侧**同一个角标、同一套显隐规则**，
                       * 只在文字上区分两种模式 —— 展示的是 RAW → `RAW`；
                       * 展示的是位图但同一张还有 RAW → `+RAW`。
                       */
                      raw={rawMode(item() ?? undefined)}
                      src={url() ?? undefined}
                      /*
                       * 竖图必须按**自己的比例**居中显示（人类 2026-09-19：一旦是 tile，
                       * 就只有这一套显示规范）。取法与导入网格**同一个夹取函数**，
                       * 不再各写一套：未知尺寸 → 占位比例；超宽 → 夹到 3:1。
                       */
                      aspect={clampDisplayAspect(
                        natural()?.width ?? 0,
                        natural()?.height ?? 0,
                      )}
                      rating={item()?.rating ?? 0}
                      colorLabel={asColorLabel(item()?.colorLabel)}
                      locked={(item()?.lockLevel ?? 0) > 0}
                      flag={flagOf()}
                      onClick={(event) => {
                        const it = item();
                        if (it === null) return;
                        // 修饰键 → 模式：与胶片带**共用同一个函数**（`lib/selection.ts`）
                        store.select(it.id, clickMode(event), visibleIds());
                        // 「去看照片了」——展开的库列表该收了（BROWSE.md §4.2）
                        props.onInteract?.();
                        props.onFocusIndex?.(index);
                      }}
                      />
                    </div>
                  );
                }}
              </For>
            </div>
          );
        }}
      />}>
        {/* `keyed` 模式给的是**水印元素本身**（不是 getter），直接渲染它 */}
        {(node) => node}
      </Show>
    </div>
  );
}
