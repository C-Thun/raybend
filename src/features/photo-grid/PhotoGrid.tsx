/**
 * `PhotoGrid` —— 中列的照片网格（`design/main.md` §3.2）。
 *
 * 组成：`VirtualGrid`（只渲染可见行）+ `rows.ts`（行模型）+ 缩略图队列
 * + `GridControlBar`（底部控制条）。
 *
 * 三件事值得说明：
 *
 * 1. **tile 尺寸由 JS 拥有**（`DESIGN.md` §12.6）：档位 → `--tile-cell-w/h`
 *    写在容器上，`Tile` 组件读这两个变量画画面区；换行数学用
 *    `computeTileFlow`（除最后一行外每行列数相同，余量全部作右边距）。
 *    这里刻意**不重新发明**尺寸计算 —— 算法与档位都在 `lib/tile-flow.ts` 里，
 *    有单测。
 * 2. **缩略图只在行被渲染时才请求**：虚拟化已经把「可见」这件事算好了，
 *    所以 `TileCell` 在自己的 `createEffect` 里请求，滚出视口就不再管它
 *    （队列里已完成的结果按 LRU 留着，滚回来是瞬时的）。
 * 3. **排除的视觉**：被排除的照片压暗 + 角标。之所以不是隐藏，是因为
 *    「批量排除」是反转操作（`DESIGN.md` §12.2）—— 看不见就没法反选回来。
 */

import {
  createEffect,
  createSignal,
  onCleanup,
  onMount,
  Show,
} from "solid-js";
import { IconAlertTriangle, IconCalendar } from "@tabler/icons-solidjs";
import { Badge } from "../../components/ui/Badge.tsx";
import { Tile } from "../../components/ui/Tile.tsx";
import { VirtualGrid } from "../../components/ui/VirtualGrid.tsx";
import { createTokenPx } from "../../components/ui/tokens.ts";
import { locale, t } from "../../i18n/index.ts";
import { formatDayLabel, formatTimeRange } from "../../lib/datetime.ts";
import { formatCount, type GroupingLocale } from "../../lib/format.ts";
import { computeTileFlow, tileImageHeight, tileSizeAt } from "../../lib/tile-flow.ts";
import type { SourceItem } from "../../api/types.ts";
import { GridControlBar } from "./GridControlBar.tsx";
import {
  buildGridRows,
  DAY_HEADER,
  itemId,
  SLICE_HEADER,
  type TileRowModel,
  type GroupRowModel,
} from "./rows.ts";
import type { PhotoGridStore } from "./store.ts";

export interface PhotoGridProps {
  store: PhotoGridStore;
  class?: string;
}

export function PhotoGrid(props: PhotoGridProps) {
  const store = props.store;
  let container: HTMLDivElement | undefined;
  const [width, setWidth] = createSignal(0);

  // 密度相关的高度/间距从令牌读（切档时重读，见 components/ui/tokens.ts）
  const captionHeight = createTokenPx("--caption-h", 26);
  const gap = createTokenPx("--gap", 4);

  onMount(() => {
    if (!container) return;
    const measure = (): void => {
      // 扣掉容器自身左右内边距（换行数学要求「内容宽」）
      const style = getComputedStyle(container as HTMLDivElement);
      const padding =
        Number.parseFloat(style.paddingLeft) +
        Number.parseFloat(style.paddingRight);
      setWidth(Math.max(0, (container?.clientWidth ?? 0) - padding));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(container);
    onCleanup(() => observer.disconnect());
  });

  const cellWidth = () => tileSizeAt(store.tileStep());
  const flow = () =>
    computeTileFlow({
      containerWidth: width(),
      cellWidth: cellWidth(),
      gap: gap(),
    });

  const rows = () =>
    buildGridRows({
      items: store.displayItems(),
      columns: flow().columns,
      cellWidth: cellWidth(),
      captionHeight: captionHeight(),
      grouping: store.grouping(),
    });

  const groupingLocale = (): GroupingLocale =>
    locale() === "en-US" ? "en-US" : "zh-CN";

  return (
    <div class={["flex min-h-0 flex-1 flex-col bg-surface-bar", props.class ?? ""].join(" ")}>
      {/* 画面区：全部状态都在这里切换 */}
      <div
        ref={container}
        class="flex min-h-0 flex-1 flex-col px-3 pt-2"
        style={{
          // Tile 组件读这两个变量画画面区 —— 档位切换就是改它们
          "--tile-cell-w": `${cellWidth()}px`,
          "--tile-cell-h": `${tileImageHeight(cellWidth())}px`,
        }}
      >
        <Show when={store.dir()} fallback={<Hint text={t("grid.pick_dir")} />}>
          <Show when={store.status() !== "error"} fallback={
            <Hint
              tone="error"
              text={t("grid.load_error", { message: store.error() ?? "" })}
              action={{ label: t("common.retry"), run: store.reload }}
            />
          }>
            <Show when={store.status() !== "loading"} fallback={<Hint text={t("common.loading")} />}>
              <Show when={rows().length > 0} fallback={<Hint text={t("grid.empty_dir")} />}>
                <VirtualGrid
                  rows={rows()}
                  overscan={2}
                  resetKey={store.dir() ?? ""}
                  renderRow={(row) =>
                    row.kind === "tiles" ? (
                      <TileRow store={store} row={row} gap={gap()} />
                    ) : (
                      <GroupHeader
                        store={store}
                        row={row}
                        locale={groupingLocale()}
                      />
                    )
                  }
                />
              </Show>
            </Show>
          </Show>
        </Show>
      </div>

      <GridControlBar
        count={store.items().length}
        dir={store.dir()}
        byTime={store.byTime()}
        onByTimeChange={store.setByTime}
        tileStep={store.tileStep()}
        onTileStepChange={store.setTileStep}
        locale={groupingLocale()}
        loadingTimes={store.loadingTimes()}
      />
    </div>
  );
}

/** 空态 / 加载态 / 错误态的统一外观 */
function Hint(props: {
  text: string;
  tone?: "muted" | "error";
  action?: { label: string; run: () => void };
}) {
  return (
    <div class="flex min-h-0 flex-1 items-center justify-center gap-2 p-panel-pad">
      <Show when={props.tone === "error"}>
        <IconAlertTriangle size={14} class="text-fg-2" aria-hidden="true" />
      </Show>
      <p class="text-fs-1 text-fg-3">{props.text}</p>
      <Show when={props.action}>
        {(action) => (
          <button
            type="button"
            class="cursor-pointer text-fs-1 underline"
            onClick={() => action().run()}
          >
            {action().label}
          </button>
        )}
      </Show>
    </div>
  );
}

/** 一行 tile */
function TileRow(props: {
  store: PhotoGridStore;
  row: TileRowModel;
  gap: number;
}) {
  return (
    <div
      class="flex items-start"
      style={{ gap: `${props.gap}px`, height: `${props.row.height}px` }}
    >
      {props.row.items.map((item) => (
        <TileCell store={props.store} item={item} />
      ))}
    </div>
  );
}

/** 一张照片（负责请求自己的缩略图） */
function TileCell(props: { store: PhotoGridStore; item: SourceItem }) {
  const id = () => itemId(props.item);
  const thumb = () => props.store.thumb(id());
  const selected = () => props.store.selectedIds().has(id());
  const excluded = () => props.store.excluded().has(id());

  // 被渲染（= 可见）时才请求 —— 虚拟化保证了这一点
  createEffect(() => props.store.requestThumb(id()));

  return (
    <div class="relative" style={{ width: "var(--tile-cell-w)" }}>
      <Tile
        label={props.item.fileName}
        sublabel={props.item.ext?.toUpperCase() ?? undefined}
        src={thumb().url ?? undefined}
        selected={selected()}
        loading={thumb().status === "loading" || thumb().status === "idle"}
        class={excluded() ? "opacity-40" : ""}
        onClick={(event) => {
          const mode =
            event.shiftKey && !event.ctrlKey && !event.metaKey
              ? "range"
              : event.ctrlKey || event.metaKey
                ? "toggle"
                : "replace";
          props.store.clickItem(id(), mode);
        }}
      />
      <Show when={excluded()}>
        <span class="pointer-events-none absolute end-1 top-1">
          <Badge tone="neutral">{t("grid.excluded")}</Badge>
        </span>
      </Show>
    </div>
  );
}

/** 分组标题行（日 / 时间片 / 未知时间） */
function GroupHeader(props: {
  store: PhotoGridStore;
  row: GroupRowModel;
  locale: GroupingLocale;
}) {
  const label = () => {
    const row = props.row;
    if (row.unknown) return t("grid.unknown_time");
    if (row.level === "day") return formatDayLabel(row.dayId, props.locale);
    if (row.startMs === null || row.endMs === null) return row.dayId;
    return formatTimeRange(
      row.startMs,
      row.endMs,
      row.offsetMinutes,
      props.locale,
    );
  };

  const selectLabel = () =>
    props.row.level === "day" ? t("grid.select_all_day") : t("grid.select_all_range");

  const isDay = (): boolean => props.row.level === "day";

  return (
    /*
     * 留白只加在**上方**（行高里已经含了它）：标题贴着自己这一组、与上一组拉开。
     * 层级靠三样一起表达：留白（20 vs 10）、字号字重（14 semibold vs 13 normal）、
     * 颜色（`fg-1` vs `fg-2`）—— 设计稿里这一块本来就没有横线也没有色块。
     */
    <div
      class="flex items-center gap-2"
      style={{
        height: `${props.row.height}px`,
        // 留白**从同一个事实源里推导**（行高 − 内容高）—— 见 rows.ts 的 DAY_HEADER
        "padding-top": `${props.row.height - (isDay() ? DAY_HEADER.contentHeight : SLICE_HEADER.contentHeight)}px`,
      }}
    >
      <Show when={isDay()}>
        <IconCalendar
          size={14}
          class="shrink-0 text-fg-1"
          aria-hidden="true"
        />
      </Show>
      <span
        class={[
          "truncate",
          isDay()
            ? "text-fs-2 font-semibold text-fg-1"
            : "text-fs-1 text-fg-2",
        ].join(" ")}
      >
        {label()}
      </span>
      <span class="shrink-0 text-fs-0 text-fg-3 tnum">
        {t("grid.count", { n: formatCount(props.row.count, props.locale) })}
      </span>
      {/* 药丸（设计稿：日组 `$state-selected` 高 20 / 时间片 `$state-hover` 高 18） */}
      <button
        type="button"
        class={[
          "flex shrink-0 cursor-pointer items-center rounded-ui px-1.5 text-fs-0",
          isDay()
            ? "h-5 bg-state-selected text-fg-2 hover:text-fg-1"
            : "h-4.5 bg-state-hover text-fg-3 hover:text-fg-1",
        ].join(" ")}
        onClick={() => props.store.selectGroup(props.row.photoIds)}
      >
        {selectLabel()}
      </button>
    </div>
  );
}
