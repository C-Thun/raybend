/**
 * 编辑工作区的状态（`ARCHITECTURE.md` §3 的状态归属）。
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
  DevelopEditBase,
  DevelopNrMethod,
  DevelopParamsPayload,
  DevelopSettings,
  EditorRenderState,
  EditGeometry,
} from "../../api/types.ts";
import { t } from "../../i18n/index.ts";

import {
  initialEditorChrome,
  editorChromeName,
  editorLutVisible,
  editorShowsFilm,
  editorShowsRight,
  resetEditorChrome,
  setEditorLutOpen,
  stepEditorTab,
  type EditorChromeState,
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
import {
  CROP_RATIOS,
  defaultParams,
  invertRatio,
  paramSpec,
  PARAM_DEFAULTS,
  type CropRatio,
} from "./params.ts";

/** 三个互斥的画布工具（`prompts/editor.pd`：同一时刻只有一个控制块）。 */
export type EditorTool = "crop" | "rotate" | "compare";

export const EDITOR_TOOLS: readonly EditorTool[] = ["crop", "rotate", "compare"];

/** 曲线编辑器的通道（W3 接管线；W1 只是能切）。 */
export type CurveChannel = "rgb" | "r" | "g" | "b";

export const CURVE_CHANNELS: readonly CurveChannel[] = ["rgb", "r", "g", "b"];

/** 轮询得到相同拍摄色温时，保留参数对象身份，避免空闲显影重复提交。 */
export function withTemperatureBaseline(current: Record<string, number>, baseline: number): Record<string, number> {
  return current.temperature === baseline ? current : { ...current, temperature: baseline };
}

export interface EditorStoreDeps {
  /** 偏好读写（默认真 `localStorage`；测试里注入假的） */
  readPrefs?: () => EditorPrefs;
  writePrefs?: (prefs: EditorPrefs) => void;
}

export interface EditorStore {
  /* ── 档位与面板（`lib/editor-chrome.ts`）────────────── */
  chromeStep: () => number;
  /** 档位名（`data-chrome` 用） */
  chromeName: () => string;
  /** LUT 面板这一刻可见吗（= 档位允许左列 **且** 面板开着） */
  lutVisible: () => boolean;
  /** LUT 面板的**用户意图**（持久化的那一个） */
  lutOpen: () => boolean;
  showsRight: () => boolean;
  showsFilm: () => boolean;
  /** `Tab`：三档循环 */
  cycleTab: () => void;
  /** 点 `toolsbar left` 的 LUT 开关（含命令面板那条路） */
  toggleLut: () => void;
  setLutOpen: (open: boolean) => void;
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
   * **手指还按在滑杆 / 曲线上**（人类 2026-09-24）。
   *
   * 它只影响一件事：拖动期间 Rust 侧**只算预览档**，松手那一下才按缩放补全尺寸
   * （`tier_for_params`）—— 「调拉杆时只对展示的像素处理，释放鼠标才对全图做处理」。
   * 它跟着载荷一起发出去（不单独开 IPC）。
   */
  paramDragging: () => boolean;
  /** 拖拽开始（滑杆的 `onValueChangeStart` / 曲线的 `pointerdown`） */
  beginParamDrag: () => void;
  /** 拖拽结束（松手）—— 这一下会让载荷重发，Rust 侧于是补全尺寸 */
  endParamDrag: () => void;

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

  /* ── 镜头 / 降噪方式（M3-W4）──────────────────────── */
  /** 镜头配置文件（`null` = 未选择；`"none"` = 显式关掉；否则是 `maker|model`） */
  lensProfile: () => string | null;
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
      writePrefs({ lutOpen: initial.lutOpen, lutCategories: initialCategories });
    } catch {
      // 写不进去不影响这次会话（偏好是锦上添花）
    }
  }

  const [chrome, setChrome] = createSignal<EditorChromeState>(
    initialEditorChrome(initial.lutOpen),
  );
  const [categories, setCategories] = createSignal<LutCategory[]>([
    ...initialCategories,
  ]);
  const [expanded, setExpanded] = createSignal<string | null>(null);
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
  const updateLensSide = (change: Partial<LensSide>): void => {
    const base = editBase();
    setLensSides((current) => ({
      ...current,
      [base]: { ...current[base], ...change },
    }));
    bumpDevelop();
  };
  const [geometry, setGeometrySignal] = createSignal<EditGeometry | null>(null);
  const [nrMethod, setNrMethodSignal] = createSignal<DevelopNrMethod | null>(null);
  const [autoAdjusting, setAutoAdjusting] = createSignal(false);
  const [renderState, setRenderState] = createSignal<EditorRenderState | null>(null);
  const [holeActive, setHoleActive] = createSignal(false);

  /** 参数动了一下：抬一次 rev（工作区据此发 IPC，DB 层据此判断要不要落库）。 */
  const bumpDevelop = (): void => {
    setDevelopRev((current) => current + 1);
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
      asShotTemperature: asShot(),
      curves: dirtyCurves,
      // 拖动中：Rust 侧只算预览档（`tier_for_params`）
      interactive: paramDragging(),
      // 镜头 / 降噪方式：它们是编辑栈的一级（会改变像素），跟着载荷一起发
      lensProfile: lensProfile(),
      lensEnabled: lensEnabled(),
      nrMethod: nrMethod(),
      geometry: geometry(),
    };
  };

  /** 面板状态一变就落盘（它只有开关两态，不需要防抖）。 */
  const persist = (next: EditorChromeState, nextCategories = categories()): void => {
    try {
      writePrefs({ lutOpen: next.lutOpen, lutCategories: [...nextCategories] });
    } catch {
      // 写不进去不影响这次会话（偏好是锦上添花）
    }
  };

  const updateChrome = (produce: (state: EditorChromeState) => EditorChromeState): void => {
    const next = produce(chrome());
    setChrome(next);
    persist(next);
  };

  return {
    chromeStep: () => chrome().step,
    chromeName: () => editorChromeName(chrome()),
    lutVisible: () => editorLutVisible(chrome()),
    lutOpen: () => chrome().lutOpen,
    showsRight: () => editorShowsRight(chrome()),
    showsFilm: () => editorShowsFilm(chrome()),
    cycleTab: () => {
      updateChrome(stepEditorTab);
    },
    toggleLut: () => {
      updateChrome((state) => setEditorLutOpen(state, !editorLutVisible(state)));
    },
    setLutOpen: (open) => {
      updateChrome((state) => setEditorLutOpen(state, open));
    },
    resetChrome: () => {
      const next = resetEditorChrome(chrome());
      setChrome(next);
      persist(next);
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

    paramValue: (id) => id === "temperature" && !temperatureExplicit()
      ? paramBaseline(id)
      : params()[id] ?? PARAM_DEFAULTS[id] ?? 0,
    paramBaseline,
    setParam: (id, value) => {
      if (id === "temperature") setTemperatureExplicit(true);
      setParams((current) => ({ ...current, [id]: value }));
      bumpDevelop();
    },
    resetParam: (id) => {
      if (id === "temperature") setTemperatureExplicit(false);
      setParams((current) => ({ ...current, [id]: paramBaseline(id) }));
      bumpDevelop();
    },
    resetParams: () => {
      batch(() => {
        setTemperatureExplicit(false);
        setParams({ ...defaultParams(), temperature: paramBaseline("temperature") });
        setCurves(identityCurves());
        setLensSides(emptyLensSides());
        setNrMethodSignal(null);
        setGeometrySignal(null);
        restoreCropRatio(null);
        bumpDevelop();
      });
    },
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
    loadDevelop: (values, loadedCurves, settings) => {
      batch(() => {
      // 换照片时把「拖动中」清掉：上一次拖到一半就换了图的话，
      // 这个标志会一直挂在 true 上 —— 那样后面的渲染全被压成预览档（画面永远偏软）。
      setParamDragging(false);
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
      setEditBase(base);
      setNrMethodSignal(settings?.nrMethod ?? null);
      setGeometrySignal(settings?.geometry ?? null);
      restoreCropRatio(settings?.geometry ?? null);
      // 从库里读回来的就是「已落库」的状态
      const nextRev = developRev() + 1;
      setDevelopRev(nextRev);
      setCommittedRev(nextRev);
      });
    },

    curveChannel,
    lensProfile,
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
    curvePoints: (channel) => curves()[channel],
    setCurvePoints: (channel, points) => {
      setCurves((current) => ({
        ...current,
        [channel]: points.map(([x, y]) => [x, y] as CurvePoint),
      }));
      bumpDevelop();
    },
    resetCurve: (channel) => {
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

    renderState,
    setRenderState,
    holeActive,
    setHoleActive,
  };
}
