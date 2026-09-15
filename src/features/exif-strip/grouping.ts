/**
 * EXIF 分组（`design/main.md` §2.2）。
 *
 * **分组是内容本身的结构，不是视觉随意切分**（用户明确）：划过某一组时同组边框一起亮，
 * 用户能看出哪些信息属于一组。三组固定：
 *
 *   组 1  机型 + 镜头
 *   组 2  焦距 + 光圈 + 快门 + 感光度
 *   组 3  尺寸 + 像素数 + 格式
 *
 * 两条实现纪律：
 *   1. **空组整个不渲染**（不要出现一个只有边框、里面什么都没有的气泡）
 *   2. 分组结果里带上「复制用的文本」——`easy copy` 复制的是**整组**，
 *      并且带上字段名（`焦距 12mm · 光圈 ƒ/2.8`），否则粘到聊天窗口里没人知道
 *      那串数字是什么
 */

import type { MessageKey } from "../../i18n/index.ts";
import {
 formatAperture,
 formatDimensions,
 formatFocalLength,
 formatIso,
 formatMegapixels,
 formatShutter,
 formatText,
} from "./exif-format.ts";
import type { ExifData, ExifGroupId } from "./types.ts";

/** 组内分隔（也是显示时用的间隔点） */
export const EXIF_PART_SEPARATOR = " · ";

export interface ExifPart {
 /** 显示文本（值本身，**不含**字段名） */
 text: string;
 /** 复制文本里用的字段名；没有则不前缀 */
 labelKey?: MessageKey;
 /** 用正文色显示（只有「最关键的识别信息」配得上，见 `design/main.md` §2.2） */
 emphasis?: boolean;
}

export interface ExifGroup {
 id: ExifGroupId;
 parts: ExifPart[];
 /** `easy copy` 复制到剪贴板的整组文本 */
 copyText: string;
}

/** 取文案的函数（注入进来，便于单测；生产传 `t`） */
export type Translate = (key: MessageKey) => string;

const identity: Translate = (key) => key;

/**
 * 把 EXIF 数据切成三个信息组。空组被丢掉；`data` 为空则返回空数组。
 *
 * `emphasis` 的规则：机型是这张照片最关键的识别信息（设计稿指定用正文色）。
 * 若是**没有机型但有镜头**（截断的元数据），则把镜头提为强调项 ——
 * 否则整组都会是次要色，看着像一片灰。
 */
export function groupExif(
 data: ExifData | null | undefined,
 translate: Translate = identity,
): ExifGroup[] {
 if (!data) return [];

 const camera = formatText(data.camera);
 const lens = formatText(data.lens);

 const cameraParts: ExifPart[] = [];
 if (camera) cameraParts.push({ text: camera, emphasis: true });
 if (lens) {
  cameraParts.push({ text: lens, labelKey: "exif.lens", emphasis: !camera });
 }

 const exposureParts = dropEmpty([
  {
   text: formatFocalLength(data.focalLengthMm),
   labelKey: "exif.focal" as MessageKey,
  },
  {
   text: formatAperture(data.fNumber),
   labelKey: "exif.aperture" as MessageKey,
  },
  {
   text: formatShutter(data.exposureSeconds),
   labelKey: "exif.shutter" as MessageKey,
  },
  { text: formatIso(data.iso), labelKey: "exif.iso" as MessageKey },
 ]);

 const fileParts = dropEmpty([
  {
   text: formatDimensions(data.widthPx, data.heightPx),
   labelKey: "exif.dimensions" as MessageKey,
  },
  {
   text: formatMegapixels(data.widthPx, data.heightPx),
   labelKey: "exif.megapixels" as MessageKey,
  },
  { text: formatText(data.format), labelKey: "exif.format" as MessageKey },
 ]);

 const candidates: Array<{ id: ExifGroupId; parts: ExifPart[] }> = [
  { id: "camera", parts: cameraParts },
  { id: "exposure", parts: exposureParts },
  { id: "file", parts: fileParts },
 ];

 // 空组整个丢掉，同时把 copyText 一次算好（不再 filter + map 两趟）
 return candidates.flatMap((group) => {
  if (group.parts.length === 0) return [];
  return [
   {
    id: group.id,
    parts: group.parts,
    copyText: group.parts
     .map((part) => partText(part, translate))
     .join(EXIF_PART_SEPARATOR),
   },
  ];
 });
}

/** 单个片段在复制文本里的写法：有字段名就带上（粘出去能看懂），没有就只是值 */
function partText(part: ExifPart, translate: Translate): string {
 if (!part.labelKey) return part.text;
 return `${translate(part.labelKey)} ${part.text}`;
}

/** 有没有任何可显示的信息（决定空态） */
export function hasExif(data: ExifData | null | undefined): boolean {
 return groupExif(data).length > 0;
}

/** 丢掉 `text` 为空的项（格式化函数的「没有值」就是 undefined） */
function dropEmpty(
 parts: Array<{ text: string | undefined; labelKey?: MessageKey }>,
): ExifPart[] {
 const out: ExifPart[] = [];
 for (const part of parts) {
  if (part.text === undefined || part.text.length === 0) continue;
  out.push(
   part.labelKey
    ? { text: part.text, labelKey: part.labelKey }
    : { text: part.text },
  );
 }
 return out;
}
