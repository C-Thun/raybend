/**
 * 编辑工作区的状态（`memory/ARCHITECTURE.md` §3 的状态归属）。
 *
 * 这一层只管**编辑器自己的界面状态**，不碰照片数据：
 *
 * | 归它管 | 归别人管 |
 * | --- | --- |
 * | `Tab` 档位与 LUT 面板（`lib/editor-chrome.ts` 的状态机） | 当前库 / 目录 / 照片清单 / 选择 → `features/browse/store.ts` |
 * | 三个工具的互斥模式、裁切比例、旋转角度 | 卡片照片与胶片带 → browse store + 胶片带适配 |
 * | 调整参数草稿、曲线通道 | 视口变换（zoom/pan）→ **Rust 独有**（`AGENTS.md` §6.1） |
 * | LUT 分类（设备级偏好） | |
 *
 * 「当前在编哪张」**不在这里**：它是 browse store 的锚点（`AGENTS.md` §11.4 的红线）——
 * 编辑不另造第二份选择模型，否则「编辑里换一张、回浏览还是旧的」这种 bug 必来。
 *
 * ⚠️ W1 的边界（写在最显眼处，别当遗漏）：参数拉杆只改**数值**，不改画面；
 * 裁切 / 旋转 / 对比的模式互斥由这里管理；画布草稿由 Rust 渲染线程持有。
 */

import { batch, createSignal } from "solid-js";

import type {
  AutoAdjustBaseline,
  DevelopEditBase,
  DevelopNrMethod,
  DevelopParamsPayload,
  DevelopSettings,
  EditorRenderState,
  EditGeometry,
} from "../../api/types.ts";
import { t } from "../../i18n/index.ts";
import type { Area, LutLayer } from "../../api/editor.ts";

import {
  initialEditorChrome,
  editorChromeName,
  editorIssuesVisible,
  editorLeftPanelVisible,
  editorLutVisible,
  editorPresetVisible,
  editorShowsFilm,
  editorShowsRight,
  resetEditorChrome,
  setEditorLeftPanel,
  stepEditorTab,
  toggleEditorLeftPanel,
  type EditorChromeState,
  type EditorLeftPanel,
} from "../../lib/editor-chrome.ts";
import {
  DEFAULT_EDITOR_PREFS,
  readEditorPrefs,
  writeEditorPrefs,
  type EditorPrefs,
} from "../../lib/editor-prefs.ts";
import {
  addLutCategory,
  ensureDefaultLutCategory,
  toggleExpandedCategory,
  type LutCategory,
} from "../../lib/lut-library.ts";
import { isIdentityCurve, type CurvePoint } from "../../lib/curve.ts";

/** 链长上限（与 Rust `store::develop::MAX_LUT_LAYERS`、着色器保持一致）。 */
export const MAX_LUT_LAYERS = 8;

/** 编辑区层数上限（**不含默认层**；与 Rust `store::develop::MAX_AREAS` 一致）。 */
export const MAX_AREAS = 32;

/** 控制点颜色槽位数量（`memory/DESIGN.md` §15.5；与 Rust `POINT_COLORS` 一致）。 */
export const POINT_COLORS = 8;

/**
 * 控制点种子的定义版本（与 Rust `store::develop::SEED_VERSION` 保持一致）。
 *
 * 种子从「线性域 `[luma, c1, c2]`」换成了「感知域 `[Y′, Cb, Cr]`」（2026-11-01），
 * 旧数据的种子没法换算 —— 载入时按 `x/y` 逐点重取一次（位置不变）。
 */
export const SEED_VERSION = 1;

/** 层行（默认层 + 附加层，按计算顺序）。 */
export type LayerRow =
  | { kind: "default"; key: "default" }
  | { kind: "area"; key: string; area: Area };

/**
 * 行序 → `(areas, defaultIndex)`（`layerRows` 与拖动的唯一换算）。
 *
 * 与 Rust `store::develop::area_rows` 同一套口径：默认层是 `defaultIndex` 那个位置，
 * `areas` 只装非默认层、顺序即计算顺序。
 */
export function rowsToLayers(rows: readonly LayerRow[]): { areas: Area[]; defaultIndex: number } {
  const areas: Area[] = [];
  let defaultIndex = 0;
  rows.forEach((row, index) => {
    if (row.kind === "default") defaultIndex = index;
    else areas.push(row.area);
  });
  return { areas, defaultIndex };
}

/** `(areas, defaultIndex)` → 行序（`rowsToLayers` 的逆）。 */
export function layersToRows(areas: readonly Area[], defaultIndex: number): LayerRow[] {
  const rows: LayerRow[] = [];
  let next = 0;
  for (let position = 0; position <= areas.length; position += 1) {
    if (position === Math.min(Math.max(defaultIndex, 0), areas.length)) {
      rows.push({ kind: "default", key: "default" });
    }
    if (next < areas.length) {
      rows.push({ kind: "area", key: String(areas[next].id), area: areas[next] });
      next += 1;
    }
  }
  // 兜底：默认层位置越界时上面那个循环可能一格都没放（数据坏了也不该爆）
  if (!rows.some((row) => row.kind === "default")) rows.unshift({ kind: "default", key: "default" });
  return rows;
}
import {
  buildPresetSnapshot,
  planPresetApply,
  prunePresetCollapsed,
  prunePresetSelection,
  type GroupParams,
  type PresetDirectory,
  type PresetGroup,
  type PresetLibrary,
  type PresetRecord,
  type PresetSelection,
  type PresetSnapshot,
} from "../../lib/presets.ts";
import { maskWeightPayload } from "../../lib/mask-debug.ts";
import {
  AREA_OPACITY,
  CROP_RATIOS,
  defaultParams,
  invertRatio,
  POINT_TOLERANCE,
  paramSpec,
  paramsInGroup,
  PARAM_DEFAULTS,
  PARAM_IDS,
  type CropRatio,
} from "./params.ts";

/** 三个互斥的画布工具（`prompts/editor.pd`：同一时刻只有一个控制块）。 */
export type EditorTool = "crop" | "rotate" | "compare";

export const EDITOR_TOOLS: readonly EditorTool[] = ["crop", "rotate", "compare"];

/** 曲线编辑器的通道（W3 接管线；W1 只是能切）。 */
export type CurveChannel = "rgb" | "r" | "g" | "b";

export const CURVE_CHANNELS: readonly CurveChannel[] = ["rgb", "r", "g", "b"];

/**
 * 预设的大类 → 参数 id（**从参数表派生**，specs §3 规则 3：不手写第二份；
 * 参数表加项时预设自动跟上）。curve / lut 不走数值表，快照逻辑单独处理。
 */
const PRESET_GROUP_PARAMS: GroupParams = {
  tone: paramsInGroup("tone").map((spec) => spec.id),
  color: paramsInGroup("color").map((spec) => spec.id),
  detail: paramsInGroup("detail").map((spec) => spec.id),
  lens: paramsInGroup("lens").map((spec) => spec.id),
};

/** 轮询得到相同拍摄色温时，保留参数对象身份，避免空闲显影重复提交。 */
export function withTemperatureBaseline(current: Record<string, number>, baseline: number): Record<string, number> {
  return current.temperature === baseline ? current : { ...current, temperature: baseline };
}

export interface EditorStoreDeps {
  /** 偏好读写（默认真 `localStorage`；测试里注入假的） */
  readPrefs?: () => EditorPrefs;
  writePrefs?: (prefs: EditorPrefs) => void;
}

export type EditorResetStage = "none" | "edits" | "automatic";

export interface EditorStore {
  /* ── 档位与面板（`lib/editor-chrome.ts`）────────────── */
  chromeStep: () => number;
  /** 档位名（`data-chrome` 用） */
  chromeName: () => string;
  /** 左列这一刻可见吗（= 档位允许左列 **且** 有面板开着；底部总览 / 信息跟着它） */
  leftVisible: () => boolean;
  /** 左列的**用户意图**（持久化的那一个）：`lut` / `issues` / `null` */
  leftPanel: () => EditorLeftPanel | null;
  /** LUT 面板这一刻可见吗（= 左列可见 **且** 开着的是 LUT） */
  lutVisible: () => boolean;
  /** 预设面板这一刻在不在（toolsbar left 的按钮按下态） */
  presetVisible: () => boolean;
  /** 定稿面板这一刻可见吗（= 左列可见 **且** 开着的是定稿） */
  issuesVisible: () => boolean;
  showsRight: () => boolean;
  showsFilm: () => boolean;
  /** `Tab`：三档循环 */
  cycleTab: () => void;
  /** 点 `toolsbar left` 的开关（含命令面板那条路）：同一个再点一次 = 关整列，点另一个 = 切过去 */
  toggleLeft: (panel: EditorLeftPanel) => void;
  setLeftPanel: (panel: EditorLeftPanel | null) => void;
  /** 换库 / 换目录时把档位复位（面板偏好不动） */
  resetChrome: () => void;

  /* ── 三个工具（互斥）────────────────────────────────── */
  tool: () => EditorTool | null;
  /** 点工具按钮：再点一次同一个 = 退出（`.pd`：裁切可反复点击开关） */
  toggleTool: (tool: EditorTool) => void;
  /** 明确退出（`Esc` / 控制块的取消） */
  closeTool: () => void;

  /* ── 裁切控制块 ─────────────────────────────────────── */
  cropRatioId: () => string;
  /** 比例项（含「自由」「原始比例」与固定比例；自定义始终可选） */
  cropRatio: () => CropRatio;
  setCropRatioId: (id: string) => void;
  /** 反转比例（4:3 ⇄ 3:4） */
  flipCropRatio: () => void;
  /** 取消比例限制 → 自由 */
  unlinkCropRatio: () => void;
  /** 横 / 纵向比例输入（有效输入即时切换「自定义」） */
  cropWidth: () => number;
  cropHeight: () => number;
  setCropSize: (width: number, height: number) => void;

  /* ── 旋转控制块 ─────────────────────────────────────── */
  angle: () => number;
  setAngle: (degrees: number) => void;
  resetAngle: () => void;

  /* ── 调整参数（M3-W3：真的改画面）──────────────────── */
  paramValue: (id: string) => number;
  /** 这一项的**基线**（色温随照片的 as-shot，其余是静态默认值） */
  paramBaseline: (id: string) => number;
  setParam: (id: string, value: number) => void;
  /** 这一项回到基线（= DB 里删掉这一行） */
  resetParam: (id: string) => void;
  resetParams: () => void;
  autoAdjustBaseline: () => AutoAdjustBaseline | null;
  /** 只记录自动算法实际产生的字段，不把已有手动调整当成基线。 */
  applyAutoAdjust: (baseline: AutoAdjustBaseline) => void;
  resetStage: () => EditorResetStage;
  /** 第一层恢复自动基线；第二层清除自动结果及基础曲线。 */
  resetDevelop: (stage: Exclude<EditorResetStage, "none">) => void;
  /** 这张照片的拍摄色温（K）—— 色温拉杆的基线；`null` = 渲染线程还没解出来 */
  asShotTemperature: () => number | null;
  /** 渲染线程报回来的拍摄色温（工作区写进来；**不算用户改动**，不抬 rev） */
  setAsShotTemperature: (kelvin: number | null) => void;
  /** 当前 IPC 载荷（帧合并后发出去的那一份；只装与基线不同的项） */
  developPayload: () => DevelopParamsPayload;
  /** 参数变过几次（与 `committedRev` 比就知道「有没有还没落库的改动」） */
  developRev: () => number;
  /** 最近一次**成功落库**（或从库里读回来）对应的 rev */
  committedRev: () => number;
  /** 落库成功之后清掉 dirty 标记 */
  markCommitted: (rev: number) => void;
  /** 有没有还没落库的改动 */
  developDirty: () => boolean;
  /**
   * **confirm（快照建立）的计数与最近一条说明** —— 纯诊断。
   *
   * 快照只在 confirm 时建立（见 `EditorWorkspace.confirmEdit` 的节点清单）：
   * 真机上「快照少了/多了」时，先看这个计数涨没涨 —— 涨了说明 confirm 发生了
   * （那问题在快照内容），没涨说明某个节点没接到 confirm（那问题在接线）。
   */
  confirmTick: () => number;
  confirmNote: () => string | null;
  noteConfirm: (note: string | null) => void;
  /**
   * **手指还按在滑杆 / 曲线上**（人类 2026-09-24）。
   *
   * 它只影响一件事：拖动期间 Rust 侧**只算预览档**，松手那一下才按缩放补全尺寸
   * （2026-10-11 起**没有档位**：任何时刻都是同一张原图，`interactive` 只用于限流合并）；
   * ⚠️ 档位**只跟「手指是否按着」有关**，与缩放无关（崔总 2026-10-11）。
   * 它跟着载荷一起发出去（不单独开 IPC）。
   */
  paramDragging: () => boolean;
  /** 拖拽开始（滑杆的 `onValueChangeStart` / 曲线的 `pointerdown`） */
  beginParamDrag: () => void;
  /** 拖拽结束（松手）—— 这一下会让载荷重发，Rust 侧于是补全尺寸 */
  endParamDrag: () => void;

  /* ── 编辑区（area）层 ─────────────────────────────── */
  /**
   * 层列表的**行序**（含默认层）：`{ kind: "default" }` 或 `{ kind: "area", area }`。
   *
   * 列表自上而下 = 计算顺序；默认层可以拖到中间（那就是「LUT 在这个层之后」）。
   */
  layerRows: () => readonly LayerRow[];
  /** 当前正在编辑哪一层：`null` = 默认层。 */
  activeAreaId: () => number | null;
  /** 换层（点面板行）。被删掉的层由 store 自己收敛，调用方不用管。 */
  selectArea: (id: number | null) => void;
  /** 当前层（`null` = 默认层） */
  activeArea: () => Area | null;
  /** 当前层的显示名（空名字回落到类型默认名，走 i18n）。 */
  activeAreaName: () => string;
  /** 是不是在默认层（全局功能只有它可用）。 */
  onDefaultLayer: () => boolean;
  /** 新建一层**全图**层（追加到末尾）。 */
  createArea: () => number;
  /**
   * **已武装**：点了「新建控制点」、正等着在画面上点一下。
   *
   * 这一态**没有建任何层** —— 以前先建一层再点，面板里会凭空多一行，
   * 下面还挂一条虚线幽灵行（崔总 2026-11-01：那个东西没有存在的必要）。
   */
  pointArmed: () => boolean;
  /** 武装 / 取消（按钮本身是开关；`Esc` 也走 `disarm`）。 */
  armPointArea: () => void;
  disarmPointArea: () => void;
  /** **落点**：到这一刻才创建那一层（追加到末尾、成为当前层）。 */
  placePointArea: (point: { x: number; y: number; seed: [number, number, number] }) => number;
  removeArea: (id: number) => void;
  /** 拖动排序：把第 `from` 行移到第 `to` 行（行序含默认层）。 */
  moveAreaRow: (from: number, to: number) => void;
  renameArea: (id: number, name: string) => void;
  setAreaEnabled: (id: number, enabled: boolean) => void;
  setAreaColor: (id: number, color: number) => void;
  /** 控制点的采样位 / 容差（拖动松手那一下写；种子不动）。 */
  setAreaPoint: (id: number, point: { x?: number; y?: number; tolerance?: number }) => void;
  /** 旧种子（`seedVersion < SEED_VERSION`）重新取样后的写回：只换 seed 与版本。 */
  adoptAreaSeed: (id: number, seed: [number, number, number]) => void;
  /** 满 32 层了吗（两个新建按钮据此变灰）。 */
  areasFull: () => boolean;
  /** `显示控制点`（眼睛）：纯视图偏好，`localStorage`，不进 profile / 撤销 / XMP。 */
  showPoints: () => boolean;
  setShowPoints: (show: boolean) => void;

  /* ── 编辑基准（SOOC / RAW，人类 2026-09-24）────── */
  /** 这次编辑拿哪个当底（**默认 RAW**；会话内跟着用户走，重进编辑器回到默认） */
  editBase: () => DevelopEditBase;
  setEditBase: (base: DevelopEditBase) => void;
  /** 这张照片两侧各有没有可用文件（缺的那一侧**禁用**，不让用户白点） */
  editBaseAvailable: () => { bitmap: boolean; raw: boolean };
  /** 工作区拿到 `develop_edit_target` 的结果后写进来 */
  setEditBaseAvailable: (available: { bitmap: boolean; raw: boolean }) => void;
  /** 已确认的成片几何；草稿由 Rust 视口持有，确认后才写这里。 */
  geometry: () => EditGeometry | null;
  setGeometry: (geometry: EditGeometry | null) => void;
  /** 换照片：把库里读回来的一份编辑栈灌进来（并把它当成「已落库」） */
  loadDevelop: (
    values: Record<string, number>,
    curves: Partial<Record<CurveChannel, readonly CurvePoint[]>>,
    settings?: DevelopSettings,
  ) => void;
  /**
   * 载入一份**要当成新工作状态**的栈（查看目标：SOOC / RAW / 定稿 / latest 均用它）。
   * 载入后由工作区调 `markCommitted` 标为已落库 —— 查看**不写** `develop_stacks`
   * （`latest` 只在用户编辑操作时写，见 `EditorWorkspace.applyViewedStack`）。
   */
  applyDevelop: EditorStore["loadDevelop"];

  /* ── 镜头 / 降噪方式（M3-W4）──────────────────────── */
  /** 镜头配置文件（`null` = 未选择；`"none"` = 显式关掉；否则是 `maker|model`） */
  lensProfile: () => string | null;
  /** 当前照片 RAW 基础曲线：null 未选择，none 显式不用。 */
  baseCurveProfile: () => string | null;
  colorState: () => import("../../api/color.ts").PhotoColorState | null;
  setColorState: (next: import("../../api/color.ts").PhotoColorState | null) => void;
  proofEnabled: () => boolean;
  /** 右栏第 3 组的页签（预设 2026-11-05 搬到左列面板；色盘是空面板占位） */
  advancedTab: () => "curve" | "swatch" | "color";
  setAdvancedTab: (tab: "curve" | "swatch" | "color") => void;
  setProofEnabled: (enabled: boolean) => void;
  proofTarget: () => import("../../lib/color-model.ts").OutputColor;
  setProofTarget: (target: import("../../lib/color-model.ts").OutputColor) => void;
  proofWarning: () => boolean;
  setProofWarning: (enabled: boolean) => void;
  baseCurvePoints: () => readonly CurvePoint[] | null;
  setBaseCurve: (id: string | null, points: readonly CurvePoint[] | null) => void;
  /** 应用层链（自上而下 = 逐层处理顺序；空 = 没有 LUT）。 */
  lutChain: () => readonly LutLayer[];
  lutEnabled: () => boolean;
  lutEnabledSetting: () => boolean | null;
  /** 点列表里的 tile：换/移到第 1 层（底层）。 */
  selectLutAsBase: (id: string) => void;
  /** 点 tile 右上角圆圈：加/减一层。 */
  toggleLutInChain: (id: string) => void;
  setLutLayerEnabled: (index: number, enabled: boolean) => void;
  /**
   * 强度 0..1（1 = 100%）。**拖动中与松手后都走它**：
   * 开关始终等于「强度 > 0」（拉到 0 就关层、拉回 >0 就开层，同一步里定）。
   */
  setLutLayerStrength: (index: number, strength: number) => void;
  /**
   * 强度**松手**时调用：这一下只是一个 confirm（撤销节点），
   * 开关早已由 `setLutLayerStrength` 按「> 0 ⇔ 开」定好。
   *
   * 只动这一层的开关，**不碰**顶部总开关（总开关关着时链照样不参与渲染）。
   */
  commitLutLayerStrength: (index: number, strength: number) => void;
  moveLutLayer: (from: number, to: number) => void;
  removeLutLayer: (index: number) => void;
  setLut: (id: string | null, enabled: boolean) => void;
  setLutEnabled: (enabled: boolean) => void;
  setLutCategories: (categories: readonly LutCategory[]) => void;
  setLensProfile: (key: string | null) => void;
  /** 配置文件那一半的开关（`null` = 默认开；**手动三根拉杆不受它影响**） */
  lensEnabled: () => boolean | null;
  setLensEnabled: (enabled: boolean | null) => void;
  /** 降噪方式（`null` = 快速档） */
  nrMethod: () => DevelopNrMethod | null;
  setNrMethod: (method: DevelopNrMethod | null) => void;
  /** 自动调整的后台请求状态。 */
  autoAdjusting: () => boolean;
  setAutoAdjusting: (busy: boolean) => void;

  /* ── 曲线 ───────────────────────────────────────────── */
  curveChannel: () => CurveChannel;
  setCurveChannel: (channel: CurveChannel) => void;
  /** 某个通道的控制点（归一化 0..1；恒等曲线是 `[[0,0],[1,1]]`） */
  curvePoints: (channel: CurveChannel) => readonly CurvePoint[];
  setCurvePoints: (channel: CurveChannel, points: readonly CurvePoint[]) => void;
  /** 某个通道回到恒等 */
  resetCurve: (channel: CurveChannel) => void;

  /* ── LUT 分类（设备级偏好）─────────────────────────── */
  lutCategories: () => readonly LutCategory[];
  /** 新建分类；重名返回 `false`（界面据此提示，不静默吞掉） */
  addCategory: (name: string) => boolean;
  expandedCategory: () => string | null;
  toggleCategory: (id: string) => void;

  /* ── 预设（设备级资产，specs/editor-presets.md）──── */
  presetDirectories: () => readonly PresetDirectory[];
  presets: () => readonly PresetRecord[];
  setPresetLibrary: (library: PresetLibrary) => void;
  presetSelection: () => PresetSelection;
  selectPresetDirectory: (id: string) => void;
  /** 点预设行：无 Shift = 单选替换；Shift = 加/减多选 */
  selectPreset: (id: string, event: { shiftKey: boolean }) => void;
  /** 目录展开态（缺省 = 展开；换照片/切页签不重置） */
  presetExpanded: (id: string) => boolean;
  togglePresetDirectory: (id: string) => void;
  /** 当前编辑栈的大类快照（新建预设用；序列化由工作区完成）。 */
  presetSnapshot: (groups: readonly PresetGroup[]) => PresetSnapshot;
  /** 按快照保存的大类**整体覆盖**当前编辑栈（LUT 丢失静默忽略；specs §3）。 */
  /** `lutDropped` = 合并后超出 8 层被丢掉的层数（0 = 没丢），调用方提示一次。 */
  applyPresetSnapshot: (snapshot: PresetSnapshot) => { wbSkipped: boolean; lutDropped: number };

  /* ── GPU 视口（M3-W2）────────────────────────────── */
  /**
   * 渲染线程的最近一次快照。
   *
   * 它住在 store 而不是工作区里，是因为**两处都要用它**：
   * 工作区拿它决定 `holeActive`，视口拿它显示「不可用 / 正在载入」的提示。
   */
  renderState: () => EditorRenderState | null;
  setRenderState: (state: EditorRenderState | null) => void;
  /**
   * 洞口那条 DOM 链要不要透明（= 渲染器 ready 且**真的画出过照片**）。
   *
   * `App.tsx` 的根节点也读它（洞口之上一直到根节点都不能有底色）——
   * 所以它只能有一份，不能各工作区自己存一份。
   */
  holeActive: () => boolean;
  setHoleActive: (value: boolean) => void;
}

/** 四个通道的恒等曲线（**每次都要新对象** —— 直接改会被当成没变）。 */
function identityCurves(): Record<CurveChannel, CurvePoint[]> {
  const identity = (): CurvePoint[] => [
    [0, 0],
    [1, 1],
  ];
  return { rgb: identity(), r: identity(), g: identity(), b: identity() };
}

export function createEditorStore(deps: EditorStoreDeps = {}): EditorStore {
  const readPrefs = deps.readPrefs ?? (() => readEditorPrefs());
  const writePrefs = deps.writePrefs ?? ((prefs: EditorPrefs) => writeEditorPrefs(prefs));

  const initial = ((): EditorPrefs => {
    try {
      return readPrefs();
    } catch {
      return { ...DEFAULT_EDITOR_PREFS };
    }
  })();

  /*
   * **默认分类**（人类 2026-09-23）：每次启动都查一遍，`默认分类` / `Default`
   * 两个名字任意一个在就不新建；没有就按**当前界面语言**建一个。
   * 用户删了不拦（开源软件不讲究那么多），下次启动照建。
   *
   * 补出来的那一次顺手落盘 —— 之后每次启动都是恒等的（不会反复写存储）。
   */
  const initialCategories = ensureDefaultLutCategory(
    initial.lutCategories,
    t("editor.lut.defaultCategory"),
  );
  if (initialCategories.length !== initial.lutCategories.length) {
    try {
      writePrefs({
        leftPanel: initial.leftPanel,
        lutCategories: initialCategories,
        showPoints: initial.showPoints,
      });
    } catch {
      // 写不进去不影响这次会话（偏好是锦上添花）
    }
  }

  const [chrome, setChrome] = createSignal<EditorChromeState>(
    initialEditorChrome(initial.leftPanel),
  );
  const [categories, setCategories] = createSignal<LutCategory[]>([
    ...initialCategories,
  ]);
  const [expanded, setExpanded] = createSignal<string | null>(null);
  /* 预设：整库放 store（页签切换不丢选中/展开态）；选中/展开是会话内状态，不持久化 */
  const [presetDirectories, setPresetDirectories] = createSignal<PresetDirectory[]>([]);
  const [presetList, setPresetList] = createSignal<PresetRecord[]>([]);
  const [presetSelectionState, setPresetSelectionState] = createSignal<PresetSelection>(null);
  const [presetCollapsed, setPresetCollapsed] = createSignal<Record<string, boolean>>({});
  /*
   * 编辑区（area）层：`areas` 只装**默认层之外**的层，`defaultIndex` 是默认层在
   * 层列表里的位置（`specs/editor-area.md` §3.1）。`activeAreaId = null` = 默认层。
   */
  const [areas, setAreas] = createSignal<Area[]>([]);
  const [defaultIndex, setDefaultIndexSignal] = createSignal(0);
  const [activeAreaId, setActiveAreaId] = createSignal<number | null>(null);
  /**
   * 控制点「已武装」：点了新建控制点、**还没在画面上落点**。
   *
   * 这一态**不建任何层**（崔总 2026-11-01：以前先建一层再点，面板里凭空多一行，
   * 还挂一个永远不该存在的虚线幽灵行）；画面点下去那一刻才创建并 append 到末尾。
   */
  const [pointArmed, setPointArmed] = createSignal(false);
  /** 层 ID 发号器：**单调递增、永不复用**（删了也往后走）。 */
  const [areaSeq, setAreaSeq] = createSignal(1);
  const [showPoints, setShowPointsSignal] = createSignal(initial.showPoints);

  const [tool, setTool] = createSignal<EditorTool | null>(null);
  const [cropRatioId, setCropRatioId] = createSignal<string>("free");
  const [cropSize, setCropSizeSignal] = createSignal({ width: 3, height: 2 });
  const restoreCropRatio = (saved: EditGeometry | null): void => {
    const setting = saved?.cropRatio;
    const valid = setting !== null && setting !== undefined
      && (setting.id === "custom" || CROP_RATIOS.some((item) => item.id === setting.id))
      && Number.isFinite(setting.width) && Number.isFinite(setting.height)
      && setting.width > 0 && setting.height > 0
      && setting.width / setting.height >= 0.01 && setting.width / setting.height <= 100;
    setCropRatioId(valid ? setting.id : "free");
    setCropSizeSignal(valid
      ? { width: setting.width, height: setting.height }
      : { width: 3, height: 2 });
  };
  const [angle, setAngleSignal] = createSignal(0);
  const [params, setParams] = createSignal<Record<string, number>>(defaultParams());
  const [curveChannel, setCurveChannel] = createSignal<CurveChannel>("rgb");
  const [asShot, setAsShot] = createSignal<number | null>(null);
  const [temperatureExplicit, setTemperatureExplicit] = createSignal(false);
  const [curves, setCurves] = createSignal<Record<CurveChannel, CurvePoint[]>>(
    identityCurves(),
  );
  const [developRev, setDevelopRev] = createSignal(0);
  const [committedRev, setCommittedRev] = createSignal(0);
  /** confirm 计数与说明（诊断；见接口上的注释） */
  const [confirmTick, setConfirmTick] = createSignal(0);
  const [confirmNote, setConfirmNote] = createSignal<string | null>(null);
  const [paramDragging, setParamDragging] = createSignal(false);
  const [editBase, setEditBase] = createSignal<DevelopEditBase>("raw");
  const [editBaseAvailable, setEditBaseAvailable] = createSignal({ bitmap: false, raw: false });
  type LensSide = { profile: string | null; enabled: boolean | null };
  const emptyLensSides = (): Record<DevelopEditBase, LensSide> => ({
    raw: { profile: null, enabled: null },
    sooc: { profile: null, enabled: null },
  });
  const [lensSides, setLensSides] = createSignal(emptyLensSides());
  const lensProfile = (): string | null => lensSides()[editBase()].profile;
  const lensEnabled = (): boolean | null => lensSides()[editBase()].enabled;
  /** 只写值不抬 rev：预设的应用路径自己统一抬一次（审计 2026-10-04，rev 只许 +1）。 */
  const applyLensSide = (change: Partial<LensSide>): void => {
    const base = editBase();
    setLensSides((current) => ({
      ...current,
      [base]: { ...current[base], ...change },
    }));
  };
  const updateLensSide = (change: Partial<LensSide>): void => {
    applyLensSide(change);
    bumpDevelop();
  };
  const [colorState, setColorState] = createSignal<import("../../api/color.ts").PhotoColorState | null>(null);
  const [proofEnabled,setProofEnabled] = createSignal(false);
  const [advancedTab,setAdvancedTab] = createSignal<"curve"|"swatch"|"color">("curve");
  const [proofTarget,setProofTarget] = createSignal<import("../../lib/color-model.ts").OutputColor>({kind:"srgb"});
  const [proofWarning,setProofWarning] = createSignal(false);
  const [baseCurveProfile, setBaseCurveProfile] = createSignal<string | null>(null);
  const [baseCurvePoints, setBaseCurvePoints] = createSignal<CurvePoint[] | null>(null);
  const [lutChain, setLutChainSignal] = createSignal<LutLayer[]>([]);
  const [lutEnabled, setLutEnabled] = createSignal(false);
  const [lutEnabledExplicit, setLutEnabledExplicit] = createSignal(false);
  const [geometry, setGeometrySignal] = createSignal<EditGeometry | null>(null);
  const [nrMethod, setNrMethodSignal] = createSignal<DevelopNrMethod | null>(null);
  const [autoAdjustBaseline, setAutoAdjustBaseline] = createSignal<AutoAdjustBaseline | null>(null);
  const [autoAdjusting, setAutoAdjusting] = createSignal(false);
  const [renderState, setRenderState] = createSignal<EditorRenderState | null>(null);
  const [holeActive, setHoleActive] = createSignal(false);

  /** 参数动了一下：抬一次 rev（工作区据此发 IPC，DB 层据此判断要不要落库）。 */
  const bumpDevelop = (): void => {
    setDevelopRev((current) => current + 1);
  };

  /**
   * **链的唯一写入口**：截断到上限、整条替换；空链关总开关，
   * 只有新建链或显式换底层才自动开，其余编辑保留用户的总开关选择。
   *
   * 所有会改链的动作都走这里 —— 撤销、预设、面板三条路不会各写一套状态机。
   */
  const commitChain = (next: readonly LutLayer[], activate = false): void => {
    // 只截断、**不逐层克隆**：没改动的层保持对象身份（调用方只替换改了的那一条）。
    // 全量 `{...layer}` 会让「改一层强度」在列表看来等于「整条链换了新对象」——
    // 2026-10-31 的「强度拉杆拖一下就断」就是这么来的（<For> 按引用比对会重建行 DOM，
    // Ark 滑块的指针捕获跟着被销毁）。身份稳定 + `<Index>` 双保险。
    const chain = next.slice(0, MAX_LUT_LAYERS);
    batch(() => {
      const wasEmpty = lutChain().length === 0;
      setLutChainSignal(chain);
      // 只有「空 → 有层」才自动打开总开关；已有链时改强度/排序/加减层
      // 都必须尊重用户刚才关掉的那个总开关（关掉 ≠ 下次编辑又打开）。
      setLutEnabled(chain.length === 0 ? false : activate || wasEmpty || lutEnabled());
      setLutEnabledExplicit(chain.length > 0);
      bumpDevelop();
    });
  };

  /**
   * 改一层的强度（**拖动中与松手后走同一条**）。
   *
   * 不变式（崔总 2026-11-01）：**不存在「开关开着但强度 0%」的层**。
   * 强度拖到 0 的**当下**就关掉该层；从 0 往上拖的**当下**就打开它。
   *
   * ❗为什么不能只靠「松手那一下」判定：那次判定依赖手势收尾（窗口 pointerup /
   * 键盘 keyup）。只要有一次没收到（指针被别的东西捕获、值从程序侧改、组件重挂），
   * 就会留下「开着但 0%」这个自相矛盾的状态 —— 而且之后的错误会一直留在栈里。
   * 把规则放在数值写入的同一步，任何入口都不可能违反它。
   *
   * ❗非有限值一律**忽略**，绝不“猜”成 100%：猜成 100% 就是「点一下开关效果
   * 突然拉满」那类怪现象的来源（滑杆回灌 / 重新挂载都可能送进一个 NaN，2026-10-31）。
   *
   * 只动这一层：顶部的总开关是另一回事，关着的时候整条链照旧不参与渲染
   * （`DevelopStack::lut_chain_active`）。
   */
  const applyLutStrength = (index: number, strength: number): void => {
    const chain = [...lutChain()];
    const current = chain[index];
    if (!current || !Number.isFinite(strength)) return;
    const value = Math.min(1, Math.max(0, strength));
    const enabled = value > 0;
    if (current.strength === value && current.enabled === enabled) return;
    chain[index] = { ...current, strength: value, enabled };
    commitChain(chain);
  };

  /* ── 编辑区（area）层：唯一写入口 ───────────────────────── */

  /** 层列表的行序（默认层可以出现在任何位置）。 */
  /**
   * 层列表的行序（默认层可以出现在任何位置）。
   *
   * ❗**行对象按内容复用身份**（2026-11-01）：`<For>` 按引用比对，每次改一层参数
   * 都新建整张行数组的话，**所有行 DOM 会被重建** —— 改名输入框丢焦点、
   * 开关按钮半拍失灵（LUT 链 2026-10-31 踩过同一个坑，那边靠 `<Index>` 解决）。
   * 这里只换「真的变了的那一行」，其余行保持同一对象。
   */
  let rowCache: readonly LayerRow[] | null = null;
  const layerRows = (): readonly LayerRow[] => {
    const rows = layersToRows(areas(), defaultIndex());
    const previous = rowCache;
    rowCache = rows.map((row, index) => {
      const old = previous?.[index];
      if (old === undefined || old.kind !== row.kind) return row;
      if (row.kind === "area" && old.kind === "area") {
        return old.area === row.area ? old : row;
      }
      return old; // 两行都是默认层：同一个对象（它没有可变内容）
    });
    return rowCache;
  };

  const activeArea = (): Area | null => {
    const id = activeAreaId();
    if (id === null) return null;
    return areas().find((area) => area.id === id) ?? null;
  };

  /** 发号：单调递增、**永不复用**（删了也往后走）。 */
  const nextAreaId = (): number => {
    const id = areaSeq();
    setAreaSeq(id + 1);
    return id;
  };

  const adoptIdFloor = (list: readonly Area[]): void => {
    for (const area of list) setAreaSeq((current) => Math.max(current, area.id + 1));
  };

  /**
   * 层的唯一写入口：截上限、整表替换；返回 `false` = 超上限没写。
   *
   * 所有会改层的动作都走这里（面板 / 画布 / 撤销重读 / 预设），
   * 于是「32 层上限」「默认层位置合法」只在两处判：这里与 Rust 的 `validate`。
   */
  const commitAreas = (
    next: readonly Area[],
    nextDefault: number = defaultIndex(),
  ): boolean => {
    if (next.length > MAX_AREAS) return false;
    const clampDefault = Math.min(Math.max(nextDefault, 0), next.length);
    batch(() => {
      setAreas([...next]);
      setDefaultIndexSignal(clampDefault);
      // 默认层位置只在有额外层时才需要写（退化形状不落 `defaultIndex`）
      bumpDevelop();
    });
    return true;
  };

  const replaceArea = (id: number, change: (area: Area) => Area): void => {
    commitAreas(areas().map((area) => (area.id === id ? change(area) : area)));
  };

  /**
   * 点「新建全图 / 新建控制点」之后，**当前层**切成新层、并且可以马上调参数。
   *
   * 层名留空（面板回落到类型默认名）—— 用户想改名双击那一行。
   */
  const createArea = (): number => {
    const id = nextAreaId();
    const area: Area = {
      id,
      type: "full",
      name: "",
      enabled: true,
      opacity: AREA_OPACITY.default,
      color: 0,
      params: {},
      curves: {},
      seedVersion: SEED_VERSION,
    };
    if (!commitAreas([...areas(), area])) return 0;
    setActiveAreaId(id);
    return id;
  };

  /**
   * 下一个控制点用哪个颜色槽：**取已用得最少的，并列取下标最小**。
   *
   * 与数据侧 `DevelopStack::next_point_color` 同一条规则（那边由单测钉着）。
   * 这里必须再算一次：层是**前端先建、再整体提交**的，落库前就要定下颜色；
   * 换个说法，这不是两份实现，是同一份 8 槽规则在两处按各自需要读数据。
   */
  const nextPointColor = (list: readonly Area[]): number => {
    const used = Array.from({ length: POINT_COLORS }, () => 0);
    for (const area of list) {
      if (area.type === "point") used[area.color % POINT_COLORS] += 1;
    }
    let best = 0;
    for (let slot = 1; slot < POINT_COLORS; slot += 1) {
      if (used[slot] < used[best]) best = slot;
    }
    return best;
  };

  /**
   * **武装**「新建控制点」：只把按钮点成主色、在 flowbar 提一句，**不建层**。
   *
   * 再点一次 = 取消（按钮本身就是开关）；上限 / 已有照片不可编辑时不武装。
   */
  const armPointArea = (): void => {
    // SOOC 没有可用来取样的 RAW 线性源；命令面板也走这里，不能只靠按钮禁用
    if (editBase() !== "raw") return;
    if (areas().length >= MAX_AREAS) return;
    setPointArmed((armed) => !armed);
  };

  const disarmPointArea = (): void => {
    setPointArmed(false);
  };

  /**
   * **落点**：到这一刻才真正创建那一层（追加到末尾、成为当前层）。
   *
   * `x` / `y` 是图像归一化坐标，`seed` 是 Rust 按「该层输入」取好的特征向量。
   */
  const placePointArea = (point: {
    x: number;
    y: number;
    seed: [number, number, number];
  }): number => {
    setPointArmed(false);
    if (editBase() !== "raw") return 0;
    if (areas().length >= MAX_AREAS) return 0;
    const id = nextAreaId();
    const area: Area = {
      id,
      type: "point",
      x: clamp01(point.x),
      y: clamp01(point.y),
      seed: [point.seed[0], point.seed[1], point.seed[2]],
      tolerance: POINT_TOLERANCE.default,
      name: "",
      enabled: true,
      opacity: AREA_OPACITY.default,
      color: nextPointColor(areas()),
      params: {},
      curves: {},
      seedVersion: SEED_VERSION,
    };
    if (!commitAreas([...areas(), area])) return 0;
    setActiveAreaId(id);
    return id;
  };

  /**
   * 这一项的基线。
   *
   * 色温是**随照片**的（`baseline: "as-shot"`）：元数据读得到就用它，
   * 读不到退回表里的静态默认值（6250）。其余参数就是静态默认值。
   */
  const paramBaseline = (id: string): number => {
    const spec = paramSpec(id);
    if (spec === undefined) return 0;
    if (spec.baseline !== "as-shot") return PARAM_DEFAULTS[id] ?? spec.min;
    const kelvin = asShot();
    if (kelvin === null) return PARAM_DEFAULTS[id] ?? spec.min;
    return Math.min(Math.max(kelvin, spec.min), spec.max);
  };

  /**
   * 发出去的载荷：**只装与基线不同的项**（与 DB 同一口径）。
   *
   * 色温的基线来自这张照片的 as-shot —— 所以「载入后标尺就在照片自己的色温上」
   * 这件事不是界面上的特例，而是数据层就长这样。
   */
  const developPayload = (): DevelopParamsPayload => {
    const values: Record<string, number> = {};
    for (const [id, value] of Object.entries(params())) {
      if (value !== paramBaseline(id)) values[id] = value;
    }
    const dirtyCurves: Record<string, [number, number][]> = {};
    for (const channel of CURVE_CHANNELS) {
      const points = curves()[channel];
      if (!isIdentityCurve(points)) {
        dirtyCurves[channel] = points.map(([x, y]) => [x, y]);
      }
    }
    return {
      values,
      color: colorState(),
      asShotTemperature: asShot(),
      curves: dirtyCurves,
      /*
       * 层：**默认层之外的**层（空 = 只有默认层 = Rust 走快路径）。
       * `defaultIndex` 恒为**当前**层列表的位置（Rust 只在有额外层时才存它）。
       *
       * **SOOC / 位图基准下控制点不参与计算**（崔总 2026-11-01）：取样与掩膜都以
       * 「解码后的线性像素」为前提，位图（相机直出 JPG）这条路没有那套输入，
       * 硬算出来的是另一种东西。这里**只改载荷**（发 `enabled: false` = 权重 0），
       * 层本身的状态 / 参数一个字都不动 —— 切回 RAW 立刻恢复。
       */
      areas: areas().map((area) => (
        editBase() === "sooc" && area.type === "point"
          ? { ...area, enabled: false }
          : { ...area }
      )),
      defaultIndex: defaultIndex(),
      // 开发期：容差映射的临时覆盖（生产构建里这个模块不会被引用，恒为 null）
      maskDebug: maskWeightPayload(),
      // 拖动中：Rust 侧只做限流合并（不换图、不换分辨率 —— 没有档位）
      interactive: paramDragging(),
      // 镜头 / 降噪方式：它们是编辑栈的一级（会改变像素），跟着载荷一起发
      lensProfile: lensProfile(),
      baseCurvePoints: editBase() === "raw" ? baseCurvePoints()?.map(([x,y]) => [x,y]) ?? null : null,
      lutChain: lutChain().map((layer) => ({ ...layer })),
      lutEnabled: lutEnabled(),
      lensEnabled: lensEnabled(),
      nrMethod: nrMethod(),
      geometry: geometry(),
    };
  };

  const automaticParams = (): Record<string, number> => ({
    ...defaultParams(), temperature: paramBaseline("temperature"),
    ...autoAdjustBaseline()?.values,
  });
  const automaticLens = (base: DevelopEditBase): LensSide => base === "raw"
    ? { profile: autoAdjustBaseline()?.lensProfile ?? null, enabled: autoAdjustBaseline()?.lensEnabled ?? null }
    : { profile: null, enabled: null };
  const hasGeometryEdits = (): boolean => {
    const value = geometry();
    return value !== null && (value.rotation !== 0 || value.crop !== null);
  };
  const resetStage = (): EditorResetStage => {
    const target = automaticParams();
    const lens = automaticLens(editBase());
    const automaticNr = autoAdjustBaseline()?.nrMethod ?? null;
    const changed = Object.entries(params()).some(([id,value]) => value !== target[id])
      || CURVE_CHANNELS.some((channel) => !isIdentityCurve(curves()[channel]))
      || lensProfile() !== lens.profile || (lensEnabled() ?? true) !== (lens.enabled ?? true)
      || (nrMethod() === "high") !== (automaticNr === "high")
      || lutChain().length > 0 || hasGeometryEdits()
      // 建过层也算「动过」：重置要连层一起清掉（`specs/editor-area.md` §2）
      || areas().length > 0;
    if (changed) return "edits";
    const automatic = autoAdjustBaseline();
    const automaticHasEdits = automatic !== null && (
      Object.entries(automatic.values).some(([id,value]) => value !== paramBaseline(id))
      || automatic.lensProfile !== null || automatic.lensEnabled === false || automatic.nrMethod === "high"
    );
    return baseCurveProfile() !== null || baseCurvePoints() !== null || automaticHasEdits ? "automatic" : "none";
  };
  const restoreResetState = (keepAutomatic: boolean): void => {
    batch(() => {
      setTemperatureExplicit(keepAutomatic && Object.prototype.hasOwnProperty.call(autoAdjustBaseline()?.values ?? {}, "temperature"));
      setParams(keepAutomatic ? automaticParams() : { ...defaultParams(), temperature: paramBaseline("temperature") });
      setCurves(identityCurves());
      setLensSides(keepAutomatic
        ? { raw: automaticLens("raw"), sooc: automaticLens("sooc") } : emptyLensSides());
      if (!keepAutomatic) {
        setBaseCurveProfile(null); setBaseCurvePoints(null); setAutoAdjustBaseline(null);
      }
      setLutChainSignal([]); setLutEnabled(false); setLutEnabledExplicit(false);
      setNrMethodSignal(keepAutomatic ? autoAdjustBaseline()?.nrMethod ?? null : null);
      setGeometrySignal(null); restoreCropRatio(null);
      // 层与层的选中态一起清（重置后只剩默认层）
      setAreas([]); setDefaultIndexSignal(0); setActiveAreaId(null); setPointArmed(false);
      bumpDevelop();
    });
  };

  /** 面板状态一变就落盘（它只有开关两态，不需要防抖）。 */
  const persist = (next: EditorChromeState, nextCategories = categories()): void => {
    try {
      writePrefs({
        leftPanel: next.leftPanel,
        lutCategories: [...nextCategories],
        showPoints: showPoints(),
      });
    } catch {
      // 写不进去不影响这次会话（偏好是锦上添花）
    }
  };

  const loadDevelopState = (values: Record<string, number>, loadedCurves: Partial<Record<CurveChannel, readonly CurvePoint[]>>, settings?: DevelopSettings): void => {
      batch(() => {
      // 换照片时把「拖动中」清掉：上一次拖到一半就换了图的话，
      // 这个标志会一直挂在 true 上 —— 那样后面的渲染全被压成预览档（画面永远偏软）。
      setParamDragging(false);
      if (settings?.asShotK !== undefined) setAsShot(settings.asShotK);
      const next = defaultParams();
      setTemperatureExplicit(Object.prototype.hasOwnProperty.call(values, "temperature"));
      next.temperature = paramBaseline("temperature");
      for (const [id, value] of Object.entries(values)) {
        if (typeof value === "number" && Number.isFinite(value)) next[id] = value;
      }
      setParams(next);
      const merged = identityCurves();
      for (const channel of CURVE_CHANNELS) {
        const points = loadedCurves[channel];
        if (points !== undefined && points.length >= 2) {
          merged[channel] = points.map(([x, y]) => [x, y] as CurvePoint);
        }
      }
      setCurves(merged);
      // 镜头 / 降噪方式：库里没有就回到默认（`null` = 未选择 / 默认开 / 快速档）
      const base = settings?.sourceBase ?? "raw";
      const sides = emptyLensSides();
      sides[base] = {
        profile: settings?.lensProfile ?? null,
        enabled: settings?.lensEnabled ?? null,
      };
      setLensSides(sides);
      setColorState(settings?.color == null ? null : structuredClone(settings.color));
      const automatic = settings?.autoAdjust;
      setAutoAdjustBaseline(automatic == null ? null : { ...automatic, values: { ...automatic.values } });
      setBaseCurveProfile(settings?.baseCurveProfile ?? null);
      setBaseCurvePoints(settings?.baseCurvePoints ?? null);
      setLutChainSignal((settings?.lutChain ?? []).map((layer) => ({ ...layer })));
      setLutEnabled(settings?.lutEnabled === true);
      setLutEnabledExplicit(settings?.lutEnabled !== null && settings?.lutEnabled !== undefined);
      setEditBase(base);
      setNrMethodSignal(settings?.nrMethod ?? null);
      setGeometrySignal(settings?.geometry ?? null);
      restoreCropRatio(settings?.geometry ?? null);
      /*
       * 编辑区层：**换照片 / 撤销重读都回到默认层**（`specs/editor-area.md` §5 的验收）。
       * `areas` / `defaultIndex` 是数据，`activeAreaId` 是界面焦点 —— 后者永远从默认层重来。
       *
       * ❗**`params` / `curves` 要按可选读**（2026-11-01 真机 bug）：旧 Rust 空层
       * 可能不带这两个字段，直接 `Object.entries(undefined)` 会让整次载入作废。
       *
       * 旧容差是 `0..1`：latest 在 Rust `load` 已折算，**定稿 / 侧车**还要在这里
       * 再兜一次，否则旧点拖 handle 时会从 0.x 起步、线长也卡在最小值。
       */
      const layers = (settings?.areas ?? []).map((area) => {
        const normalized: Area = {
          ...area,
          params: { ...(area.params ?? {}) },
          curves: Object.fromEntries(
            Object.entries(area.curves ?? {}).map(([channel, points]) => [
              channel,
              (points as [number, number][]).map(([x, y]) => [x, y] as [number, number]),
            ]),
          ),
        };
        if (
          normalized.type === "point"
          && Number.isFinite(normalized.tolerance)
          && normalized.tolerance <= 1
        ) {
          normalized.tolerance = POINT_TOLERANCE.min
            + Math.max(0, normalized.tolerance) * (POINT_TOLERANCE.max - POINT_TOLERANCE.min);
        }
        return normalized;
      });
      setAreas(layers);
      adoptIdFloor(layers);
      setDefaultIndexSignal(Math.min(Math.max(settings?.defaultIndex ?? 0, 0), layers.length));
      setActiveAreaId(null);
      setPointArmed(false);
      // 从库里读回来的就是「已落库」的状态
      const nextRev = developRev() + 1;
      setDevelopRev(nextRev);
      setCommittedRev(nextRev);
      });
  };

  /**
   * 写面板状态（唯一入口）。
   *
   * ❗**必须用 setter 的函数式更新，不能 `produce(chrome())`**（2026-10-31 真机爆栈）：
   * 那样会**同步读一次 `chrome` 信号**，于是「调用方是 effect」时它把自己订阅到了 chrome 上 ——
   * 写完立刻失效 → 再跑 → 再写 …… 生产构建没有循环保护，直接同步递归到
   * `RangeError: Maximum call stack size exceeded`。
   *
   * 真机现场（`left-panel.tsx` 的 `issueFocusTick` effect → `setLeftPanel("issues")`）：
   * 爆栈发生在 `finalize()` 的 `setIssueFocusTick()` 那一行，**后面的 createIssue 一行没跑**
   * —— 表现为「点定稿确认什么都不生成、按钮永久变灰（busy 没复位）、连 LUT 开关都失灵」。
   * 回归：`store.test.ts` 的「在 effect 里切左列面板不会自激」。
   */
  const updateChrome = (produce: (state: EditorChromeState) => EditorChromeState): void => {
    let next: EditorChromeState | undefined;
    setChrome((state) => (next = produce(state)));
    if (next !== undefined) persist(next);
  };

  return {
    chromeStep: () => chrome().step,
    chromeName: () => editorChromeName(chrome()),
    leftVisible: () => editorLeftPanelVisible(chrome()),
    leftPanel: () => chrome().leftPanel,
    lutVisible: () => editorLutVisible(chrome()),
    issuesVisible: () => editorIssuesVisible(chrome()),
    presetVisible: () => editorPresetVisible(chrome()),
    showsRight: () => editorShowsRight(chrome()),
    showsFilm: () => editorShowsFilm(chrome()),
    cycleTab: () => {
      updateChrome(stepEditorTab);
    },
    toggleLeft: (panel) => {
      updateChrome((state) => toggleEditorLeftPanel(state, panel));
    },
    setLeftPanel: (panel) => {
      updateChrome((state) => setEditorLeftPanel(state, panel));
    },
    resetChrome: () => {
      // 同上：走 setter 的函数式更新，不读信号（读就会把调用方订阅成依赖）
      let next: EditorChromeState | undefined;
      setChrome((state) => (next = resetEditorChrome(state)));
      if (next !== undefined) persist(next);
    },

    tool,
    toggleTool: (next) => {
      // 每次重进裁切都从已确认记录恢复；取消的草稿不能污染下一次。
      batch(() => {
        if (next === "crop" && tool() !== "crop") restoreCropRatio(geometry());
        setTool((current) => (current === next ? null : next));
      });
    },
    closeTool: () => setTool(null),

    cropRatioId,
    cropRatio: () =>
      CROP_RATIOS.find((item) => item.id === cropRatioId()) ?? {
        id: "custom",
        labelKey: "editor.crop.custom",
        ratio: cropSize().width / Math.max(1e-6, cropSize().height),
      },
    setCropRatioId: (id) => setCropRatioId(id),
    flipCropRatio: () => {
      const current = CROP_RATIOS.find((item) => item.id === cropRatioId());
      if (current === undefined) {
        // 自定义比例：反转就是把横纵对调
        const { width, height } = cropSize();
        setCropSizeSignal({ width: height, height: width });
        return;
      }
      const flipped = invertRatio(current);
      setCropRatioId(flipped.id);
      if (flipped.ratio !== null) {
        // 反转之后横纵输入也跟着换边（`4:3` → `3:4`）
        setCropSizeSignal({ width: flipped.ratio, height: 1 });
      }
    },
    unlinkCropRatio: () => setCropRatioId("free"),
    cropWidth: () => cropSize().width,
    cropHeight: () => cropSize().height,
    setCropSize: (width, height) => {
      // 输入立即进入自定义；非法/暂未完成的数字保留最近有效的宽高。
      setCropRatioId("custom");
      if (![width, height].every((value) => Number.isFinite(value) && value > 0)) return;
      if (width / height < 0.01 || width / height > 100) return;
      setCropSizeSignal({ width, height });
    },

    angle,
    setAngle: (degrees) => setAngleSignal(degrees),
    resetAngle: () => setAngleSignal(0),

    paramValue: (id) => {
      const area = activeArea();
      // 非默认层：层里没有这一项就是基线（色温同样按照片的 as-shot 解释）
      if (area !== null) return area.params?.[id] ?? paramBaseline(id);
      return id === "temperature" && !temperatureExplicit()
        ? paramBaseline(id)
        : params()[id] ?? PARAM_DEFAULTS[id] ?? 0;
    },
    paramBaseline,
    setParam: (id, value) => {
      const area = activeArea();
      if (area !== null) {
        // 全局阶段参数只属于默认层（界面已经灰掉，这里是硬约束）
        const target = paramSpec(id)?.target ?? "layered";
        if (target !== "layered") return;
        replaceArea(area.id, (current) => ({
          ...current,
          params: { ...(current.params ?? {}), [id]: value },
        }));
        return;
      }
      if (id === "temperature") setTemperatureExplicit(true);
      setParams((current) => ({ ...current, [id]: value }));
      bumpDevelop();
    },
    resetParam: (id) => {
      const area = activeArea();
      if (area !== null) {
        replaceArea(area.id, (current) => {
          const next = { ...(current.params ?? {}) };
          delete next[id];
          return { ...current, params: next };
        });
        return;
      }
      if (id === "temperature") setTemperatureExplicit(false);
      setParams((current) => ({ ...current, [id]: paramBaseline(id) }));
      bumpDevelop();
    },
    resetParams: () => restoreResetState(false),
    autoAdjustBaseline,
    applyAutoAdjust: (baseline) => {
      batch(() => {
        const values: Record<string, number> = {};
        for (const [id,value] of Object.entries(baseline.values)) {
          const spec = paramSpec(id);
          if (spec === undefined || !Number.isFinite(value)) continue;
          values[id] = Math.min(spec.max, Math.max(spec.min, value));
        }
        const automatic = { ...baseline, values };
        setAutoAdjustBaseline(automatic);
        setParams((current) => ({ ...current, ...values }));
        if (Object.prototype.hasOwnProperty.call(values, "temperature")) setTemperatureExplicit(true);
        if (automatic.lensProfile !== null || automatic.lensEnabled !== null) {
          setLensSides((current) => ({ ...current, raw: { profile: automatic.lensProfile, enabled: automatic.lensEnabled } }));
        }
        if (automatic.nrMethod !== null) setNrMethodSignal(automatic.nrMethod);
        bumpDevelop();
      });
    },
    resetStage,
    resetDevelop: (stage) => restoreResetState(stage === "edits"),
    asShotTemperature: asShot,
    setAsShotTemperature: (kelvin) => {
      // 只换基线：**不抬 rev**（这不是用户的改动，标 dirty 会让「松手落库」误判）
      batch(() => {
        setAsShot((current) => (current === kelvin ? current : kelvin));
        if (!temperatureExplicit()) {
          // 状态轮询反复读到相同值时不提交显影；真正变化时两路信号合为一次更新。
          setParams((current) => withTemperatureBaseline(current, paramBaseline("temperature")));
        }
      });
    },
    developPayload,
    developRev,
    committedRev,
    markCommitted: (rev) => setCommittedRev(rev),
    developDirty: () => developRev() !== committedRev(),
    confirmTick,
    confirmNote,
    noteConfirm: (note) => {
      setConfirmTick((current) => current + 1);
      setConfirmNote(note);
    },
    paramDragging,
    beginParamDrag: () => setParamDragging(true),
    endParamDrag: () => setParamDragging(false),
    editBase,
    setEditBase: (base) => {
      if (base === editBase()) return;
      batch(() => {
        setEditBase(base);
        bumpDevelop();
      });
    },
    editBaseAvailable,
    setEditBaseAvailable,
    geometry,
    setGeometry: (next) => {
      batch(() => { setGeometrySignal(next); restoreCropRatio(next); bumpDevelop(); });
    },
    loadDevelop: loadDevelopState,
    applyDevelop: (values, curves, settings) => { loadDevelopState(values, curves, settings); bumpDevelop(); },

    curveChannel,
    lensProfile,
    colorState,
    proofEnabled, setProofEnabled, proofTarget, setProofTarget, proofWarning, setProofWarning,
    advancedTab,setAdvancedTab,
    setColorState: (next) => { setColorState(next == null ? null : structuredClone(next)); bumpDevelop(); },
    baseCurveProfile,
    baseCurvePoints,
    lutChain,
    lutEnabled,
    lutEnabledSetting: () => lutEnabledExplicit() ? lutEnabled() : null,
    selectLutAsBase: (id) => {
      const chain = [...lutChain()];
      const index = chain.findIndex((layer) => layer.id === id);
      if (index > 0) {
        const [layer] = chain.splice(index, 1);
        chain.unshift(layer);
      } else if (index < 0) {
        // 换底层：新的这一层用默认（开 / 100%），不继承旧底层的强度。
        const fresh = { id, enabled: true, strength: 1 };
        if (chain.length === 0) chain.push(fresh);
        else chain[0] = fresh;
      }
      commitChain(chain, true);
    },
    toggleLutInChain: (id) => {
      const chain = [...lutChain()];
      const index = chain.findIndex((layer) => layer.id === id);
      if (index >= 0) chain.splice(index, 1);
      else if (chain.length < MAX_LUT_LAYERS) chain.push({ id, enabled: true, strength: 1 });
      commitChain(chain);
    },
    setLutLayerEnabled: (index, enabled) => {
      const chain = [...lutChain()];
      const current = chain[index];
      if (!current) return;
      /*
       * 设计口径（崔总 2026-11-01）：**不允许存在「开关开着但强度 0%」的层**。
       *
       * 强度拖到 0% 会自动关层；反过来把 0% 的那层点开，强度直接给回 100% ——
       * 否则用户拿到的是一个「开着却什么都没发生」的层，开关与画面互相矛盾。
       * 与「强度 ≥ 0 自动关层 / >0 自动开层」是同一条规则的两个方向。
       */
      const strength = enabled && current.strength <= 0 ? 1 : current.strength;
      if (current.enabled === enabled && current.strength === strength) return;
      chain[index] = { ...current, enabled, strength };
      commitChain(chain);
    },
    setLutLayerStrength: (index, strength) => applyLutStrength(index, strength),
    commitLutLayerStrength: (index, strength) => applyLutStrength(index, strength),
    moveLutLayer: (from, to) => {
      const chain = [...lutChain()];
      if (from === to || !chain[from] || to < 0 || to >= chain.length) return;
      const [layer] = chain.splice(from, 1);
      chain.splice(to, 0, layer);
      commitChain(chain);
    },
    removeLutLayer: (index) => {
      const chain = [...lutChain()];
      if (!chain[index]) return;
      chain.splice(index, 1);
      commitChain(chain);
    },
    setLut: (id, enabled) => {
      if (id === null || !enabled) {
        commitChain([]);
        return;
      }
      const chain = [...lutChain()];
      const index = chain.findIndex((layer) => layer.id === id);
      if (index > 0) {
        const [layer] = chain.splice(index, 1);
        chain.unshift(layer);
      } else if (index < 0) {
        chain[0] = { id, enabled: true, strength: 1 };
      }
      commitChain(chain, true);
    },
    setLutEnabled: (enabled) => {
      if (enabled === lutEnabled()) return;
      batch(() => {
        setLutEnabled(enabled && lutChain().length > 0);
        setLutEnabledExplicit(lutChain().length > 0);
        bumpDevelop();
      });
    },
    setLutCategories: (next) => { setCategories([...next]); },
    setBaseCurve: (id, points) => {
      batch(() => {
        setBaseCurveProfile(id);
        setBaseCurvePoints(points === null ? null : points.map(([x,y]) => [x,y] as CurvePoint));
        bumpDevelop();
      });
    },
    setLensProfile: (key) => updateLensSide({ profile: key }),
    lensEnabled,
    setLensEnabled: (enabled) => updateLensSide({ enabled }),
    nrMethod,
    setNrMethod: (method) => {
      setNrMethodSignal(method);
      bumpDevelop();
    },
    autoAdjusting,
    setAutoAdjusting,

    setCurveChannel,
    curvePoints: (channel) => {
      const area = activeArea();
      if (area !== null) {
        const points = area.curves?.[channel];
        return points !== undefined && points.length >= 2
          ? points.map(([x, y]) => [x, y] as CurvePoint)
          : [[0, 0], [1, 1]];
      }
      return curves()[channel];
    },
    setCurvePoints: (channel, points) => {
      const area = activeArea();
      if (area !== null) {
        const mapped = points.map(([x, y]) => [x, y] as [number, number]);
        replaceArea(area.id, (current) => ({
          ...current,
          curves: { ...(current.curves ?? {}), [channel]: mapped },
        }));
        return;
      }
      setCurves((current) => ({
        ...current,
        [channel]: points.map(([x, y]) => [x, y] as CurvePoint),
      }));
      bumpDevelop();
    },
    resetCurve: (channel) => {
      const area = activeArea();
      if (area !== null) {
        replaceArea(area.id, (current) => {
          const next = { ...(current.curves ?? {}) };
          delete next[channel];
          return { ...current, curves: next };
        });
        return;
      }
      setCurves((current) => ({
        ...current,
        [channel]: [
          [0, 0],
          [1, 1],
        ],
      }));
      bumpDevelop();
    },

    lutCategories: () => categories(),
    addCategory: (name) => {
      const result = addLutCategory(categories(), name);
      if (result.createdId === null) return false;
      setCategories(result.categories);
      setExpanded(result.createdId);
      persist(chrome(), result.categories);
      return true;
    },
    expandedCategory: expanded,
    toggleCategory: (id) => setExpanded((current) => toggleExpandedCategory(current, id)),

    presetDirectories: () => presetDirectories(),
    presets: () => presetList(),
    setPresetLibrary: (library) => {
      const directories = [...library.directories];
      const presets = [...library.presets];
      const directoryIds = new Set(directories.map((directory) => directory.id));
      const presetIds = new Set(presets.map((preset) => preset.id));
      setPresetDirectories(directories);
      setPresetList(presets);
      // 整库回写后**收敛选中与折叠记录**（审计 2026-10-04）：
      // 删掉的条目不能留在选中态里（拖拽 / 应用按钮会按幽灵 id 行动）。
      setPresetSelectionState((current) =>
        prunePresetSelection(current, directoryIds, presetIds));
      setPresetCollapsed((current) => prunePresetCollapsed(current, directoryIds));
    },
    presetSelection: () => presetSelectionState(),
    selectPresetDirectory: (id) => setPresetSelectionState({ kind: "directory", id }),
    selectPreset: (id, event) => {
      if (!event.shiftKey) {
        setPresetSelectionState({ kind: "presets", ids: [id] });
        return;
      }
      setPresetSelectionState((current) => {
        if (current?.kind !== "presets") return { kind: "presets", ids: [id] };
        const ids = current.ids.includes(id)
          ? current.ids.filter((entry) => entry !== id)
          : [...current.ids, id];
        return { kind: "presets", ids };
      });
    },
    presetExpanded: (id) => presetCollapsed()[id] !== true,
    togglePresetDirectory: (id) =>
      setPresetCollapsed((current) => ({ ...current, [id]: current[id] !== true })),
    presetSnapshot: (groups) => buildPresetSnapshot(groups, {
      photoColor: colorState(),
      // 镜像返回对象里 `paramValue` 的取值口径（色温未动过时用基线）
      values: Object.fromEntries(PARAM_IDS.map((id) => [id,
        id === "temperature" && !temperatureExplicit()
          ? paramBaseline(id)
          : params()[id] ?? PARAM_DEFAULTS[id] ?? 0])),
      curves: curves(),
      nrMethod: nrMethod(),
      lensProfile: lensProfile(),
      lensEnabled: lensEnabled(),
      lutChain: lutChain().map((layer) => ({ ...layer })),
      lutEnabled: lutEnabled(),
      asShotK: asShot(),
    }, PRESET_GROUP_PARAMS),
    applyPresetSnapshot: (snapshot) => {
      const plan = planPresetApply(snapshot, PRESET_GROUP_PARAMS, PARAM_DEFAULTS, (id) =>
        categories().some((category) =>
          category.entries.some((entry) => entry.id === id && entry.available !== false)), asShot(),
        undefined,
        // 预设的 LUT **追加**到现有链末尾（崔总 2026-10-31），所以要交链的现状进去。
        { currentLutChain: lutChain(), currentLutEnabled: lutEnabled() });
      if (plan.appliedGroups.length === 0) return { wbSkipped: false, lutDropped: 0 };
      let lutDropped = 0;
      batch(() => {
        setParams((current) => ({ ...current, ...plan.values }));
        // v3：白平衡是**偏移量**——`wbExplicit` 才表示用户看到了一个明确的偏移；
        // Δmired = 0 = 跟随拍摄色温（回到基线，不算显式值）。
        if (Object.prototype.hasOwnProperty.call(plan.values, "temperature")) {
          setTemperatureExplicit(plan.wbExplicit);
        }
        setCurves((current) => ({ ...current, ...plan.curves }));
        if (plan.hasDetail) setNrMethodSignal(plan.nrMethod === "high" ? "high" : null);
        if (plan.hasLens) applyLensSide({ profile: plan.lens.profile, enabled: plan.lens.enabled });
        if (plan.lut !== null && plan.lut !== "missing") {
          // 走链的唯一写入口（截到 8 层 + bumpDevelop）；总开关随后按**预设口径**显式落定 ——
          // `commitChain` 的「空 → 有层就自动开」是给面板用的，不能覆盖预设里明确存的"关"。
          lutDropped = Math.max(0, plan.lut.chain.length - MAX_LUT_LAYERS);
          commitChain(plan.lut.chain, false);
          setLutEnabled(plan.lut.chain.length > 0 && plan.lut.enabled);
          setLutEnabledExplicit(plan.lut.chain.length > 0);
        }
        bumpDevelop();
      });
      return { wbSkipped: plan.wbSkipped, lutDropped };
    },

    layerRows,
    activeAreaId,
    selectArea: (id) => {
      // 只切焦点：不存在的层（刚被删掉）回到默认层
      if (id !== null && !areas().some((area) => area.id === id)) {
        setActiveAreaId(null);
        return;
      }
      setActiveAreaId(id);
    },
    activeArea,
    activeAreaName: () => {
      const area = activeArea();
      if (area === null) return t("editor.area.default");
      if (area.name !== "") return area.name;
      return t(area.type === "point" ? "editor.area.point" : "editor.area.full");
    },
    onDefaultLayer: () => activeArea() === null,
    createArea,
    placePointArea,
    pointArmed,
    armPointArea,
    disarmPointArea,
    removeArea: (id) => {
      const rows = layerRows();
      const index = rows.findIndex((row) => row.kind === "area" && row.area.id === id);
      if (index < 0) return;
      const next = areas().filter((area) => area.id !== id);
      // 删当前层 ⇒ 选**下一层**（没有下一层就上一层，再没有就默认层）
      if (activeAreaId() === id) {
        const neighbour = rows[index + 1] ?? rows[index - 1];
        setActiveAreaId(neighbour !== undefined && neighbour.kind === "area" ? neighbour.area.id : null);
      }
      // 默认层在它之后 ⇒ 位置往前挪一格
      const currentDefault = defaultIndex();
      const defaultRow = rows.findIndex((row) => row.kind === "default");
      commitAreas(next, index < defaultRow ? Math.max(0, currentDefault - 1) : currentDefault);
    },
    moveAreaRow: (from, to) => {
      const rows = [...layerRows()];
      if (from === to || from < 0 || to < 0 || from >= rows.length || to >= rows.length) return;
      const [row] = rows.splice(from, 1);
      rows.splice(to, 0, row);
      const next = rowsToLayers(rows);
      commitAreas(next.areas, next.defaultIndex);
    },
    renameArea: (id, name) => {
      const trimmed = Array.from(name).slice(0, 30).join("");
      replaceArea(id, (area) => ({ ...area, name: trimmed }));
    },
    setAreaEnabled: (id, enabled) => replaceArea(id, (area) => ({ ...area, enabled })),
    setAreaColor: (id, color) => {
      if (!Number.isInteger(color) || color < 0 || color >= POINT_COLORS) return;
      replaceArea(id, (area) => ({ ...area, color }));
    },
    /** 旧种子重取样后写回（**位置 / 容差 / 参数都不动**，只换 seed 与版本）。 */
    adoptAreaSeed: (id, seed) =>
      replaceArea(id, (area) => {
        if (area.type !== "point") return area;
        return {
          ...area,
          seed: [seed[0], seed[1], seed[2]],
          seedVersion: SEED_VERSION,
        };
      }),
    setAreaPoint: (id, point) =>
      replaceArea(id, (area) => {
        if (area.type !== "point") return area;
        return {
          ...area,
          x: clamp01(point.x ?? area.x),
          y: clamp01(point.y ?? area.y),
          // 容差是 100..1000 的**数值域**（不是 0..1）：夹在契约给的上下限里
          tolerance: clampTolerance(point.tolerance ?? area.tolerance),
        };
      }),
    areasFull: () => areas().length >= MAX_AREAS,
    showPoints,
    setShowPoints: (show) => {
      setShowPointsSignal(show);
      // 纯视图偏好：落盘即可，**不抬 rev**（不进 profile / 哈希 / 撤销）
      persist(chrome(), categories());
    },

    renderState,
    setRenderState,
    holeActive,
    setHoleActive,
  };
}

/** 容差（100..1000）；NaN / 越界一律回到合法值，避免把坏数据送进落库校验。 */
function clampTolerance(value: number): number {
  if (!Number.isFinite(value)) return POINT_TOLERANCE.default;
  return Math.min(POINT_TOLERANCE.max, Math.max(POINT_TOLERANCE.min, value));
}

/** 归一化坐标（0..1）；NaN / 越界一律夹回来 —— 画布点击偶尔会给出洞口的边缘值。 */
function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}
