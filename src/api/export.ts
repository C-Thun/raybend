import { isTauriRuntime } from "./tauri-env.ts";
import { toBytes } from "./db.ts";
import {
  presetErrors,
  type AssetVariants,
  type ExportPreset,
  type VariantRef,
  type VariantSnapshot,
} from "../lib/export-model.ts";
let coreModule: Promise<typeof import("@tauri-apps/api/core")> | undefined;
function call<T>(command: string, args: Record<string, unknown>): Promise<T> {
  coreModule ??= import("@tauri-apps/api/core");
  return coreModule.then((core) => core.invoke<T>(command, args));
}
export async function getExportVariants(
  repositoryId: string,
  assetIds: readonly number[],
): Promise<AssetVariants[]> {
  return isTauriRuntime()
    ? call("export_variants", { repositoryId, assetIds: [...assetIds] })
    : [];
}
export async function getExportSnapshots(
  repositoryId: string,
  references: readonly VariantRef[],
): Promise<VariantSnapshot[]> {
  return isTauriRuntime()
    ? call("export_snapshots", { repositoryId, references: [...references] })
    : [];
}
export async function validateExportPreset(
  preset: ExportPreset,
): Promise<{ errors: Record<string, string>; warnings: string[] }> {
  return isTauriRuntime()
    ? call("export_preset_validate", { preset })
    : { errors: presetErrors(preset), warnings: [] };
}
export async function getExportVariantImage(
  repositoryId: string,
  reference: VariantRef,
  size: "grid" | "strip" | "screen",
  captured?: VariantSnapshot,
): Promise<Uint8Array | null> {
  return isTauriRuntime()
    ? toBytes(
        await call("export_variant_image", {
          repositoryId,
          reference,
          size,
          captured: captured ?? null,
        }),
      )
    : null;
}
