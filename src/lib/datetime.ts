/**
 * 墙上时间格式化（`AGENTS.md` §7 的 EXIF 时区口径）。
 *
 * 库里存的是 Unix 毫秒，但**相机里显示的那个数字**才是用户认得的 ——
 * M1-3 定的口径是：EXIF 带 `OffsetTime*` 就按它换算成 UTC 并记下偏移；
 * 不带就把墙上时间**原样当 UTC 存**（`offset_min = NULL`）。
 *
 * 所以格式化必须分两种情况：
 *
 * | 偏移 | 怎么显示 |
 * | --- | --- |
 * | 有（分钟） | 把毫秒按该偏移平移，再用 **UTC** 格式化 —— 得到的正是相机上的数字 |
 * | 无（`null`） | 直接用 UTC 格式化（存的就是墙上时间） |
 * | 不知道（`undefined`，例如 mtime 兜底） | 用运行环境的本地时区 |
 *
 * 为什么不直接 `new Date(ms).toLocaleString()`：那会按**本机时区**显示，
 * 而本机时区与拍摄地无关 —— 用户会看到「明明是上午拍的，却显示成下午」。
 */

/**
 * 毫秒 → 墙上时间的显示串。
 *
 * `offsetMinutes`：
 *   * 数字 → 按该偏移（东八区 = 480）
 *   * `null` → 按 UTC（相机没写时区）
 *   * `undefined` → 按本机时区
 */
export function formatClock(
  ms: number,
  offsetMinutes: number | null | undefined,
  locale: string,
): string {
  if (!Number.isFinite(ms)) return "";
  // `undefined` = 不知道该用哪个时区 → 交给运行环境的本地时区
  const useLocalZone = offsetMinutes === undefined;
  // `null` = 相机没写时区，毫秒本身就是墙上时间 → 按 UTC 看
  const offset = useLocalZone ? 0 : (offsetMinutes ?? 0);
  const shifted = ms + offset * 60_000;

  const options: Intl.DateTimeFormatOptions = {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  };
  if (!useLocalZone) options.timeZone = "UTC";
  return new Intl.DateTimeFormat(locale, options).format(shifted);
}

/**
 * 时间片标题上的首尾时间（`design/main.md` §4.8.2）。
 * 首尾同一分钟时只显示一个时刻，不写「09:12–09:12」。
 */
export function formatTimeRange(
  startMs: number,
  endMs: number,
  offsetMinutes: number | null | undefined,
  locale: string,
): string {
  const start = formatClock(startMs, offsetMinutes, locale);
  const end = formatClock(endMs, offsetMinutes, locale);
  if (start === "" || end === "") return "";
  return start === end ? start : `${start}–${end}`;
}

/**
 * **日期 + 时间**（编辑右栏「信息」的拍摄时间用）。
 *
 * 时区口径与 [`formatClock`] 完全一致（同一份规则，别另立一套）——
 * 差别只在多显示日期：形如 `2026/9/13 14:23`（跟随 locale）。
 */
export function formatDateTime(
  ms: number,
  offsetMinutes: number | null | undefined,
  locale: string,
): string {
  if (!Number.isFinite(ms)) return "";
  const useLocalZone = offsetMinutes === undefined;
  const offset = useLocalZone ? 0 : (offsetMinutes ?? 0);
  const shifted = ms + offset * 60_000;

  const options: Intl.DateTimeFormatOptions = {
    year: "numeric",
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  };
  if (!useLocalZone) options.timeZone = "UTC";
  return new Intl.DateTimeFormat(locale, options).format(shifted);
}

/** `YYYY-MM-DD` 是否是我们自己产出的日期键 */
const DAY_KEY_RE = /^(\d{4})-(\d{2})-(\d{2})$/;

/**
 * 日组标题（`YYYY-MM-DD` → 本地化日期，如「2026年8月15日 周六」）。
 *
 * 认不出来的键**原样返回** —— 界面上出现 `2026-08-15` 也比出现 `Invalid Date` 强。
 */
export function formatDayLabel(dayId: string, locale: string): string {
  const match = DAY_KEY_RE.exec(dayId);
  if (!match) return dayId;
  const [, year, month, day] = match;
  const ms = Date.UTC(Number(year), Number(month) - 1, Number(day));
  return new Intl.DateTimeFormat(locale, {
    timeZone: "UTC",
    year: "numeric",
    month: "long",
    day: "numeric",
    weekday: "short",
  }).format(ms);
}
