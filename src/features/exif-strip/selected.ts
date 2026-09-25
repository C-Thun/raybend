/** One in-memory source for the currently selected file's metadata across the three workflows. */
import { createSignal } from "solid-js";
import type { FileExif } from "../../api/types.ts";
import { toExifData } from "./from-file.ts";
import type { ExifData } from "./types.ts";

export interface SelectedFileMetadata {
  path(): string | null;
  file(): FileExif | null;
  data(): ExifData | null;
  select(path: string | null, fallback?: ExifData | null): void;
  enrich(extra: ExifData): void;
}

export function createSelectedFileMetadata(load: (path: string) => Promise<FileExif>): SelectedFileMetadata {
  const [path, setPath] = createSignal<string | null>(null);
  const [file, setFile] = createSignal<FileExif | null>(null);
  const [fallback, setFallback] = createSignal<ExifData | null>(null);
  let revision = 0;

  const select = (nextPath: string | null, nextFallback: ExifData | null = null): void => {
    const current = ++revision;
    setPath(nextPath);
    setFile(null);
    setFallback(nextFallback);
    if (nextPath === null) return;
    void load(nextPath).then(
      (result) => { if (current === revision) setFile(result); },
      () => { if (current === revision) setFile(null); },
    );
  };

  const enrich = (extra: ExifData): void => {
    // Functional setters read the previous value without subscribing the calling effect.
    // Publishing an identical object must also be a no-op (lens matching enriches from an effect).
    setFallback((current) => {
      const changes = Object.entries(extra).filter(([key, value]) =>
        value !== undefined && value !== current?.[key as keyof ExifData]);
      if (changes.length === 0) return current;
      return { ...(current ?? {}), ...Object.fromEntries(changes) };
    });
  };

  return {
    path,
    file,
    data: () => {
      const base = fallback();
      const loaded = file();
      if (loaded === null) return base;
      const fresh = toExifData(loaded);
      if (base === null) return fresh;
      const merged = { ...base };
      for (const [key, value] of Object.entries(fresh)) {
        if (value !== undefined) Object.assign(merged, { [key]: value });
      }
      return merged;
    },
    select,
    enrich,
  };
}
