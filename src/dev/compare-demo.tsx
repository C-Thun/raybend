/**
 * 画廊里的「对比」演示（`/dev/kitchen-sink`）。
 *
 * 为什么要有它：对比的每一格各要一份图片 URL，而这份 URL 的一生跨三处持有者
 * （单图槽位 / 总览槽位 / 多图缓存）且全程异步。它出的问题是**画面上一格空白** ——
 * 控制台不报错、单元测试只盖得住 store 那一半。这里用**真的** `CompareView` +
 * 假后端把它跑起来，于是：
 *
 * * `pnpm smoke:ui` 能在真 DOM 上断言「来回点画幅 / 滚轮缩放之后每一格都还有图」
 *   —— 2026-10-08「点一张、另一张变空白」的回归靶子；
 * * 人（与 Agent）能在没有照片库的情况下打开**探针浮层**看每一格的 URL 状态：
 *   `Ctrl+Alt+Shift+D`（见 `components/ui/viewer/probe.ts`）。
 *
 * 视图与 store 都是真的，这里只喂数据（与 `viewer-demo.tsx` 同一条路子）。
 */

import { createSignal, Show } from "solid-js";
import {
  CompareView,
  createViewerStore,
  type ViewerPhoto,
} from "../components/ui/viewer/index.ts";
import { DEMO_PHOTOS, demoImageFor } from "./demo-photos.ts";

export function CompareDemo() {
  const [open, setOpen] = createSignal(false);
  const photos: readonly ViewerPhoto[] = DEMO_PHOTOS;

  const store = createViewerStore({
    /*
     * 字节里带的就是这张照片的 path —— `makeUrl` 靠它把「哪一格的图」区分出来
     * （真机上是 Rust 侧渲染出来的 AVIF 字节，这里只是让每一格长得不一样、尺寸对得上）。
     */
    loadThumb: async (key) => new TextEncoder().encode(key),
    loadScreen: async (key) => new TextEncoder().encode(key),
    makeUrl: (bytes) => demoImageFor(new TextDecoder().decode(bytes)),
    revokeUrl: () => undefined,
    // 对比最多 4 张：缓存上限放到 4（store 的下限本来也是 4）
    cacheLimit: 4,
  });

  const openCompare = (): void => {
    setOpen(true);
    store.show(photos, 0);
  };

  return (
    <div class="flex w-full flex-col gap-2" data-demo="compare">
      <div class="flex items-center gap-2">
        <button
          type="button"
          data-demo-action="open-compare"
          class="rounded-ui bg-surface-layer px-2 py-1 text-fs-1 text-fg-1"
          onClick={openCompare}
        >
          打开对比
        </button>
        <button
          type="button"
          data-demo-action="close-compare"
          class="rounded-ui bg-surface-layer px-2 py-1 text-fs-1 text-fg-1"
          onClick={() => {
            store.close();
            setOpen(false);
          }}
        >
          关闭对比
        </button>
        <span class="text-fs-1 text-fg-3">Ctrl+Alt+Shift+D 开探针</span>
      </div>

      <Show when={open()}>
        {/* `CompareView` 是 `absolute inset-0`：这里给一个有高度的定位父级 */}
        <div class="relative h-80 overflow-hidden rounded-ui">
          <CompareView
            photos={photos}
            store={store}
            onClose={() => setOpen(false)}
            /*
             * 与 browse 的 `focusComparePhoto` 同一条口径：点哪格就把哪张设为当前照片
             * （browse 那边还要 `setAnchor`，那是选择集合的事，演示里没有选择集合）。
             */
            onFocus={(photo) => {
              const at = photos.findIndex((item) => item.id === photo.id);
              if (at >= 0) store.focus(at);
            }}
          />
        </div>
      </Show>
    </div>
  );
}
