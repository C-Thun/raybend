/** Portable color data only; no IPC, system monitor or proof state. */
export type OutputColor = { kind: "srgb" | "display_p3" | "adobe_rgb" }
  | { kind: "custom_rgb_icc"; profile_id: string };
export interface ColorDefaults {
  untagged_input: { kind: "srgb" | "require_assignment" } | { kind: "rgb_icc"; profile_id: string };
  output: OutputColor;
}

export function validOutputColor(value: unknown): value is OutputColor {
  if (typeof value !== "object" || value === null || !("kind" in value)) return false;
  const object = value as Record<string, unknown>;
  if (["srgb", "display_p3", "adobe_rgb"].includes(String(object.kind))) return Object.keys(object).length === 1;
  return object.kind === "custom_rgb_icc" && Object.keys(object).length === 2
    && typeof object.profile_id === "string" && /^[0-9a-f]{64}$/.test(object.profile_id);
}

export function outputColorChoice(value: OutputColor | undefined): string {
  return value?.kind === "custom_rgb_icc" ? value.profile_id : value?.kind ?? "srgb";
}
export function outputColorFromChoice(value: string): OutputColor | null {
  if (value === "srgb" || value === "display_p3" || value === "adobe_rgb") return {kind:value};
  return /^[0-9a-f]{64}$/.test(value) ? {kind:"custom_rgb_icc",profile_id:value} : null;
}
export function readColorDefaults(raw: string | null): ColorDefaults | null {
  if (raw === null) return null;
  try {
    const value = JSON.parse(raw) as Partial<ColorDefaults>;
    if (typeof value !== "object" || value === null || Object.keys(value).length!==2) return null;
    if (!validOutputColor(value.output) || typeof value.untagged_input !== "object" || value.untagged_input === null) return null;
    const input = value.untagged_input;
    if (input.kind === "srgb" || input.kind === "require_assignment") return Object.keys(input).length === 1 ? value as ColorDefaults : null;
    return input.kind === "rgb_icc" && /^[0-9a-f]{64}$/.test(input.profile_id) && Object.keys(input).length === 2 ? value as ColorDefaults : null;
  } catch { return null; }
}


/** Photo interpretation is portable; display/proof state never enters this payload. */
export type SourceColor =
  | { kind: "auto" }
  | { kind: "embedded_icc" | "assigned_rgb_icc" | "assigned_raw_camera"; profile_id: string }
  | { kind: "assumed_srgb" | "tagged_srgb" }
  | { kind: "raw_camera_matrix"; matrix_id: string };
export interface PhotoColorState {
  process_version: "legacy_srgb8_v1" | "linear_rec2020_v2";
  source: SourceColor;
}
