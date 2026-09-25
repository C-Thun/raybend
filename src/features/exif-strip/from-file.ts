/**
 * `src/api/types.ts` 的 `FileExif` → 界面用的 `ExifData`（`design/main.md` §2.2）。
 *
 * 为什么要转换而不是让两个类型合一：传输形状是**后端的字段**（毫秒、制造厂商与型号分开、
 * 扩展名单独给），而界面形状是**给人看的**（秒、品牌与机型分组、格式显示名）。
 * 显示规则（`ƒ/2.8`、`1/125s`、`20.2 MP`）已经在 `exif-format.ts` 里，这里只做映射 ——
 * 于是「显示怎么改」不用动 Rust，「后端加字段」也不用改排版。
 */

import type { FileExif } from "../../api/types.ts";
import type { ExifData } from "./types.ts";
import { formatText } from "./exif-format.ts";

/** 格式显示名：RAW 一律显示 `RAW`（不细分 RW2/ORF…），常见位图给通用写法。 */
export function formatLabel(file: FileExif): string | undefined {
  if (file.kind === "raw") return "RAW";
  const ext = file.ext?.trim().toLowerCase();
  if (!ext) return undefined;
  switch (ext) {
    case "jpg":
    case "jpeg":
      return "JPEG";
    case "tif":
    case "tiff":
      return "TIFF";
    case "heic":
    case "heif":
      return "HEIF";
    default:
      return ext.toUpperCase();
  }
}

/**
 * 转成界面形状。
 *
 * 两个约定：
 *   * 品牌与机型分别保留；其中一个缺失时分组层仍显示另一个；
 *   * 所有「没有值」都变成 `undefined`（而不是 `null`）：`ExifStrip` 用
 *     `undefined` 判断该不该渲染这一项，两者混用会让空组跑出来。
 */
export function toExifData(file: FileExif): ExifData {
  return {
    cameraMake: formatText(file.cameraMake ?? undefined),
    camera: formatText(file.cameraModel ?? undefined),
    lens: formatText(file.lens ?? undefined),
    focalLengthMm: file.focalMm ?? undefined,
    fNumber: file.fNumber ?? undefined,
    // 后端给毫秒（1/125 秒 → 8ms），界面按秒算
    exposureSeconds:
      file.exposureMs === null || !Number.isFinite(file.exposureMs)
        ? undefined
        : file.exposureMs / 1000,
    iso: file.iso ?? undefined,
    widthPx: file.width ?? undefined,
    heightPx: file.height ?? undefined,
    format: formatLabel(file),
  };
}
