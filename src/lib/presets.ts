/**
 * 编辑预设的**形状与纯逻辑**（`specs/editor-presets.md` §3）。
 *
 * 只放数据与纯函数（清洗 / 快照构建 / 应用计划 / 落点规则 / 重名判定）：
 * 持久化在 `api/presets.ts`，面板在 `features/editor/preset-panel.tsx`。
 *
 * `lib` 不许 import `features`（`scripts/check-architecture.mjs`）——
 * 大类 → 参数 id 的映射由 features/editor 侧从 `params.ts` 的 `PARAMS`
 * 派生后作为 `GroupParams` 注入，这里不写第二份。
 */

import { IDENTITY_CURVE } from "./curve.ts";
import type { PhotoColorState } from "./color-model.ts";

/** `默认` 目录的固定 id（名称走语言包，DB 里的 `name` 只是占位）。 */
export const DEFAULT_DIRECTORY_ID = "default";

/** 六个大类（顺序即界面 chips 顺序；未来加「色彩管理」时在此追加）。 */
export const PRESET_GROUPS = ["tone", "color", "detail", "lens", "curve", "lut", "colorManagement"] as const;
export const DEFAULT_PRESET_GROUPS = PRESET_GROUPS.filter(group=>group !== "colorManagement");
export type PresetGroup = (typeof PRESET_GROUPS)[number];

/** 带数值参数的四个大类（与 `ParamGroup` 同名同义；curve / lut 单独处理）。 */
export type NumericGroupId = "tone" | "color" | "detail" | "lens";

/** 大类 → 参数 id 列表（由 `features/editor/params.ts` 派生后注入）。 */
export type GroupParams = Record<NumericGroupId, readonly string[]>;

/** 一份大类快照（`payload` 的契约形状）。 */
export interface PresetSnapshot {
  version: number;
  tone?: Record<string, number>;
  color?: Record<string, number>;
  /** 清晰度大类：数值参数 + 降噪方式，同层扁平（specs §3 的 JSON 形状）。 */
  detail?: PresetDetailGroup;
  /** 镜头大类：数值参数 + 配置文件与启用开关，同层扁平。 */
  lens?: PresetLensGroup;
  curve?: Partial<Record<string, [number, number][]>>;
  lut?: { id: string | null; enabled: boolean };
  /** Reusable selection strategy, never a source file's embedded ICC or monitor. */
  colorManagement?: { processVersion: "linear_rec2020_v2"; input: "automatic" | {profileId: string} };
}

/** 清晰度大类的载荷形状（索引签名放宽到 `number | string | null` 以容纳 `nrMethod`）。 */
export interface PresetDetailGroup {
  /** 降噪方式（`null` = 快速档）；其余键 = 参数 id → 数值。 */
  nrMethod?: string | null;
  [paramId: string]: number | string | null | undefined;
}

/** 镜头大类的载荷形状（同上，容纳 `profile` / `enabled`）。 */
export interface PresetLensGroup {
  profile?: string | null;
  enabled?: boolean | null;
  [paramId: string]: number | string | boolean | null | undefined;
}

export interface PresetDirectory {
  id: string;
  name: string;
  sortOrder: number;
  createdAt: number;
}

export interface PresetRecord {
  id: string;
  directoryId: string;
  name: string;
  payload: PresetSnapshot;
  createdAt: number;
  updatedAt: number;
}

export interface PresetLibrary {
  directories: PresetDirectory[];
  presets: PresetRecord[];
}

/** 面板的选中态：目录单选，或预设（可多选）。 */
export type PresetSelection =
  | { kind: "directory"; id: string }
  | { kind: "presets"; ids: readonly string[] }
  | null;

/** 名称上限（**Unicode 字符数**，与 Rust `store::presets::valid_name` 同一口径）。 */
export const PRESET_NAME_MAX = { directory: 40, preset: 80 } as const;
export type PresetNameKind = keyof typeof PRESET_NAME_MAX;

/** 名称不合法时的原因（面板换成本地化文案；`null` = 合法）。 */
export type PresetNameError = "empty" | "tooLong" | "control";

/**
 * 名称校验：去首尾空白后按**字符**（不是 UTF-16 单元）计数 ——
 * 80 个 emoji 是合法的 80 字符名，不能被 `String.length` 的 160 单元误伤。
 * 控制字符与 Rust 侧 `valid_name` 一致一律拒绝。
 */
export function validatePresetName(raw: string, kind: PresetNameKind): PresetNameError | null {
  const name = raw.trim();
  if (name === "") return "empty";
  if (Array.from(name).length > PRESET_NAME_MAX[kind]) return "tooLong";
  if (/[\u0000-\u001f\u007f-\u009f]/u.test(name)) return "control";
  return null;
}

/** 新建目录 / 预设的结果（面板据此区分「重名」与其它后端失败，审计 2026-10-04）。 */
export type PresetCreateOutcome =
  | { ok: true }
  | { ok: false; message: string };

/** 库刷新后收敛选中：失效的目录 / 预设不再留在选中态里（撤销全部则回 `null`）。 */
export function prunePresetSelection(
  selection: PresetSelection,
  directoryIds: ReadonlySet<string>,
  presetIds: ReadonlySet<string>,
): PresetSelection {
  if (selection === null) return null;
  if (selection.kind === "directory") {
    return directoryIds.has(selection.id) ? selection : null;
  }
  const ids = selection.ids.filter((id) => presetIds.has(id));
  return ids.length === 0 ? null : { kind: "presets", ids };
}

/** 库刷新后收敛折叠记录：删掉的目录不再留残留键。 */
export function prunePresetCollapsed(
  record: Readonly<Record<string, boolean>>,
  directoryIds: ReadonlySet<string>,
): Record<string, boolean> {
  const out: Record<string, boolean> = {};
  for (const [id, collapsed] of Object.entries(record)) {
    if (directoryIds.has(id)) out[id] = collapsed;
  }
  return out;
}

/* ══════════════════════════════════════════════════════════════
 * 清洗：任何输入 → 合法形状（坏条目丢弃，不抛错 —— 存储/IPC 里的垃圾
 * 不能让面板起不来；口径照 `lib/lut-library.ts` 的 `sanitize*`）
 * ══════════════════════════════════════════════════════════════ */

/**
 * 存储/IPC 里的名称 → 面板可用名称。`maxChars` 是**字符数**（与 Rust 同一口径）：
 * 用 `Array.from` 数码点，`String.length` 会把 emoji 的代理对当成两个字符。
 * 控制字符拒绝（Rust `valid_name` 同样拒绝）。
 */
function trimmedName(raw: unknown, maxChars: number): string | null {
  if (typeof raw !== "string") return null;
  const name = raw.trim();
  if (name === "" || Array.from(name).length > maxChars) return null;
  if (/[\u0000-\u001f\u007f-\u009f]/u.test(name)) return null;
  return name;
}

function sanitizeId(raw: unknown): string | null {
  if (typeof raw !== "string") return null;
  return /^[A-Za-z0-9-]{1,128}$/.test(raw) ? raw : null;
}

function finiteNumber(raw: unknown): number | null {
  return typeof raw === "number" && Number.isFinite(raw) ? raw : null;
}

function sanitizeValues(raw: unknown): Record<string, number> {
  const out: Record<string, number> = {};
  if (typeof raw !== "object" || raw === null) return out;
  for (const [id, value] of Object.entries(raw as Record<string, unknown>)) {
    const number = finiteNumber(value);
    if (number !== null && sanitizeId(id) !== null) out[id] = number;
  }
  return out;
}

function sanitizeCurve(raw: unknown): Partial<Record<string, [number, number][]>> {
  const out: Partial<Record<string, [number, number][]>> = {};
  if (typeof raw !== "object" || raw === null) return out;
  for (const [channel, value] of Object.entries(raw as Record<string, unknown>)) {
    if (!Array.isArray(value)) continue;
    const points: [number, number][] = [];
    for (const point of value) {
      if (!Array.isArray(point) || point.length !== 2) continue;
      const x = finiteNumber(point[0]);
      const y = finiteNumber(point[1]);
      if (x === null || y === null) continue;
      points.push([Math.min(1, Math.max(0, x)), Math.min(1, Math.max(0, y))]);
    }
    if (points.length >= 2) out[channel] = points;
  }
  return out;
}

/** 任何输入 → 合法快照；不是 `{version:1, …}` 就返回 `null`（调用方丢行）。 */
export function sanitizePresetSnapshot(raw: unknown): PresetSnapshot | null {
  if (typeof raw !== "object" || raw === null) return null;
  const record = raw as Record<string, unknown>;
  if (record.version !== 1 && record.version !== 2) return null;
  if (record.version === 2 && !Object.prototype.hasOwnProperty.call(record,"colorManagement")) return null;
  const snapshot: PresetSnapshot = { version: record.version };
  let present = false;
  for (const group of ["tone", "color", "detail", "lens"] as const) {
    if (typeof record[group] !== "object" || record[group] === null) continue;
    const values = sanitizeValues(record[group]);
    const extra = record[group] as Record<string, unknown>;
    if (group === "detail") {
      snapshot.detail = { ...values, nrMethod: extra.nrMethod === "high" ? "high" : null };
    } else if (group === "lens") {
      snapshot.lens = {
        ...values,
        profile: typeof extra.profile === "string" && extra.profile !== "" ? extra.profile : null,
        enabled: extra.enabled === true ? true : extra.enabled === false ? false : null,
      };
    } else {
      snapshot[group] = values;
    }
    present = true;
  }
  if (typeof record.curve === "object" && record.curve !== null) {
    snapshot.curve = sanitizeCurve(record.curve);
    present = true;
  }
  if (typeof record.lut === "object" && record.lut !== null) {
    const lut = record.lut as Record<string, unknown>;
    snapshot.lut = {
      id: typeof lut.id === "string" && lut.id !== "" ? lut.id : null,
      enabled: lut.enabled === true,
    };
    present = true;
  }
  if (Object.prototype.hasOwnProperty.call(record,"colorManagement")) {
    snapshot.colorManagement = record.colorManagement as PresetSnapshot["colorManagement"];
    present = true;
  }
  return present ? snapshot : null;
}

/** 任何输入 → 合法库（坏行丢弃）。 */
export function sanitizePresetLibrary(raw: unknown): PresetLibrary {
  if (typeof raw !== "object" || raw === null) return { directories: [], presets: [] };
  const record = raw as Record<string, unknown>;
  const directories: PresetDirectory[] = [];
  const dirIds = new Set<string>();
  if (Array.isArray(record.directories)) {
    for (const item of record.directories) {
      if (typeof item !== "object" || item === null) continue;
      const entry = item as Record<string, unknown>;
      const id = sanitizeId(entry.id);
      const name = trimmedName(entry.name, PRESET_NAME_MAX.directory);
      const sortOrder = finiteNumber(entry.sortOrder);
      const createdAt = finiteNumber(entry.createdAt);
      if (id === null || name === null || sortOrder === null || createdAt === null) continue;
      if (dirIds.has(id)) continue;
      dirIds.add(id);
      directories.push({ id, name, sortOrder, createdAt });
    }
  }
  const presets: PresetRecord[] = [];
  if (Array.isArray(record.presets)) {
    for (const item of record.presets) {
      if (typeof item !== "object" || item === null) continue;
      const entry = item as Record<string, unknown>;
      const id = sanitizeId(entry.id);
      const directoryId = sanitizeId(entry.directoryId);
      const name = trimmedName(entry.name, PRESET_NAME_MAX.preset);
      const createdAt = finiteNumber(entry.createdAt);
      const updatedAt = finiteNumber(entry.updatedAt);
      const payload = sanitizePresetSnapshot(entry.payload);
      if (id === null || directoryId === null || name === null
        || createdAt === null || updatedAt === null || payload === null) continue;
      if (!dirIds.has(directoryId)) continue;
      presets.push({ id, directoryId, name, payload, createdAt, updatedAt });
    }
  }
  return { directories, presets };
}

/* ══════════════════════════════════════════════════════════════
 * 快照：构建与应用计划
 * ══════════════════════════════════════════════════════════════ */

/** 快照里实际保存了哪些大类。 */
export function snapshotGroups(snapshot: PresetSnapshot): PresetGroup[] {
  const groups: PresetGroup[] = [];
  for (const group of PRESET_GROUPS) {
    if (snapshot[group] !== undefined) groups.push(group);
  }
  return groups;
}

/** 构建快照的输入（由 store 提供**当前生效值**，不是 dirty 子集）。 */
export interface PresetSnapshotSource {
  photoColor?: PhotoColorState | null;
  values: Record<string, number>;
  curves: Record<string, readonly (readonly [number, number])[]>;
  nrMethod: string | null;
  lensProfile: string | null;
  lensEnabled: boolean | null;
  lutId: string | null;
  lutEnabled: boolean;
}

/**
 * 按大类构建**完整快照**：勾选的大类写全字段（含等于默认值的项）——
 * 「覆盖」语义要求应用时能把该大类恢复到预设保存时的状态（specs §3 规则 1）。
 */
export function buildPresetSnapshot(
  groups: readonly PresetGroup[],
  source: PresetSnapshotSource,
  groupParams: GroupParams,
): PresetSnapshot {
  const snapshot: PresetSnapshot = { version: 1 };
  for (const group of groups) {
    switch (group) {
      case "colorManagement": {
        const color=source.photoColor;
        if (color?.process_version === "linear_rec2020_v2") {
          snapshot.version=2;
          snapshot.colorManagement={processVersion:"linear_rec2020_v2",input:color.source.kind === "assigned_rgb_icc" ? {profileId:color.source.profile_id} : "automatic"};
        }
        break;
      }
      case "tone":
      case "color":
      case "detail":
      case "lens": {
        const values: Record<string, number> = {};
        for (const id of groupParams[group]) {
          values[id] = finiteNumber(source.values[id]) ?? 0;
        }
        if (group === "detail") {
          snapshot.detail = { ...values, nrMethod: source.nrMethod === "high" ? "high" : null };
        } else if (group === "lens") {
          snapshot.lens = {
            ...values,
            profile: source.lensProfile,
            enabled: source.lensEnabled,
          };
        } else {
          snapshot[group] = values;
        }
        break;
      }
      case "curve": {
        const curve: PresetSnapshot["curve"] = {};
        for (const [channel, points] of Object.entries(source.curves)) {
          curve[channel] = points.map(([x, y]) => [x, y] as [number, number]);
        }
        snapshot.curve = curve;
        break;
      }
      case "lut":
        snapshot.lut = { id: source.lutId, enabled: source.lutEnabled };
        break;
    }
  }
  return snapshot;
}

export function presetColorChoice(snapshot: PresetSnapshot): string|null|undefined {
  if (!Object.prototype.hasOwnProperty.call(snapshot,"colorManagement")) return undefined;
  const group=snapshot.colorManagement;
  if (snapshot.version !== 2 || !group || typeof group !== "object" || Object.keys(group).length !== 2 || group.processVersion !== "linear_rec2020_v2") throw new Error("PRESET_COLOR_UNSUPPORTED");
  if (group.input === "automatic") return null;
  if (typeof group.input === "object" && group.input !== null && Object.keys(group.input).length === 1 && /^[0-9a-f]{64}$/.test(group.input.profileId)) return group.input.profileId;
  throw new Error("PRESET_COLOR_UNSUPPORTED");
}

/** 应用计划（store 拿到后逐项写入；纯函数便于单测）。 */
export interface PresetApplyPlan {
  /** 逐项 `setParam`（含「回到默认」的项 —— 覆盖语义要求写全）。 */
  values: Record<string, number>;
  /** 曲线四通道全部（缺 = 恒等）；空对象 = 没应用曲线大类。 */
  curves: Record<string, [number, number][]>;
  hasDetail: boolean;
  nrMethod: string | null;
  hasLens: boolean;
  lens: { profile: string | null; enabled: boolean | null };
  /** `null` = 未应用 LUT 大类；`"missing"` = LUT 丢失，忽略（不动当前）。 */
  lut: { id: string | null; enabled: boolean } | "missing" | null;
  /** 实际生效的大类（LUT 丢失时不含 `lut`）。 */
  appliedGroups: PresetGroup[];
}

/**
 * 把快照变成应用计划：保存的大类各自**整体覆盖**；
 * LUT 的 id 非空但在库里找不到（或不可用）→ `"missing"`，静默忽略（崔总：要有保底）。
 */
export function planPresetApply(
  snapshot: PresetSnapshot,
  groupParams: GroupParams,
  defaults: Readonly<Record<string, number>>,
  lutExists: (id: string) => boolean,
  channels: readonly string[] = ["rgb", "r", "g", "b"],
): PresetApplyPlan {
  presetColorChoice(snapshot);
  const plan: PresetApplyPlan = {
    values: {},
    curves: {},
    hasDetail: false,
    nrMethod: null,
    hasLens: false,
    lens: { profile: null, enabled: null },
    lut: null,
    appliedGroups: [],
  };
  for (const group of snapshotGroups(snapshot)) {
    switch (group) {
      case "tone":
      case "color":
      case "detail":
      case "lens": {
        const raw = snapshot[group];
        if (raw === undefined) break;
        for (const id of groupParams[group]) {
          plan.values[id] = finiteNumber(raw[id]) ?? defaults[id] ?? 0;
        }
        if (group === "detail") {
          plan.hasDetail = true;
          plan.nrMethod = raw.nrMethod === "high" ? "high" : null;
        }
        if (group === "lens") {
          plan.hasLens = true;
          plan.lens = {
            profile: typeof raw.profile === "string" && raw.profile !== "" ? raw.profile : null,
            enabled: raw.enabled === true ? true : raw.enabled === false ? false : null,
          };
        }
        plan.appliedGroups.push(group);
        break;
      }
      case "curve": {
        for (const channel of channels) {
          const points = snapshot.curve?.[channel];
          plan.curves[channel] = points !== undefined
            ? points.map(([x, y]) => [x, y] as [number, number])
            : IDENTITY_CURVE.map(([x, y]) => [x, y] as [number, number]);
        }
        plan.appliedGroups.push(group);
        break;
      }
      case "lut": {
        const lut = snapshot.lut;
        if (lut === undefined) break;
        if (lut.id === null) {
          plan.lut = { id: null, enabled: false };
          plan.appliedGroups.push("lut");
        } else if (lutExists(lut.id)) {
          plan.lut = { id: lut.id, enabled: lut.enabled === true };
          plan.appliedGroups.push("lut");
        } else {
          // 丢失保底：LUT 文件没了 / 被删了 → 忽略该项，不报错
          plan.lut = "missing";
        }
        break;
      }
    }
  }
  return plan;
}

/* ══════════════════════════════════════════════════════════════
 * 目录规则
 * ══════════════════════════════════════════════════════════════ */

/**
 * 新建预设的落点：选中的目录 → 选中预设所在的目录 → `default`
 （选中的目录已不在列表里也落 `default`，specs §5.3）。
 */
export function resolveCreateDirectory(
  selection: PresetSelection,
  directories: readonly { id: string }[],
  presets: readonly { id: string; directoryId: string }[],
): string {
  let candidate: string | null = null;
  if (selection?.kind === "directory") {
    candidate = selection.id;
  } else if (selection?.kind === "presets" && selection.ids.length > 0) {
    candidate = presets.find((preset) => preset.id === selection.ids[0])?.directoryId ?? null;
  }
  return candidate !== null && directories.some((directory) => directory.id === candidate)
    ? candidate
    : DEFAULT_DIRECTORY_ID;
}

/** 同目录内预设名是否已被占（大小写与首尾空格都算同一个）。 */
export function isPresetNameTaken(
  name: string,
  directoryId: string,
  presets: readonly { name: string; directoryId: string }[],
): boolean {
  const wanted = name.trim().toLowerCase();
  return wanted !== ""
    && presets.some((preset) => preset.directoryId === directoryId
      && preset.name.trim().toLowerCase() === wanted);
}

/** 目录名是否已被占（同上口径）。 */
export function isDirectoryNameTaken(
  name: string,
  directories: readonly { name: string }[],
): boolean {
  const wanted = name.trim().toLowerCase();
  return wanted !== ""
    && directories.some((directory) => directory.name.trim().toLowerCase() === wanted);
}

/** 建一个目录 id（本地唯一即可；不引第三方 uuid，口径照 `newLutCategoryId`）。 */
export function newPresetDirectoryId(
  existing: readonly { id: string }[],
  random: () => number = Math.random,
): string {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    const id = `presetdir-${Math.floor(random() * 0xffffffff).toString(36)}`;
    if (!existing.some((directory) => directory.id === id)) return id;
  }
  return `presetdir-${Date.now().toString(36)}${Math.floor(random() * 1e6).toString(36)}`;
}

/** 建一个预设 id（同上）。 */
export function newPresetId(
  existing: readonly { id: string }[],
  random: () => number = Math.random,
): string {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    const id = `preset-${Math.floor(random() * 0xffffffff).toString(36)}`;
    if (!existing.some((preset) => preset.id === id)) return id;
  }
  return `preset-${Date.now().toString(36)}${Math.floor(random() * 1e6).toString(36)}`;
}
