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

import { compareNatural } from "./natural-order.ts";

/** 参与分组的照片（只要 id、拍摄时间、时区偏移，以及排序用的名字）。 */
export interface TimePhotoLike {
  id: string;
  takenAtMs: number | null;
  /**
   * 决定**片内顺序**的名字（通常是库内相对路径或文件名）。
   *
   * 给了它就按**文件名自然序**排（`P1000019.JPG` 与 `P1000019.RW2` 挨着，
   * 见 `lib/natural-order.ts`）；不给才退回「调用方传入的顺序」。
   */
  name?: string;
  /**
   * 该张照片的时区偏移（分钟）：
   *   * 数字 → 用它（EXIF 写了 `OffsetTime*`）
   *   * `null` → 相机没写时区，毫秒本身就是墙上时间 → 按 **UTC** 分组
   *   * 省略 / `undefined` → 用 `options.offsetMinutes`，再退到本机时区
   */
  offsetMinutes?: number | null;
}

/** 一个时间片（一次连续拍摄）。 */
export interface TimeSlice {
  /** 稳定 id（用于选中与折叠，形如 `2026-08-15 #2`） */
  id: string;
  /** 片内第一张的拍摄时间 */
  startMs: number;
  /** 片内最后一张的拍摄时间 */
  endMs: number;
  /**
   * 这一片用的时区偏移（分钟）——显示时间范围时用它把毫秒还原成
   * 「相机上的数字」（见 `lib/datetime.ts`）。
   */
  offsetMinutes: number;
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
   * 兜底的时区偏移（分钟，东八区 = 480）—— 只对**没带偏移**的照片生效。
   * 省略时用**运行环境的本地时区**（浏览器里就是用户的时区）。
   */
  offsetMinutes?: number;
}

const MS_PER_MINUTE = 60_000;

/** 带上「用哪个偏移看它」的照片（分组与显示时间范围都要用） */
interface TimedPhoto {
  id: string;
  /** 相机记下的那个瞬间（Unix 毫秒，**原样**） */
  takenAtMs: number;
  offset: number;
  /**
   * **可见时刻**：`takenAtMs + offset`（把不同偏移的照片摆到同一条「墙上的钟」上）。
   *
   * 分组（哪天、哪一片）只认它，不认原始毫秒 —— 这是人类 2026-09-18 定的口径：
   * **同一个用户可见的时间，必须落进同一个篮子**。
   *
   * 为什么不用原始毫秒（2026-09-18 的实证）：同一张照片的 JPG（EXIF 带 `+08:00`）
   * 与 RW2（当时读不到 `OffsetTime*`，被当成 UTC）原始毫秒差了整整 8 小时 ——
   * 界面上显示的时刻一模一样，代码却把它们切成两片、甚至切成两天，
   * 于是同一天出现两个日期标题、一次纯位图一次纯 RAW。
   */
  visibleMs: number;
}

/** 本地时区偏移（分钟，东为正），取给定时刻的偏移（能正确处理夏令时） */
/**
 * 本机在这个时刻的时区偏移（分钟，东八区 = 480）。
 *
 * 导出给浏览网格的分组用（`features/browse/rows.ts`）—— 口径必须与导入网格一致：
 * 照片没带偏移时按**本机时区**看它的日期，而不是别的一厢情愿的默认值。
 */
export function localOffsetMinutes(ms: number): number {
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
 * **分组只看时间，片内按文件名自然序**：
 *
 * - 片的**边界**与 `startMs` / `endMs` 完全由时间决定（相邻间隔 > 阈值就断开）；
 * - 片内照片按 `name` 的**自然序**排（`lib/natural-order.ts`）；没给 `name` 时才退回
 *   调用方传入的顺序。
 *
 * 为什么片内不按时间排（2026-09-17 人类报的问题，报了两次）：同一张照片的 JPG 与 RAW，
 * 时间来源可能不同（EXIF / 文件名兜底 / mtime），秒级差异就会让 RAW 整批沉到片尾，
 * 看起来像「按格式分了两层」。
 *
 * ⚠️ 第一次只修成「保持传入顺序」，而**浏览网格**的传入顺序恰恰就是后端的时间序
 * （`ORDER BY taken_at DESC, id DESC`）—— 等于没修，问题照旧。现在明确按名字排：
 * 两个网格传进来的都可能是时间序，片内让人看到的是文件名序。
 *
 * **分组只认「可见时刻」**（人类 2026-09-18 定的口径）：`takenAtMs + offset`。
 * 只要用户看到的时刻相同，就必须落进同一个日期桶、同一片 —— 无论它们的原始毫秒
 * 与偏移怎么不同。见 `TimedPhoto::visibleMs` 里记的那次实证（JPG 与 RW2 差 8 小时）。
 */
export function groupByTime(
  photos: readonly TimePhotoLike[],
  options: TimeGroupingOptions,
): TimeGrouping {
  const gapMs =
    Number.isFinite(options.gapMinutes) && options.gapMinutes > 0
      ? options.gapMinutes * MS_PER_MINUTE
      : Number.POSITIVE_INFINITY;

  // 调用方的输入序号：片内按它排（见上面的说明）
  const inputOrder = new Map<string, number>();
  const nameOf = new Map<string, string>();
  photos.forEach((photo, index) => {
    inputOrder.set(photo.id, index);
    if (typeof photo.name === "string" && photo.name !== "") nameOf.set(photo.id, photo.name);
  });

  const timed: TimedPhoto[] = [];
  const untimed: string[] = [];
  for (const photo of photos) {
    if (photo.takenAtMs === null || !Number.isFinite(photo.takenAtMs)) {
      untimed.push(photo.id);
      continue;
    }
    // 偏移的优先级：这张自己的 → 调用方给的兜底 → 本机时区。
    // `null`（有值但为空）表示「相机没写时区」→ 按 UTC 看，不要再套本机时区。
    let offset: number;
    if (typeof photo.offsetMinutes === "number") {
      offset = photo.offsetMinutes;
    } else if (photo.offsetMinutes === null) {
      offset = 0;
    } else {
      offset = options.offsetMinutes ?? localOffsetMinutes(photo.takenAtMs);
    }
    timed.push({
      id: photo.id,
      takenAtMs: photo.takenAtMs,
      offset,
      visibleMs: photo.takenAtMs + offset * MS_PER_MINUTE,
    });
  }

  // 按**可见时刻**升序（同偏移时与按原始毫秒同序）；同一时刻的按 id 兜底，保证顺序确定
  timed.sort((a, b) =>
    a.visibleMs === b.visibleMs
      ? a.id.localeCompare(b.id)
      : a.visibleMs - b.visibleMs,
  );

  const days = new Map<string, { photos: TimedPhoto[]; offset: number }>();

  for (const photo of timed) {
    // 日期也按可见时刻算（`dayKey` 内部再加一次偏移，效果等价于对 `visibleMs` 取 UTC 日期）
    const key = dayKey(photo.takenAtMs, photo.offset);
    const bucket = days.get(key);
    if (bucket) {
      bucket.photos.push(photo);
    } else {
      days.set(key, { photos: [photo], offset: photo.offset });
    }
  }

  const result: TimeDay[] = [];
  for (const [key, bucket] of days) {
    const slices: TimeSlice[] = [];
    let current: TimedPhoto[] = [];
    const flush = (): void => {
      if (current.length === 0) return;
      // `current` 此刻是**时间序**（切分需要），先把两头的时间记下来（那是片的语义），
      // 再把片内顺序换回调用方给的顺序（那是给人看的顺序）
      const first = current[0];
      const last = current[current.length - 1];
      const ids = current.map((photo) => photo.id);
      ids.sort((a, b) => {
        // 片内：先按文件名自然序（给了名字的话），相同再用传入顺序兜底
        const na = nameOf.get(a);
        const nb = nameOf.get(b);
        if (na !== undefined && nb !== undefined) {
          const byName = compareNatural(na, nb);
          if (byName !== 0) return byName;
        }
        return (inputOrder.get(a) ?? 0) - (inputOrder.get(b) ?? 0);
      });
      slices.push({
        id: `${key} #${slices.length + 1}`,
        startMs: first.takenAtMs,
        endMs: last.takenAtMs,
        // 同一片内的偏移理论上一致（都来自同一台相机）；用第一张的即可
        offsetMinutes: first.offset,
        photoIds: ids,
      });
      current = [];
    };
    for (const photo of bucket.photos) {
      const previous = current[current.length - 1];
      // ⚠️ 用**可见时刻**差，不是原始毫秒差：用户看到的间隔 > 阈值才该断片。
      // 同偏移时两者相等（行为不变）；混合偏移时这一条正是「同一段连续拍摄被切成两片」的解药。
      if (previous && photo.visibleMs - previous.visibleMs > gapMs) {
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
            offsetMinutes: 0,
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
