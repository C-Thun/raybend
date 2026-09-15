//! 库 ID：**可排序的定长 base62**（`REPOSITORY.md` §2.5）。
//!
//! 形制来自用户的 Luclin `Gid` 方案（时间戳 + 随机数 → 压缩成定长字符串），
//! 针对本项目做了两点裁剪：
//!
//! 1. **去掉 shard 段** —— 那是为分布式分库准备的，单机应用用不上；
//! 2. **固定 16 位**（`Gid` 默认 18 位）：40 bit 随机 + 时间戳，对「一台机器上几十个库」
//!    这个量级足够的抗碰撞能力，同时比 UUID 的 36 字符短一半。
//!
//! 结构（自左向右）：
//!
//! ```text
//! [ 时间刻度（10µs，自 SINGULARITY） | 随机 40 bit ]  → base62 → 左补 '0' 到 16 位
//! ```
//!
//! 两个重要性质（有测试守着）：
//!
//! * **可排序**：字母表按 ASCII 顺序（`0-9` < `A-Z` < `a-z`），定宽编码 ⇒
//!   字典序 = 数值序 = 时间序。按 ID 排序就是按创建时间排序。
//! * **抗碰撞**：同一毫秒内生成也不怕，靠 40 bit 随机段分开。

use crate::error::{Error, Result};

/// base62 字母表（**必须保持 ASCII 升序**，否则可排序性就没了）。
const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// 库 ID 的固定长度。
pub const ID_LEN: usize = 16;

/// 随机段占的位数（5 字节）。
const RANDOM_BITS: u32 = 40;

/// 时间刻度：每秒 10 万格（即 10µs 一格），与用户方案一致。
const TICKS_PER_SEC: u64 = 100_000;

/// 时间起点（2025-06-15 前后），与用户方案一致 —— 让刻度落在更少的位置上。
const SINGULARITY_SECS: u64 = 1_750_000_000;

/// 时间部分能表示的最大刻度数（受 16 位 base62 的容量限制）。
/// `62^16 ≈ 2^95.1`，减去 40 bit 随机段 ⇒ 时间可用约 55 bit。
const MAX_TICKS: u64 = (1 << 55) - 1;

/// 生成一个新的库 ID（当前时间）。
pub fn new_repository_id() -> Result<String> {
    new_repository_id_at(super::time::now_millis())
}

/// 按给定时间生成（测试与「补建历史库」用）。
pub fn new_repository_id_at(now_millis: i64) -> Result<String> {
    let mut random = [0u8; 5];
    getrandom::fill(&mut random).map_err(|e| Error::Random(e.to_string()))?;
    Ok(encode(now_millis, &random))
}

/// 时间 + 随机字节 → 16 位 ID。
fn encode(now_millis: i64, random: &[u8; 5]) -> String {
    let ticks = ticks_from_millis(now_millis);
    let rnd = u64::from_be_bytes([
        0, 0, 0, random[0], random[1], random[2], random[3], random[4],
    ]);
    #[allow(clippy::cast_lossless)]
    let value: u128 = (u128::from(ticks) << RANDOM_BITS) | u128::from(rnd);
    base62_fixed(value, ID_LEN)
}

/// Unix 毫秒 → 自 SINGULARITY 起的 10µs 刻度（时钟早于起点时按 0 处理）。
fn ticks_from_millis(now_millis: i64) -> u64 {
    if now_millis <= 0 {
        return 0;
    }
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let secs = (now_millis / 1000) as u64;
    let sub_ms = now_millis.rem_euclid(1000);
    let since = secs.saturating_sub(SINGULARITY_SECS);
    // 毫秒部分再换算成 10µs 刻度（1ms = 100 格）
    let ticks = since
        .saturating_mul(TICKS_PER_SEC)
        .saturating_add((sub_ms as u64).saturating_mul(100));
    ticks.min(MAX_TICKS)
}

/// 把数值写成定长 base62（左边补 `'0'`）。
fn base62_fixed(mut value: u128, len: usize) -> String {
    let mut buf = [b'0'; 32];
    let mut idx = buf.len();
    while value > 0 && idx > 0 {
        idx -= 1;
        #[allow(clippy::cast_possible_truncation)]
        let digit = (value % 62) as usize;
        buf[idx] = ALPHABET[digit];
        value /= 62;
    }
    let all = &buf[idx..];
    if all.len() >= len {
        return String::from_utf8_lossy(&all[all.len() - len..]).into_owned();
    }
    let mut out = String::with_capacity(len);
    for _ in 0..(len - all.len()) {
        out.push('0');
    }
    out.push_str(&String::from_utf8_lossy(all));
    out
}

/// 校验一个字符串是不是合法的库 ID。
#[must_use]
pub fn is_valid(id: &str) -> bool {
    id.len() == ID_LEN && id.bytes().all(|b| ALPHABET.contains(&b))
}

/// 从 ID 里解出**创建时间**（Unix 毫秒；只有诊断价值）。
///
/// 时间精度是 10µs，这里返回毫秒；解不出来（格式非法）返回 `None`。
#[must_use]
pub fn created_at_millis(id: &str) -> Option<i64> {
    if !is_valid(id) {
        return None;
    }
    let mut value: u128 = 0;
    for b in id.bytes() {
        let digit = ALPHABET.iter().position(|&a| a == b)?;
        value = value.checked_mul(62)?.checked_add(digit as u128)?;
    }
    #[allow(clippy::cast_possible_truncation)]
    let ticks = (value >> RANDOM_BITS) as u64;
    let secs = ticks / TICKS_PER_SEC;
    let sub_ticks = ticks % TICKS_PER_SEC;
    // 刻度 → 毫秒（1 刻度 = 10µs = 0.01ms）
    let millis = secs
        .saturating_add(SINGULARITY_SECS)
        .saturating_mul(1000)
        .saturating_add(sub_ticks / 100);
    i64::try_from(millis).ok()
}

/// 校验并要求「是合法库 ID」，否则报 [`Error::NotARepository`]（调用方给路径）。
pub fn require_valid(id: &str, path: &std::path::Path) -> Result<()> {
    if is_valid(id) {
        return Ok(());
    }
    Err(Error::NotARepository {
        path: path.to_path_buf(),
        reason: format!("库 ID 格式不合法：{id:?}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_have_the_expected_shape() {
        let id = new_repository_id().unwrap();
        assert_eq!(id.len(), ID_LEN);
        assert!(is_valid(&id), "生成的 ID 必须合法：{id}");
        assert!(id.bytes().all(|b| b.is_ascii_alphanumeric()));
    }

    #[test]
    fn ids_are_sortable_by_creation_time() {
        // 同一秒、跨秒、跨天都要保持「字典序 = 时间序」
        let times = [
            1_789_430_400_000_i64,
            1_789_430_400_001,
            1_789_430_401_000,
            1_789_516_800_000,
            1_900_000_000_000,
        ];
        let mut ids: Vec<String> = times
            .iter()
            .map(|t| new_repository_id_at(*t).unwrap())
            .collect();
        let sorted = {
            let mut c = ids.clone();
            c.sort();
            c
        };
        assert_eq!(ids, sorted, "ID 的字典序必须与时间序一致");
        ids.dedup();
        assert_eq!(ids.len(), times.len());
    }

    #[test]
    fn same_millisecond_is_still_unique() {
        // 同一毫秒内连续生成：全靠 40 bit 随机段分开
        let mut seen = HashSet::new();
        for _ in 0..10_000 {
            assert!(
                seen.insert(new_repository_id_at(1_789_430_400_000).unwrap()),
                "同一毫秒内出现了重复 ID"
            );
        }
    }

    #[test]
    fn concurrent_generation_never_collides() {
        let ids = std::sync::Mutex::new(HashSet::new());
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    for _ in 0..2_000 {
                        let id = new_repository_id().unwrap();
                        assert!(ids.lock().unwrap().insert(id), "并发下出现了重复 ID");
                    }
                });
            }
        });
        assert_eq!(ids.lock().unwrap().len(), 16_000);
    }

    #[test]
    fn created_at_roundtrips() {
        for t in [1_789_430_400_000_i64, 1_789_430_400_123, 2_000_000_000_000] {
            let id = new_repository_id_at(t).unwrap();
            let back = created_at_millis(&id).expect("应当能解出时间");
            // 刻度是 10µs，毫秒级必须完全一致
            assert_eq!(back, t, "id={id}");
        }
    }

    #[test]
    fn clock_before_singularity_still_works() {
        // 系统时钟跑偏（早于起点）不该 panic，也不该生成非法 ID
        let id = new_repository_id_at(1_000_000_000_000).unwrap(); // 2001 年
        assert!(is_valid(&id));
        let id = new_repository_id_at(0).unwrap();
        assert!(is_valid(&id));
        let id = new_repository_id_at(-12345).unwrap();
        assert!(is_valid(&id));
    }

    #[test]
    fn far_future_is_clamped_not_broken() {
        let id = new_repository_id_at(i64::MAX).unwrap();
        assert!(is_valid(&id), "远期时间也要给出合法 ID");
        assert_eq!(id.len(), ID_LEN);
    }

    #[test]
    fn invalid_ids_are_rejected() {
        for bad in [
            "",
            "短",
            "123456789012345",   // 15 位
            "12345678901234567", // 17 位
            "123456789012345-",  // 非法字符
            "123456789012345_",
            "中文中文中文中文中文中文中文中", // 长度虽然接近，但字符非法
            "0000000000000000 ",
        ] {
            assert!(!is_valid(bad), "不该通过：{bad:?}");
        }
        // 合法字符集内的全 '0' 是合法格式（虽然不现实）
        assert!(is_valid("0000000000000000"));
    }

    #[test]
    fn require_valid_reports_the_path() {
        let p = std::path::Path::new("/mnt/d/照片库");
        assert!(require_valid("Ab3xY9zQ1mNp7Kd2", p).is_ok());
        let err = require_valid("坏ID", p).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("照片库"), "{msg}");
        assert!(matches!(err, Error::NotARepository { .. }));
    }

    #[test]
    fn fixed_width_encoding_pads_with_zeros() {
        // 极小值也要是 16 位（不能因为数值小就短一截，否则排序会错）
        let s = base62_fixed(1, ID_LEN);
        assert_eq!(s.len(), ID_LEN);
        assert!(s.starts_with("000000000000000"));
        assert!(s.ends_with('1'));
        let zero = base62_fixed(0, ID_LEN);
        assert_eq!(zero, "0".repeat(ID_LEN));
    }

    #[test]
    fn decode_rejects_malformed_input() {
        assert!(created_at_millis("太短").is_none());
        assert!(created_at_millis("123456789012345-").is_none());
    }
}
