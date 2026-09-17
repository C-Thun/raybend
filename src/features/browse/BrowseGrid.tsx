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
  IconPhoto,
  IconPhotoOff,
} from "@tabler/icons-solidjs";

import { getThumbBytes } from "../../api/db.ts";
import { StateWatermark } from "../../components/ui/StateWatermark.tsx";
import { Tile, type TileColorLabel } from "../../components/ui/Tile.tsx";
import { VirtualGrid } from "../../components/ui/VirtualGrid.tsx";
import { createTokenPx } from "../../components/ui/tokens.ts";
import { createThumbQueue } from "../../components/ui/thumb-queue.ts";
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
  /** 网格滚动位置的复位键（换库/换目录时用）。 */
  resetKey?: string;
  class?: string;
  /** 尺寸档位（受控；控制条改它）。 */
  tileStep?: number;
  /** 按时间分组（受控）。 */
  grouped?: boolean;
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
  createEffect(() => {
    const list = groups();
    store.setDisplayOrder(list ? sliceOrder(store.timeline(), list) : null);
  });

  const rows = createMemo<BrowseRowModel[]>(() => {
    return buildBrowseRows({
      total: store.total(),
      columns: flow().columns,
      cellSize: cellWidth(),
      groups: groups(),
      gap: gap(),
    });
  });

  /** 行 → 下标区间（按需取数用）。 */
  const rowIndexRange = (start: number, end: number): [number, number] => {
    const list = rows();
    if (list.length === 0) return [0, 0];
    const from = list[Math.max(0, Math.min(start, list.length - 1))];
    const to = list[Math.max(0, Math.min(end - 1, list.length - 1))];
    const first = from.kind === "tiles" ? from.start : from.start;
    const last = to.kind === "tiles" ? to.start + to.count : to.start + to.count;
    return [first, last];
  };

  /*
   * 缩略图队列：与导入网格共用同一个实现（`components/ui/thumb-queue.ts`）。
   * 浏览的路径是「库根 + 库内相对路径」；换库时清空（否则会串图）。
   */
  const thumbs = createThumbQueue({
    load: async (path) => {
      const bytes = await getThumbBytes(path, "grid");
      return bytes ?? null;
    },
  });
  createEffect(() => {
    const root = props.root;
    if (root === null) return;
    thumbs.clear();
  });
  onCleanup(() => thumbs.clear());

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
   *   2. 出错且什么都没拿到（有数据时不清屏：报错还要能接着看图）
   *   3. 首次加载中（`total() === 0` 才算「首次」：翻页不置 loading，见 store 的说明）
   *   4. 真的没有照片
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
    >
      <Show when={watermark()} fallback={<VirtualGrid
        rows={rows()}
        resetKey={props.resetKey}
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
              <For each={Array.from({ length: row.count }, (_, i) => row.start + i)}>
                {(index) => {
                  const item = () => store.itemAt(index);
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
                      flag={
                        item() === null
                          ? null
                          : store.picks().has(item()!.id)
                            ? "pick"
                            : store.rejects().has(item()!.id)
                              ? "reject"
                              : null
                      }
                      onClick={(event) => {
                        const it = item();
                        if (it === null) return;
                        const mode = event.shiftKey
                          ? "range"
                          : event.ctrlKey || event.metaKey
                            ? "toggle"
                            : "replace";
                        store.select(it.id, mode);
                      }}
                    />
                  );
                }}
              </For>
            </div>
          );
        }}
      />}>
        {(node) => node()}
      </Show>
    </div>
  );
}
