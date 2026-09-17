//! 照片的**展示用基本信息**：宽高 + EXIF 方向 —— **只读头，不解码**。
//!
//! ## 为什么单独有这个模块（2026-09-16 人类提出）
//!
//! 网格要让照片**保比例**显示，就必须在渲染前知道每张的宽高；而**方向**更是硬需求 ——
//! 竖拍照片在 EXIF 里往往是「宽 6000 高 4000 + 方向=6」，不把方向算进去，
//! **所有竖图都会被当成横图躺着显示**（这正是当时的现象）。
//!
//! ## 成本（本机实测，见 `plans/photo-meta-and-tile-display.md`）
//!
//! * 只读头：ext4 上 **0.022 ms/张**（46,000 张/秒）—— 解析成本可忽略，瓶颈只在 I/O；
//! * 对比：完整解码一张 ≈ 53–220 ms/张（我们的缩略图管线 ≈ 53 ms/张）。
//!
//! 所以「进目录时把所有条目的宽高+方向都读一遍」是划算的；真正的成本在**慢介质**上，
//! 由调用方用并行 + 渐进来化解（见 `meta_cache`）。
//!
//! ## 三种来源的取尺寸顺序
//!
//! 1. **真栅格格式**（JPEG / PNG / TIFF…）：`image::image_dimensions` —— 只读头、
//!    比 EXIF 里的尺寸字段**可信**（EXIF 的 `PixelXDimension` 在编辑过的文件上会过期）；
//! 2. **RAW**（RW2 / ORF / NEF…）：`image` 解不了 → 退回 EXIF 的尺寸字段
//!    （`ImageWidth`/`ImageLength` 或 `PixelXDimension/PixelYDimension`）——
//!    这正是 rawler 落地前的可用路径，够网格排版用；
//! 3. 都拿不到（异常文件 / 罕见的 RAW 变体）→ 返回 **0×0**（调用方按默认比例占位），
//!    **不报错**：一个读不出尺寸的文件不该让整个目录列不出来。
//!
//! 方向一律来自 EXIF（`1..8`，缺失或非法按 `1` 处理）；返回的宽高**已经应用过方向**
//! （竖图就是 `w < h`）。

use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::error::{Error, Result};
use crate::media::exif;
use crate::media::kind::{self, MediaKind};

/// 「尺寸未知」的哨兵值（`0` 而不是 `None`：调用方只关心「有没有一个能用的比例」）。
pub const UNKNOWN_EDGE: u32 = 0;

/// 一张照片的展示用基本信息。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PhotoMeta {
    /// 宽（**已应用方向**）
    pub width: u32,
    /// 高（**已应用方向**）
    pub height: u32,
    /// 原始 EXIF 方向（`1..8`）
    pub orientation: u16,
    /// 文件字节数（缓存失效判据之一）
    pub file_size: u64,
    /// 修改时间（Unix 毫秒；读不到时 `0`。缓存失效判据之一）
    pub mtime_ms: i64,
}

impl PhotoMeta {
    /// 宽高都已知（能算出一个真实比例）。
    #[must_use]
    pub const fn has_size(&self) -> bool {
        self.width > UNKNOWN_EDGE && self.height > UNKNOWN_EDGE
    }
}

/// 把任意 EXIF 值收敛成 `1..8`（非法值当 `1` = 不旋转）。
#[must_use]
pub fn normalize_orientation(raw: Option<i64>) -> u16 {
    match raw {
        Some(value) if (1..=8).contains(&value) => u16::try_from(value).unwrap_or(1),
        _ => 1,
    }
}

/// 方向是否意味着**转 90°**（5..8 这四种要交换宽高）。
#[must_use]
pub const fn orientation_swaps_axes(orientation: u16) -> bool {
    matches!(orientation, 5..=8)
}

/// 把方向应用到宽高上：竖拍（5..8）交换两个轴。
#[must_use]
pub const fn oriented_size(width: u32, height: u32, orientation: u16) -> (u32, u32) {
    if orientation_swaps_axes(orientation) {
        (height, width)
    } else {
        (width, height)
    }
}

/// 读一个文件的展示用基本信息 —— **只读头**（栅格格式不整图解码）。
///
/// 文件不存在 / 读不了 → `Err(PathNotFound)`；**文件在但读不出尺寸 → `Ok` 且宽高为 0**。
pub fn read_photo_meta(path: &Path) -> Result<PhotoMeta> {
    let metadata = std::fs::metadata(path).map_err(|_| Error::PathNotFound(path.to_path_buf()))?;
    let file_size = metadata.len();
    let mtime_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|since| i64::try_from(since.as_millis()).ok())
        .unwrap_or(0);

    /*
     * EXIF：方向必读（栅格格式的尺寸不吃它，但**摆正**要用），RAW 的尺寸也只能靠它。
     *
     * RAW 走 `read_file_raw`：RW2 / ORF 的魔数不是 TIFF 的 `0x002A`，
     * `kamadak-exif` 会**整体放弃**（实测 RW2 拿到的是方向 1 + 0×0 —— 竖拍躺着、比例退回占位）。
     * 那个入口多一层 TIFF 家族兜底（见 `media::tiff`）。
     */
    let is_raw = path
        .file_name()
        .is_some_and(|name| kind::kind_of_file(&name.to_string_lossy()) == MediaKind::Raw);
    let exif = if is_raw {
        exif::read_file_raw(path)
    } else {
        exif::read_file(path)
    };
    let orientation = normalize_orientation(exif.orientation);

    // 先试真栅格格式的头部（快、且比 EXIF 的尺寸字段可信）
    let from_header = image::image_dimensions(path).ok();

    let (raw_width, raw_height) = match from_header {
        Some((width, height)) => (width, height),
        None => (
            positive_edge(exif.width),
            positive_edge(exif.height),
        ),
    };

    let (width, height) = oriented_size(raw_width, raw_height, orientation);

    Ok(PhotoMeta {
        width,
        height,
        orientation,
        file_size,
        mtime_ms,
    })
}

/// EXIF 里的尺寸字段是 `i64` 且可能是 0 / 负数 —— 收敛成非负的 `u32`。
fn positive_edge(value: Option<i64>) -> u32 {
    match value {
        Some(value) if value > 0 => u32::try_from(value).unwrap_or(UNKNOWN_EDGE),
        _ => UNKNOWN_EDGE,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        UNKNOWN_EDGE, normalize_orientation, orientation_swaps_axes, oriented_size,
        read_photo_meta,
    };
    use std::path::Path;

    /// 造一个真 JPEG（`image` crate 写出来的是标准 JFIF，无 EXIF）
    fn write_jpeg(dir: &Path, name: &str, width: u32, height: u32) -> std::path::PathBuf {
        let path = dir.join(name);
        let image = image::RgbImage::new(width, height);
        image.save(&path).expect("写得出来");
        path
    }

    /// 造一个只带 EXIF 的**裸 TIFF**（RAW 就是这条路径）。
    ///
    /// 手写最小 TIFF：`MM` 字节序 + 一个 IFD，含 Orientation / ImageWidth / ImageLength。
    fn write_bare_tiff_with_orientation(
        dir: &Path,
        name: &str,
        orientation: u16,
        width: u32,
        height: u32,
    ) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"MM\x00\x2a"); // 大端 + 魔数
        bytes.extend_from_slice(&8u32.to_be_bytes()); // 第一个 IFD 的偏移
        let entries: [(u16, u16, u32); 3] = [
            (0x0112, 3, u32::from(orientation)), // Orientation (SHORT)
            (0x0100, 4, width),                  // ImageWidth (LONG)
            (0x0101, 4, height),                 // ImageLength (LONG)
        ];
        bytes.extend_from_slice(&(entries.len() as u16).to_be_bytes());
        for (tag, kind, value) in entries {
            bytes.extend_from_slice(&tag.to_be_bytes());
            bytes.extend_from_slice(&kind.to_be_bytes());
            bytes.extend_from_slice(&1u32.to_be_bytes()); // count
            // IFD 的值槽固定 4 字节：SHORT 占**前 2 字节**（大端），其余补零
            let mut slot = [0u8; 4];
            if kind == 3 {
                slot[..2].copy_from_slice(&(value as u16).to_be_bytes());
            } else {
                slot.copy_from_slice(&value.to_be_bytes());
            }
            bytes.extend_from_slice(&slot);
        }
        bytes.extend_from_slice(&0u32.to_be_bytes()); // 没有下一个 IFD
        std::fs::write(&path, bytes).expect("写得出来");
        path
    }

    #[test]
    fn 方向值非法时当作不旋转() {
        assert_eq!(normalize_orientation(Some(1)), 1);
        assert_eq!(normalize_orientation(Some(6)), 6);
        assert_eq!(normalize_orientation(Some(0)), 1);
        assert_eq!(normalize_orientation(Some(9)), 1);
        assert_eq!(normalize_orientation(Some(-3)), 1);
        assert_eq!(normalize_orientation(None), 1);
    }

    #[test]
    fn 只有5到8才交换宽高() {
        for orientation in 1..=8u16 {
            assert_eq!(
                orientation_swaps_axes(orientation),
                (5..=8).contains(&orientation),
                "方向 {orientation}"
            );
        }
        // 0 / 9 这种脏值按不交换处理（normalize 之后本来也不会出现）
        assert!(!orientation_swaps_axes(0));
        assert!(!orientation_swaps_axes(9));
    }

    #[test]
    fn 应用方向后竖图就是竖的() {
        assert_eq!(oriented_size(6000, 4000, 1), (6000, 4000));
        assert_eq!(oriented_size(6000, 4000, 3), (6000, 4000));
        assert_eq!(oriented_size(6000, 4000, 6), (4000, 6000), "方向 6 = 顺时针 90°");
        assert_eq!(oriented_size(6000, 4000, 8), (4000, 6000), "方向 8 = 逆时针 90°");
        // 未知尺寸不能因为交换变成「有尺寸」
        assert_eq!(oriented_size(0, 0, 6), (0, 0));
    }

    #[test]
    fn 读jpeg拿到真实宽高与尺寸外的信息() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_jpeg(dir.path(), "a.jpg", 40, 20);
        let meta = read_photo_meta(&path).unwrap();
        assert_eq!((meta.width, meta.height), (40, 20));
        assert_eq!(meta.orientation, 1, "没有 EXIF 就是不移正");
        assert!(meta.has_size());
        assert!(meta.file_size > 0, "文件大小要带上（缓存失效要用）");
        assert!(meta.mtime_ms > 0, "修改时间要带上（缓存失效要用）");
    }

    #[test]
    fn 读png同样只读头() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        image::RgbImage::new(15, 30).save(&path).unwrap();
        let meta = read_photo_meta(&path).unwrap();
        assert_eq!((meta.width, meta.height), (15, 30));
        assert_eq!(meta.orientation, 1, "PNG 没有方向概念");
    }

    #[test]
    fn raw类文件靠exif拿尺寸_并且应用方向() {
        let dir = tempfile::tempdir().unwrap();
        // 竖拍：存储是 6000×4000，方向 6 → 展示应当是 4000×6000
        let path = write_bare_tiff_with_orientation(dir.path(), "a.rw2", 6, 6000, 4000);
        let meta = read_photo_meta(&path).unwrap();
        assert_eq!(meta.orientation, 6);
        assert_eq!(
            (meta.width, meta.height),
            (4000, 6000),
            "RAW 也要按方向摆正，否则竖图全躺下"
        );
    }

    #[test]
    fn 读不出尺寸的文件返回零而不是报错() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-a-photo.jpg");
        std::fs::write(&path, "这不是图片".as_bytes()).unwrap();
        let meta = read_photo_meta(&path).expect("文件在就该给结果");
        assert_eq!((meta.width, meta.height), (UNKNOWN_EDGE, UNKNOWN_EDGE));
        assert!(!meta.has_size());
        assert!(meta.file_size > 0);
    }

    #[test]
    fn 空文件也不炸() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.jpg");
        std::fs::write(&path, b"").unwrap();
        let meta = read_photo_meta(&path).unwrap();
        assert!(!meta.has_size());
        assert_eq!(meta.file_size, 0);
    }

    #[test]
    fn 文件不存在时报路径错() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("没有这个.jpg");
        assert!(read_photo_meta(&missing).is_err());
    }
}
