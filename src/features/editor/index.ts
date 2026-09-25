/**
 * `editor` 模块的唯一对外出口（`ARCHITECTURE.md` §2）。
 *
 * 对外只有四样：**工作区要用的 store 工厂**、**中列视口**、**左右两栏**、**胶片带适配**。
 * 参数表、拉杆、小零件都是内部实现 —— 工作区不需要知道它们的文件在哪。
 * （`SliderRow` 例外：W3 的管线面板与它同源，但那时也仍然只从这里取。）
 */

export { createEditorStore, CURVE_CHANNELS, EDITOR_TOOLS } from "./store.ts";
export type { CurveChannel, EditorStore, EditorStoreDeps, EditorTool } from "./store.ts";

export { EditorViewport, EDITOR_ZOOM_STEP } from "./viewport.tsx";
export type { EditorViewportProps } from "./viewport.tsx";

export { EditorPanels } from "./panels.tsx";
export type { EditorPanelsProps, EditorPhotoInfo } from "./panels.tsx";

export { LutPanel, LUT_PANEL_WIDTH } from "./lut-panel.tsx";

export { EditorPanelToggles, EditorResetTool, EditorToolbar } from "./toolbar.tsx";

export { createEditorStrip, editorEmptyKind, editorEmptyOffersImport, editorViewportNotice, editorVisibleRenderState } from "./source.ts";
export type {
  EditorEmptyKind,
  EditorStripDeps,
  EditorViewportNotice,
  EditorViewportNoticeInput,
} from "./source.ts";

export { GROUP_LABEL_KEY, PARAM_GROUPS, paramsInGroup, PARAMS } from "./params.ts";
export type { ParamGroup, ParamSpec } from "./params.ts";

export { PendingNote } from "./parts.tsx";

export { createLensQuery } from "./lens-query.ts";
export type { LensQuery, LensQueryState } from "./lens-query.ts";
