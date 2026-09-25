/**
 * EXIF 分组（`design/main.md` §2.2）。
 *
 * **分组是内容本身的结构，不是视觉随意切分**（用户明确）：划过某一组时同组边框一起亮，
 * 用户能看出哪些信息属于一组。四组固定：
 *
 *   组 1  相机品牌 + 机型
 *   组 2  镜头
 *   组 3  焦距 + 光圈 + 快门 + 感光度
 *   组 4  尺寸 + 像素数 + 格式
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
 * 把 EXIF 数据切成四个信息组。空组被丢掉；`data` 为空则返回空数组。
 *
 * `emphasis` 的规则：机型是最关键的识别信息；只有品牌时强调品牌。
 * 品牌与机型都缺失但有镜头时强调镜头。
 */
export function groupExif(
 data: ExifData | null | undefined,
 translate: Translate = identity,
): ExifGroup[] {
 if (!data) return [];

 const make = formatText(data.cameraMake);
 const camera = cameraModelWithoutMake(make, formatText(data.camera));
 const lens = formatText(data.lens);

 const cameraParts: ExifPart[] = [];
 if (make) cameraParts.push({ text: make, emphasis: !camera });
 if (camera) cameraParts.push({ text: camera, emphasis: true });
 const lensParts: ExifPart[] = lens ? [{ text: lens, labelKey: "exif.lens", emphasis: !camera && !make }] : [];

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
  { id: "lens", parts: lensParts },
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

/** Many cameras repeat Make at the start of Model; the separate brand chip must not echo it. */
function cameraModelWithoutMake(make: string | undefined, model: string | undefined): string | undefined {
 if (!make || !model) return model;
 const firstWord = make.split(/[\s,]+/)[0];
 const prefixes = [make, firstWord].filter((value) => value.length >= 3);
 for (const prefix of prefixes) {
  if (model.localeCompare(prefix, undefined, { sensitivity: "base" }) === 0) return undefined;
  if (model.slice(0, prefix.length).toLowerCase() === prefix.toLowerCase() &&
      model[prefix.length] === " ") {
   return formatText(model.slice(prefix.length));
  }
 }
 return model;
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
