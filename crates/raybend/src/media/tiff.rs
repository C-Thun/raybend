//! 最小 TIFF / RAW 头解析：**只取我们真正要的那几样** —— 方向、尺寸、相机、镜头、拍摄时间、曝光参数。
//!
//! # 为什么要自己写这一小段
//!
//! `kamadak-exif` 只认标准 TIFF 魔数 `0x002A`（它的 `tiff.rs` 里写死成 `TIFF_FORTY_TWO`）。
//! 而 **Panasonic 的 RW2 用的是 `IIU\0`（0x0055）**，Olympus 的 ORF 又有自己的一套 ——
//! 结果就是：**整类 RAW 的 EXIF 读取静默失败**（方向丢成默认 1、尺寸 0×0、相机/时间全空）。
//! 实测（2026-09-17，`/mnt/c/src/tmp/pic/P1000019.RW2`）：
//!
//! ```text
//! 元数据：0×0（已按方向换算），方向=1        ← 网格铺 tile 拿到的
//! EXIF：方向=None，宽高=None×None，时间=None  ← kamadak 直接放弃
//! ```
//!
//! 后果：① 竖拍 RAW 躺着显示；② tile 比例退回占位；③ **库外的 RAW 读不到任何 EXIF**
//! （导入工作流右栏与「按时间」分组都空着）。
//!
//! # 范围与边界
//!
//! - 覆盖：标准 TIFF（NEF / DNG / CR2 / ARW / PEF / SRW…）、**RW2（0x0055）**、ORF（`RO`/`RS`）。
//! - **不覆盖 CR3**（ISO-BMFF 容器，EXIF 在 box 里）—— 那是 ExifTool 级的工作量，
//!   而 CR3 的方向/尺寸可以走 rawler（worker 进程）那条路拿。
//! - **只读 IFD0 与 EXIF IFD 两层**，不跟随任何其它偏移；每一项都做边界检查：
//!   损坏文件必须返回 `None` 而不是 panic（RAW 是不可信输入，见 `AGENTS.md` §6.3）。

/// 从 TIFF/RAW 头里能拿到的信息。**全部可空** —— 相机各异，缺什么是常态。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TiffInfo {
    /// EXIF 方向（1–8）；没有就是 `None`
    pub orientation: Option<i64>,
    /// 宽（**未按方向换算**，与 EXIF 里的原始值一致）
    pub width: Option<i64>,
    /// 高（同上）
    pub height: Option<i64>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    pub software: Option<String>,
    /// 拍摄时间原样字符串：`DateTimeOriginal` 优先，退回 IFD0 的 `DateTime`
    pub datetime: Option<String>,
    /// 时区偏移原样字符串（`OffsetTimeOriginal` → `OffsetTime` → `OffsetTimeDigitized`），
    /// 形如 `+08:00` / `-05:30` / `Z`。**必须读** —— 见 `media::exif` 兜底那段的事故记录：
    /// 不读它，同一张照片的 RAW 会比 JPG 差一整个时区，按时间分组就会分成两片。
    pub offset_time: Option<String>,
    /// 快门时间（**秒**；调用方按需换算成毫秒）
    pub exposure_secs: Option<f64>,
    pub f_number: Option<f64>,
    /// **曝光补偿**（EV；SRATIONAL，负值 = 减光）。编辑右栏「与调节相关」那组要用
    /// （人类 2026-09-24：与调节关系紧的信息要优先显示）。
    pub exposure_bias_ev: Option<f64>,
    pub iso: Option<i64>,
    pub focal_mm: Option<f64>,
}

impl TiffInfo {
    /// 一样都没拿到 —— 调用方据此当「没读出来」处理。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
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

// 用到的标签（TIFF / EXIF 标准编号）
const TAG_IMAGE_WIDTH: u16 = 0x0100;
const TAG_IMAGE_LENGTH: u16 = 0x0101;
const TAG_MAKE: u16 = 0x010f;
const TAG_MODEL: u16 = 0x0110;
const TAG_ORIENTATION: u16 = 0x0112;
const TAG_SOFTWARE: u16 = 0x0131;
const TAG_DATETIME: u16 = 0x0132;
const TAG_EXPOSURE_BIAS: u16 = 0x9204;
// 时区偏移（EXIF 2.31）；三个都在 EXIF 子 IFD 里
const TAG_OFFSET_TIME: u16 = 0x9010;
const TAG_OFFSET_TIME_ORIGINAL: u16 = 0x9011;
const TAG_OFFSET_TIME_DIGITIZED: u16 = 0x9012;
/// RW2（Panasonic）把宽高放在这里，标准标签根本不出现（实测 5264×3904）
const TAG_RW2_WIDTH: u16 = 0x0002;
const TAG_RW2_HEIGHT: u16 = 0x0003;
const TAG_EXIF_IFD: u16 = 0x8769;
// EXIF IFD 里的
const TAG_EXPOSURE: u16 = 0x829a;
const TAG_FNUMBER: u16 = 0x829d;
const TAG_ISO: u16 = 0x8827;
const TAG_DATETIME_ORIGINAL: u16 = 0x9003;
const TAG_FOCAL: u16 = 0x920a;
const TAG_PIXEL_X: u16 = 0xa002;
const TAG_PIXEL_Y: u16 = 0xa003;
const TAG_LENS: u16 = 0xa434;

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
    let mut datetime_fallback: Option<String> = None;

    for entry in read_ifd(bytes, order, ifd0)? {
        match entry.tag {
            TAG_ORIENTATION => info.orientation = entry.short_value(order),
            // 标准 TIFF 的宽高 …
            TAG_IMAGE_WIDTH => info.width = entry.int_value(order),
            TAG_IMAGE_LENGTH => info.height = entry.int_value(order),
            // …以及 RW2 那一套（见上面常量注释）
            TAG_RW2_WIDTH => info.width = info.width.or(entry.int_value(order)),
            TAG_RW2_HEIGHT => info.height = info.height.or(entry.int_value(order)),
            TAG_MAKE => info.make = entry.ascii(bytes, order),
            TAG_MODEL => info.model = entry.ascii(bytes, order),
            TAG_SOFTWARE => info.software = entry.ascii(bytes, order),
            TAG_DATETIME => datetime_fallback = entry.ascii(bytes, order),
            // EXIF IFD 指针：尺寸的第二来源，也是绝大多数拍摄参数的所在地
            TAG_EXIF_IFD => exif_ifd = entry.int_value(order).and_then(|v| usize::try_from(v).ok()),
            _ => {}
        }
    }

    if let Some(offset) = exif_ifd
        && let Some(entries) = read_ifd(bytes, order, offset) {
            let mut datetime_original = None;
            // 三个偏移标签按偏好挑：Original > 通用 > Digitized（与 `exif.rs` 主路同序）
            let (mut off_original, mut off_plain, mut off_digitized) = (None, None, None);
            for entry in entries {
                match entry.tag {
                    // EXIF IFD 里的尺寸更准（是裁剪后的），优先
                    TAG_PIXEL_X => info.width = entry.int_value(order).or(info.width),
                    TAG_PIXEL_Y => info.height = entry.int_value(order).or(info.height),
                    TAG_DATETIME_ORIGINAL => datetime_original = entry.ascii(bytes, order),
                    TAG_OFFSET_TIME_ORIGINAL => off_original = entry.ascii(bytes, order),
                    TAG_OFFSET_TIME => off_plain = entry.ascii(bytes, order),
                    TAG_OFFSET_TIME_DIGITIZED => off_digitized = entry.ascii(bytes, order),
                    TAG_EXPOSURE => info.exposure_secs = entry.rational(bytes, order),
                    TAG_FNUMBER => info.f_number = entry.rational(bytes, order),
                    TAG_EXPOSURE_BIAS => info.exposure_bias_ev = entry.srational(bytes, order),
                    TAG_FOCAL => info.focal_mm = entry.rational(bytes, order),
                    TAG_ISO => info.iso = entry.int_value(order),
                    TAG_LENS => info.lens = entry.ascii(bytes, order),
                    _ => {}
                }
            }
            if let Some(original) = datetime_original {
                info.datetime = Some(original);
            }
            info.offset_time = off_original.or(off_plain).or(off_digitized);
        }
    // DateTimeOriginal 没有就退回 IFD0 的 DateTime（**只在这一处 move**，别在分支里就搬走）
    if info.datetime.is_none() {
        info.datetime = datetime_fallback;
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

    /// 有符号 32 位（SRATIONAL 的分子用）
    fn i32(self, bytes: &[u8], offset: usize) -> Option<i32> {
        let raw: [u8; 4] = bytes.get(offset..offset + 4)?.try_into().ok()?;
        Some(match self {
            Self::Little => i32::from_le_bytes(raw),
            Self::Big => i32::from_be_bytes(raw),
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

    /// 取这条的**载荷字节**：≤4 字节就内联在值字段里，否则按偏移去文件里取。
    fn data(self, bytes: &[u8], order: ByteOrder) -> Option<Vec<u8>> {
        let size = match self.field_type {
            1 | 2 | 6 | 7 => 1, // BYTE / ASCII / SBYTE / UNDEFINED
            3 | 8 => 2,         // SHORT / SSHORT
            4 | 9 | 11 => 4,    // LONG / SLONG / FLOAT
            5 | 10 | 12 => 8,   // RATIONAL / SRATIONAL / DOUBLE
            _ => return None,
        };
        let total = (self.count as usize).checked_mul(size)?;
        if total == 0 {
            return None;
        }
        if total <= 4 {
            return Some(self.value_bytes[..total].to_vec());
        }
        let offset = order.u32(&self.value_bytes, 0)? as usize;
        bytes
            .get(offset..offset.checked_add(total)?)
            .map(<[u8]>::to_vec)
    }

    /// ASCII（类型 2）：读到第一个 NUL 为止，去掉首尾空白。
    fn ascii(self, bytes: &[u8], order: ByteOrder) -> Option<String> {
        if self.field_type != 2 {
            return None;
        }
        let data = self.data(bytes, order)?;
        let end = data.iter().position(|b| *b == 0).unwrap_or(data.len());
        // 相机字符串时不时带 Latin-1 / 全角空格：`from_utf8_lossy` 永不 panic
        let text = String::from_utf8_lossy(&data[..end]).trim().to_string();
        (!text.is_empty()).then_some(text)
    }

    /// RATIONAL（类型 5）：前 8 字节是分子/分母。分母为 0 或非有限值都当没读到。
    fn rational(self, bytes: &[u8], order: ByteOrder) -> Option<f64> {
        if self.field_type != 5 || self.count == 0 {
            return None;
        }
        let data = self.data(bytes, order)?;
        let numerator = f64::from(order.u32(&data, 0)?);
        let denominator = f64::from(order.u32(&data, 4)?);
        if denominator == 0.0 {
            return None;
        }
        let value = numerator / denominator;
        value.is_finite().then_some(value)
    }

    /// SRATIONAL（类型 10）：分子是**有符号**的 —— 曝光补偿的负值（−1⅃ EV）就靠它。
    fn srational(self, bytes: &[u8], order: ByteOrder) -> Option<f64> {
        if self.field_type != 10 || self.count == 0 {
            return None;
        }
        let data = self.data(bytes, order)?;
        let numerator = f64::from(order.i32(&data, 0)?);
        let denominator = f64::from(order.u32(&data, 4)?);
        if denominator == 0.0 {
            return None;
        }
        let value = numerator / denominator;
        value.is_finite().then_some(value)
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

        /// 值形态（决定 type / count / 载荷怎么摆）。
    #[derive(Debug, Clone)]
    enum Val<'a> {
        Short(u16),
        Long(u32),
        /// ASCII：字节进数据区（`count` = 长度 + 1，带结尾 NUL）
        Ascii(&'a str),
        /// RATIONAL：8 字节进数据区
        Rational(u32, u32),
        /// SRATIONAL：分子**有符号**（曝光补偿）
        SRational(i32, u32),
    }

    /// 造一个 TIFF：**两个 IFD**（IFD0 + EXIF IFD）。
    ///
    /// 为什么要支持两个：曝光/光圈/ISO/焦距/`DateTimeOriginal` 这些标签按规范**只活在
    /// EXIF IFD 里** —— 把它们塞进 IFD0 的测试是**假测试**（解析器永远读不到，而测试还以为
    /// 是解析器的错）。第一版构造器就是这么写的，栽了一次。
    fn build(magic: u16, big_endian: bool, ifd0: &[(u16, Val)], exif: &[(u16, Val)]) -> Vec<u8> {
        let u16b = |v: u16| {
            if big_endian {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let u32b = |v: u32| {
            if big_endian {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };

        // ── 布局（先把偏移算清楚，再写字节）──
        let has_exif = !exif.is_empty();
        let ifd0_entries = ifd0.len() + usize::from(has_exif);
        let ifd0_start = 8usize;
        let ifd0_size = 2 + ifd0_entries * 12 + 4;
        let exif_start = ifd0_start + ifd0_size;
        let exif_size = if has_exif { 2 + exif.len() * 12 + 4 } else { 0 };
        let data_start = exif_start + exif_size;

        // ── 数据区 + 每条的值字段 ──
        let mut data: Vec<u8> = Vec::new();
        let mut fields: Vec<[u8; 4]> = Vec::new();
        let mut meta: Vec<(u16, u32)> = Vec::new(); // (type, count)
        let push = |val: &Val,
                        data: &mut Vec<u8>,
                        fields: &mut Vec<[u8; 4]>,
                        meta: &mut Vec<(u16, u32)>| {
            let (field_type, count, field) = match val {
                Val::Short(v) => {
                    let mut f = [0u8; 4];
                    f[..2].copy_from_slice(&u16b(*v));
                    (3u16, 1u32, f)
                }
                Val::Long(v) => (4, 1, u32b(*v)),
                Val::Ascii(text) => {
                    let offset = (data_start + data.len()) as u32;
                    data.extend_from_slice(text.as_bytes());
                    data.push(0); // TIFF 的 ASCII 必须以 NUL 结尾
                    (2, text.len() as u32 + 1, u32b(offset))
                }
                Val::Rational(num, den) => {
                    let offset = (data_start + data.len()) as u32;
                    data.extend_from_slice(&u32b(*num));
                    data.extend_from_slice(&u32b(*den));
                    (5, 1, u32b(offset))
                }
                Val::SRational(num, den) => {
                    let offset = (data_start + data.len()) as u32;
                    // 补码位型不变：`i32 as u32` 后走同一套字节序编码
                    data.extend_from_slice(&u32b(*num as u32));
                    data.extend_from_slice(&u32b(*den));
                    (10, 1, u32b(offset))
                }
            };
            meta.push((field_type, count));
            fields.push(field);
        };
        for (_, val) in ifd0 {
            push(val, &mut data, &mut fields, &mut meta);
        }
        if has_exif {
            // 指针条目的值后填（那时才知道 exif_start）
            fields.push(u32b(exif_start as u32));
            meta.push((4, 1));
        }
        let ifd0_field_count = fields.len();
        for (_, val) in exif {
            push(val, &mut data, &mut fields, &mut meta);
        }

        // ── 拼出来 ──
        let mut out = Vec::new();
        out.extend_from_slice(if big_endian { b"MM" } else { b"II" });
        out.extend_from_slice(&u16b(magic));
        out.extend_from_slice(&u32b(ifd0_start as u32));

        let write_ifd = |out: &mut Vec<u8>, tags: &[u16], field_slice: &[[u8; 4]], meta_slice: &[(u16, u32)]| {
            out.extend_from_slice(&u16b(tags.len() as u16));
            for (index, tag) in tags.iter().enumerate() {
                let (field_type, count) = meta_slice[index];
                out.extend_from_slice(&u16b(*tag));
                out.extend_from_slice(&u16b(field_type));
                out.extend_from_slice(&u32b(count));
                out.extend_from_slice(&field_slice[index]);
            }
            out.extend_from_slice(&u32b(0)); // 没有下一个 IFD
        };

        let mut ifd0_tags: Vec<u16> = ifd0.iter().map(|(tag, _)| *tag).collect();
        if has_exif {
            ifd0_tags.push(TAG_EXIF_IFD);
        }
        write_ifd(&mut out, &ifd0_tags, &fields[..ifd0_field_count], &meta[..ifd0_field_count]);
        if has_exif {
            let exif_tags: Vec<u16> = exif.iter().map(|(tag, _)| *tag).collect();
            write_ifd(
                &mut out,
                &exif_tags,
                &fields[ifd0_field_count..],
                &meta[ifd0_field_count..],
            );
        }
        out.extend_from_slice(&data);
        out
    }

    #[test]
    fn reads_orientation_and_size_from_standard_tiff() {
        let bytes = build(
            MAGIC_TIFF,
            false,
            &[
                (TAG_ORIENTATION, Val::Short(6)),
                (TAG_IMAGE_WIDTH, Val::Short(4000)),
                (TAG_IMAGE_LENGTH, Val::Short(3000)),
            ],
            &[],
        );
        let info = parse(&bytes).expect("标准 TIFF 应当能解析");
        assert_eq!(info.orientation, Some(6));
        assert_eq!(info.width, Some(4000));
        assert_eq!(info.height, Some(3000));
        assert!(!info.is_empty());
    }

    #[test]
    fn reads_signed_exposure_bias_from_the_exif_ifd() {
        // SRATIONAL 的分子是**负数**（−4/3 = −1⅃ EV）：解析错符号的测试当场就红
        let bytes = build(
            MAGIC_TIFF,
            false,
            &[],
            &[(TAG_EXPOSURE_BIAS, Val::SRational(-4, 3))],
        );
        let info = parse(&bytes).expect("应当能解析");
        assert!(
            (info.exposure_bias_ev.expect("要读到") - (-4.0 / 3.0)).abs() < 1e-9,
            "{:?}",
            info.exposure_bias_ev
        );
        // 0 EV 是有效值（无补偿），不许被当「没读到」滤掉
        let bytes = build(
            MAGIC_TIFF,
            false,
            &[],
            &[(TAG_EXPOSURE_BIAS, Val::SRational(0, 1))],
        );
        assert_eq!(parse(&bytes).expect("应当能解析").exposure_bias_ev, Some(0.0));
    }

    #[test]
    fn reads_offset_time_from_the_exif_ifd() {
        /*
         * 回归（2026-09-18 的事故，人类两次上报的那个现象，根因就在这条标签上）：
         * RAW 的 `OffsetTimeOriginal` 原先**根本没读**，注释还写着「TIFF 家族外层一般不带
         * OffsetTime，所以只能是 None」—— 对 Panasonic RW2 是错的：它的 EXIF 子 IFD 里
         * 明明写着 `+08:00`。
         *
         * 后果：同一张照片的 JPG 按 `+08:00` 换算、RW2 把墙上时间当 UTC，**整整差 8 小时**；
         * 按时间分组时两种格式落进不同的片，8 小时还会跨天 —— 界面上就是
         * 「同一天的分组标题出现两次、一次纯位图一次纯 RAW」。
         *
         * 真实数据（`C:\src\tmp\pic`）：142 个 RW2 里 22 个是方向 8 的竖拍，全都带这个标签。
         */
        let bytes = build(
            MAGIC_RW2,
            false,
            &[(TAG_RW2_WIDTH, Val::Short(5264)), (TAG_RW2_HEIGHT, Val::Short(3904))],
            &[
                (TAG_DATETIME_ORIGINAL, Val::Ascii("2026:09:13 00:54:28")),
                (TAG_OFFSET_TIME_ORIGINAL, Val::Ascii("+08:00")),
            ],
        );
        let info = parse(&bytes).expect("RW2 必须能解析");
        assert_eq!(info.datetime.as_deref(), Some("2026:09:13 00:54:28"));
        assert_eq!(info.offset_time.as_deref(), Some("+08:00"), "时区偏移不能丢");
    }

    #[test]
    fn offset_time_prefers_original_and_falls_back_to_plain() {
        // 偏好顺序与 `exif.rs` 主路一致：Original > 通用 > Digitized
        let both = build(
            MAGIC_RW2,
            false,
            &[],
            &[
                (TAG_OFFSET_TIME, Val::Ascii("-05:30")),
                (TAG_OFFSET_TIME_ORIGINAL, Val::Ascii("+08:00")),
            ],
        );
        assert_eq!(
            parse(&both).and_then(|i| i.offset_time).as_deref(),
            Some("+08:00"),
            "Original 优先于通用"
        );

        // 只有通用那个时也要认
        let plain = build(MAGIC_RW2, false, &[], &[(TAG_OFFSET_TIME, Val::Ascii("-05:30"))]);
        assert_eq!(
            parse(&plain).and_then(|i| i.offset_time).as_deref(),
            Some("-05:30")
        );

        // 一个都没有 → None（不许猜）
        let none = build(MAGIC_RW2, false, &[], &[]);
        assert_eq!(parse(&none).and_then(|i| i.offset_time), None);
    }

    #[test]
    fn reads_rw2_magic_and_its_own_size_tags() {
        // 这条就是这个模块存在的理由：RW2 的魔数是 0x0055，宽高在 0x0002/0x0003
        let bytes = build(
            MAGIC_RW2,
            false,
            &[
                (TAG_RW2_WIDTH, Val::Short(5264)),
                (TAG_RW2_HEIGHT, Val::Short(3904)),
                (TAG_ORIENTATION, Val::Short(8)),
            ],
            &[],
        );
        let info = parse(&bytes).expect("RW2 必须能解析");
        assert_eq!(info.orientation, Some(8), "竖拍 RAW 的方向不能丢");
        assert_eq!((info.width, info.height), (Some(5264), Some(3904)));
    }

    #[test]
    fn reads_make_model_lens_software() {
        // ASCII：**必须能读长字符串**（第一版构造器只塞得进 4 字节，测试假绿）
        let bytes = build(
            MAGIC_RW2,
            false,
            &[
                (TAG_MAKE, Val::Ascii("Panasonic")),
                (TAG_MODEL, Val::Ascii("DC-S5M2")),
                (TAG_SOFTWARE, Val::Ascii("Ver.1.1")),
            ],
            &[(TAG_LENS, Val::Ascii("LUMIX S 20-60/F3.5-5.6"))],
        );
        let info = parse(&bytes).expect("应当能解析");
        assert_eq!(info.make.as_deref(), Some("Panasonic"));
        assert_eq!(info.model.as_deref(), Some("DC-S5M2"));
        assert_eq!(info.software.as_deref(), Some("Ver.1.1"));
        assert_eq!(info.lens.as_deref(), Some("LUMIX S 20-60/F3.5-5.6"));
    }

    #[test]
    fn reads_exposure_fnumber_focal_and_iso_from_exif_ifd() {
        // ⚠️ 这些标签按规范**只活在 EXIF IFD 里**：塞进 IFD0 的测试是假测试
        let bytes = build(
            MAGIC_RW2,
            false,
            &[],
            &[
                (TAG_EXPOSURE, Val::Rational(1, 250)),
                (TAG_FNUMBER, Val::Rational(28, 10)),
                (TAG_FOCAL, Val::Rational(50, 1)),
                (TAG_ISO, Val::Short(800)),
            ],
        );
        let info = parse(&bytes).expect("应当能解析");
        assert_eq!(info.exposure_secs, Some(1.0 / 250.0));
        assert_eq!(info.f_number, Some(2.8));
        assert_eq!(info.focal_mm, Some(50.0));
        assert_eq!(info.iso, Some(800));
    }

    #[test]
    fn datetime_original_wins_over_ifd0_datetime() {
        let bytes = build(
            MAGIC_RW2,
            false,
            &[(TAG_DATETIME, Val::Ascii("2020:01:01 00:00:00"))],
            &[(TAG_DATETIME_ORIGINAL, Val::Ascii("2026:08:15 12:34:56"))],
        );
        // 这条能过，说明 DateTimeOriginal 确实是从 EXIF IFD 里读的
        assert_eq!(
            parse(&bytes).unwrap().datetime.as_deref(),
            Some("2026:08:15 12:34:56")
        );
    }

    #[test]
    fn ifd0_datetime_is_the_fallback() {
        let bytes = build(
            MAGIC_TIFF,
            false,
            &[(TAG_DATETIME, Val::Ascii("2026:08:15 12:34:56"))],
            &[],
        );
        assert_eq!(
            parse(&bytes).unwrap().datetime.as_deref(),
            Some("2026:08:15 12:34:56")
        );
    }

    #[test]
    fn pixel_dimensions_from_exif_ifd_win_over_ifd0() {
        let bytes = build(
            MAGIC_RW2,
            false,
            &[
                (TAG_RW2_WIDTH, Val::Short(5264)),
                (TAG_RW2_HEIGHT, Val::Short(3904)),
            ],
            &[
                (TAG_PIXEL_X, Val::Long(5184)),
                (TAG_PIXEL_Y, Val::Long(3888)),
            ],
        );
        let info = parse(&bytes).expect("应当能解析");
        assert_eq!(
            (info.width, info.height),
            (Some(5184), Some(3888)),
            "裁剪后的真实尺寸优先"
        );
    }

    #[test]
    fn zero_denominator_is_not_a_number() {
        let bytes = build(MAGIC_TIFF, false, &[], &[(TAG_EXPOSURE, Val::Rational(1, 0))]);
        let info = parse(&bytes).expect("应当能解析，只是快门读不到");
        assert_eq!(info.exposure_secs, None);
    }

    #[test]
    fn reads_orf_magics() {
        for magic in [MAGIC_ORF_RO, MAGIC_ORF_RS] {
            let bytes = build(magic, false, &[(TAG_ORIENTATION, Val::Short(3))], &[]);
            assert_eq!(
                parse(&bytes).map(|i| i.orientation),
                Some(Some(3)),
                "ORF 魔数 {magic:#06x}"
            );
        }
    }

    #[test]
    fn reads_big_endian() {
        let bytes = build(MAGIC_TIFF, true, &[(TAG_ORIENTATION, Val::Short(5))], &[]);
        assert_eq!(parse(&bytes).map(|i| i.orientation), Some(Some(5)));
    }

    #[test]
    fn rejects_unknown_magic_and_short_inputs() {
        let bogus = build(0x1234, false, &[(TAG_ORIENTATION, Val::Short(6))], &[]);
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
        let mut bytes = build(MAGIC_TIFF, false, &[(TAG_ORIENTATION, Val::Short(6))], &[]);
        bytes.truncate(12);
        let info = parse(&bytes);
        assert!(info.is_none() || info.expect("要么 None 要么空").orientation.is_none());
    }

    #[test]
    fn absurd_ifd0_offset_does_not_panic() {
        let mut bytes = build(MAGIC_TIFF, false, &[(TAG_ORIENTATION, Val::Short(6))], &[]);
        bytes[4..8].copy_from_slice(&0xffff_ff00u32.to_le_bytes());
        assert!(parse(&bytes).is_none());
    }

    #[test]
    fn absurd_exif_ifd_pointer_still_reads_ifd0() {
        // EXIF IFD 指针越界：IFD0 的字段仍然要读得到
        let bytes = build(
            MAGIC_TIFF,
            false,
            &[
                (TAG_ORIENTATION, Val::Short(6)),
                (TAG_EXIF_IFD, Val::Long(0x00ff_ff00)),
            ],
            &[],
        );
        assert_eq!(parse(&bytes).map(|i| i.orientation), Some(Some(6)));
    }

    #[test]
    fn ascii_offset_out_of_range_is_none_not_panic() {
        let mut bytes = build(MAGIC_TIFF, false, &[(TAG_MAKE, Val::Ascii("Panasonic"))], &[]);
        // 第一条的值字段（值区）在：头 8 + 条目表起点 2 + 8 = 18
        bytes[18..22].copy_from_slice(&0x00ff_ff00u32.to_le_bytes());
        let info = parse(&bytes).expect("整体仍应解析");
        assert_eq!(info.make, None, "越界就当没读到，而不是 panic");
    }

    #[test]
    fn entry_count_is_capped() {
        let mut bytes = build(MAGIC_TIFF, false, &[(TAG_ORIENTATION, Val::Short(6))], &[]);
        bytes[8..10].copy_from_slice(&65535u16.to_le_bytes());
        let _ = parse(&bytes); // 不 panic 即可
    }

    #[test]
    fn wrong_type_is_ignored_not_guessed() {
        // 方向字段写成 ASCII 类型：不该当数字读
        let bytes = build(MAGIC_TIFF, false, &[(TAG_ORIENTATION, Val::Ascii("abcd"))], &[]);
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
        assert!(info.model.is_some(), "样本的机身型号应当读得出来：{info:?}");
        assert!(
            info.datetime.is_some(),
            "样本的拍摄时间应当读得出来：{info:?}"
        );
    }
}
