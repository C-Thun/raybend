//! 时间工具：**库里一律存 Unix 毫秒（`i64`）**，格式化用自带算法（不引 `chrono`）。
//!
//! 为什么不引日期库：我们只需要「拿当前时间」与「格式化成可读串」两件事，
//! 而这两件事的复杂度都被下面的 `civil_from_days` 吃掉了（Howard Hinnant 的算法，
//! 1970 前后都正确，无需时区数据库）。真正复杂的时区/本地化展示交给前端 JS。
//!
//! **一律 UTC**：快照文件名、日志时间戳都用 UTC，避免同一台机器跨时区时文件名顺序错乱。
//! 用户看到的拍摄时间会在前端按本地时区展示（那是另一层的事）。

use std::time::{SystemTime, UNIX_EPOCH};

/// 当前时间的 Unix 毫秒。
///
/// 系统时钟早于 1970 时返回 0（而不是 panic）—— 这种机器上的时间是错的，
/// 但库仍然要能用。
#[must_use]
pub fn now_millis() -> i64 {
    from_system_time(SystemTime::now())
}

/// `SystemTime` → Unix 毫秒。
#[must_use]
pub fn from_system_time(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_millis()).unwrap_or(i64::MAX),
    }
}

/// Unix 毫秒 → `(年, 月, 日, 时, 分, 秒)`（UTC）。
#[must_use]
pub fn civil(millis: i64) -> (i64, u32, u32, u32, u32, u32) {
    // 向下取整的除法（毫秒可能为负，Rust 的 / 是截断除法，必须自己修正）
    let secs = millis.div_euclid(1000);
    let ms = millis.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let (hh, mm, ss) = (
        (sod / 3600) as u32,
        ((sod % 3600) / 60) as u32,
        (sod % 60) as u32,
    );
    let _ = ms;
    (y, m, d, hh, mm, ss)
}

/// 天数（自 1970-01-01）→ `(年, 月, 日)`。
///
/// 算法出处：Howard Hinnant, *chrono-Compatible Low-Level Date Algorithms*。
/// 以 400 年为一个 era（146097 天），era 内部再用 4 年/1 年/月的三段线性近似。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]，3 月为 0
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// `20260915-143002Z`（UTC，定宽 → 字典序即时间序，用于快照文件名）。
#[must_use]
pub fn format_compact(millis: i64) -> String {
    let (y, mo, d, h, mi, s) = civil(millis);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}Z")
}

/// `2026-09-15 14:30:02 UTC`（给人看的）。
#[must_use]
pub fn format_human(millis: i64) -> String {
    let (y, mo, d, h, mi, s) = civil(millis);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02} UTC")
}

/// 由年月日（公历）算「距 1970-01-01 的天数」。Howard Hinnant 的算法。
///
/// 公开是因为导入模版的 ISO 周号（`:CWEEK`）要从它推 —— 与其再抄一份日期算法，
/// 不如让两边共用同一个（日历算法抄错的代价是跨年那几天落到错的目录里）。
#[must_use]
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = i64::from(if m > 2 { m - 3 } else { m + 9 }); // [0, 11]，3 月为 0
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// 由「墙上时间」构造 Unix 毫秒（**当作 UTC**）。`civil` 的逆函数。
///
/// 这是给 EXIF 用的：`DateTimeOriginal` 是**不带时区**的墙上时间（当地钟表读数）。
/// 超出范围的输入返回 `None`（而不是静默算出个垃圾值）——EXIF 里什么烂值都有。
/// 秒可以是 60（漏斗秒）会被夹到 59。
#[must_use]
pub fn from_civil(y: i64, mo: u32, d: u32, hh: u32, mi: u32, ss: u32) -> Option<i64> {
    if !(1..=12).contains(&mo) || d == 0 || d > 31 || hh > 23 || mi > 59 || ss > 60 {
        return None;
    }
    let days = days_from_civil(y, mo, d);
    let sod = i64::from(hh) * 3600 + i64::from(mi) * 60 + i64::from(ss.min(59));
    let ms = days * 86_400_000 + sod * 1000;
    // **日历往返校验**：`2026-02-31` 这种「格式合法、日历非法」的输入在 EXIF 与文件名里
    // 都出现过。算出来的天数会被 `civil` 还原成 3 月 3 日 —— 一比对就知道是烂值。
    // （只比年月日：秒可能是漏斗秒 60，上面已夹到 59。）
    let (cy, cmo, cd, _, _, _) = civil(ms);
    if (cy, cmo, cd) != (y, mo, d) {
        return None;
    }
    Some(ms)
}

/// 由「带时区偏移的墙上时间」构造 Unix 毫秒。
///
/// `offset_min` 是该时刻**当地的 UTC 偏移**（东八区 = `480`，西五区 = `-300`），
/// 与 EXIF 的 `OffsetTimeOriginal`（`+08:00`）对应。
#[must_use]
pub fn from_civil_with_offset(
    y: i64,
    mo: u32,
    d: u32,
    hh: u32,
    mi: u32,
    ss: u32,
    offset_min: i32,
) -> Option<i64> {
    from_civil(y, mo, d, hh, mi, ss).map(|ms| ms - i64::from(offset_min) * 60_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn epoch_and_known_instants() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        // 2001-09-09T01:46:40Z == 1_000_000_000 秒
        assert_eq!(civil(1_000_000_000_000), (2001, 9, 9, 1, 46, 40));
        // 2026-09-15T00:00:00Z
        assert_eq!(civil(1_789_430_400_000), (2026, 9, 15, 0, 0, 0));
    }

    #[test]
    fn leap_years_and_month_ends() {
        assert_eq!(civil(1_709_164_800_000), (2024, 2, 29, 0, 0, 0)); // 2024-02-29 闰日
        assert_eq!(civil(1_704_067_199_000), (2023, 12, 31, 23, 59, 59));
        assert_eq!(civil(1_704_067_200_000), (2024, 1, 1, 0, 0, 0));
        assert_eq!(civil(951_782_400_000), (2000, 2, 29, 0, 0, 0)); // 2000 是闰年（能被 400 整除）
        assert_eq!(civil(2_019_686_400_000), (2034, 1, 1, 0, 0, 0));
    }

    #[test]
    fn pre_epoch_times_are_handled() {
        // 1969-12-31T23:59:59Z
        assert_eq!(civil(-1000), (1969, 12, 31, 23, 59, 59));
        // 1969-12-31T23:59:59.999
        assert_eq!(civil(-1), (1969, 12, 31, 23, 59, 59));
        // 1900-01-01T00:00:00Z
        assert_eq!(civil(-2_208_988_800_000), (1900, 1, 1, 0, 0, 0));
    }

    #[test]
    fn sub_second_precision_does_not_shift_seconds() {
        for ms in [0, 1, 499, 500, 999] {
            let (.., s) = civil(1_700_000_000_000 + ms);
            assert_eq!(s, civil(1_700_000_000_000).5, "毫秒不该影响秒：{ms}");
        }
        // 整秒边界：同一秒内不进位，跨过去才 +1
        let base = civil(1_700_000_000_000).5;
        assert_eq!(civil(1_700_000_000_999).5, base);
        assert_eq!(civil(1_700_000_001_000).5, base + 1);
    }

    #[test]
    fn compact_format_is_sortable() {
        let a = format_compact(1_000_000_000_000);
        let b = format_compact(1_789_430_400_000);
        assert_eq!(a, "20010909-014640Z");
        assert_eq!(b, "20260915-000000Z");
        assert!(a < b, "字典序应当与时间序一致");
        assert_eq!(a.len(), b.len(), "定宽才能保证排序");
    }

    #[test]
    fn human_format_is_readable() {
        assert_eq!(format_human(1_789_430_400_000), "2026-09-15 00:00:00 UTC");
    }

    #[test]
    fn now_is_reasonable() {
        let now = now_millis();
        // 2020-01-01 之后、2150 之前（防止单位写错成秒或微秒）
        assert!(now > 1_577_836_800_000, "now={now} 太小");
        assert!(now < 5_680_252_800_000, "now={now} 太大");
        assert!(now % 1000 < 1000);
    }

    #[test]
    fn system_time_roundtrip() {
        let t = UNIX_EPOCH + Duration::from_millis(1_789_516_800_123);
        assert_eq!(from_system_time(t), 1_789_516_800_123);
        // 1970 之前
        let before = UNIX_EPOCH - Duration::from_millis(1500);
        assert_eq!(from_system_time(before), -1500);
    }

    #[test]
    fn from_civil_matches_the_epoch() {
        assert_eq!(from_civil(1970, 1, 1, 0, 0, 0), Some(0));
        assert_eq!(from_civil(1970, 1, 1, 0, 0, 1), Some(1000));
        assert_eq!(from_civil(1970, 1, 2, 0, 0, 0), Some(86_400_000));
        assert_eq!(
            from_civil(1969, 12, 31, 23, 59, 59),
            Some(-1000),
            "1970 之前"
        );
        // 闰年 2 月 29 日
        assert_eq!(from_civil(2024, 2, 29, 12, 0, 0), Some(1_709_208_000_000));
    }

    #[test]
    fn from_civil_roundtrips_with_civil() {
        // 拿真实照片里的时间戳来回跑一遍（含闰年、跨年、1970 前后）
        for ms in [
            0,
            1_789_516_800_000,
            1_709_208_000_000,
            -1,
            -2_208_988_800_000, // 1900-01-01
            4_102_444_800_000,  // 2100-01-01
        ] {
            let (y, mo, d, hh, mi, ss) = civil(ms);
            assert_eq!(
                from_civil(y, mo, d, hh, mi, ss),
                Some(ms - ms.rem_euclid(1000)),
                "{ms}"
            );
        }
    }

    #[test]
    fn from_civil_rejects_garbage() {
        // EXIF 里什么烂值都有：宁可返回 None 也不算出个垃圾时间
        assert_eq!(from_civil(2026, 0, 1, 0, 0, 0), None, "月份 0");
        assert_eq!(from_civil(2026, 13, 1, 0, 0, 0), None, "月份 13");
        assert_eq!(from_civil(2026, 1, 0, 0, 0, 0), None, "日 0");
        assert_eq!(from_civil(2026, 1, 32, 0, 0, 0), None, "日 32");
        assert_eq!(from_civil(2026, 1, 1, 24, 0, 0), None, "时 24");
        assert_eq!(from_civil(2026, 1, 1, 0, 60, 0), None, "分 60");
        assert_eq!(from_civil(2026, 1, 1, 0, 0, 61), None, "秒 61");
        // 漏斗秒 60 允许（夹到 59）
        assert_eq!(
            from_civil(2026, 1, 1, 0, 0, 60),
            from_civil(2026, 1, 1, 0, 0, 59)
        );
        // 0 年、负年份不该 panic
        assert!(from_civil(0, 1, 1, 0, 0, 0).is_some());
        assert!(from_civil(-1000, 6, 15, 12, 30, 0).is_some());
    }

    #[test]
    fn from_civil_checks_the_real_calendar() {
        // 格式合法但日历非法 —— EXIF 与文件名里都真的出现过这种值
        assert_eq!(from_civil(2026, 2, 31, 0, 0, 0), None, "2 月没有 31 日");
        assert_eq!(from_civil(2026, 2, 30, 0, 0, 0), None);
        assert_eq!(from_civil(2026, 4, 31, 0, 0, 0), None, "4 月只有 30 天");
        assert_eq!(from_civil(2026, 2, 29, 0, 0, 0), None, "2026 不是闰年");
        assert!(from_civil(2024, 2, 29, 0, 0, 0).is_some(), "2024 是闰年");
        assert!(
            from_civil(2000, 2, 29, 0, 0, 0).is_some(),
            "2000 是闰年（能被 400 整除）"
        );
        assert_eq!(
            from_civil(1900, 2, 29, 0, 0, 0),
            None,
            "1900 不是闰年（能被 100 整除）"
        );
    }

    #[test]
    fn offset_conversion_points_the_right_way() {
        // 东八区 08:00 的墙上时间 = UTC 00:00
        let utc = from_civil_with_offset(2026, 9, 16, 8, 0, 0, 480).unwrap();
        assert_eq!(utc, from_civil(2026, 9, 16, 0, 0, 0).unwrap());
        // 西五区 08:00 = UTC 13:00
        let utc = from_civil_with_offset(2026, 9, 16, 8, 0, 0, -300).unwrap();
        assert_eq!(utc, from_civil(2026, 9, 16, 13, 0, 0).unwrap());
        // 偏移为 0 时与不带偏移完全一致
        assert_eq!(
            from_civil_with_offset(2026, 9, 16, 8, 0, 0, 0),
            from_civil(2026, 9, 16, 8, 0, 0)
        );
    }
}
