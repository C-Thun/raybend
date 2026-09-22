/**
 * 编辑参数表（**纯数据**，`design/editor.md` §3.7 + `DESIGN.md` §14.10）。
 *
 * 这一份表同时喂三处，所以必须只有一份：
 *
 * * 右栏第 2 组页签（`影调` / `色彩` / `清晰度` / `镜头`）的拉杆；
 * * W3 的 IPC 载荷（参数名 = 管线的一级）；
 * * 「全部重置」与「这一项回到默认」。
 *
 * 每条参数自带：范围、步长、双极还是单极、要不要显示极值、几位小数 ——
 * 这些是**编辑器自己的口径**，写进组件里就会散成好几处。
 *
 * ⚠️ 本波（M3-W1）拉杆只改数值、**不改画面**：显影管线在 W3 落地。
 * 但参数名与范围在这里定死，W3 只接管线、不动界面。
 */

import type { MessageKey } from "../../i18n/index.ts";

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
  origin: "center" | "start";
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

/**
 * 参数表（顺序即界面顺序）。
 *
 * 范围口径：曝光是 EV（−2..+2），其余调性/色彩都是百分比式的 −100..100，
 * 降噪与锐化是 0..100 的单极；色温按开尔文给两端极值文案（2500/10000）——
 * 它的实际白平衡换算在 W3 的管线里做，这里只是**用户看到的刻度**。
 */
export const PARAMS: readonly ParamSpec[] = [
  // ── 影调 ──────────────────────────────────────────────
  { id: "exposure", labelKey: "editor.param.exposure", group: "tone", min: -2, max: 2, step: 0.05, origin: "center", limits: true, decimals: 2, minLabel: "-2", maxLabel: "+2" },
  { id: "contrast", labelKey: "editor.param.contrast", group: "tone", min: -100, max: 100, step: 1, origin: "center", limits: true, decimals: 0 },
  { id: "highlights", labelKey: "editor.param.highlights", group: "tone", min: -100, max: 100, step: 1, origin: "center", limits: true, decimals: 0 },
  { id: "blacks", labelKey: "editor.param.blacks", group: "tone", min: -100, max: 100, step: 1, origin: "center", limits: true, decimals: 0 },

  // ── 色彩 ──────────────────────────────────────────────
  { id: "temperature", labelKey: "editor.param.temperature", group: "color", min: 2500, max: 10000, step: 50, origin: "center", limits: true, decimals: 0, unit: "K", minLabel: "2500", maxLabel: "10000" },
  { id: "saturation", labelKey: "editor.param.saturation", group: "color", min: -100, max: 100, step: 1, origin: "center", limits: true, decimals: 0 },
  { id: "vibrance", labelKey: "editor.param.vibrance", group: "color", min: -100, max: 100, step: 1, origin: "center", limits: true, decimals: 0 },

  // ── 清晰度 ────────────────────────────────────────────
  { id: "lumaNr", labelKey: "editor.param.lumaNr", group: "detail", min: 0, max: 100, step: 1, origin: "start", limits: true, decimals: 0 },
  { id: "colorNr", labelKey: "editor.param.colorNr", group: "detail", min: 0, max: 100, step: 1, origin: "start", limits: true, decimals: 0 },
  { id: "sharpenAmount", labelKey: "editor.param.sharpenAmount", group: "detail", min: 0, max: 100, step: 1, origin: "start", limits: true, decimals: 0 },
  { id: "sharpenRadius", labelKey: "editor.param.sharpenRadius", group: "detail", min: 0, max: 100, step: 1, origin: "start", limits: true, decimals: 0 },

  // ── 镜头 ──────────────────────────────────────────────
  { id: "distortion", labelKey: "editor.param.distortion", group: "lens", min: -100, max: 100, step: 1, origin: "center", limits: true, decimals: 0 },
  { id: "vignette", labelKey: "editor.param.vignette", group: "lens", min: -100, max: 100, step: 1, origin: "center", limits: true, decimals: 0 },
  { id: "chromatic", labelKey: "editor.param.chromatic", group: "lens", min: -100, max: 100, step: 1, origin: "center", limits: true, decimals: 0 },
];

export const PARAM_IDS: readonly string[] = PARAMS.map((param) => param.id);

/** 某一组的参数（右栏按页签取）。 */
export function paramsInGroup(group: ParamGroup): readonly ParamSpec[] {
  return PARAMS.filter((param) => param.group === group);
}

/**
 * 参数默认值表。
 *
 * 口径：**双极参数取区间中点**（曝光正好落在 0、色温落在 2500..10000 的中间 ——
 * 「不动」在双极拉杆上就是把手在正中），**单极参数取左端**（0）。
 *
 * 注意别用「是不是跨 0」判双极：色温的区间全是正数，但它仍然是双极的
 * （填充从中点向把手方向长）—— 判据是 `origin`，不是区间的符号。
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
 * 某个参数是不是「动过」（与默认值不同）。
 *
 * 用来把「已调整」显示出来（也让 W3 的「只把非默认项发给管线」有据可依）。
 */
export function isParamDirty(id: string, value: number): boolean {
  const base = PARAM_DEFAULTS[id];
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
  min: -360,
  max: 360,
  step: 0.1,
  origin: "center",
  limits: true,
  decimals: 1,
  unit: "°",
  minLabel: "-360",
  maxLabel: "+360",
};
