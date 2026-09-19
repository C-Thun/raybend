/**
 * 数字格式化 —— 供 `Badge`、网格计数、进度这类地方复用。
 *
 * 为什么单独成文件：这些数字遍布全界面，格式必须**处处一致**，
 * 且 `DESIGN.md` §7.2 要求数字密集处用 tabular 数字（那是 CSS 的事），
 * 这里负责的只是**分组与单位**这一层。
 *
 * 分组的取值来自设计稿（`design/main.md` §5）：中文用**空格**分组
 * （`1 248 张`，SI 风格），英文用**逗号**（`1,248 items`）。
 * 注意这与 `Intl.NumberFormat("zh-CN")` 的默认行为（逗号）故意不同 ——
 * 设计稿明确画的是空格，若要改成 locale 标准行为，改本文件一处即可。
 */

/** 千位分组的分隔符：按语言取，而不是按系统 locale 自动判断 */
export type GroupingLocale = "zh-CN" | "en-US";

const GROUP_SEPARATOR: Record<GroupingLocale, string> = {
  "zh-CN": " ",
  "en-US": ",",
};

/**
 * 非指数形式的纯数字串。
 *
 * `String(1e21)` 是 `"1e+21"` —— 直接分组会产出 `1 e+21` 这种垃圾。
 * 这里用 `toLocaleString` 关掉分组与小数位，它会给出完整的十进制位数。
 */
function expandDigits(value: number): string {
  const plain = value.toLocaleString("en-US", {
    useGrouping: false,
    maximumFractionDigits: 0,
  });
  return plain.includes("e") ? value.toString() : plain;
}

/** 计数格式化。非有限值（NaN / ±Infinity）一律按 0 处理。 */
export function formatCount(
  value: number,
  locale: GroupingLocale = "zh-CN",
): string {
  // NaN 会一路穿到 DOM 上显示成 "NaN 张"，比显示 0 更糟（tile-flow 踩过一次同类坑）
  if (!Number.isFinite(value)) return "0";

  const rounded = Math.round(value);
  const negative = rounded < 0;
  const digits = expandDigits(Math.abs(rounded));

  if (digits.length <= 3) return (negative ? "-" : "") + digits;

  const separator = GROUP_SEPARATOR[locale] ?? " ";
  let grouped = "";
  for (let i = 0; i < digits.length; i += 1) {
    // 从右往左每 3 位插一个分隔符
    const fromEnd = digits.length - i;
    if (i > 0 && fromEnd % 3 === 0) grouped += separator;
    grouped += digits[i];
  }

  return (negative ? "-" : "") + grouped;
}

/**
 * 计数文案里 **超过上限** 的写法：`999+`。
 * 用于紧凑位置（如小徽标）—— 数字位数太多会把胶囊撑变形。
 */
export function formatCountCapped(
  value: number,
  max: number,
  locale: GroupingLocale = "zh-CN",
): string {
  if (!Number.isFinite(value)) return "0";
  const limit = Number.isFinite(max) && max > 0 ? Math.floor(max) : 0;
  if (limit === 0) return formatCount(value, locale);
  return Math.round(value) > limit
    ? `${formatCount(limit, locale)}+`
    : formatCount(value, locale);
}

/**
 * 路径的**末级名字**（文件名或最后一级目录名）。
 *
 * 用在「状态条中间那一段」这类地方：界面要的是**名字**，路径本身另有 `PathText` 负责缩写。
 * 两种分隔符都吃（Windows 的 `\` 与库内相对路径的 `/`）—— 这个仓里两种都会出现：
 * 源目录是系统路径（`C:\src\pic\2026-08-15`），库内路径统一用 `/`（`photos/2026-08-15`）。
 *
 * 边界口径：
 *   * 末尾有分隔符（`"a/b/"`）→ 取 `b`（不是空串）；
 *   * 纯分隔符（`"/"`）或空串 → 原样返回（没有「名字」可言，别硬编一个新的）；
 *   * 没有分隔符（`"P1000023.RW2"`）→ 原样返回。
 */
export function fileNameOf(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  if (trimmed === "") return path;
  const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  return cut < 0 ? trimmed : trimmed.slice(cut + 1);
}
