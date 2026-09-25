/**
 * EXIF 数据的形状（`design/main.md` §2.2 的图片信息区）。
 *
 * **刻意收「数值 + 单位分开」**：EXIF 抽出来的是 `focal_length: 12`、`f_number: 2.8`、
 * `exposure_time: 0.008` 这样的数，而不是给人看的字符串。把「怎么显示」留给
 * `exif-format.ts` 里的纯函数，好处有两个：
 *   1. 格式规则（`ƒ/2.8`、`1/125s`、`20.2 MP`）只有一处，且可被单元测试钉住
 *   2. 将来要改排版（例如加 35mm 等效焦距、改单位制）不用回头动 Rust 侧
 *
 * FlowBar 省略色彩空间；完整 EXIF 在右侧信息栏逐项呈现。
 */

export interface ExifData {
 /** 机型，如 `DC-G9`。通常是该条最醒目的识别信息。 */
 camera?: string;
 /** 相机厂商，显示在型号左侧，与型号共同组成一组 EasyCopy。 */
 cameraMake?: string;
 /** 镜头，如 `LEICA DG 12-60mm F2.8-4.0` */
 lens?: string;

 /** 焦距（毫米） */
 focalLengthMm?: number;
 /** 光圈 F 值，如 2.8 */
 fNumber?: number;
 /** 快门时间（秒），如 0.008（即 1/125s） */
 exposureSeconds?: number;
 /** 感光度，如 200 */
 iso?: number;

 /** 像素尺寸 */
 widthPx?: number;
 heightPx?: number;
 /** 文件格式的显示名，如 `RAW` / `NEF` / `JPEG`（由解码层给出，这里不做映射） */
 format?: string;
}

/** 四个信息组的固定标识（顺序即显示顺序，`design/main.md` §2.2） */
export type ExifGroupId = "camera" | "lens" | "exposure" | "file";
