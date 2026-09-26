import { isTauriRuntime } from "./tauri-env.ts";

let coreModule: Promise<typeof import("@tauri-apps/api/core")> | undefined;
function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  coreModule ??= import("@tauri-apps/api/core");
  return coreModule.then((core) => core.invoke<T>(command, args));
}

export interface LutRecord {
  id: string;
  categoryId: string;
  name: string;
  originalFilename: string;
  originalPath: string;
  fileRelPath: string;
  coverRelPath: string;
  format: "cube" | "hald";
  hidden: boolean;
  fileHash: string | null;
  createdAt: number;
  available: boolean;
  coverAvailable: boolean;
}
export interface LutLibrary {
  categories: { id: string; name: string }[];
  entries: LutRecord[];
}
export interface LutImportResult { library: LutLibrary; imported: number; duplicates: number; restored: number; skipped: string[] }

export async function getLutLibrary(legacyCategories: { id: string; name: string }[] = []): Promise<LutLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<LutLibrary>("lut_library", { legacyCategories });
}
export async function createLutCategory(id: string, name: string): Promise<LutLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<LutLibrary>("lut_create_category", { id, name });
}
export async function importLutDirectory(path: string, categoryId: string): Promise<LutImportResult | null> {
  if (!isTauriRuntime()) return null;
  return call<LutImportResult>("lut_import_directory", { path, categoryId });
}
export async function hideLut(id: string): Promise<LutLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<LutLibrary>("lut_hide", { id });
}
export async function getLutCover(id: string): Promise<Uint8Array | null> {
  if (!isTauriRuntime()) return null;
  const bytes = await call<number[] | Uint8Array>("lut_cover", { id });
  return bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
}
