/**
 * tiles → 看图件 / 胶片带的**共享转换**（2026-09-23 抽出）。
 *
 * 这两件事本来各写一份：
 *
 * * `PhotoGrid` 里有一份「显示序 → `ViewerPhoto[]`」（进看图时给 viewer）；
 * * `BrowseWorkspace` 里有一份「`ViewerPhoto` → `TilesViewingInfo`」（底部状态栏那四个数）。
 *
 * 编辑工作区（`workspaces/editor`）**两个都要**：它没有网格，但同样有胶片带与状态栏。
 * 按 `AGENTS.md` §2.12「同一个能力只允许有一套实现」，把它们收进这里一份 ——
 * 网格、浏览、编辑三处都调它，不许再抄第二遍。
 *
 * 依赖方向说明：本文件放在 `components/ui/viewer/`（看图件这一侧），
 * 向 `components/ui/tiles/`（网格那一侧）取**类型**与最小读数接口 —— 两边同属 `components/ui` 层，
 * 不违反 `scripts/check-architecture.mjs` 的分层规则。
 */

import type { TilesSource } from "../tiles/source.ts";
import type { TilesViewingInfo } from "../tiles/TilesControlBar.tsx";
import type { ViewerPhoto, ViewerState } from "./store.ts";

/**
 * 网格 / 数据源的**显示序** → 看图件要的照片清单。
 *
 * 规则：
 * * 还没取回来的格子（分页没到的那些）**跳过** —— 它们没有路径，进了列表只会是个破图；
 * * 标记字段是**可选**的（导入侧没有标记概念），有才带；
 * * 真实宽高从数据源拿（`naturalOf`），拿不到就不带 —— 看图件自己会在图载入后补上。
 */
export function photosFromSource(source: TilesSource): ViewerPhoto[] {
  const out: ViewerPhoto[] = [];
  for (let index = 0; index < source.count(); index += 1) {
    const item = source.itemAt(index);
    if (item === null) continue;
    const natural = source.naturalOf(item.id);
    const marks = item.marks;
    out.push({
      id: item.id,
      path: item.path,
      ...(item.imageKey ? {imageKey:item.imageKey} : {}),
      ...(item.exportVariant ? {exportVariant:item.exportVariant} : {}),
      fileName: item.fileName,
      ...(natural === null ? {} : { natural }),
      ...(marks === undefined
        ? {}
        : {
            marks: {
              rating: marks.rating,
              colorLabel: marks.colorLabel,
              likeState: marks.likeState ?? null,
              lockLevel: marks.locked ? 1 : 0,
            },
            flag: marks.flag,
          }),
    });
  }
  return out;
}

/**
 * 看图件当前那张 → 底部状态栏要的四个数（文件名 + 锁 + 评级 + 色标 + 旗标 + 赞踩）。
 *
 * `null`（没有当前照片）给一份全空的，状态栏自己会显示占位。
 */
export function viewingInfoOf(photo: ViewerPhoto | null): TilesViewingInfo {
  const like = photo?.marks?.likeState;
  return {
    fileName: photo?.fileName ?? null,
    lockLevel: photo?.marks?.lockLevel ?? 0,
    rating: photo?.marks?.rating ?? 0,
    colorLabel: photo?.marks?.colorLabel ?? null,
    flag: photo?.flag ?? null,
    like: like === "like" || like === "dislike" ? like : null,
  };
}

/**
 * 胶片带真正用到的**最小读数**（`ViewerStore` 天然满足这个形状）。
 *
 * 编辑视口是 GPU 直绘、不需要看图件的取图能力，但它同样要有胶片带 ——
 * 所以胶片带只声明自己用到的这三个方法，编辑侧给一个轻量适配即可，
 * **不必为了凑类型而造第二个 `ViewerStore`**。
 */
export type FilmStripViewer = {
  state: () => Pick<ViewerState, "photos" | "index" | "active">;
  current: () => ViewerPhoto | null;
  goTo: (index: number) => void;
};
