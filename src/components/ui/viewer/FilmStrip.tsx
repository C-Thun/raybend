/**
 * 胶片带（`BROWSE.md` §5.5、`design/browse.md` §2.5、`plans/M2-W2.md` 2.1）。
 *
 * ```text
 * ┌───────────────────────────────────────────────────────────────┐  104 高
 * │ ▢ ▢ ▣ ◻ ▢ …                                                   │  96×80 一张，横向滚动
 * └───────────────────────────────────────────────────────────────┘
 * ```
 *
 * 画布（`Shell / Browse / View` 的 `FilmStrip`）给的值：**104 高**、缩略 **96×80**、
 * 间距 6、左右内边距 8、底 `$surface-main`；缩略圆角 4；
 * **选中** = 铺 `$state-selected`，**当前那张**（锚点）再加 **1px `$brand` 描边**。
 *
 * ## 三条纪律
 *
 * 1. **选择逻辑与 tiles 完全一致**（`BROWSE.md` §5.2）——不是「照着写一遍」，
 *    而是真的共用：修饰键 → 模式走 `lib/selection.ts` 的 `clickMode()`，
 *    区间/翻转语义走同一个 `store.select()`。这里**没有**第二套选择代码。
 * 2. 列表就是**看图件手里那份**（`viewer.state().photos`）——所以胶片带与看图
 *    永远同源，不会出现「图在胶片带里、看图里没有」这类错位。
 * 3. 缩略图与网格**共用同一个队列**（`ThumbQueue`，由工作区建、两边传同一个）——
 *    网格里已经缓存的照片，胶片带里立刻就有，不重复取一遍。
 */

import { createEffect, createMemo, For, on, Show, type JSX } from "solid-js";
import { IconLock } from "@tabler/icons-solidjs";

import { clickMode } from "../../../lib/selection.ts";
import { t } from "../../../i18n/index.ts";
import type { ThumbQueue } from "../thumb-queue.ts";
import type { ViewerPhoto, ViewerStore } from "./index.ts";
import {
  captureFilmStripAnchor,
  restoreFilmStripScroll,
  type FilmStripAnchor,
  type FilmStripItemMetric,
} from "./film-strip-anchor.ts";

export interface FilmStripProps {
  /** 看图件：列表与当前下标都从它来（胶片带与看图同源） */
  viewer: ViewerStore;
  /**
   * 当前选中集（画「选中底色」用）。
   *
   * ⚠️ 这里**不再直接持有 browse store**（人类 2026-09-19 要求两侧共用同一份组件）：
   * 胶片带只认「选中集 + 一个选择回调」，谁调用它、库里怎么存标记都与它无关 ——
   * 于是 import 与 browse 能用**同一条**胶片带，而选择语义仍由各自的 store 决定。
   */
  selectedIds: ReadonlySet<string>;
  /**
   * 点一张：模式（replace / toggle / range）已经由本组件按修饰键算好
   * （`clickMode()`，与网格用的是同一个函数）。
   * 调用方负责把它落进自己的选择模型（browse 会把它交给 `store.select`）。
   */
  onSelect: (id: string, mode: "replace" | "toggle" | "range") => void;
  /** 与网格共用的缩略图队列 */
  thumbs: ThumbQueue;
  /**
   * **只显示这几张**（按给定顺序）—— 对比态下再按一次回车进入的那种状态
   * （`BROWSE.md` §5.5、`plans/M2-W2.md` 2.4）。不传 = 显示全部。
   *
   * 进入这个状态时：整条胶片带加 **1px 主色细边框**，而且**即使不按 Ctrl**，
   * 点一张也是「带 Ctrl 的效果」（点什么就把什么移出对比）。
   */
  onlyIds?: readonly string[];
  class?: string;
}

export function FilmStrip(props: FilmStripProps): JSX.Element {
  let scroller: HTMLDivElement | undefined;

  const allPhotos = createMemo(() => props.viewer.state().photos);
  /** 对比态子集：给定 id 顺序优先（胶片带与画幅顺序一致），否则就是全部 */
  const photos = createMemo<ViewerPhoto[]>(() => {
    const only = props.onlyIds;
    if (only === undefined) return [...allPhotos()];
    const byId = new Map(allPhotos().map((photo) => [photo.id, photo]));
    return only.flatMap((id) => {
      const photo = byId.get(id);
      return photo === undefined ? [] : [photo];
    });
  });
  /** 「只看对比图」这种特殊状态：整条加主色细边框、点击 = 移出对比 */
  const compareOnly = (): boolean => props.onlyIds !== undefined;
  const index = () => props.viewer.state().index;
  /** 当前这张在**全列表**里的下标（`goTo` 要的是全列表下标） */
  const fullIndex = (photo: ViewerPhoto): number =>
    allPhotos().findIndex((item) => item.id === photo.id);
  const selectedIds = () => props.selectedIds;
  // 选择集合里存的是**字符串** id（`lib/selection.ts` 的口径），看图件的 id 也是字符串
  const isSelected = (photo: ViewerPhoto): boolean => selectedIds().has(photo.id);

  /*
   * ══ 像素锚定：胶片带内容换过以后，眼前那张不横跳 ══
   *
   * 与 PhotoGrid 的纵向规则相同，只把「行 / 顶边 / scrollTop」换成
   * 「照片 / 左边 / scrollLeft」。内容指纹覆盖全列表与「只看对比图」状态；
   * 参考照片被筛掉时退到当前照片，两张都不存在才保持原位。
   */
  let lastSeen: FilmStripAnchor | null = null;
  let pendingPin: (FilmStripAnchor & { key: string }) | null = null;

  const itemMetrics = (): FilmStripItemMetric[] => {
    if (scroller === undefined) return [];
    return Array.from(scroller.querySelectorAll<HTMLElement>("[data-strip-id]")).map(
      (node) => ({
        id: node.dataset.stripId ?? "",
        start: node.offsetLeft,
        size: node.offsetWidth,
      }),
    );
  };

  const rememberReference = (): void => {
    if (scroller === undefined) return;
    const anchor = captureFilmStripAnchor(itemMetrics(), scroller.scrollLeft);
    if (anchor !== null) lastSeen = anchor;
  };

  const contentKey = createMemo(() =>
    `${compareOnly() ? "compare" : "all"}\u001e${photos().map((photo) => photo.id).join("\u001f")}`,
  );

  createEffect<string | undefined>((previous) => {
    const key = contentKey();
    if (previous !== undefined && key !== previous && lastSeen !== null) {
      pendingPin = { ...lastSeen, key };
      queueMicrotask(() => {
        const pin = pendingPin;
        if (pin === null || pin.key !== contentKey() || scroller === undefined) return;
        const next = restoreFilmStripScroll(
          itemMetrics(),
          pin,
          props.viewer.current()?.id ?? null,
        );
        pendingPin = null;
        if (next === null) return;
        scroller.scrollLeft = next;
        rememberReference();
      });
    }
    return key;
  });

  /**
   * 当前这张要**滚进视野**。
   *
   * 为什么放在 effect 里而不是「点击时顺手滚一下」：看图件内部也会换图
   * （`←`/`→`、对比态退出时的定位），胶片带必须跟着 —— 只认「当前下标」这一个信号，
   * 谁改的都不关心。
   */
  createEffect(
    on(
      // 内容换过时由上面的像素锚定负责；这里只响应「当前照片真的变了」。
      () => [index(), props.viewer.state().active] as const,
      ([at, active]) => {
        if (!active || scroller === undefined) return;
        const node = scroller.querySelector<HTMLElement>(`[data-strip-item="${at}"]`);
        node?.scrollIntoView({ block: "nearest", inline: "nearest" });
        queueMicrotask(rememberReference);
      },
    ),
  );

  /**
   * 移出一张之后，画面落到**还在对比里的那一张**（就近）。
   *
   * `BROWSE.md` §5.5 的收尾：「直到只剩一幅图时……**在胶片带中定位到这最后一张图的位位置**」。
   * 推而广之：只要被点掉的正是当前那张，就该退到仍在对比里的邻居 ——
   * 否则画面会停在**已经移出对比**的那张上（右栏与状态栏也会跟着错位）。
   */
  const goToSurvivor = (removed: ViewerPhoto, keep: readonly string[]): void => {
    const at = index();
    let best = -1;
    let bestDistance = Number.POSITIVE_INFINITY;
    for (const id of keep) {
      if (id === removed.id) continue;
      const candidate = allPhotos().findIndex((photo) => photo.id === id);
      if (candidate < 0) continue;
      const distance = Math.abs(candidate - at);
      // 严格小于：同样近时取**前面**那张（胶片带上手感更顺）
      if (distance < bestDistance) {
        bestDistance = distance;
        best = candidate;
      }
    }
    if (best >= 0 && best !== at) props.viewer.goTo(best);
  };

  const onThumb = (photo: ViewerPhoto, event: MouseEvent): void => {
    // 「只看对比图」时：**不按 Ctrl 也是 Ctrl 的效果**（点什么就把什么移出对比）
    const compare = compareOnly();
    const mode = compare ? "toggle" : clickMode(event);
    const at = fullIndex(photo);
    const wasCurrent = at >= 0 && at === index();
    /*
     * 先把「现在还在对比里的那几张」抓下来。
     *
     * 不能等 `select()` 之后再读 `props.onlyIds` —— 那一下会把选择改成只剩一张，
     * 对比态随之退出、`onlyIds` 立刻变回 `undefined`（2026-09-18 冒烟实测：
     * 于是「落到还在对比里的那张」就永远不生效，画面停在已移出的那张上）。
     */
    const keep = compare ? [...(props.onlyIds ?? [])] : [];
    // ① 选择：与 tiles **同一套**（修饰键判定也是同一个函数）
    props.onSelect(photo.id, mode);
    if (compare) {
      // 点的是**要移出对比**的那张：不跳过去；若它正是当前那张，落到还在对比里的邻居
      if (wasCurrent) goToSurvivor(photo, keep);
      return;
    }
    // ② 看哪张：点它就切到它（`BROWSE.md` §5.7）；下标要按**全列表**算
    if (at >= 0) props.viewer.goTo(at);
  };

  return (
    <div
      ref={scroller}
      data-filmstrip="open"
      onScroll={rememberReference}
      /*
       * `overflow-x-auto` + `shrink-0` 的子项：横向滚动条只在需要时出现；
       * 纵向不滚（104 高放得下 80 的缩略 + 上下各 12 的呼吸）。
       *
       * 里面那一行是 `w-max mx-auto`：**装得下就居中**（人类 2026-09-20），
       * 装不下时 auto 外边距归零、就还是原来那样从左排起、横向滚动。
       * 不用 `justify-center`：flex 的居中对**溢出**会两头都截，起始那几张滚不回来。
       */
      class={[
        "h-[104px] shrink-0 overflow-x-auto overflow-y-hidden bg-surface-main",
        // 「只看对比图」= 整条 1px 主色细边框（画布 `Shell / Browse / Compare` 的 FilmStrip）
        compareOnly() ? "border border-brand" : "",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      data-strip-mode={compareOnly() ? "compare" : "all"}
    >
      <div class="mx-auto flex h-full w-max items-center gap-1.5 px-2">
      <For each={photos()}>
        {(photo) => {
          const lockLevel = (): number => photo.marks?.lockLevel ?? 0;
          createEffect(() => {
            props.thumbs.request(photo.path);
          });
          return (
            <button
              type="button"
              data-strip-item={fullIndex(photo)}
              data-strip-id={photo.id}
              data-current={fullIndex(photo) === index() ? "true" : undefined}
              aria-current={fullIndex(photo) === index() ? "true" : undefined}
              aria-label={photo.fileName}
              title={photo.fileName}
              onClick={(event) => onThumb(photo, event)}
              class={[
                "relative h-20 w-24 shrink-0 overflow-hidden rounded-ui p-0.5",
                /*
                 * 选中 = 铺 `$state-selected`（照片四周留 2px，底色才看得见 —— 与网格
                 * 的 tile 同一个手法）；当前那张（锚点）再加 1px 主色描边。
                 */
                isSelected(photo) ? "bg-state-selected" : "bg-transparent",
                fullIndex(photo) === index()
                  ? "border border-brand"
                  : "border border-transparent hover:bg-state-hover",
              ].join(" ")}
            >
              <Show
                when={props.thumbs.get(photo.path).url}
                fallback={<span class="block h-full w-full rounded-ui bg-surface-track" />}
              >
                {(url) => (
                  <img
                    /*
                     * `object-contain` 而不是 `cover`：胶片带里**竖图必须完整显示**，
                     * 不能被裁成横的（人类 2026-09-19 点名）。
                     * 这与网格 tile 的取图口径一致：照片永远完整居中，只容不改比例。
                     */
                    class="h-full w-full rounded-ui object-contain"
                    src={url()}
                    alt=""
                    draggable={false}
                  />
                )}
              </Show>

              {/* 锁徽标（`BROWSE.md` §3.1：锁在 tiles / 看图 / 胶片带上都要看得出来） */}
              <Show when={lockLevel() > 0}>
                <span
                  class="absolute right-1 top-1 flex h-4 w-4 items-center justify-center rounded-ui text-fg-1"
                  style={{ background: "var(--tile-bar-scrim)" }}
                >
                  <IconLock size={10} aria-label={t("grid.locked")} />
                </span>
              </Show>
            </button>
          );
        }}
      </For>
      </div>
    </div>
  );
}
