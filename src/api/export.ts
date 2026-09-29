import { isTauriRuntime } from "./tauri-env.ts";
import { onTauriEvent } from "./events.ts";
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
  rawOriginal?: boolean,
): Promise<Uint8Array | null> {
  return isTauriRuntime()
    ? toBytes(
        await call("export_variant_image", {
          repositoryId,
          reference,
          size,
          captured: captured ?? null,
          rawOriginal: rawOriginal ?? null,
        }),
      )
    : null;
}

export interface ExportQueueView {revision:number;generation:number;queues:Record<string,import("../lib/export-model.ts").ExportQueueItem[]>;enabled:string[]}
export type ExportQueueAction="wake"|"status"|"enqueue"|"enable"|"disable"|"stop"|"reset"|"remove"|"retry";
export async function exportQueue(action:ExportQueueAction,payload:Record<string,unknown>={}):Promise<ExportQueueView> {
  return isTauriRuntime()?call("export_queue",{action,generation:null,items:null,presetId:null,ids:null,...payload}):{revision:0,generation:0,queues:{},enabled:[]};
}
export async function onExportState(handler:(view:ExportQueueView)=>void):Promise<()=>void> {
  return onTauriEvent("export://state",handler);
}
/** 暂不启用：目标名/尺寸诊断接口，产品不提供导出预览入口。 */
export async function exportPreview(repositoryId:string,reference:VariantRef,preset:ExportPreset):Promise<{target:string;width:number;height:number}> {
  return call("export_preview",{repositoryId,reference,preset});
}
/** 暂不启用：预设文件交换，保留底层能力，没有界面/菜单/命令入口。 */
export async function exportPresetsFile(path:string,content:string|null=null):Promise<string>{return call("export_presets_file",{path,content});}

export async function getExportVariantDetails(repositoryId:string,captured:VariantSnapshot):Promise<{width:number;height:number;histogram:import("../lib/histogram.ts").HistogramCounts}> {
  return call("export_variant_details",{repositoryId,captured});
}

/** RAW 真实小图命中才去掉模拟滤镜；字节和来源作为同一个队列结果发布。 */
export async function getVariantThumb(repositoryId: string, reference: VariantRef,
  size: "grid" | "strip", captured?: VariantSnapshot): Promise<{ bytes: Uint8Array; approximate: boolean } | null> {
  if (reference.variant === "raw") {
    const cached = await getExportVariantImage(repositoryId, reference, size, captured, true);
    if (cached !== null && cached.byteLength > 0) return { bytes: cached, approximate: false };
  }
  const bytes = await getExportVariantImage(repositoryId, reference, size, captured, reference.variant === "raw" ? false : undefined);
  return bytes === null ? null : { bytes, approximate: reference.variant === "raw" };
}
