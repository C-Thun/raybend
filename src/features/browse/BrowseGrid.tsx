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
import type { ViewerPhoto } from "../../components/ui/viewer/index.ts";
import { VirtualGrid } from "../../components/ui/VirtualGrid.tsx";
import { createTokenPx } from "../../components/ui/tokens.ts";
import { createThumbQueue, type ThumbQueue } from "../../components/ui/thumb-queue.ts";
import { clickMode } from "../../lib/selection.ts";
import { t } from "../../i18n/index.ts";
import {
  computeTileFlow,
  clampTileStepIndex,
  DEFAULT_TILE_STEP_INDEX,
  tileSizeAt,
} from "../../lib/tile-flow.ts";
import { buildBrowseRows, browseGroups, sliceOrder, type BrowseRowModel } from "./rows.ts";
import type { BrowseStore } from "./store.ts";

/**
 * 把库里的色标字符串收紧成 `Tile` 认的联合类型。
 *
 * **不用类型断言糊过去**：值来自数据库（可能有历史脏数据、或者将来加了新颜色），
 * 认不出来的就当没有 —— 比让界面显示一个不存在的颜色强。
 */
const COLOR_LABELS = new Set<string>(["red", "yellow", "green", "blue", "purple"]);

function asColorLabel(value: string | null | undefined): TileColorLabel | null {
  return typeof value === "string" && COLOR_LABELS.has(value)
    ? (value as TileColorLabel)
    : null;
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
        out.push({
          id: String(item.id),
          path,
          fileName: item.fileName,
          // 元数据里的真实宽高：看图靠它算拖动边界（RAW 尤其要紧，见 viewer/store.ts）
          ...(item.width !== null && item.height !== null
            ? { natural: { width: item.width, height: item.height } }
            : {}),
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

  /** 「当前那张」落在哪一行（虚拟列表按行渲染，滚动得按行来） */
  const focusRow = (): number | undefined => {
    const index = props.focusIndex;
    if (index === undefined) return undefined;
    return rows().findIndex((row) => row.kind === "tiles" && row.slots.includes(index));
  };

  /** 网格里的键盘：只接「回车进看图」（方向键与区间选本来就在 tile 的点击语义里）。 */
  function onGridKeyDown(event: KeyboardEvent): void {
    if (event.key !== "Enter") return;
    const selected = store.selectedIds();
    const only = selected[0];
    if (selected.length !== 1 || only === undefined) return;
    event.preventDefault();
    // **这个回车已经被我们用掉了**：不让它继续冒泡到 window ——
    // 否则刚打开的看图件会在同一个事件里又收到一次「回车＝退出」，闪一下就关
    // （2026-09-18 冒烟实测：active 在同一个 tick 里变回 false）。
    event.stopPropagation();
    openViewerAt(only);
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

  /** 库里照片的绝对路径。 */
  const absPath = (relPath: string): string | null => {
    const root = props.root;
    if (root === null) return null;
    const sep = root.endsWith("/") || root.endsWith("\\") ? "" : "/";
    return `${root}${sep}${relPath}`;
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
        focusRow={focusRow()}
        onVisibleRange={(start, end) => {
          const [from, to] = rowIndexRange(start, end);
          void store.ensureRange(from, to + 1);
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
                      selected={
                        item() !== null &&
                        store.selection().ids.has(String(item()?.id))
                      }
                      loading={item() === null}
                      context="library"
                      label={item()?.fileName ?? ""}
                      tag={item()?.ext?.toUpperCase() ?? undefined}
                      src={url() ?? undefined}
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
