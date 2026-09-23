/**
 * 编辑参数表（`design/editor.md` §3.7 + `DESIGN.md` §14.10）。
 *
 * # 数字的真相在 `src/api/develop-params.json`
 *
 * 范围 / 步长 / 默认值 / 填充原点 / 本波接没接管线 —— 这些**不是**写在这里的，
 * 而是从 `src/api/develop-params.json` 读进来；Rust 侧（`crates/raybend/src/develop/params.rs`）
 * 对着同一份文件逐条断言。改数改那一处，两侧的测试会盯着。
 *
 * 本文件只补**界面装饰**：语言包 key、分组、小数位、单位、极值文案、要不要显示极值。
 *
 * # 为什么装饰与数字要分开
 *
 * 数字是**管线与界面共同依赖的口径**（改错一处就是「拉杆能拖、画面不对」）；
 * 装饰（文案、小数位）只有界面关心。混在一起写，就会有人为了让标签好看去动范围。
 */

import type { MessageKey } from "../../i18n/index.ts";
// `with { type: "json" }` 不是多余的：`node --test` 跑单测时（`pnpm test`）
// 没有它就报 ERR_IMPORT_ATTRIBUTE_MISSING —— Vite 那边两种写法都认。
import contract from "../../api/develop-params.json" with { type: "json" };

/** 右栏第 2 组的四个页签（也是参数的分组名）。 */
export type ParamGroup = "tone" | "color" | "detail" | "lens";

export const PARAM_GROUPS: readonly ParamGroup[] = ["tone", "color", "detail", "lens"];

/** 分组 → 页签文案 key。 */
export const GROUP_LABEL_KEY: Record<ParamGroup, MessageKey> = {
  tone: "editor.group.tone",
  color: "editor.group.color",
  detail: "editor.group.detail",
  lens: "editor.group.lens",
};

/** 填充从哪长（与 Rust 侧 `Origin` 同一套值）。 */
export type ParamOrigin = "center" | "start";

/** 默认值的来源（与 Rust 侧 `Baseline` 同一套值）。 */
export type ParamBaseline = "static" | "as-shot";

/** 这根杆住在哪（纯界面概念，不进契约文件）。 */
export type ParamPlacement = "panel" | "overview";

export interface ParamSpec {
  id: string;
  labelKey: MessageKey;
  group: ParamGroup;
  min: number;
  max: number;
  step: number;
  /**
   * 填充从哪长（`DESIGN.md` §14.10）：
   * `center` = 双极参数（曝光 / 反差 / 色温…），`start` = 单极参数（降噪 / 锐化）。
   * 与「把手在哪」无关 —— 把手永远只由当前值决定。
   */
  origin: ParamOrigin;
  /**
   * 本波（M3-W3）这条参数接进管线了吗。
   *
   * `false` = M3-W4 接（清晰度 / 镜头两组）：界面**必须禁用**并写明哪一波接 ——
   * 一个能拖但没反应的拉杆比一个禁用的拉杆更糟。
   */
  wired: boolean;
  /** 默认值从哪来（色温是 `as-shot`：随照片的元数据走）。 */
  baseline: ParamBaseline;
  /**
   * 这根杆住在哪：`panel` = 参数组页签里；`overview` = 总览页的直方图下面。
   *
   * 人类 2026-09-24 定：动态反差**暂时**挂在直方图下面、不占单独的面板块。
   * 同一根杆只能出现一次 —— 所以 `paramsInGroup` 必须把 `overview` 的排除掉，
   * 否则「同一个东西两种表达」就是 bug（`AGENTS.md` §2.12）。
   */
  placement: ParamPlacement;
  /** 两端是否显示极值。不显示时**槽位仍然保留**（否则同一列轨道会左右不齐）。 */
  limits: boolean;
  /** 小数位（值文本用） */
  decimals: number;
  /** 值后缀，如色温的 `K` */
  unit?: string;
  /** 极值文案（不给就按 `decimals` 格式化） */
  minLabel?: string;
  maxLabel?: string;
}

/** 契约文件里的一条（只关心数字那几项）。 */
interface ContractParam {
  id: string;
  min: number;
  max: number;
  step: number;
  default: number;
  origin: string;
  wired: boolean;
  baseline: string;
}

const CONTRACT: { version: number; params: ContractParam[] } = contract as {
  version: number;
  params: ContractParam[];
};

/**
 * 界面装饰（按 id）。
 *
 * `group` / `labelKey` / `decimals` / `unit` / 极值文案 —— 都在这里；
 * 数字不在这里（见文件头）。
 */
interface Decoration {
  group: ParamGroup;
  labelKey: MessageKey;
  decimals: number;
  unit?: string;
  minLabel?: string;
  maxLabel?: string;
  /** 不写 = `panel`（参数组页签里） */
  placement?: ParamPlacement;
}

const DECORATIONS: Record<string, Decoration> = {
  exposure: { group: "tone", labelKey: "editor.param.exposure", decimals: 2, minLabel: "-2", maxLabel: "+2" },
  contrast: { group: "tone", labelKey: "editor.param.contrast", decimals: 0 },
  highlights: { group: "tone", labelKey: "editor.param.highlights", decimals: 0 },
  blacks: { group: "tone", labelKey: "editor.param.blacks", decimals: 0 },
  // 归在影调组，但**暂时**住在总览页的直方图下面（人类 2026-09-24 定）
  dynamicContrast: {
    group: "tone",
    labelKey: "editor.param.dynamicContrast",
    decimals: 0,
    placement: "overview",
  },
  temperature: {
    group: "color",
    labelKey: "editor.param.temperature",
    decimals: 0,
    unit: "K",
    minLabel: "2500",
    maxLabel: "10000",
  },
  saturation: { group: "color", labelKey: "editor.param.saturation", decimals: 0 },
  vibrance: { group: "color", labelKey: "editor.param.vibrance", decimals: 0 },
  lumaNr: { group: "detail", labelKey: "editor.param.lumaNr", decimals: 0 },
  colorNr: { group: "detail", labelKey: "editor.param.colorNr", decimals: 0 },
  sharpenAmount: { group: "detail", labelKey: "editor.param.sharpenAmount", decimals: 0 },
  sharpenRadius: { group: "detail", labelKey: "editor.param.sharpenRadius", decimals: 0 },
  distortion: { group: "lens", labelKey: "editor.param.distortion", decimals: 0 },
  vignette: { group: "lens", labelKey: "editor.param.vignette", decimals: 0 },
  chromatic: { group: "lens", labelKey: "editor.param.chromatic", decimals: 0 },
};

/** 契约里的 `origin` 字符串 → 我们的联合类型（写错了当场报错，不静默吞）。 */
function parseOrigin(id: string, origin: string): ParamOrigin {
  if (origin === "center" || origin === "start") return origin;
  // i18n-exempt: 契约文件写坏了才会看到的开发期错误（不是界面文案）
  throw new Error(`develop-params.json：${id} 的 origin 不认识：${origin}`);
}

/** 契约里的 `baseline` 字符串 → 我们的联合类型。 */
function parseBaseline(id: string, baseline: string): ParamBaseline {
  if (baseline === "static" || baseline === "as-shot") return baseline;
  // i18n-exempt: 同上（契约文件写坏了才会看到）
  throw new Error(`develop-params.json：${id} 的 baseline 不认识：${baseline}`);
}

/**
 * 参数表（顺序即界面顺序，**由契约文件决定**）。
 *
 * 每一条都必须有装饰：契约里多一条没写装饰的，这里当场抛错（漏掉比画错更难发现）。
 */
export const PARAMS: readonly ParamSpec[] = CONTRACT.params.map((param) => {
  const decoration = DECORATIONS[param.id];
  if (decoration === undefined) {
    // i18n-exempt: 同上（契约文件写坏了才会看到）
    throw new Error(`develop-params.json 里的 ${param.id} 没有界面装饰（params.ts）`);
  }
  return {
    id: param.id,
    labelKey: decoration.labelKey,
    group: decoration.group,
    placement: decoration.placement ?? "panel",
    min: param.min,
    max: param.max,
    step: param.step,
    origin: parseOrigin(param.id, param.origin),
    wired: param.wired,
    baseline: parseBaseline(param.id, param.baseline),
    // 极值一律显示（`DESIGN.md` §14.10 的槽位规则由组件保证）
    limits: true,
    decimals: decoration.decimals,
    ...(decoration.unit === undefined ? {} : { unit: decoration.unit }),
    ...(decoration.minLabel === undefined ? {} : { minLabel: decoration.minLabel }),
    ...(decoration.maxLabel === undefined ? {} : { maxLabel: decoration.maxLabel }),
  };
});

export const PARAM_IDS: readonly string[] = PARAMS.map((param) => param.id);

/** 契约文件里声明的版本（改结构时两侧一起看它）。 */
export const PARAM_CONTRACT_VERSION: number = CONTRACT.version;

/** 装饰表里没被契约覆盖的 id（测试用：两边必须一一对应）。 */
export const DECORATED_IDS: readonly string[] = Object.keys(DECORATIONS);

/** 某一组的参数（右栏按页签取）—— **不含**寄居在别处的（`placement: "overview"`）。 */
export function paramsInGroup(group: ParamGroup): readonly ParamSpec[] {
  return PARAMS.filter((param) => param.group === group && param.placement === "panel");
}

/** 寄居在别处的参数（总览页直方图下面那根）—— 与 `paramsInGroup` 互斥，合起来是全集。 */
export function paramsInOverview(): readonly ParamSpec[] {
  return PARAMS.filter((param) => param.placement === "overview");
}

/** 按 id 取口径。 */
export function paramSpec(id: string): ParamSpec | undefined {
  return PARAMS.find((param) => param.id === id);
}

/** 这一条接进管线了吗（没接的界面禁用）。 */
export function isParamWired(id: string): boolean {
  return paramSpec(id)?.wired === true;
}

/**
 * 参数默认值表。
 *
 * 口径：**双极参数取区间中点**（曝光正好落在 0、色温落在 2500..10000 的中间 ——
 * 「不动」在双极拉杆上就是把手在正中），**单极参数取左端**（0）。
 *
 * ⚠️ 色温的**真实**默认值是这张照片的 as-shot 色温（随照片变）—— 这里给的是
 * 读不到元数据时的兜底。判断「动过没有」要用 `isParamDirty(id, value, baseline)`。
 */
export const PARAM_DEFAULTS: Readonly<Record<string, number>> = Object.freeze(
  Object.fromEntries(
    PARAMS.map((param) => [
      param.id,
      param.origin === "center" ? (param.min + param.max) / 2 : param.min,
    ]),
  ),
);

/** 一份全新的编辑参数（每个参数都在默认值上）。 */
export function defaultParams(): Record<string, number> {
  return { ...PARAM_DEFAULTS };
}

/**
 * 某个参数是不是「动过」（与基线不同）。
 *
 * `baseline` 不给时用静态默认值；色温这类 `as-shot` 参数由调用方把**这张照片的**
 * 基线传进来（编辑器 store 有它）。
 */
export function isParamDirty(id: string, value: number, baseline?: number): boolean {
  const base = baseline ?? PARAM_DEFAULTS[id];
  return base === undefined ? false : value !== base;
}

/** 值文本（那个跟在标签右边的小一号数字）。 */
export function formatParamValue(spec: ParamSpec, value: number): string {
  const text = value.toFixed(spec.decimals);
  // 双极参数带显式正负号 —— 「+0.35」比「0.35」更能说明「我在加」
  const signed = spec.origin === "center" && value > 0 ? `+${text}` : text;
  return spec.unit === undefined ? signed : `${signed}${spec.unit}`;
}

/** 极值文案（`limits` 为假时调用方不显示，但仍会占槽位）。 */
export function formatLimit(
  spec: ParamSpec,
  end: "min" | "max",
): string {
  const custom = end === "min" ? spec.minLabel : spec.maxLabel;
  if (custom !== undefined) return custom;
  return (end === "min" ? spec.min : spec.max).toFixed(spec.decimals);
}

/* ══════════════════════════════════════════════════════════════
 * 裁切比例预设（`prompts/editor.pd` 的右栏裁切控制块）
 * ══════════════════════════════════════════════════════════════ */

export interface CropRatio {
  id: string;
  labelKey: MessageKey | null;
  /** `null` = 自由（不锁比例） */
  ratio: number | null;
}

/**
 * 比例下拉的项（顺序即界面顺序）。
 *
 * `自由` 与 `原始比例` 是**语义项**（不锁 / 锁到这张图自己的比例），
 * 其余是固定比例；`自定义` 不在表里 —— 它由用户手输横纵比之后**自动切过去**
 * （`.pd`：「用户手动输入后上面的比例选项自动变成`自定义`」）。
 */
export const CROP_RATIOS: readonly CropRatio[] = [
  { id: "free", labelKey: "editor.crop.free", ratio: null },
  { id: "original", labelKey: "editor.crop.original", ratio: null },
  { id: "1:1", labelKey: null, ratio: 1 },
  { id: "3:2", labelKey: null, ratio: 3 / 2 },
  { id: "2:3", labelKey: null, ratio: 2 / 3 },
  { id: "4:3", labelKey: null, ratio: 4 / 3 },
  { id: "3:4", labelKey: null, ratio: 3 / 4 },
  { id: "16:9", labelKey: null, ratio: 16 / 9 },
  { id: "9:16", labelKey: null, ratio: 9 / 16 },
];

/** 比例项在下拉里的显示文本（固定比例直接写 `3:2`，语义项走语言包）。 */
export function cropRatioLabel(
  item: CropRatio,
  translate: (key: MessageKey) => string,
): string {
  return item.labelKey === null ? item.id : translate(item.labelKey);
}

/** 反转比例：`4:3 ⇄ 3:4`。自由的项反转还是自己。 */
export function invertRatio(item: CropRatio): CropRatio {
  if (item.ratio === null) return item;
  const flipped = 1 / item.ratio;
  const match = CROP_RATIOS.find((candidate) => candidate.ratio !== null && Math.abs(candidate.ratio - flipped) < 1e-6);
  return match ?? { id: `${flipped.toFixed(4)}`, labelKey: null, ratio: flipped };
}

/* ══════════════════════════════════════════════════════════════
 * 旋转角度
 * ══════════════════════════════════════════════════════════════ */

/** 角度拉杆：默认在正中，往右顺时针，两端 ±360（`.pd`）。 */
export const ANGLE_SPEC: ParamSpec = {
  id: "angle",
  labelKey: "editor.rotate.angle",
  group: "tone",
  placement: "panel",
  min: -360,
  max: 360,
  step: 0.1,
  origin: "center",
  wired: false,
  baseline: "static",
  limits: true,
  decimals: 1,
  unit: "°",
  minLabel: "-360",
  maxLabel: "+360",
};
