/**
 * EXIF 数值 → 显示文本（纯函数，可单测）。
 *
 * 规则来自 `design/main.md` §2.2 的示例：
 *   `DC-G9` · `LEICA DG 12-60mm F2.8-4.0`
 *   `12mm · ƒ/2.8 · 1/125s · ISO 200`
 *   `5184 × 3888 · 20.2 MP · RAW`
 *
 * 三条纪律：
 *   1. **不产出「未知」之类的占位**：拿不到值就返回 `undefined`，由分组逻辑整项跳过 ——
 *      界面上宁可少一项，也不要出现 `f/undefined`
 *   2. **不做单位换算之外的加工**（不猜等效焦距、不推断镜头型号）
 *   3. 非法输入（NaN / 0 / 负数）一律当「没有值」处理
 */

/** 非有限值 / 非正数一律视为「没有值」 */
function positive(value: number | undefined): number | undefined {
 if (value === undefined || !Number.isFinite(value) || value <= 0)
  return undefined;
 return value;
}

/** 去掉多余的小数位：`2.80 → 2.8`、`4.00 → 4` */
function trimNumber(value: number, digits: number): string {
 const fixed = value.toFixed(digits);
 return fixed.includes(".") ? fixed.replace(/\.?0+$/, "") : fixed;
}

export function formatFocalLength(mm: number | undefined): string | undefined {
 const value = positive(mm);
 if (value === undefined) return undefined;
 // 焦距一般取整；少量镜头（如 10.5mm 鱼眼）要保留一位
 const text = Number.isInteger(value) ? String(value) : trimNumber(value, 1);
 return `${text}mm`;
}

export function formatAperture(
 fNumber: number | undefined,
): string | undefined {
 const value = positive(fNumber);
 if (value === undefined) return undefined;
 // 设计稿用的是 `ƒ`（U+0192），不是普通的 `f`
 return `ƒ/${trimNumber(value, 1)}`;
}

/**
 * 快门：EXIF 给的是秒（0.008）。
 * 短于 1 秒写成 `1/125s`（这是摄影习惯），长于等于 1 秒写成 `2s`。
 */
export function formatShutter(seconds: number | undefined): string | undefined {
 const value = positive(seconds);
 if (value === undefined) return undefined;
 if (value >= 1) return `${trimNumber(value, 1)}s`;

 const denominator = Math.round(1 / value);
 /*
  * 上限不是拍脑袋：机械/电子快门最快到 1/32000 左右，再快的分母只可能是
  * 元数据里的垃圾值（实测有相机把「无快门」写成 1e-9）。宁可不显示，
  * 也不要在界面上出现 `1/1000000000s`。
  */
 if (!Number.isFinite(denominator) || denominator < 1) return undefined;
 if (denominator > MAX_SHUTTER_DENOMINATOR) return undefined;
 return `1/${denominator}s`;
}

/** 快门分母上限（≈1/60000s，已超过任何真实快门） */
export const MAX_SHUTTER_DENOMINATOR = 60_000;

export function formatIso(iso: number | undefined): string | undefined {
 const value = positive(iso);
 if (value === undefined) return undefined;
 return `ISO ${Math.round(value)}`;
}

/** 像素尺寸：`5184 × 3888`（乘号是 `×`，两侧留空格） */
export function formatDimensions(
 width: number | undefined,
 height: number | undefined,
): string | undefined {
 const w = positive(width);
 const h = positive(height);
 if (w === undefined || h === undefined) return undefined;
 return `${Math.round(w)} × ${Math.round(h)}`;
}

/** 像素数：`20.2 MP`（按百万像素计，保留一位小数） */
export function formatMegapixels(
 width: number | undefined,
 height: number | undefined,
): string | undefined {
 const w = positive(width);
 const h = positive(height);
 if (w === undefined || h === undefined) return undefined;
 return `${trimNumber((w * h) / 1_000_000, 1)} MP`;
}

/** 机型 / 镜头 / 格式这类文本：去掉首尾空白，空串视为没有值 */
export function formatText(value: string | undefined): string | undefined {
 if (typeof value !== "string") return undefined;
 const trimmed = value.trim();
 return trimmed.length > 0 ? trimmed : undefined;
}
