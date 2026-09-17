//! 最小 TIFF / RAW 头解析：**只取方向与尺寸**。
//!
//! # 为什么要自己写这一小段
//!
//! `kamadak-exif` 只认标准 TIFF 魔数 `0x002A`（`tiff.rs` 里写死的 `TIFF_FORTY_TWO`）。
//! 而 **Panasonic 的 RW2 用的是 `IIU\0`（0x0055）**，Olympus 的 ORF 又有自己的一套 ——
//! 结果就是：**整条 RAW 的 EXIF 读取静默失败**（方向丢成默认 1、尺寸变成 0×0）。
//! 实测（2026-09-17，`/mnt/c/src/tmp/pic/P1000019.RW2`）：
//!
//! ```text
//! 元数据：0×0（已按方向换算），方向=1        ← 网格铺 tile 拿到的
//! EXIF：方向=None，宽高=None×None，时间=None  ← kamadak 直接放弃
//! ```
//!
//! 后果一是**竖拍 RAW 躺着显示**，二是 tile 比例退回占位值。
//! 这里把「TIFF 家族但魔数不是 0x2A」的几种收进来，只解析我们真正要的两个字段。
//!
//! # 范围与边界
//!
//! - 覆盖：标准 TIFF（NEF / DNG / CR2 / ARW / PEF / SRW…）、**RW2（0x0055）**、ORF（`RO`/`RS`）。
//! - **不覆盖 CR3**（ISO-BMFF 容器，EXIF 在 box 里）—— 那是 ExifTool 级的工作量，
//!   而且 CR3 的尺寸/方向可以走 rawler（worker 进程）那条路拿。
//! - 只读 IFD0 + EXIF IFD 两层，**不跟随任何其它偏移**，每一项都做边界检查：
//!   损坏文件必须返回 `None` 而不是 panic（RAW 是不可信输入，见 `AGENTS.md` §6.3）。

/// 从 TIFF/RAW 头里能拿到的信息。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TiffInfo {
    /// EXIF 方向（1–8）；没有就是 `None`
    pub orientation: Option<i64>,
    /// 宽（**未按方向换算**，与 EXIF 里的原始值一致）
    pub width: Option<i64>,
    /// 高（同上）
    pub height: Option<i64>,
}

impl TiffInfo {
    /// 三样都没拿到 —— 调用方据此当「没读出来」处理。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.orientation.is_none() && self.width.is_none() && self.height.is_none()
    }
}

/// 标准 TIFF。
const MAGIC_TIFF: u16 = 0x002a;
/// Panasonic RW2。
const MAGIC_RW2: u16 = 0x0055;
/// Olympus ORF（小端的「RO」，也是目前最常见的那一种）。
const MAGIC_ORF_RO: u16 = 0x4f52;
/// Olympus ORF（小端的「RS」）。
const MAGIC_ORF_RS: u16 = 0x5352;
/// 大端写法下的 ORF 两种。
const MAGIC_ORF_OR_BE: u16 = 0x524f;
const MAGIC_ORF_RS_BE: u16 = 0x5253;

/// 一个 IFD 最多看多少个条目（真实文件通常几十个；这是防损坏文件的护栏）。
const MAX_ENTRIES: usize = 512;

/// 解析头部。`bytes` 至少要含 8 字节的 TIFF 头；给一大段（比如前 1 MiB）也行。
#[must_use]
pub fn parse(bytes: &[u8]) -> Option<TiffInfo> {
    let order = ByteOrder::detect(bytes)?;
    let magic = order.u16(bytes, 2)?;
    if !matches!(
        magic,
        MAGIC_TIFF | MAGIC_RW2 | MAGIC_ORF_RO | MAGIC_ORF_RS | MAGIC_ORF_OR_BE | MAGIC_ORF_RS_BE
    ) {
        return None;
    }
    let ifd0 = order.u32(bytes, 4)? as usize;

    let mut info = TiffInfo::default();
    let mut exif_ifd: Option<usize> = None;

    for entry in read_ifd(bytes, order, ifd0)? {
        match entry.tag {
            0x0112 => info.orientation = entry.short_value(order),
            // 标准 TIFF 的宽高 …
            0x0100 => info.width = entry.int_value(order),
            0x0101 => info.height = entry.int_value(order),
            /*
             * …以及 RW2（Panasonic）那一套：实测 `P1000019.RW2` 的 IFD0 里
             * 宽高在 **0x0002 / 0x0003**（5264×3904），标准标签 0x0100/0x0101 根本不出现。
             * 只认标准标签的话，这张图的尺寸仍然是 0×0 —— 那就白修了。
             */
            0x0002 => info.width = info.width.or(entry.int_value(order)),
            0x0003 => info.height = info.height.or(entry.int_value(order)),
            // EXIF IFD 指针：尺寸的第二来源（`PixelXDimension` 才是裁剪后的真实尺寸）
            0x8769 => exif_ifd = entry.int_value(order).and_then(|v| usize::try_from(v).ok()),
            _ => {}
        }
    }

    // EXIF IFD 里的尺寸优先（与 `kamadak` 那条路的口径一致：PixelX/YDimension 更准）
    if let Some(offset) = exif_ifd
        && let Some(entries) = read_ifd(bytes, order, offset) {
            for entry in entries {
                match entry.tag {
                    0xa002 => info.width = entry.int_value(order).or(info.width),
                    0xa003 => info.height = entry.int_value(order).or(info.height),
                    _ => {}
                }
            }
        }

    Some(info)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ByteOrder {
    Little,
    Big,
}

impl ByteOrder {
    fn detect(bytes: &[u8]) -> Option<Self> {
        match bytes.get(0..2)? {
            b"II" => Some(Self::Little),
            b"MM" => Some(Self::Big),
            _ => None,
        }
    }

    fn u16(self, bytes: &[u8], offset: usize) -> Option<u16> {
        let raw: [u8; 2] = bytes.get(offset..offset + 2)?.try_into().ok()?;
        Some(match self {
            Self::Little => u16::from_le_bytes(raw),
            Self::Big => u16::from_be_bytes(raw),
        })
    }

    fn u32(self, bytes: &[u8], offset: usize) -> Option<u32> {
        let raw: [u8; 4] = bytes.get(offset..offset + 4)?.try_into().ok()?;
        Some(match self {
            Self::Little => u32::from_le_bytes(raw),
            Self::Big => u32::from_be_bytes(raw),
        })
    }
}

/// IFD 里的一条。`value_bytes` 是那 4 字节的**原始**值字段（可能是内联值，也可能是偏移）。
#[derive(Debug, Clone, Copy)]
struct Entry {
    tag: u16,
    field_type: u16,
    count: u32,
    value_bytes: [u8; 4],
}

impl Entry {
    /// 内联的**短整数**（SHORT=3）。类型不对就 `None`。
    ///
    /// TIFF 的规矩：值长度 ≤ 4 字节时就地存在这 4 字节里（左对齐）。
    fn short_value(self, order: ByteOrder) -> Option<i64> {
        if self.field_type != 3 || self.count == 0 {
            return None;
        }
        order.u16(&self.value_bytes, 0).map(i64::from)
    }

    /// 内联的**整数**（SHORT 或 LONG）—— 尺寸字段两种写法都见过。
    fn int_value(self, order: ByteOrder) -> Option<i64> {
        match self.field_type {
            3 => self.short_value(order),
            4 if self.count > 0 => order.u32(&self.value_bytes, 0).map(i64::from),
            _ => None,
        }
    }
}

/// 读一个 IFD 的全部条目（只看前 [`MAX_ENTRIES`] 条，越界一律放弃）。
fn read_ifd(bytes: &[u8], order: ByteOrder, offset: usize) -> Option<Vec<Entry>> {
    let count = usize::from(order.u16(bytes, offset)?);
    let count = count.min(MAX_ENTRIES);
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let base = offset.checked_add(2)?.checked_add(index.checked_mul(12)?)?;
        // 12 字节一条：tag(2) + type(2) + count(4) + value(4)
        let tag = order.u16(bytes, base)?;
        let field_type = order.u16(bytes, base + 2)?;
        let count_value = order.u32(bytes, base + 4)?;
        let value_bytes: [u8; 4] = bytes.get(base + 8..base + 12)?.try_into().ok()?;
        entries.push(Entry {
            tag,
            field_type,
            count: count_value,
            value_bytes,
        });
    }
    Some(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个最小的 TIFF：IFD0 里放给定的 (tag, type, count, value) 四项。
    /// `magic` 用来模拟 RW2 / ORF 这些非 0x2A 的魔数。
    fn build(magic: u16, big_endian: bool, entries: &[(u16, u16, u32, u32)]) -> Vec<u8> {
        let mut out = Vec::new();
        let u16b = |v: u16, out: &mut Vec<u8>| {
            out.extend_from_slice(&if big_endian { v.to_be_bytes() } else { v.to_le_bytes() });
        };
        let u32b = |v: u32, out: &mut Vec<u8>| {
            out.extend_from_slice(&if big_endian { v.to_be_bytes() } else { v.to_le_bytes() });
        };
        out.extend_from_slice(if big_endian { b"MM" } else { b"II" });
        u16b(magic, &mut out);
        u32b(8, &mut out); // IFD0 紧跟在头后面
        u16b(entries.len() as u16, &mut out);
        for (tag, field_type, count, value) in entries {
            u16b(*tag, &mut out);
            u16b(*field_type, &mut out);
            u32b(*count, &mut out);
            // 内联值：SHORT 要按字节序放在前两字节
            if *field_type == 3 {
                u16b(*value as u16, &mut out);
                out.extend_from_slice(&[0, 0]);
            } else {
                u32b(*value, &mut out);
            }
        }
        u32b(0, &mut out); // 没有下一个 IFD
        out
    }

    #[test]
    fn reads_orientation_and_size_from_standard_tiff() {
        let bytes = build(
            MAGIC_TIFF,
            false,
            &[(0x0112, 3, 1, 6), (0x0100, 3, 1, 4000), (0x0101, 3, 1, 3000)],
        );
        let info = parse(&bytes).expect("标准 TIFF 应当能解析");
        assert_eq!(info.orientation, Some(6));
        assert_eq!(info.width, Some(4000));
        assert_eq!(info.height, Some(3000));
        assert!(!info.is_empty());
    }

    #[test]
    fn reads_rw2_magic_which_kamadak_rejects() {
        // 这条就是这个模块存在的理由：RW2 的魔数是 0x0055
        let bytes = build(
            MAGIC_RW2,
            false,
            &[(0x0112, 3, 1, 8), (0x0100, 4, 1, 5184), (0x0101, 4, 1, 3888)],
        );
        let info = parse(&bytes).expect("RW2 必须能解析");
        assert_eq!(info.orientation, Some(8), "竖拍 RAW 的方向不能丢");
        assert_eq!((info.width, info.height), (Some(5184), Some(3888)));
    }

    #[test]
    fn reads_orf_magics() {
        for magic in [MAGIC_ORF_RO, MAGIC_ORF_RS] {
            let bytes = build(magic, false, &[(0x0112, 3, 1, 3)]);
            assert_eq!(parse(&bytes).map(|i| i.orientation), Some(Some(3)), "ORF 魔数 {magic:#06x}");
        }
    }

    #[test]
    fn reads_big_endian() {
        let bytes = build(MAGIC_TIFF, true, &[(0x0112, 3, 1, 5)]);
        assert_eq!(parse(&bytes).map(|i| i.orientation), Some(Some(5)));
    }

    #[test]
    fn rejects_unknown_magic_and_short_inputs() {
        let bogus = build(0x1234, false, &[(0x0112, 3, 1, 6)]);
        assert!(parse(&bogus).is_none(), "不是 TIFF 家族就别硬解析");
        assert!(parse(b"").is_none());
        assert!(parse(b"II").is_none());
        assert!(parse(b"IXX").is_none());
        assert!(parse(&[0u8; 7]).is_none());
        // PNG 头（很常见的那种「不是 TIFF」）
        assert!(parse(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]).is_none());
    }

    #[test]
    fn truncated_entry_table_does_not_panic() {
        // 声明有 5 个条目但只给 1 个：必须返回 None（或至少不 panic）
        let mut bytes = build(MAGIC_TIFF, false, &[(0x0112, 3, 1, 6)]);
        bytes.truncate(12);
        let info = parse(&bytes);
        assert!(info.is_none() || info.expect("要么 None 要么空").orientation.is_none());
    }

    #[test]
    fn absurd_offsets_do_not_panic() {
        // IFD 偏移指到天上去
        let mut bytes = build(MAGIC_TIFF, false, &[(0x0112, 3, 1, 6)]);
        bytes[4..8].copy_from_slice(&0xffff_ff00u32.to_le_bytes());
        assert!(parse(&bytes).is_none());

        // EXIF IFD 指针越界：IFD0 仍然要能读出方向
        let bytes = build(
            MAGIC_TIFF,
            false,
            &[(0x0112, 3, 1, 6), (0x8769, 4, 1, 0x00ff_ff00)],
        );
        assert_eq!(parse(&bytes).map(|i| i.orientation), Some(Some(6)));
    }

    #[test]
    fn entry_count_is_capped() {
        // 声称有 65535 个条目：不能因此分配巨大内存或长时间循环
        let mut bytes = build(MAGIC_TIFF, false, &[(0x0112, 3, 1, 6)]);
        bytes[8..10].copy_from_slice(&65535u16.to_le_bytes());
        let _ = parse(&bytes); // 不 panic 即可
    }

    #[test]
    fn wrong_type_is_ignored_not_guessed() {
        // 方向字段写成 ASCII 类型（2）：不该当数字读
        let bytes = build(MAGIC_TIFF, false, &[(0x0112, 2, 4, 0x3631_3233)]);
        assert_eq!(parse(&bytes).map(|i| i.orientation), Some(None));
    }

    #[test]
    fn empty_info_is_reported_as_empty() {
        let info = TiffInfo::default();
        assert!(info.is_empty());
        assert!(!TiffInfo {
            orientation: Some(1),
            ..Default::default()
        }
        .is_empty());
    }

    #[test]
    fn parses_a_real_rw2_when_the_sample_is_present() {
        // 样本在这台机器上（`AGENTS.md` 记的样本目录）；别的机器上跳过。
        let path = std::path::Path::new("/mnt/c/src/tmp/pic/P1000019.RW2");
        if !path.exists() {
            return;
        }
        let bytes = std::fs::read(path).expect("读样本");
        let info = parse(&bytes).expect("真 RW2 应当能解析");
        assert!(info.orientation.is_some(), "样本的方向应当读得出来：{info:?}");
        assert!(info.width.unwrap_or(0) > 0, "样本的宽度应当读得出来：{info:?}");
    }
}
