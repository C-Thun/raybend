//! 日志用的时间格式化：**自研、零依赖、可单测**（`specs/logging-and-crash-report.md` §4.3）。
//!
//! 为什么不引 `chrono` / `time`：这里只需要「UTC ISO8601 毫秒」与「本地偏移」两件事，
//! 前者是纯日历算术（下面 40 行），后者一次平台调用。为这两件事拉一棵依赖树不划算，
//! 而且格式要**定长 24 字符**（人读 + 机读），自研才能钉死。
//!
//! 日历算法用 Howard Hinnant 的 `civil_from_days`（公有领域，
//! 见 https://howardhinnant.github.io/date_algorithms.html ）—— 它把「自 epoch 起的天数」
//! 换成公历年月日，闰年规则内建，不查表。

use std::time::{SystemTime, UNIX_EPOCH};

/// UTC ISO8601（毫秒，定长 24 字符）：`2026-10-10T22:41:03.184Z`。
///
/// 1970 年之前的时间（时钟被调到过去、或设备时间未同步）返回 `1970-01-01T00:00:00.000Z`
/// —— 日志的立场是「绝不因为时间戳而 panic 或丢行」。
#[must_use]
pub fn utc_iso_millis(at: SystemTime) -> String {
    let Ok(since) = at.duration_since(UNIX_EPOCH) else {
        return "1970-01-01T00:00:00.000Z".to_string();
    };
    let secs = since.as_secs();
    let millis = since.subsec_millis();
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    format_utc_parts(days, rem, millis)
}

/// 本地时间 ISO8601（到秒，带偏移）：`2026-10-12T01:33:08+08:00`（会话头用）。
///
/// `offset_minutes` 是「本地 = UTC + offset」（东八区 = +480）。
#[must_use]
pub fn local_iso_seconds(at: SystemTime, offset_minutes: i32) -> String {
    let Ok(since) = at.duration_since(UNIX_EPOCH) else {
        return "1970-01-01T00:00:00+00:00".to_string();
    };
    let shifted = since.as_secs() as i64 + i64::from(offset_minutes) * 60;
    let days = shifted.div_euclid(86_400);
    let rem = shifted.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let sign = if offset_minutes < 0 { '-' } else { '+' };
    let abs = offset_minutes.unsigned_abs();
    format!(
        "{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}{sign}{:02}:{:02}",
        abs / 60,
        abs % 60
    )
}

/// ISO8601 里 `+08:00` 那一段（偏移未知时给 `+00:00`）。
#[must_use]
pub fn offset_suffix(offset_minutes: Option<i32>) -> String {
    let minutes = offset_minutes.unwrap_or(0);
    let sign = if minutes < 0 { '-' } else { '+' };
    let abs = minutes.unsigned_abs();
    format!("{sign}{:02}:{:02}", abs / 60, abs % 60)
}

fn format_utc_parts(days: i64, rem_secs: u64, millis: u32) -> String {
    let (y, m, d) = civil_from_days(days);
    let (h, mi, s) = (rem_secs / 3600, (rem_secs % 3600) / 60, rem_secs % 60);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{millis:03}Z")
}

/// 「自 epoch 起的天数」→（年，月，日）。算法：Howard Hinnant `civil_from_days`。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    // 把 epoch 挪到 0000-03-01（3 月为年首，闰日落在年末，闰年规则变得整齐）
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // 当年在 400 年周期里的第几天 [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]，3 月 = 0
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 本地时区偏移（分钟，本地 = UTC + offset）。拿不到返回 `None`。
///
/// **已知局限**：Windows 只取标准偏移（`GetTimeZoneInformation` 的 `Bias`），
/// 不处理夏令时（中国无夏令时；真要精确可后续换 `GetTimeZoneInformationForYear`）。
/// 这也是本函数只在**会话头**露脸、不进每行日志的原因 —— 每行日志一律 UTC（定长、可排序）。
#[must_use]
pub fn local_offset_minutes() -> Option<i32> {
    platform::local_offset_minutes()
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};

    pub(super) fn local_offset_minutes() -> Option<i32> {
        let mut tz: TIME_ZONE_INFORMATION = unsafe {
            // SAFETY: TIME_ZONE_INFORMATION 是纯 POD（整数 + 定长 UTF-16 数组），全零合法。
            std::mem::zeroed()
        };
        // 返回 TIME_ZONE_ID_INVALID(0xFFFF_FFFF) 表示失败（常量：TIME_ZONE_ID_INVALID）
        let result = unsafe {
            // SAFETY: 传入的是本函数栈上的有效可写指针，调用期间不别名。
            GetTimeZoneInformation(&mut tz)
        };
        if result == u32::MAX {
            return None;
        }
        // Bias 的语义是「UTC = 本地 + Bias」，我们要的是相反方向（且单位是分钟）
        Some(-tz.Bias)
    }
}

#[cfg(not(windows))]
mod platform {
    /// 非 Windows（WSL 下的 Linux 构建、CI）：不猜时区，交给会话头写 `+00:00`。
    pub(super) fn local_offset_minutes() -> Option<i32> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: u64, millis: u32) -> SystemTime {
        UNIX_EPOCH + Duration::new(secs, millis * 1_000_000)
    }

    #[test]
    fn utc_format_is_fixed_width_and_matches_known_instants() {
        assert_eq!(utc_iso_millis(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
        assert_eq!(utc_iso_millis(at(0, 1)), "1970-01-01T00:00:00.001Z");
        // 2026-10-10T22:41:03.184Z（规格里的样例时刻）
        assert_eq!(utc_iso_millis(at(1_791_672_063, 184)), "2026-10-10T22:41:03.184Z");
        // 闰日：2024-02-29（2024 能被 4 整除且不是 100 的倍数）
        assert_eq!(utc_iso_millis(at(1_709_164_800, 0)), "2024-02-29T00:00:00.000Z");
        // 世纪闰年特例：2000-02-29（能被 400 整除）
        assert_eq!(utc_iso_millis(at(951_782_400, 0)), "2000-02-29T00:00:00.000Z");
        // 非闰的整百年：1900 不在我们的窗口里；这里只钉「1970 之前退化为 epoch」的降级行为
        let before_epoch = UNIX_EPOCH - Duration::from_secs(86_400);
        assert_eq!(
            utc_iso_millis(before_epoch),
            "1970-01-01T00:00:00.000Z",
            "1970 之前退化为 epoch（宁可时间不对，也不丢行）"
        );
        // 定长：所有样例都是 24 个字符
        for secs in [0_u64, 1_791_672_063, 1_709_164_800] {
            assert_eq!(utc_iso_millis(at(secs, 0)).len(), 24);
        }
    }

    #[test]
    fn utc_format_handles_far_future_and_leap_boundaries() {
        // 2038 问题（32 位秒溢出的经典边界）与 2100 年（非闰年的整百年）
        assert_eq!(utc_iso_millis(at(2_147_484_000, 0)), "2038-01-19T03:20:00.000Z");
        // 2100-03-01：2100 不是闰年 → 2 月 28 天
        assert_eq!(utc_iso_millis(at(4_107_542_400, 0)), "2100-03-01T00:00:00.000Z");
    }

    #[test]
    fn local_format_applies_offset_both_directions() {
        // 2026-10-12T01:33:08+08:00 = 2026-10-11T17:33:08Z
        assert_eq!(
            local_iso_seconds(at(1_791_739_988, 0), 480),
            "2026-10-12T01:33:08+08:00"
        );
        // 西五区：同一时刻是前一天
        assert_eq!(
            local_iso_seconds(at(1_791_739_988, 0), -300),
            "2026-10-11T12:33:08-05:00"
        );
        // 偏移未知 → 按 UTC 写 +00:00
        assert_eq!(
            local_iso_seconds(at(1_791_739_988, 0), 0),
            "2026-10-11T17:33:08+00:00"
        );
        assert_eq!(offset_suffix(Some(480)), "+08:00");
        assert_eq!(offset_suffix(Some(-300)), "-05:00");
        assert_eq!(offset_suffix(None), "+00:00");
    }

    #[test]
    fn day_arithmetic_crosses_month_and_year_rollovers() {
        // 2026-12-31T23:59:59Z → 第二天是 2027-01-01
        assert_eq!(utc_iso_millis(at(1_798_761_599, 0)), "2026-12-31T23:59:59.000Z");
        assert_eq!(utc_iso_millis(at(1_798_761_600, 0)), "2027-01-01T00:00:00.000Z");
    }
}
