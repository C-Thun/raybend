//! 导入前的空间预检（`plans/M1-6.md` §3.3）。
//!
//! 为什么单独成一步：导入是**不可逆地把数据搬到另一个卷上**的操作，
//! 中途写满会让用户面对一堆半成品（我们靠 `.part` 保证不出现坏文件，但那一批就是做不完）。
//! 所以开工前先问一句「目标卷够不够」，不够就**先告诉用户再决定**
//! （本机 `C:` 只剩 ~58GB 这种现实情况，不是理论风险）。
//!
//! 两件事分开：**判断是纯函数**（好测），**估字节数要扫一遍源**（只 stat，不读内容）。

use std::path::PathBuf;

use crate::error::Result;
use crate::import::fsops::Scanner;
use crate::media::scan::{Cancel, ScanOptions};

/// 余量：源总字节的百分比。
///
/// 复制过程中的临时文件、目录项、以及紧随其后的缩略图缓存都要地方 ——
/// 卡着「刚好够」开工，最后一条大概率失败。
pub const MARGIN_PERCENT: u64 = 5;

/// 预检结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceVerdict {
    /// 够用 —— 或者**不知道**（拿不到可用空间时不拦人：网盘/特殊 FS 上可能读不到）。
    Ok,
    /// 偏紧：可用空间低于「需要 + 余量」。界面该提示一下再让用户决定。
    Tight {
        /// 需要多少字节（含余量）。
        needed: u64,
        /// 目标卷可用多少。
        free: u64,
    },
}

impl SpaceVerdict {
    /// 是不是「偏紧」。
    #[must_use]
    pub fn is_tight(self) -> bool {
        matches!(self, Self::Tight { .. })
    }
}

/// 需要的字节数（含余量）。
#[must_use]
pub fn needed_with_margin(needed_bytes: u64) -> u64 {
    needed_bytes.saturating_add(needed_bytes.saturating_mul(MARGIN_PERCENT) / 100)
}

/// 判一下够不够。
///
/// `free_bytes` 为 `None`（读不到）时返回 [`SpaceVerdict::Ok`] ——
/// 「不知道」不该被当成「没空间」而拦住用户。
#[must_use]
pub fn check_space(needed_bytes: u64, free_bytes: Option<u64>) -> SpaceVerdict {
    let Some(free) = free_bytes else {
        return SpaceVerdict::Ok;
    };
    let needed = needed_with_margin(needed_bytes);
    if free < needed {
        SpaceVerdict::Tight { needed, free }
    } else {
        SpaceVerdict::Ok
    }
}

/// 轻量估一下这批源目录总共多少字节（只走目录树、只 stat，不读内容）。
///
/// 扫描参数与真正导入时一致（`include_subdirs` 决定 `max_depth`），
/// 否则估出来的数会跟实际要搬的量对不上。
pub fn estimate_bytes(
    scanner: &dyn Scanner,
    roots: &[PathBuf],
    include_subdirs: bool,
    cancel: &Cancel,
) -> Result<u64> {
    let opts = ScanOptions {
        max_depth: if include_subdirs { 32 } else { 0 },
        ..ScanOptions::default()
    };
    let mut total = 0u64;
    for root in roots {
        let mut sum = 0u64;
        let mut on_file = |file: crate::media::scan::ScannedFile| {
            sum = sum.saturating_add(file.size_bytes);
            true
        };
        scanner.walk(root, &opts, cancel, &mut on_file)?;
        total = total.saturating_add(sum);
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::fsops::MemoryFs;
    use crate::media::scan::ScannedFile;
    use std::path::Path;

    fn scanned(rel: &str, size: u64) -> ScannedFile {
        let name = rel.rsplit('/').next().unwrap_or(rel).to_string();
        ScannedFile {
            abs_path: PathBuf::from(format!("/src/{rel}")),
            rel_path: rel.to_string(),
            ext: crate::media::kind::extension(&name),
            kind: crate::media::kind::kind_of_file(&name),
            stem_folded: crate::media::kind::stem_folded(&name),
            file_name: name,
            size_bytes: size,
            mtime_ms: None,
        }
    }

    #[test]
    fn unknown_free_space_does_not_block() {
        assert_eq!(check_space(1_000_000, None), SpaceVerdict::Ok);
    }

    #[test]
    fn plenty_of_room_is_ok() {
        assert_eq!(check_space(1000, Some(10_000)), SpaceVerdict::Ok);
        // 刚好压在余量线上也算够（`free >= needed * 1.05`）
        assert_eq!(check_space(1000, Some(1050)), SpaceVerdict::Ok);
    }

    #[test]
    fn tight_space_is_reported_with_both_numbers() {
        let verdict = check_space(1000, Some(1049));
        assert!(verdict.is_tight());
        match verdict {
            SpaceVerdict::Tight { needed, free } => {
                assert_eq!(needed, 1050);
                assert_eq!(free, 1049);
            }
            SpaceVerdict::Ok => panic!("应当判为偏紧"),
        }
        // 完全不够时也一样（而且数字要能对得上提示文案）
        assert!(check_space(1_000_000, Some(1234)).is_tight());
        assert!(check_space(1000, Some(0)).is_tight());
    }

    #[test]
    fn nothing_to_copy_is_always_ok() {
        assert_eq!(check_space(0, Some(0)), SpaceVerdict::Ok, "一条都不用搬");
        assert_eq!(needed_with_margin(0), 0);
    }

    #[test]
    fn margin_does_not_overflow() {
        // 巨型数值不该 panic（saturating）
        let huge = u64::MAX / 2;
        assert!(needed_with_margin(huge) >= huge);
        assert!(needed_with_margin(u64::MAX) == u64::MAX);
    }

    #[test]
    fn estimate_bytes_sums_every_root() {
        let mut fs = MemoryFs::new();
        fs.set_scan("/src/a", vec![scanned("x.jpg", 100), scanned("y.jpg", 250)]);
        fs.set_scan("/src/b", vec![scanned("z.jpg", 1000)]);
        let cancel = Cancel::new();
        let total = estimate_bytes(
            &fs,
            &[PathBuf::from("/src/a"), PathBuf::from("/src/b")],
            true,
            &cancel,
        )
        .expect("估算");
        assert_eq!(total, 1350);
    }

    #[test]
    fn estimate_bytes_of_nothing_is_zero() {
        let fs = MemoryFs::new();
        assert_eq!(
            estimate_bytes(&fs, &[], true, &Cancel::new()).unwrap(),
            0
        );
        assert_eq!(
            estimate_bytes(&fs, &[PathBuf::from("/src/空")], true, &Cancel::new()).unwrap(),
            0
        );
    }

    #[test]
    fn estimate_bytes_stops_when_cancelled() {
        let mut fs = MemoryFs::new();
        fs.set_scan("/src/a", vec![scanned("x.jpg", 100)]);
        let cancel = Cancel::new();
        cancel.cancel();
        // 已取消 → MemoryFs 的 walk 直接返回 cancelled，一个文件都不报
        let total = estimate_bytes(&fs, &[PathBuf::from("/src/a")], true, &cancel).unwrap();
        assert_eq!(total, 0);
        let _ = Path::new("/dev/null"); // 保持 Path 的使用（有的平台没有这个文件）
    }
}
