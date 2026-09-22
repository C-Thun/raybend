/**
 * tiles → view / film / compare 的共享控制器。
 *
 * import 与 browse 的数据来源不同，但「什么时候进入对比、当前锚点是谁、Enter 请求
 * 交给哪份网格、四态怎么复位、对比集缩到一张时怎么收尾」都不是数据差异。
 * 这些状态只在这里维护，两个工作区不再各抄一套。
 */

import { createEffect, createMemo, createSignal } from "solid-js";

import type { SelectionState } from "../../lib/selection.ts";
import {
  chromeName,
  chromeShowsFilm,
  chromeShowsLeft,
  chromeShowsRight,
  nextChromeStep,
  type ChromeMode,
} from "../../lib/viewer-chrome.ts";
import { compareIds } from "../../lib/viewer-compare.ts";
import type {
  ViewerPhoto,
  ViewerStore,
} from "../../components/ui/viewer/store.ts";

export interface PhotoViewingControllerDeps {
  viewer: ViewerStore;
  /**
   * 这个工作区用哪一套档位表（`lib/viewer-chrome.ts`）。
   *
   * browse 四档、import 三档（2026-09-23 改口径）—— **差异属于配置，不属于控制器**，
   * 所以控制器只有一份，档位表按 flow 传进来。
   */
  chromeMode: () => ChromeMode;
  selection: () => SelectionState;
  setAnchor: (id: string) => void;
  /** 补读后的真实尺寸；`null` 时继续使用 viewer 打开瞬间的快照。 */
  naturalOf: (id: string) => { width: number; height: number } | null;
  /** 对比一形成就补齐这些照片的真实尺寸。 */
  ensureNatural: (ids: readonly string[]) => void | Promise<void>;
  /** browse 用它收起库列表；import 不需要传。 */
  onPreparingViewer?: () => void;
}

export interface PhotoViewingController {
  viewer: ViewerStore;
  /** 档位名（`data-chrome` 属性用） */
  chrome: () => string;
  /** 档位下标（`lib/editor-chrome.ts` 判「进出仅 view」要用） */
  chromeStep: () => number;
  /** 这三条是**当前档位的读数** —— 工作区不再自己拼 `chromeShowsX(chrome())` */
  showsLeft: () => boolean;
  showsRight: () => boolean;
  filmVisible: () => boolean;
  cycleChrome: () => void;
  resetChrome: () => void;
  prepareViewer: () => void;
  openRequest: () => number;
  requestOpen: () => void;
  comparedIds: () => readonly string[];
  comparePhotos: () => readonly ViewerPhoto[];
  comparing: () => boolean;
  selectedCount: () => number;
  filmOnlyIds: () => readonly string[] | undefined;
  toggleCompareStrip: () => void;
  focusComparePhoto: (photo: ViewerPhoto) => void;
}

export function createPhotoViewingController(
  deps: PhotoViewingControllerDeps,
): PhotoViewingController {
  const [chromeStep, setChromeStep] = createSignal(0);
  const [compareStrip, setCompareStrip] = createSignal(false);
  const [openRequest, setOpenRequest] = createSignal(0);

  const comparedIds = createMemo<string[]>(() =>
    compareIds(
      deps.selection().ids,
      deps.viewer.state().photos.map((photo) => photo.id),
      deps.selection().anchor,
    ),
  );

  const comparePhotos = createMemo<ViewerPhoto[]>(() => {
    const byId = new Map(deps.viewer.state().photos.map((photo) => [photo.id, photo]));
    return comparedIds().flatMap((id) => {
      const photo = byId.get(id);
      if (photo === undefined) return [];
      const natural = deps.naturalOf(id);
      return [natural === null ? photo : { ...photo, natural }];
    });
  });

  const comparing = (): boolean =>
    deps.viewer.state().active && comparePhotos().length >= 2;

  /* 一套补读规则供两侧共用：只读参与对比的 2–4 张。 */
  createEffect(() => {
    const ids = comparedIds();
    if (ids.length >= 2) void deps.ensureNatural(ids);
  });

  /* 只剩一张就自动离开「胶片带仅显示对比集」。 */
  createEffect(() => {
    if (!comparing()) setCompareStrip(false);
  });

  const resetChrome = (): void => {
    setChromeStep(0);
  };
  const prepareViewer = (): void => {
    deps.onPreparingViewer?.();
    resetChrome();
  };

  return {
    viewer: deps.viewer,
    chrome: () => chromeName(deps.chromeMode(), chromeStep()),
    chromeStep,
    showsLeft: () => chromeShowsLeft(deps.chromeMode(), chromeStep()),
    showsRight: () => chromeShowsRight(deps.chromeMode(), chromeStep()),
    filmVisible: () => chromeShowsFilm(deps.chromeMode(), chromeStep()),
    cycleChrome: () => {
      setChromeStep((current) => nextChromeStep(deps.chromeMode(), current));
    },
    resetChrome,
    prepareViewer,
    openRequest,
    // 只发请求；真正打开仍由 PhotoGrid 用它手里的完整显示序完成。
    requestOpen: () => {
      setOpenRequest((request) => request + 1);
    },
    comparedIds,
    comparePhotos,
    comparing,
    selectedCount: () => deps.selection().ids.size,
    filmOnlyIds: () => (compareStrip() ? comparedIds() : undefined),
    toggleCompareStrip: () => {
      setCompareStrip((enabled) => !enabled);
    },
    focusComparePhoto: (photo) => {
      const at = deps.viewer.state().photos.findIndex((item) => item.id === photo.id);
      if (at >= 0) deps.viewer.focus(at);
      deps.setAnchor(photo.id);
    },
  };
}
