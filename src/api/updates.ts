import { isTauriRuntime } from "./tauri-env.ts";
import type { UpdateSource } from "../lib/update-source.ts";
export interface AvailableUpdate {version:string;notes:string|null}
export async function updateCheck(source:UpdateSource):Promise<AvailableUpdate|null>{if(!isTauriRuntime())throw new Error("UPDATE_NATIVE_REQUIRED");const {invoke}=await import("@tauri-apps/api/core");return invoke("updates_check",{source});}
export async function updateDownload(source:UpdateSource):Promise<void>{const {invoke}=await import("@tauri-apps/api/core");await invoke("updates_download",{source});}
export async function updateInstall(source:UpdateSource):Promise<void>{const {invoke}=await import("@tauri-apps/api/core");await invoke("updates_install",{source});}
