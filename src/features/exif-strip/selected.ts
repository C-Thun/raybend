/** One in-memory source for the currently selected file's metadata across the three workflows. */
import { batch, createSignal } from "solid-js";
import type { FileExif } from "../../api/types.ts";
import { RAW_FORMAT_LABEL } from "./from-file.ts";
import { toExifData } from "./from-file.ts";
import type { ExifData } from "./types.ts";

/**
 * **显示变体**对 flowinfo 的覆盖：尺寸 + 文件类型。
 *
 * 两者都跟当前显示的变体走（`specs/display-size-source.md` §6.1；
 * 崔总 2026-10-11 定「文件类型也要跟着变」）：
 *
 * * `width`/`height` = 该变体的显示尺寸（`lib/display-size.ts`）；
 * * `base` = 该变体的来源基底 —— `"raw"` 时文件类型显示 `RAW`；
 *   `"sooc"` / `null` 时保持文件自己的格式（JPEG/HEIF/…，位图就该显示它自己的）。
 */
export interface VariantFileInfo {
  width: number;
  height: number;
  base: "raw" | "sooc" | null;
}

export interface SelectedFileMetadata {
  path(): string | null;
  file(): FileExif | null;
  data(): ExifData | null;
  /** Null while reading; a failed read still settles with file=null. */
  settled(): { path: string; file: FileExif | null } | null;
  /**
   * 选中一个文件。
   *
   * `variant` = **当前显示变体**（`lib/display-size.ts` 算好的尺寸 + 来源基底）——
   * 它优先级最高：文件里读到的 EXIF 说的是源文件，而 flowinfo 要显示的是
   * 「现在屏幕上这张多大、是什么」（切到定稿/RAW 后与源文件可以不同，
   * `specs/display-size-source.md` §6.1）。传 `null` = 没有变体口径，用文件自己的。
   */
  select(path: string | null, fallback?: ExifData | null, variant?: VariantFileInfo | null): void;
  enrich(extra: ExifData): void;
}

export function createSelectedFileMetadata(load: (path: string) => Promise<FileExif>): SelectedFileMetadata {
  const [path, setPath] = createSignal<string | null>(null);
  const [file, setFile] = createSignal<FileExif | null>(null);
  const [fallback, setFallback] = createSignal<ExifData | null>(null);
  const [override, setOverride] = createSignal<VariantFileInfo | null>(null);
  const [settled, setSettled] = createSignal<{ path: string; file: FileExif | null } | null>(null);
  let revision = 0;

  /*
   * **进程内缓存**（崔总 2026-10-11）：选中的信息是「点中才拿」的，但来回点同几张
   * 不该重复读盘 —— 对 RAW 尤其要紧：EXIF 里没有镜头名时要问 RAW worker（实测冷启
   * 动那一次 ~0.9 s，**同一张再来一次就白等**）。只存成功的，失败不记（下次重试）。
   */
  const cache = new Map<string, FileExif>();
  const EXIF_CACHE_LIMIT = 32;
  const loadOnce = async (nextPath: string): Promise<FileExif> => {
    const hit = cache.get(nextPath);
    if (hit !== undefined) {
      cache.delete(nextPath);
      cache.set(nextPath, hit); // LRU：命中挪到队尾
      return hit;
    }
    const value = await load(nextPath);
    cache.set(nextPath, value);
    while (cache.size > EXIF_CACHE_LIMIT) {
      const oldest = cache.keys().next().value;
      if (oldest === undefined) break;
      cache.delete(oldest);
    }
    return value;
  };

  const select = (
    nextPath: string | null,
    nextFallback: ExifData | null = null,
    nextVariant: VariantFileInfo | null = null,
  ): void => {
    const current = ++revision;
    batch(() => {
      setSettled(null);
      setPath(nextPath);
      setFile(null);
      setFallback(nextFallback);
      setOverride(nextVariant);
    });
    if (nextPath === null) return;
    const finish = (result: FileExif | null): void => {
      if (current !== revision) return;
      batch(() => { setFile(result); setSettled({ path: nextPath, file: result }); });
    };
    void loadOnce(nextPath).then(
      finish,
      () => finish(null),
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
    settled,
    data: () => {
      const base = fallback();
      const loaded = file();
      const variant = override();
      let merged: ExifData | null = base;
      if (loaded !== null) {
        const fresh = toExifData(loaded);
        if (base === null) {
          merged = fresh;
        } else {
          merged = { ...base };
          for (const [key, value] of Object.entries(fresh)) {
            if (value !== undefined) Object.assign(merged, { [key]: value });
          }
        }
      }
      /*
       * 显示变体的覆盖**最后上**（优先级最高）：
       *   * 尺寸 —— 文件里读到的 EXIF 尺寸说的是源文件，而这里要的是「现在这个变体多大」；
       *   * 文件类型 —— 来源基底是 RAW 的变体（RAW / RAW 基底的 latest 与定稿）
       *     一律显示 `RAW`，哪怕基准位图是 JPEG（崔总 2026-10-11 定）；
       *     SOOC 基底不动：位图自己的格式（JPEG/HEIF…）就是对的那个。
       */
      if (variant === null) return merged;
      const shown: ExifData = {
        ...(merged ?? {}),
        widthPx: variant.width,
        heightPx: variant.height,
      };
      if (variant.base === "raw") shown.format = RAW_FORMAT_LABEL;
      return shown;
    },
    select,
    enrich,
  };
}
