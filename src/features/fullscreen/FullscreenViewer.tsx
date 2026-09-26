/**
 * **全屏看图**（沉浸式，`?fullscreen=1`）—— 一扇独立窗口里的一张照片，**没有任何控件**。
 *
 * 人类 2026-09-23 的口径：
 *
 * | 操作 | 行为 |
 * | --- | --- |
 * | 打开 | 默认**适应窗口**；图是当前目录显示序里的那张（清单与网格同一份来源） |
 * | 双击 | 适应窗口 ↔ 100% |
 * | 滚轮 | 以鼠标位置为锚缩放 |
 * | 拖动 | 平移 |
 * | `Esc` / `Enter` | 退出（关掉这扇窗） |
 * | `←` / `→`、`PageUp` / `PageDown` | 上一张 / 下一张（**到头就停**，与主窗口看图一致） |
 *
 * 三处**刻意复用**（`AGENTS.md` §2.12：同一个能力只允许有一套实现）：
 *
 * 1. 状态与数学 → `components/ui/viewer/store.ts`（同一份 `createViewerStore`）；
 * 2. 滚轮手感 → `components/ui/viewer/interaction.ts::createWheelZoom`；
 * 3. 步进 → store 的 `next()` / `prev()`（它们本来就「到头停住 + 重置成适配」，不再写第二套）。
 *
 * 与主窗口看图的**唯一**差别：这里没有 chrome、没有胶片带、没有对比，
 * 且图片档位固定取 `screen`（与主窗口看图的 `loadScreen` 同一个 purpose —— 同一个东西
 * 不该在两条路上取不同的图）。所以 100% = **当前这张渲染图**的 1:1，
 * 大图的屏幕档（长边 1920）在 4K 上放大时不会再取更清的档 —— 那是「另一个 purpose」的事，
 * 不在本次范围（真要改就加 `original` 档并同步 Rust 的 `ImagePurpose`）。
 */

import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import { IconLoader2 } from "@tabler/icons-solidjs";

import { getExportVariantImage } from "../../api/export.ts";
import { getViewImage } from "../../api/db.ts";
import {
  closeFullscreen,
  getFullscreenPayload,
  onFullscreenPayload,
} from "../../api/fullscreen.ts";
import type { FullscreenPayload } from "../../api/types.ts";
import { createViewerStore } from "../../components/ui/viewer/store.ts";
import { createWheelZoom } from "../../components/ui/viewer/interaction.ts";
import { t } from "../../i18n/index.ts";

export function FullscreenViewer() {
  let host: HTMLDivElement | undefined;
  const [dragging, setDragging] = createSignal(false);
  /** 清单还没到 / 清单为空：给一句话，别让人对着黑屏猜 */
  const [empty, setEmpty] = createSignal(false);

  const variantImages=new Map<string, NonNullable<FullscreenPayload["items"][number]["exportVariant"]>>();
  const store = createViewerStore({
    loadScreen: (path) => {const variant=variantImages.get(path);return variant===undefined?getViewImage(path,"screen"):getExportVariantImage(variant.repositoryId,variant.reference,"screen");},
    /*
     * 多图 URL 缓存：当前 + 前后各一张预载 = 3 张，给到 6 留余量。
     * 不加大也能跑，但换图时刚预载好的邻居可能已被挤出去，预载就白做了。
     */
    cacheLimit: 6,
  });

  const state = () => store.state();

  /**
   * 「正在载入」：换图那一刻就为真（store 的 `loadFor` 第一件事就是置 `loading`），
   * 所以遮罩是**立即**出现的，不是等超时。
   */
  const busy = (): boolean => store.imageStatus() === "loading";

  /**
   * 已应用的清单版本号（初始值 -1 = 一份都还没应用）。
   *
   * ❗ 为什么必须有它：挂载时的 `getFullscreenPayload()` 与后来的
   * `fullscreen://payload` 事件**两条路都可能到**，而且可能乱序 ——
   * 用户刚开窗又在主窗点了另一张（或连点两次开），事件先到、初始读取后到，
   * 没这个守卫就会「拿旧清单覆盖新清单」（症状：换图后显示的还是上一张）。
   * 版本号在 Rust 侧递增（`FullscreenState::store`），前端只做比较。
   */
  let appliedRevision = -1;

  const applyPayload = (payload: FullscreenPayload | null): void => {
    if (payload === null || payload.items.length === 0) {
      setEmpty(true);
      return;
    }
    // 迟到的旧包：丢掉（新的那份已经在画了）
    if (payload.revision <= appliedRevision) return;
    appliedRevision = payload.revision;
    setEmpty(false);
    variantImages.clear();
    for(const item of payload.items)if(item.exportVariant!==undefined)variantImages.set(JSON.stringify([payload.revision,item.id,item.exportVariant]),item.exportVariant);
    store.show(
      payload.items.map((item) => ({
        id: item.id,
        path: item.exportVariant===undefined?item.path:JSON.stringify([payload.revision,item.id,item.exportVariant]),
        fileName: item.fileName,
      })),
      payload.index,
    );
  };

  onMount(() => {
    /*
     * 视口尺寸：`ResizeObserver` 通知（全屏窗口也会因为切屏 / DPI 变化而改尺寸）。
     * 必须在**取图之前**量到 —— 适配倍率是按视口算的，量晚了会先按 0 算一遍。
     */
    const measure = (): void => {
      if (host === undefined) return;
      store.setViewport({ width: host.clientWidth, height: host.clientHeight });
    };
    measure();
    const observer = new ResizeObserver(measure);
    if (host !== undefined) observer.observe(host);
    onCleanup(() => observer.disconnect());

    // 整扇窗就这一页：把焦点收上来，键盘才一定落到这里（窗口是 Rust 侧建好并聚焦的）
    host?.focus();

    // 挂载取一次清单（页面刷新也走这条）+ 订阅「窗口已开着时换图」
    void getFullscreenPayload()
      .then(applyPayload)
      .catch(() => {
        setEmpty(true);
      });
    let dispose: (() => void) | undefined;
    void onFullscreenPayload(applyPayload).then((off) => {
      dispose = off;
    });
    onCleanup(() => dispose?.());

    /*
     * 窗口被**重新显示**时再读一次清单 —— 这条兼底已经**不需要**了：
     * `Esc` 现在是真销毁（`fullscreen_close`），每次打开都是新窗口、页面重新挂载，
     * 挂载时那次 `getFullscreenPayload()` 就是最新清单。
     *
     * 历史（别回去）：旧实现为了秒开把 `Esc` 做成 `hide()`，重开复用窗口，
     * 靠事件 + `visibilitychange` 推新清单 —— 而原生窗口 hide/show 不保证触发
     * 页面可见性变化、隐藏期间 WebView 也可能被挂起，清单就停在旧的。
     */

    const onKey = (event: KeyboardEvent): void => {
      switch (event.key) {
        case "Escape":
        case "Enter":
          event.preventDefault();
          /*
           * `.catch` 是**必须**的：Rust 侧关窗后本窗口就没了，
           * 「命令的响应」很可能回不来 —— 不接就是一条未处理的拒绝，
           * 在控制台里看着像功能坏了，而其实关窗成功了。
           */
          void closeFullscreen().catch(() => {});
          return;
        /*
         * 载入期间**只放 Esc / Enter 走**（人类 2026-09-23：「阻止除了 esc/enter
         * 退出外的一切操作」）。连按方向键时如果还继续换图，就是在还没看到上一张的
         * 情况下又发下一张的请求 —— 越按越慢、还什么都看不到。
         */
        case "ArrowLeft":
        case "PageUp":
          event.preventDefault();
          if (!busy()) store.prev();
          return;
        case "ArrowRight":
        case "PageDown":
          event.preventDefault();
          if (!busy()) store.next();
          return;
        default:
      }
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => window.removeEventListener("keydown", onKey));
  });

  /*
   * **前后邻图预载**（人类 2026-09-23：「每浏览一幅图，都在后台对前后两张做预载」）。
   *
   * 触发时机是「当前这张已经就绪」—— 不抢当前这张的带宽/解码（RAW 解码在 Rust 侧
   * 是串行的，先抢只会让正在等的那张更慢）；就绪后它才在后台把邻居拉进 URL 缓存。
   *
   * 两条纪律（2026-09-23 晚修「越预载越慢」时定的）：
   *   1. **串行**：同一时刻最多一个预载在飞。并行发两个只会让用户真正要的那张
   *      排在它们后面（解码队列是单例）；
   *   2. **用户翻走就停链**：每一步之前看一眼当前位置 —— 翻走了就不要再给旧位置的
   *      邻居占解码队列（在飞的那一个停不了，但后面那个可以不发）。
   *
   * 顺序先「下一张」（方向键的主方向），再「上一张」。
   */
  createEffect(() => {
    if (store.imageStatus() !== "ready") return;
    const photos = state().photos;
    const at = state().index;
    const forward = photos[at + 1];
    const backward = photos[at - 1];
    void (async () => {
      const moved = (): boolean => state().index !== at;
      if (forward !== undefined && !moved()) await store.ensureImage(forward);
      if (backward !== undefined && !moved()) await store.ensureImage(backward);
    })();
  });

  /* 滚轮：与主窗口看图**同一份实现**（按帧合并，锚点是鼠标位置）；载入期间不响应 */
  const wheel = createWheelZoom({
    resolveAnchor: (event) => {
      const rect = host?.getBoundingClientRect();
      return rect === undefined
        ? null
        : { x: event.clientX - rect.left, y: event.clientY - rect.top };
    },
    apply: (factor, anchor) => {
      if (busy()) return;
      store.zoomBy(factor, anchor ?? undefined);
    },
  });
  onCleanup(wheel.dispose);

  /* 拖动平移：指针捕获，拖出屏幕也不丢事件 */
  let dragFrom: { x: number; y: number } | null = null;

  function onPointerDown(event: PointerEvent): void {
    // 载入期间不接拖动（遮罩盖着，但指针事件会冒泡到宿主）
    if (busy()) return;
    if (event.button !== 0) return;
    dragFrom = { x: event.clientX, y: event.clientY };
    setDragging(true);
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function onPointerMove(event: PointerEvent): void {
    if (dragFrom === null) return;
    const from = dragFrom;
    dragFrom = { x: event.clientX, y: event.clientY };
    store.panBy(event.clientX - from.x, event.clientY - from.y);
  }

  function endDrag(): void {
    dragFrom = null;
    setDragging(false);
  }

  return (
    <div
      ref={(element: HTMLDivElement) => {
        host = element;
      }}
      class={[
        // 沉浸式：整屏、没有 chrome、没有背景装饰 —— 最深的那级中性面当画布底
        "fixed inset-0 overflow-hidden bg-surface-track outline-none select-none",
        dragging() ? "cursor-grabbing" : "cursor-grab",
      ].join(" ")}
      tabindex="-1"
      data-fullscreen="open"
      onWheel={wheel.onWheel}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onDblClick={() => {
        if (busy()) return;
        store.toggleFit();
      }}
    >
      <Show when={!empty()} fallback={<Hint text={t("fullscreen.empty")} />}>
        <Show when={store.imageUrl()}>
          {(url) => (
            <img
              src={url()}
              alt={store.current()?.fileName ?? ""}
              draggable={false}
              onLoad={(event) =>
                store.setNatural({
                  width: event.currentTarget.naturalWidth,
                  height: event.currentTarget.naturalHeight,
                })
              }
              class="pointer-events-none absolute top-1/2 left-1/2 max-w-none select-none"
              style={{
                // 与主窗口看图同一套顺序：先居中，再平移，最后按元素中心缩放
                transform: `translate(-50%, -50%) translate3d(${state().pan.x}px, ${state().pan.y}px, 0) scale(${state().zoom})`,
                "will-change": "transform",
              }}
            />
          )}
        </Show>
        <Show when={store.imageStatus() === "error"}>
          <Hint text={t("fullscreen.failed")} />
        </Show>

        {/*
          载入遮罩（人类 2026-09-23）：**无边框毛玻璃框**，中间是浅色字 + 浅色动图
          + 「载入中」。它要**立即**出现 —— 换图那一下屏幕本来就是空的，
          没提示就只是「卡了几秒，什么也没有」。

          它同时是**输入闸门**：铺满整屏且 `pointer-events-auto`，鼠标落在它上面
          （拖动/滚轮/双击的处理器另有 `busy()` 判断，因为事件会冒泡到宿主）。
        */}
        <Show when={busy()}>
          <div
            class="absolute inset-0 z-30 flex items-center justify-center backdrop-blur-md"
            data-fullscreen-loading="on"
          >
            <div class="flex items-center gap-2 rounded-ui bg-surface-layer/70 px-4 py-2">
              <IconLoader2 size={16} class="animate-spin text-fg-2" aria-hidden="true" />
              <span class="text-fs-2 text-fg-1">{t("fullscreen.loading")}</span>
            </div>
          </div>
        </Show>
      </Show>
    </div>
  );
}

/** 居中一句话（空态 / 读图失败）。没有按钮 —— 退出只有 `Esc` / `Enter` 两个键。 */
function Hint(props: { text: string }) {
  return (
    <p class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 text-fs-2 text-fg-3">
      {props.text}
    </p>
  );
}

export default FullscreenViewer;
