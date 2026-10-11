/**
 * 照片拖动（tiles → 垃圾桶热区 / 相片桶）—— **统一 pointer 方案**
 * （`specs/library-recycle-bin.md` §5；与 export 的拖动同源：都走 `lib/pointer-drag.ts`
 * 的 `trackPointerDrag` + `dropHostAt`，预览都走 `components/ui/DragPreview.tsx`）。
 *
 * ## 为什么不是 HTML5 DnD
 *
 * 1. 预览要与 export 同款（跟随指针的卡片堆叠 + 数量角标）；
 * 2. 落点高亮要实时（`dragover` 能做，但拖动中的自定义预览做不了）；
 * 3. 两套拖动并存会互相抢手势（`pointerdown` 与 `dragstart` 是同一次按下的两个出口）。
 *
 * ## 状态为什么是模块级单例
 *
 * 拖动**跨组件**：中列 tiles 起拖、左列相片桶卡片命中。这是**临时手势态**，
 * 不进任何业务 store（与 `lib/action-slot.ts` 同一种做法）——拖动结束即清空。
 *
 * ## 命中的两类目标
 *
 * | 目标 | 怎么命中 |
 * | --- | --- |
 * | 回收站热区（移入 / 粉碎） | `[data-photo-drop-zone]`（工作区渲染的那个方块） |
 * | 相片桶卡片 | `[data-photo-drop-bucket]`（`OrganizationPanel` 挂的属性） |
 *
 * 落点判定一律用 `dropHostAt`（`elementFromPoint`）—— 与 export 同一套；
 * 旧实现用「注册热区矩形」，松手时热区已随预览卸载、矩形变 `null`，判不中（2026-10-11 修）。
 */

import { createSignal } from "solid-js";

import { dropHostAt, trackPointerDrag, type DragPoint } from "../../lib/pointer-drag.ts";

export type PhotoDropTarget =
  | { kind: "recycle" }
  | { kind: "bucket"; bucketId: number; name: string }
  | { kind: "none" };

export interface PhotoDragState {
  /** 被拖的照片（资产 id 字符串；与 selection 同一口径）。 */
  ids: readonly string[];
  x: number;
  y: number;
  target: PhotoDropTarget;
}

const [photoDrag, setPhotoDrag] = createSignal<PhotoDragState | null>(null);

/** 当前拖动状态（`null` = 没在拖）。**只读**——拖动由 `beginPhotoDrag` 启动。 */
export { photoDrag };

/** 相片桶卡片的命中属性（值是 bucket id；`data-photo-drop-name` 是显示名）。 */
export const BUCKET_DROP_ATTR = "data-photo-drop-bucket";

/** 垃圾桶热区的命中属性（元素由工作区渲染）。 */
export const RECYCLE_DROP_ATTR = "data-photo-drop-zone";

/** 拖哪几张：拖**选中项**带整批；拖未选中项只带它一个（文件管理器同一套直觉）。 */
export function dragIdsFor(id: string, selected: ReadonlySet<string>): string[] {
  return selected.has(id) ? [...selected] : [id];
}

/** 指针此刻压在哪个落点上（松手判定与拖动高亮共用同一份逻辑）。 */
export function photoDropTargetAt(point: DragPoint): PhotoDropTarget {
  if (dropHostAt(point, `[${RECYCLE_DROP_ATTR}]`) !== null) {
    return { kind: "recycle" };
  }
  const bucket = dropHostAt(point, `[${BUCKET_DROP_ATTR}]`);
  if (bucket !== null) {
    const bucketId = Number(bucket.getAttribute(BUCKET_DROP_ATTR));
    if (Number.isSafeInteger(bucketId) && bucketId > 0) {
      return {
        kind: "bucket",
        bucketId,
        name: bucket.getAttribute("data-photo-drop-name") ?? "",
      };
    }
  }
  return { kind: "none" };
}

export interface BeginPhotoDragOptions {
  /** 被拖的照片 id（拖选中项 = 整批；拖未选中项 = 只它一个）。 */
  ids: readonly string[];
  /** 指针在热区内松手（photos 侧 = 移入回收站；回收站侧 = 真删）。 */
  onRecycle: () => void;
  /** 在桶卡片上松手。 */
  onBucket: (bucketId: number, name: string) => void;
  /** 拖动**真正开始**时（越过阈值）的回调，例如把左列切到桶面板。 */
  onStart?: () => void;
}

/** 从 tile 的 `pointerdown` 起拖（网格与胶片带同一套）。 */
export function beginPhotoDrag(event: PointerEvent, options: BeginPhotoDragOptions): void {
  if (event.button !== 0) return;
  let started = false;
  let current: PhotoDragState | null = null;

  trackPointerDrag(event, {
    threshold: 6,
    cancelOutsideWindow: true,
    start: (point) => {
      started = true;
      options.onStart?.();
      current = { ids: [...options.ids], x: point.x, y: point.y, target: { kind: "none" } };
      setPhotoDrag(current);
    },
    move: (point) => {
      if (!started || current === null) return;
      current = { ...current, x: point.x, y: point.y, target: photoDropTargetAt(point) };
      setPhotoDrag(current);
    },
    end: (point, cancelled) => {
      const final = current;
      /*
       * **先判落点，再清预览**：清预览会让热区元素跟着卸载，命中要在这之前算好。
       * 再用拖动过程中最后算出的 `target` **兜底** —— 松手那一帧 DOM 可能正在变
       * （热区重绘、缩略图加载完重排），那一下 `elementFromPoint` 可能恰好问不到；
       * 而拖动时已经亮过的目标就是用户瞄准的那个（2026-10-11）。
       */
      const fresh = !started || cancelled || final === null ? null : photoDropTargetAt(point);
      const target: PhotoDropTarget =
        fresh !== null && fresh.kind !== "none" ? fresh : (final?.target ?? { kind: "none" });
      current = null;
      setPhotoDrag(null);
      if (!started || cancelled || final === null) return;
      if (target.kind === "recycle") options.onRecycle();
      else if (target.kind === "bucket") options.onBucket(target.bucketId, target.name);
    },
  });
}
