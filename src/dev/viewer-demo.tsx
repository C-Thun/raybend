/**
 * 画廊里的「看图」演示（`/dev/kitchen-sink`）。
 *
 * 为什么要有它：真机上要看图得先有照片（要后端），而**交互本身**（滚轮锚点缩放、
 * 双击适配↔100%、Esc 返回）是可以在这里实测的 —— 用一张**尺寸真实**的 SVG 图片
 * （`naturalWidth/Height` 有值，比例才算得出来），假后端只负责把它交出去。
 *
 * `src/features/viewer` 的 store 与视图都是真的，这里只是喂数据。
 */

import { createSignal, Show } from "solid-js";
import { createViewerStore, Viewer } from "../components/ui/viewer/index.ts";

/** 一张 1600×1000 的图（SVG data URL：尺寸是真的、字节很小） */
function fakeImageUrl(): string {
  const svg =
    '<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="1000">' +
    // 具名颜色：色值只允许写在 tokens.css（`pnpm lint:colors` 管着）；
    // 这里只是张占位图，像不像品牌色不重要，「尺寸真实」才重要
    '<rect width="1600" height="1000" fill="darkslategray"/>' +
    '<circle cx="800" cy="500" r="260" fill="mediumaquamarine"/>' +
    "</svg>";
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

const PHOTOS = [
  { id: "p1", path: "/demo/1.jpg", fileName: "P1000001.JPG" },
  { id: "p2", path: "/demo/2.jpg", fileName: "P1000002.JPG" },
  { id: "p3", path: "/demo/3.jpg", fileName: "P1000003.JPG" },
];

export function ViewerDemo() {
  const [closedBy, setClosedBy] = createSignal<string | null>(null);
  const store = createViewerStore({
    // 字节只是触发器：URL 由 makeUrl 给（浏览器里没有真缩略图后端）
    loadThumb: async () => new Uint8Array([1]),
    loadScreen: async () => new Uint8Array([2]),
    makeUrl: fakeImageUrl,
    revokeUrl: () => {},
  });

  return (
    <div class="flex w-full flex-col gap-2" data-demo="viewer">
      <div class="flex items-center gap-2">
        <button
          type="button"
          data-demo-action="open-viewer"
          class="rounded-ui bg-surface-layer px-2 py-1 text-fs-1 text-fg-1"
          onClick={() => {
            setClosedBy(null);
            store.show(PHOTOS, 0);
          }}
        >
          打开看图
        </button>
        <Show when={closedBy() !== null}>
          <span class="text-fs-1 text-fg-3">已关闭（Esc）</span>
        </Show>
      </div>

      <Show when={store.state().active}>
        <div class="relative flex h-64 flex-col overflow-hidden rounded-ui">
          <Viewer
            store={store}
            onClose={() => setClosedBy("esc")}
            class="rounded-ui"
          />
        </div>
      </Show>
    </div>
  );
}
