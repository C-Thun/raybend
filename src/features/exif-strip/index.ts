/**
 * `exif-strip` 模块的**唯一对外出口**（`ARCHITECTURE.md` §2）。
 *
 * 别的层（`shell/`、`workspaces/`）只从这里拿东西；模块内部的
 * `exif-format.ts` / `grouping.ts` 是私有实现，不对外暴露文件路径。
 * 这样「模块的公开面」就是一件可以 review 的事 —— 加一个导出就要想一次。
 */

export { ExifStrip } from "./ExifStrip.tsx";
export type { ExifStripProps } from "./ExifStrip.tsx";

/** 分组与空态判定：M1-3 接上真实 EXIF 后，工作区可能也需要它（例如判断是否有信息可展示） */
export { EXIF_PART_SEPARATOR, groupExif, hasExif } from "./grouping.ts";
export type { ExifGroup, ExifPart } from "./grouping.ts";

/** 数据形状：`src/api` 的 EXIF 命令返回它 */
export type { ExifData, ExifGroupId } from "./types.ts";

/** 传输形状（`src/api/types.ts` 的 `FileExif`）→ 界面形状（`ExifData`） */
export { toExifData } from "./from-file.ts";
/** 浏览侧：列表项直接转界面形状（不再走 IPC —— 字段本来就在手上） */
export { assetItemExif } from "./from-asset.ts";

/*
 * EXIF 的**显示格式**（纯函数）：`ƒ/2.8` / `1/125s` / `20.2 MP` 这类写法只允许有一处实现。
 * 2026-09-23 对外导出，因为编辑右栏的「信息」页签也要显示同一批字段 ——
 * 不给导出就会有人（包括我）在别处再抄一份格式规则。
 */
export {
  formatAperture,
  formatDimensions,
  formatExposureBias,
  formatFocalLength,
  formatIso,
  formatMegapixels,
  formatShutter,
  formatText,
} from "./exif-format.ts";
