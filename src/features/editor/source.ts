/**
 * 编辑工作区的**两个纯逻辑块**：胶片带适配 + 空态判定。
 *
 * 为什么单独一个文件：两者都是「看一眼就能验、但在组件里很难验」的规则 ——
 * 写在 JSX 里就只能靠肉眼，抽出来就能单测（本仓的既定做法）。
 *
 * ⚠️ **当前在编哪张由 browse store 的锚点决定**（`AGENTS.md` §11.4 的红线：
 * 「胶片带选择与对比锚点只能调用 `TilesSource.select / setAnchor`」）——
 * 编辑不另造第二份选择模型，否则「编辑里换一张、回浏览还是旧的」这类 bug 必来。
 * 所以这里的适配器**读**锚点、**写**回 browse store，自己不持有当前照片。
 */

import type { FilmStripViewer, ViewerPhoto } from "../../components/ui/viewer/index.ts";

/** 一些照片 + 当前那张的下标（胶片带要的最小形状）。 */
export interface EditorStripDeps {
  /** 当前目录的显示序清单（`photosFromSource(browseSource)`） */
  photos: () => readonly ViewerPhoto[];
  /** browse store 的锚点 → 当前那张的 id */
  anchorId: () => string | null;
  /** 用户点了胶片带里的某一格 → 写回 browse store 的选择 */
  goTo: (index: number) => void;
}

/**
 * 胶片带的轻量适配（`FilmStripViewer` 的三个读数）。
 *
 * 编辑视口是 GPU 直绘，不需要看图件的取图能力 —— 所以这里**不造第二个 `ViewerStore`**，
 * 只把「列表 + 当前下标」这两件事从 browse store 的数据映射过来。
 */
export function createEditorStrip(deps: EditorStripDeps): FilmStripViewer {
  const index = (): number => {
    const id = deps.anchorId();
    if (id === null) return -1;
    return deps.photos().findIndex((photo) => photo.id === id);
  };
  return {
    state: () => ({
      photos: deps.photos(),
      // 锚点不在清单里（刚换目录 / 被筛掉）时下标给 0，但 `active` 为假 ——
      // 胶片带据此不高亮任何一张，而状态栏显示空态
      index: Math.max(0, index()),
      active: index() >= 0,
    }),
    current: () => {
      const at = index();
      return at < 0 ? null : (deps.photos()[at] ?? null);
    },
    goTo: (next) => deps.goTo(next),
  };
}

/* ══════════════════════════════════════════════════════════════
 * 空态（`design/editor.md` §3.8 画了四态）
 * ══════════════════════════════════════════════════════════════ */

/**
 * 视口里该印哪个水印（四态之一）。
 *
 * | 值 | 何时 | 水印 |
 * | --- | --- | --- |
 * | `no-repository` | 一个库都没有 | 「还没有库」+ 去导入 |
 * | `no-directory` | 有库但没选目录 | 「这个库还没有导入目录」 |
 * | `no-photos` | 目录里没有照片 | 「这个目录里还没有照片」 |
 * | `no-selection` | 有照片但**没选中** | 「从下面的胶片带里选一张」 |
 * | `null` | 有照片且选中了 | 画照片（W2 起是 GPU 直出） |
 *
 * 顺序即优先级：**外层的缺失先报** —— 没有库的时候说「没有照片」会把人引到错的方向。
 */
export type EditorEmptyKind =
  | "no-repository"
  | "no-directory"
  | "no-photos"
  | "no-selection"
  | null;

export interface EditorEmptyInput {
  /** 库列表还在读（读的过程中不报空态 —— 否则会闪一下「还没有库」，很难看） */
  loadingRepositories: boolean;
  repositoryId: string | null;
  scopePath: string | null;
  /** 当前目录的照片数（browse store 的 `total()`） */
  photoCount: number;
  /** 有没有当前照片（锚点是否落在清单里） */
  hasSelection: boolean;
  /** 照片清单还在读 */
  loadingPhotos: boolean;
}

export function editorEmptyKind(input: EditorEmptyInput): EditorEmptyKind {
  if (input.loadingRepositories) return null;
  if (input.repositoryId === null) return "no-repository";
  if (input.scopePath === null) return "no-directory";
  if (input.loadingPhotos) return null;
  if (input.photoCount === 0) return "no-photos";
  if (!input.hasSelection) return "no-selection";
  return null;
}

/** 空态水印的图标名（桶在 `viewport.tsx` 里映射到具体图标组件）。 */
export type EditorEmptyIcon = "library" | "folder" | "photo" | "select";

export function editorEmptyIcon(kind: Exclude<EditorEmptyKind, null>): EditorEmptyIcon {
  switch (kind) {
    case "no-repository":
      return "library";
    case "no-directory":
      return "folder";
    case "no-photos":
      return "photo";
    case "no-selection":
      return "select";
  }
}

/** 空态是否该给「去导入」那颗按钮（只有「没有库」时才给 —— 别的态去了导入也解决不了）。 */
export function editorEmptyOffersImport(kind: EditorEmptyKind): boolean {
  return kind === "no-repository";
}

/* ══════════════════════════════════════════════════════════════
 * 洞口该显示什么提示（M3-W2）
 * ══════════════════════════════════════════════════════════════ */

/**
 * 洞口里的提示种类。
 *
 * | 值 | 何时 | 文案 |
 * | --- | --- | --- |
 * | `browser` | 浏览器预览（没有 Tauri / 没有渲染线程） | 「桌面版会在这里直绘照片」 |
 * | `init` | 渲染器还在初始化 | 「正在准备 GPU 视口…」 |
 * | `loading` | 照片在解码 | 「正在载入照片…」 |
 * | `decode-error` | 这张照片解不开 | 解码错误原文 |
 * | `render-error` | 渲染线程报错 / 已放弃重启 | 错误原文 + 重试 |
 * | `null` | 照片已经画出来了（或本来就是空态水印） | 什么都不显示 |
 */
export type EditorViewportNotice =
  | "browser"
  | "init"
  | "loading"
  | "decode-error"
  | "render-error";

export interface EditorViewportNoticeInput {
  /** 四态空态（非 `null` 时提示让位给水印） */
  empty: EditorEmptyKind;
  /** 有没有选中照片 */
  hasPhoto: boolean;
  /** 渲染线程的状态快照（`null` = 拿不到：浏览器 / 还没 bind） */
  state: {
    bound: boolean;
    ready: boolean;
    paintedPath: string | null;
    decode: "idle" | "loading" | "ready" | "error";
    decodeError: string | null;
    lastError: string | null;
  } | null;
}

/**
 * 洞口里该印哪个提示（纯函数，顺序即优先级）。
 *
 * 三条口径：
 *
 * 1. **空态优先**：没有可编辑的照片时，提示让位给四态水印（那是更准确的信息）；
 * 2. **照片没出来才提示**：`paintedPath` 有值就是画出来了 —— 再挂一条「正在载入」是噪声；
 * 3. **错误压过进度**：解码/渲染失败时显示错误原文（不是永远转圈的「正在载入」）。
 */
export function editorViewportNotice(
  input: EditorViewportNoticeInput,
): EditorViewportNotice | null {
  if (input.empty !== null) return null;
  if (!input.hasPhoto) return null;
  if (input.state === null) return "browser";
  if (input.state.lastError !== null) return "render-error";
  if (input.state.decode === "error") return "decode-error";
  if (input.state.paintedPath !== null) return null;
  if (!input.state.bound || !input.state.ready) return "init";
  if (input.state.decode === "loading") return "loading";
  return "init";
}
