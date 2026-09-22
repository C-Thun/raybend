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
 * 裁切 / 旋转 / 对比只切**模式与右栏控制块**，画布上的框线在 W5。
 */

import { createSignal } from "solid-js";

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
  toggleExpandedCategory,
  type LutCategory,
} from "../../lib/lut-library.ts";
import {
  CROP_RATIOS,
  defaultParams,
  invertRatio,
  type CropRatio,
} from "./params.ts";

/** 三个互斥的画布工具（`prompts/editor.pd`：同一时刻只有一个控制块）。 */
export type EditorTool = "crop" | "rotate" | "compare";

export const EDITOR_TOOLS: readonly EditorTool[] = ["crop", "rotate", "compare"];

/** 曲线编辑器的通道（W3 接管线；W1 只是能切）。 */
export type CurveChannel = "rgb" | "r" | "g" | "b";

export const CURVE_CHANNELS: readonly CurveChannel[] = ["rgb", "r", "g", "b"];

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
  /** 比例项（含「自由」「原始比例」与固定比例；自定义由手输产生） */
  cropRatio: () => CropRatio;
  setCropRatioId: (id: string) => void;
  /** 反转比例（4:3 ⇄ 3:4） */
  flipCropRatio: () => void;
  /** 取消比例限制 → 自由 */
  unlinkCropRatio: () => void;
  /** 横 / 纵向比例输入（手输之后比例选项自动变成「自定义」） */
  cropWidth: () => number;
  cropHeight: () => number;
  setCropSize: (width: number, height: number) => void;

  /* ── 旋转控制块 ─────────────────────────────────────── */
  angle: () => number;
  setAngle: (degrees: number) => void;
  resetAngle: () => void;

  /* ── 调整参数（W3 接管线）──────────────────────────── */
  paramValue: (id: string) => number;
  setParam: (id: string, value: number) => void;
  resetParams: () => void;

  /* ── 曲线 ───────────────────────────────────────────── */
  curveChannel: () => CurveChannel;
  setCurveChannel: (channel: CurveChannel) => void;

  /* ── LUT 分类（设备级偏好）─────────────────────────── */
  lutCategories: () => readonly LutCategory[];
  /** 新建分类；重名返回 `false`（界面据此提示，不静默吞掉） */
  addCategory: (name: string) => boolean;
  expandedCategory: () => string | null;
  toggleCategory: (id: string) => void;
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

  const [chrome, setChrome] = createSignal<EditorChromeState>(
    initialEditorChrome(initial.lutOpen),
  );
  const [categories, setCategories] = createSignal<LutCategory[]>([
    ...initial.lutCategories,
  ]);
  const [expanded, setExpanded] = createSignal<string | null>(null);
  const [tool, setTool] = createSignal<EditorTool | null>(null);
  const [cropRatioId, setCropRatioId] = createSignal<string>("free");
  const [cropSize, setCropSizeSignal] = createSignal({ width: 3, height: 2 });
  const [angle, setAngleSignal] = createSignal(0);
  const [params, setParams] = createSignal<Record<string, number>>(defaultParams());
  const [curveChannel, setCurveChannel] = createSignal<CurveChannel>("rgb");

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
      // 三工具互斥：按下的那个亮、其余灭；再按同一个 = 退出（`.pd` 明确）
      setTool((current) => (current === next ? null : next));
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
      // 手输即「自定义」（`.pd`：用户手动输入后上面的比例选项自动变成自定义）
      setCropSizeSignal({ width, height });
      setCropRatioId("custom");
    },

    angle,
    setAngle: (degrees) => setAngleSignal(degrees),
    resetAngle: () => setAngleSignal(0),

    paramValue: (id) => params()[id] ?? 0,
    setParam: (id, value) => setParams((current) => ({ ...current, [id]: value })),
    resetParams: () => setParams(defaultParams()),

    curveChannel,
    setCurveChannel,

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
  };
}
