/**
 * 拖动预览（**全局唯一一份**）：整叠卡片 + 数量角标。
 *
 * ## 为什么在 `components/ui/`
 *
 * browse 的「照片拖到垃圾桶/相片桶」与 export 的「定稿拖到预设」本来是**两套内联 JSX**，
 * 结果尺寸与透明度各走各的 —— 同样的手势看着像两个功能（崔总 2026-10-11 点名）。
 * 现在**只有一个组件**，两边都必须用它；要调就只调这里。
 *
 * 取值来历（都是崔总 2026-10-11 定的组合）：
 * - **尺寸** = browse 那档（`size-24`，一张卡 96px）
 * - **透明度** = export 那档（`opacity-80`）
 * - **卡片底** = `surface-layer`（照片容器的浮起档，与 export 一致，别动）
 * - **数量角标底 = 辅色**（`bg-brand-2` + `text-fg-on-brand`）：它从「缩尺寸、去掉文字」那次起
 *   一直是 `surface-layer` 的白 —— 那是个界面里不存在的颜色（页面上没有别的纯白小块）。
 *
 * 跟随方式：整叠卡片的**中心落在指针上**（+8px，看着像「捏着」而不是「贴着」）；
 * 不向窗口内夹取（夹取会在指针出窗时把幽灵钉在边缘，看着像卡死）——
 * 出窗已由 `trackPointerDrag` 的 `cancelOutsideWindow` 中止。
 */

import { IconPhoto } from "@tabler/icons-solidjs";
import { For, Show } from "solid-js";
import { Portal } from "solid-js/web";

/** 一张卡（`url` 还没取到就画占位图标）。 */
export interface DragPreviewCard {
  /** 缩略图地址 —— **响应式**：拖动途中缩略图可能才加载好（还没取到给 `null`/`undefined`）。 */
  url?: (() => string | null | undefined) | undefined;
}

export interface DragPreviewProps {
  x: number;
  y: number;
  /** 最多画 4 张（更多就靠角标说数）。 */
  items: readonly DragPreviewCard[];
  /** 角标显示的总数（可以大于 `items.length`）。 */
  count: number;
}

export function DragPreview(props: DragPreviewProps) {
  return (
    <Portal>
      <div
        data-drag-preview
        aria-hidden="true"
        class="pointer-events-none fixed z-(--z-modal) opacity-80"
        style={{
          left: `${props.x + 8}px`,
          top: `${props.y + 8}px`,
          transform: "translate(-50%,-50%)",
        }}
      >
        <div class="relative size-24">
          <For each={props.items.slice(0, 4)}>
            {(item, index) => (
              <div
                data-drag-card
                class="absolute inset-0 overflow-hidden rounded-ui border border-fg-3 bg-surface-layer"
                style={{
                  transform: `translate(${index() * 9}px, ${-index() * 3}px) rotate(${(index() - (Math.min(4, props.items.length) - 1) / 2) * 8}deg)`,
                  "transform-origin": "50% 85%",
                }}
              >
                <Show when={item.url?.()} fallback={<IconPhoto size={32} class="m-auto mt-8 text-fg-3" />}>
                  {(src) => <img class="size-full object-contain" src={src()} alt="" draggable={false} />}
                </Show>
              </div>
            )}
          </For>
          {/*
            数量角标：**辅色底**（崔总 2026-10-11）——
            它从「缩尺寸、去掉文字」那次起就一直是 `surface-layer` 的白，
            而那是界面里不存在的颜色（页面上没有别的纯白小块）。
          */}
          <span data-drag-count class="tnum absolute right-0 bottom-0 rounded-ui bg-brand-2 px-2 py-1 text-fs-2 text-fg-on-brand">
            {props.count}
          </span>
        </div>
      </div>
    </Portal>
  );
}
