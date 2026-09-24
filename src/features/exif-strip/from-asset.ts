/**
 * 库里的照片（`src/api/types.ts` 的 `AssetItem`）→ 界面形状的 `ExifData`。
 *
 * 为什么不复用 `readFileExif` 那次 IPC：**列表项本来就带着这些字段** ——
 * 浏览网格要拿它们排序、筛选、显示，所以数据已经在手上，再问一次后端纯属浪费
 * （而这正是「本地应用实时性优先」的一个侧面：能立刻给出就别绕一圈）。
 *
 * 显示规则（机型合成 / 毫秒→秒 / 格式显示名）**不在**这里重写：先把 `AssetItem`
 * 搬成 `FileExif`，再交给 `toExifData` —— 于是导入与浏览两条路的排版必然一致。
 */

import type { AssetItem, MediaKind } from "../../api/types.ts";
import { toExifData } from "./from-file.ts";
import type { ExifData } from "./types.ts";

/** 列表项的 `isRaw` 还原成传输层的 `kind` */
function kindOf(item: AssetItem): MediaKind {
  return item.isRaw ? "raw" : "image";
}

export function assetItemExif(item: AssetItem): ExifData {
  return toExifData({
    cameraMake: item.cameraMake,
    cameraModel: item.cameraModel,
    lens: item.lens,
    focalMm: item.focalMm,
    fNumber: item.fNumber,
    exposureMs: item.exposureMs,
    // 库里不存曝光补偿（EXIF 是文件的属性，不进库）；需要时走 `readFileExif` 按需读
    exposureBiasEv: null,
    iso: item.iso,
    width: item.width,
    height: item.height,
    orientation: item.orientation,
    // 下面几项 flowbar 不显示，但 `FileExif` 要求齐全 —— 照实搬运，没有就 null
    takenAtMs: item.takenAt,
    takenAtSource: null,
    takenAtOffsetMin: item.takenAtOffsetMin,
    ext: item.ext,
    kind: kindOf(item),
  });
}
