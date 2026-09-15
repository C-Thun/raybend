/**
 * 按时间分组与时间片（`DESIGN.md` §12.7、`design/main.md` §4.8.2）。
 *
 * ```text
 * 一级：按拍摄日期（本地日期）→ 一天一组
 * 二级：该天内按拍摄时间升序，相邻两张间隔 > 阈值 → 断为新片
 * ```
 *
 * 为什么要有时间片（用户给的理由，别简化掉）：一次导入往往包含「同一场活动的
 * 多段拍摄」（上午出门、下午回来、晚上又拍）。只按天太粗（一天可能上千张），
 * 只按张又太细。时间片正好对应「一次连续拍摄」，所以它才是**可一键选中的单位**。
 *
 * 两条硬规则：
 *   - 阈值（默认 1 小时）**必须是配置项**，不能写死在代码里；
 *   - **拍摄时间缺失**的照片归入「未知时间」组、放最后，且**不参与时间片切分**。
 *
 * 时区：库里存的是 Unix 毫秒，但「哪一天」取决于用户在哪儿 —— 所以按**本地日期**分组。
 * 测试里为了让结果确定，可以显式传 `offsetMinutes`（东八区 = 480）。
 */

/** 参与分组的照片（只要 id 与拍摄时间）。 */
export interface TimePhotoLike {
  id: string;
  takenAtMs: number | null;
}

/** 一个时间片（一次连续拍摄）。 */
export interface TimeSlice {
  /** 稳定 id（用于选中与折叠，形如 `2026-08-15 #2`） */
  id: string;
  /** 片内第一张的拍摄时间 */
  startMs: number;
  /** 片内最后一张的拍摄时间 */
  endMs: number;
  /** 片内的照片 id（按拍摄时间升序） */
  photoIds: string[];
}

/** 一天（一级分组）。 */
export interface TimeDay {
  /** `YYYY-MM-DD`（本地日期） */
  id: string;
  /** 这一天的 0 点（本地）对应的 Unix 毫秒 */
  dayStartMs: number;
  /** 这一天里的时间片（按时间升序） */
  slices: TimeSlice[];
  /** 这一天的全部照片 id（= 各片拼接；全选当天用） */
  photoIds: string[];
}

export interface TimeGrouping {
  /** 按日期**倒序**（最近的在前，与照片流的方向一致）+ 未知时间组在最后 */
  days: TimeDay[];
  /** 拍摄时间缺失的那些（不参与切分） */
  unknown: TimeSlice | null;
}

export interface TimeGroupingOptions {
  /** 时间片阈值（**分钟**）。<= 0 或非法值 → 不切分（一天一片） */
  gapMinutes: number;
  /**
   * 时区偏移（分钟，东八区 = 480）。
   * 省略时用**运行环境的本地时区**（浏览器里就是用户的时区）。
   */
  offsetMinutes?: number;
}

const MS_PER_MINUTE = 60_000;

/** 本地时区偏移（分钟，东为正），取给定时刻的偏移（能正确处理夏令时） */
function localOffsetMinutes(ms: number): number {
  // `getTimezoneOffset()` 返回「UTC − 本地」，符号与直觉相反
  return -new Date(ms).getTimezoneOffset();
}

/** 本地日期键 `YYYY-MM-DD` */
function dayKey(ms: number, offsetMinutes: number): string {
  const shifted = new Date(ms + offsetMinutes * MS_PER_MINUTE);
  const year = shifted.getUTCFullYear();
  const month = String(shifted.getUTCMonth() + 1).padStart(2, "0");
  const day = String(shifted.getUTCDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

/** 本地 0 点对应的 Unix 毫秒 */
function dayStartMs(key: string, offsetMinutes: number): number {
  const [year, month, day] = key.split("-").map(Number);
  return Date.UTC(year, month - 1, day) - offsetMinutes * MS_PER_MINUTE;
}

/**
 * 分组。
 *
 * 输入顺序**不影响结果**（内部会按时间排序）—— 这样调用方不必关心
 * 扫描顺序，也不会出现「换个排序方式分组就变了」这种诡异现象。
 */
export function groupByTime(
  photos: readonly TimePhotoLike[],
  options: TimeGroupingOptions,
): TimeGrouping {
  const gapMs =
    Number.isFinite(options.gapMinutes) && options.gapMinutes > 0
      ? options.gapMinutes * MS_PER_MINUTE
      : Number.POSITIVE_INFINITY;

  const timed: Array<{ id: string; takenAtMs: number }> = [];
  const untimed: string[] = [];
  for (const photo of photos) {
    if (photo.takenAtMs === null || !Number.isFinite(photo.takenAtMs)) {
      untimed.push(photo.id);
    } else {
      timed.push({ id: photo.id, takenAtMs: photo.takenAtMs });
    }
  }

  // 按时间升序；同一时刻的按 id 兜底，保证顺序确定（可断言）
  timed.sort((a, b) =>
    a.takenAtMs === b.takenAtMs
      ? a.id.localeCompare(b.id)
      : a.takenAtMs - b.takenAtMs,
  );

  const days = new Map<
    string,
    { photos: Array<{ id: string; takenAtMs: number }>; offset: number }
  >();

  for (const photo of timed) {
    const offset = options.offsetMinutes ?? localOffsetMinutes(photo.takenAtMs);
    const key = dayKey(photo.takenAtMs, offset);
    const bucket = days.get(key);
    if (bucket) {
      bucket.photos.push(photo);
    } else {
      days.set(key, { photos: [photo], offset });
    }
  }

  const result: TimeDay[] = [];
  for (const [key, bucket] of days) {
    const slices: TimeSlice[] = [];
    let current: Array<{ id: string; takenAtMs: number }> = [];
    const flush = (): void => {
      if (current.length === 0) return;
      slices.push({
        id: `${key} #${slices.length + 1}`,
        startMs: current[0].takenAtMs,
        endMs: current[current.length - 1].takenAtMs,
        photoIds: current.map((photo) => photo.id),
      });
      current = [];
    };
    for (const photo of bucket.photos) {
      const previous = current[current.length - 1];
      if (previous && photo.takenAtMs - previous.takenAtMs > gapMs) {
        flush();
      }
      current.push(photo);
    }
    flush();

    result.push({
      id: key,
      dayStartMs: dayStartMs(key, bucket.offset),
      slices,
      photoIds: slices.flatMap((slice) => slice.photoIds),
    });
  }

  // 最近的日期在前（与「最新的照片先看」一致）；同一天不可能重复
  result.sort((a, b) => (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));

  return {
    days: result,
    unknown:
      untimed.length > 0
        ? {
            id: "unknown",
            startMs: 0,
            endMs: 0,
            photoIds: untimed,
          }
        : null,
  };
}

/** 分组后一共有几组（天 + 未知时间算一组） */
export function groupCount(grouping: TimeGrouping): number {
  return grouping.days.length + (grouping.unknown ? 1 : 0);
}

/** 某个日组 / 时间片里有多少张 */
export function sizeOf(group: { photoIds: readonly string[] }): number {
  return group.photoIds.length;
}
