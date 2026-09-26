/** 导出纯数据契约；queue 不进任何持久化载荷。 */
export function formatSupportsQuality(format: ExportFormat): boolean {
  return ["jpeg", "webp", "avif"].includes(format);
}
export const EXPORT_FORMATS = ["webp", "avif", "jpeg", "png"] as const;
export type ExportFormat = typeof EXPORT_FORMATS[number];
export const EXISTING_FILE_POLICIES = ["overwrite", "skip", "append"] as const;
export type ExistingFilePolicy = typeof EXISTING_FILE_POLICIES[number];
export type ExportSizeMode = "original" | "percent" | "maxEdge";
export const DEFAULT_EXPORT_TEMPLATE = ":CYEAR-:CMONTH-:CDAY/:FILENAME";
export type ExportScope = "all" | "edited" | "issues";
export interface ExportPreset {
  id: string;
  name: string;
  format: ExportFormat;
  quality: number;
  maxEdge: number;
  sizeMode: ExportSizeMode;
  percent: number;
  directory: string;
  template: string;
  existingFile: ExistingFilePolicy;
}
export interface VariantRef {
  assetId: number;
  variant: string;
}
export interface VariantSummary {
  relPath: string;
  reference: VariantRef;
  name: string;
  sourceBase: "raw" | "sooc";
  profileHash: string | null;
  main?: boolean;
  edited?: boolean;
  createdAt?: number | null;
}
export interface AssetVariants {
  assetId: number;
  variants: VariantSummary[];
}
export interface VariantSnapshot {
  reference: VariantRef;
  name: string;
  relPath: string;
  profileHash: string;
  stack: unknown;
  sourceSignature: string;
}
export interface ExportQueueItem {
  id: string;
  repositoryId: string;
  root: string;
  snapshot: VariantSnapshot;
  preset: ExportPreset;
  status: "pending" | "running" | "done" | "skipped" | "failed";
  error: string | null;
  output?: string | null;
  sequence: number;
}
export function variantKey(repository: string, ref: VariantRef): string {
  return JSON.stringify([repository, ref.assetId, ref.variant]);
}
/**
 * 从 `variantKey()` 造出的键里取回 `assetId`；**形状不对或不是 JSON 一律返回 `null`，不抛**。
 *
 * 存在的理由：`[repository, assetId, variant]` 这个元组形状原本被**三处**各自
 * `JSON.parse(key)[1]` 取用 —— 位置耦合，且形状一变**不会抛错、只会静默取到 `undefined`**
 * （在界面上只表现为“锚点丢了、不高亮”，很难察觉）。现在形状只由这一对函数拥有
 * （`AGENTS.md` §2.12：同一个能力只允许有一套实现）。
 */
export function variantAssetId(key: string): number | null {
  try {
    const parsed: unknown = JSON.parse(key);
    if (!Array.isArray(parsed)) return null;
    const asset: unknown = parsed[1];
    return typeof asset === "number" && Number.isSafeInteger(asset) ? asset : null;
  } catch {
    return null;
  }
}
export function visibleVariants(
  variants: readonly VariantSummary[],
  _scope: ExportScope,
): readonly VariantSummary[] {
  // Filtering belongs to photos. Every admitted photo exposes the full deduplicated list.
  return variants.filter(v => v.main === true || v.reference.variant !== "raw");
}
export function mainVariant(variants: readonly VariantSummary[]): VariantSummary | null {
  return variants.find(v => v.main) ?? variants.find(v => v.reference.variant === "latest") ?? variants[0] ?? null;
}
export function admitsPhoto(variants: readonly VariantSummary[], scope: ExportScope): boolean {
  return scope === "all" || (scope === "issues"
    ? variants.some(v => v.reference.variant.startsWith("issue:"))
    : (mainVariant(variants)?.edited ?? variants.some(v => v.reference.variant === "latest")));
}
export function orderedIssues(variants: readonly VariantSummary[], promoted: (v: VariantSummary) => boolean): readonly VariantSummary[] {
  const main=mainVariant(variants);
  return variants.filter(v => v !== main && v.reference.variant !== "raw" &&
    !(main?.profileHash && v.profileHash === main.profileHash && v.sourceBase === main.sourceBase))
    .sort((a,b) => Number(promoted(b))-Number(promoted(a)) ||
      Number(a.reference.variant === "sooc")-Number(b.reference.variant === "sooc") ||
      (b.createdAt??0)-(a.createdAt??0) || b.reference.variant.localeCompare(a.reference.variant));
}

export function queueFinished(status: ExportQueueItem["status"]): boolean {
  return status === "done" || status === "skipped";
}
export function queueProgress(items: readonly ExportQueueItem[]): {
  remaining: number;
  total: number;
  processing: boolean;
} {
  return {
    remaining: items.filter((item) => !queueFinished(item.status)).length,
    total: items.length,
    processing: items.some((item) => item.status === "running"),
  };
}
export function presetErrors(p: ExportPreset): Record<string, string> {
  const errors: Record<string, string> = {};
  if (
    !p.name.trim() ||
    [...p.name].length > 128 ||
    /[\u0000-\u001f\u007f]/.test(p.name)
  )
    errors.name = "name";
  if (!EXPORT_FORMATS.includes(p.format))
    errors.format = "format";
  if (p.format !== "png" && (!Number.isInteger(p.quality) || p.quality < 1 || p.quality > 100))
    errors.quality = "quality";
  if (!["original", "percent", "maxEdge"].includes(p.sizeMode)) errors.sizeMode = "sizeMode";
  if (!Number.isInteger(p.maxEdge) || p.maxEdge < (p.sizeMode === "maxEdge" ? 1 : 0) || p.maxEdge > 65535)
    errors.maxEdge = "maxEdge";
  if (!Number.isInteger(p.percent) || p.percent < 1 || p.percent > 100) errors.percent = "percent";
  if (!p.directory || p.directory.includes("\0"))
    errors.directory = "directory";
  if (!p.template.trim()) errors.template = "template";
  if (!EXISTING_FILE_POLICIES.includes(p.existingFile)) errors.existingFile = "existingFile";
  return errors;
}
export function readPresets(raw: string | null): ExportPreset[] {
  if (raw === null) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (
      typeof parsed !== "object" ||
      parsed === null ||
      !("version" in parsed) ||
      ![1, 2, 3].includes(parsed.version as number) ||
      !("presets" in parsed) ||
      !Array.isArray(parsed.presets)
    )
      return [];
    const ids = new Set<string>();
    const names = new Set<string>();
    return parsed.presets
      .map((p: unknown) => parsed.version === 1 && typeof p === "object" && p !== null
        ? {...p, format: (p as ExportPreset).format === ("tiff" as string) ? "png" : (p as ExportPreset).format,
          sizeMode: (p as ExportPreset).maxEdge > 0 ? "maxEdge" : "original", percent: 100} : p)
      .map((p: unknown) => (parsed.version as number) < 3 && typeof p === "object" && p !== null
        ? {...p, existingFile: "append"} : p)
      .filter((p: unknown): p is ExportPreset => {
        if (typeof p !== "object" || p === null) return false;
        const candidate = p as ExportPreset;
        if (
          typeof candidate.id !== "string" ||
          !candidate.id ||
          candidate.id.length > 128 ||
          typeof candidate.name !== "string" ||
          typeof candidate.directory !== "string" ||
          typeof candidate.template !== "string" ||
          Object.keys(presetErrors(candidate)).length > 0 ||
          ids.has(candidate.id) ||
          names.has(candidate.name)
        )
          return false;
        ids.add(candidate.id);
        names.add(candidate.name);
        return true;
      })
      .map((p: ExportPreset) => ({
        id: p.id,
        name: p.name,
        format: p.format,
        quality: p.quality,
        maxEdge: p.maxEdge,
        sizeMode: p.sizeMode,
        percent: p.percent,
        directory: p.directory,
        template: p.template,
        existingFile: p.existingFile,
      }));
  } catch {
    return [];
  }
}
export function serializePresets(presets: readonly ExportPreset[]): string {
  return JSON.stringify({
    version: 3,
    presets: presets.map((p) => ({
      id: p.id,
      name: p.name,
      format: p.format,
      quality: p.quality,
      maxEdge: p.maxEdge,
      sizeMode: p.sizeMode,
      percent: p.percent,
      directory: p.directory,
      template: p.template,
      existingFile: p.existingFile,
    })),
  });
}


/** 有数据时必须让网格挂载，才能触发按需取稿；后台重读不覆盖已有网格。 */
export function exportGalleryState(input: {repository: string | null;scope: string | null;count: number;loading: boolean;error: string | null}): "repository" | "directory" | "loading" | "error" | "empty" | null {
  if (input.repository === null) return "repository";
  if (input.scope === null) return "directory";
  if (input.count > 0) return null;
  if (input.error !== null) return "error";
  return input.loading ? "loading" : "empty";
}
