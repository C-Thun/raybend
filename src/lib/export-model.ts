/** 导出纯数据契约；queue 不进任何持久化载荷。 */
export type ExportFormat = "jpeg" | "tiff" | "png" | "webp" | "avif";
export type ExportScope = "all" | "edited" | "sooc";
export interface ExportPreset {
  id: string;
  name: string;
  format: ExportFormat;
  quality: number;
  maxEdge: number;
  directory: string;
  template: string;
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
  status: "pending" | "running" | "done" | "failed";
  error: string | null;
  sequence: number;
}
export function variantKey(repository: string, ref: VariantRef): string {
  return JSON.stringify([repository, ref.assetId, ref.variant]);
}
export function visibleVariants(
  variants: readonly VariantSummary[],
  scope: ExportScope,
): readonly VariantSummary[] {
  return variants.filter(
    (variant) =>
      scope === "all" ||
      (scope === "edited"
        ? variant.reference.variant === "latest" ||
          variant.reference.variant.startsWith("issue:")
        : variant.reference.variant === "sooc"),
  );
}
export function queueProgress(items: readonly ExportQueueItem[]): {
  remaining: number;
  total: number;
  processing: boolean;
} {
  return {
    remaining: items.filter((item) => item.status !== "done").length,
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
  if (!["jpeg", "tiff", "png", "webp", "avif"].includes(p.format))
    errors.format = "format";
  if (!Number.isInteger(p.quality) || p.quality < 1 || p.quality > 100)
    errors.quality = "quality";
  if (!Number.isInteger(p.maxEdge) || p.maxEdge < 0 || p.maxEdge > 65535)
    errors.maxEdge = "maxEdge";
  if (!p.directory || p.directory.includes("\0"))
    errors.directory = "directory";
  if (!p.template.trim()) errors.template = "template";
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
      parsed.version !== 1 ||
      !("presets" in parsed) ||
      !Array.isArray(parsed.presets)
    )
      return [];
    const ids = new Set<string>();
    const names = new Set<string>();
    return parsed.presets
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
        directory: p.directory,
        template: p.template,
      }));
  } catch {
    return [];
  }
}
export function serializePresets(presets: readonly ExportPreset[]): string {
  return JSON.stringify({
    version: 1,
    presets: presets.map((p) => ({
      id: p.id,
      name: p.name,
      format: p.format,
      quality: p.quality,
      maxEdge: p.maxEdge,
      directory: p.directory,
      template: p.template,
    })),
  });
}
