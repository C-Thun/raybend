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
const TAG_GPS_IFD: u16 = 0x8825;
const TAG_INTEROP_IFD: u16 = 0xa005;
// EXIF IFD 里的
const TAG_EXPOSURE: u16 = 0x829a;
const TAG_FNUMBER: u16 = 0x829d;
const TAG_ISO: u16 = 0x8827;
const TAG_DATETIME_ORIGINAL: u16 = 0x9003;
const TAG_FOCAL: u16 = 0x920a;
const TAG_PIXEL_X: u16 = 0xa002;
const TAG_PIXEL_Y: u16 = 0xa003;
const TAG_LENS: u16 = 0xa434;

/// One recognition rule for TIFF and the supported RAW IFD families. A byte-order
/// prefix alone is insufficient: Canon CRW, for example, starts with II too.
fn family_order(bytes: &[u8]) -> Option<ByteOrder> {
    let order = ByteOrder::detect(bytes)?;
    matches!(
        order.u16(bytes, 2)?,
        MAGIC_TIFF | MAGIC_RW2 | MAGIC_ORF_RO | MAGIC_ORF_RS | MAGIC_ORF_OR_BE | MAGIC_ORF_RS_BE
    )
    .then_some(order)
}

pub(crate) fn is_tiff_family(bytes: &[u8]) -> bool {
    family_order(bytes).is_some()
}

/// 厂商 MakerNote（EXIF 0x927C）—— Olympus / OM System 的 ORF 预览就住在这里。
const TAG_MAKER_NOTE: u16 = 0x927c;

/// 单个内嵌 JPEG 的**上限**（`read()` 那边还会按 16 MiB 与文件长度再筛一次）。
/// 这里只是不让一个坏标签把长度撑成天文数字。
const JPEG_SPAN_LIMIT: usize = 64 * 1024 * 1024;

/// TIFF 的 JPEG 压缩值（`Compression`）：6 = 旧式 JPEG，7 = 新式 JPEG。
const COMPRESSION_JPEG: [i64; 2] = [6, 7];
/// `PhotometricInterpretation` 的两个「这是真 RAW」取值：CFA 与 LinearRaw。
/// 它们也可能是 JPEG 压缩（lossless JPEG）的，不能当预览。
const PHOTOMETRIC_CFA: i64 = 32803;
const PHOTOMETRIC_LINEAR_RAW: i64 = 34892;

/// 内嵌 JPEG 的位置；复用同一份 TIFF 类型/字节序解析，不读取传感器数据。
/// 只走有界头部内的 IFD 链和 SubIFD，偏移循环、越界或过大的目录直接略过。
///
/// 认得四种写法：标准 `0x0201/0x0202`、Panasonic `0x002e JpgFromRaw`、
/// **JPEG 压缩的整块 strip**（DNG / 3FR：`Compression=6/7` + `0x0111/0x0117`，
/// 排除 CFA/LinearRaw 的真 RAW 条带）、以及 Olympus 厂商 MakerNote（见
/// [`olympus_maker_note_spans`]）。
///
/// **大的在前**（`sort` 是倒序）：调用方 [`crate::media::embedded::read`] 取第一个能解开的，
/// 而小图请求（网格 384 / 胶片带 192 / AI 224）要的是「缩下去」而不是「放大上来」——
/// 一张 160×120 放大到 tile 只会糊。ORF 的 3200×2400 预览与 160×120 缩略图同时存在，
/// 就是靠这条规则拿到大的那张。
pub(crate) fn embedded_jpeg_spans(bytes: &[u8]) -> Vec<(u64, usize)> {
    let Some(order) = family_order(bytes) else { return Vec::new() };
    let mut pending = vec![order.u32(bytes, 4).unwrap_or(0) as usize];
    let mut seen = Vec::new();
    let mut spans = Vec::new();
    while let Some(offset) = pending.pop() {
        if offset == 0 || seen.contains(&offset) || seen.len() >= 16 { continue; }
        seen.push(offset);
        let Some(count) = order.u16(bytes, offset).map(usize::from).filter(|n| *n <= MAX_ENTRIES) else { continue };
        let Some(entries) = read_ifd(bytes, order, offset) else { continue };
        let (mut start, mut len) = (None, None);
        // JPEG 压缩的「整块条带」预览（DNG / 3FR / 部分厂商）：
        // Compression=6/7 且不是 CFA/LinearRaw —— 真 RAW 也用 JPEG 系 lossless 压缩，
        // 不排掉它就会拿传感器条带当预览（实测 Sigma fp DNG 的 raw 条带 28 MB）。
        let (mut compression, mut photometric) = (None, None);
        let (mut strip_start, mut strip_len) = (None, None);
        for entry in entries {
            match entry.tag {
                0x0201 => start = entry.int_value(order),
                0x0202 => len = entry.int_value(order),
                0x0103 if entry.count == 1 => compression = entry.int_value(order),
                0x0106 if entry.count == 1 => photometric = entry.int_value(order),
                0x0111 if entry.count == 1 => strip_start = entry.int_value(order),
                0x0117 if entry.count == 1 => strip_len = entry.int_value(order),
                // Panasonic JpgFromRaw: UNDEFINED payload, offset in the value slot.
                0x002e if entry.field_type == 7 && entry.count > 4 => {
                    if let Some(at) = order.u32(&entry.value_bytes, 0) {
                        spans.push((u64::from(at), entry.count as usize));
                    }
                }
                0x014a if entry.field_type == 4 && entry.count <= 16 => {
                    if let Some(data) = entry.data(bytes, order) {
                        for chunk in data.as_chunks::<4>().0 {
                            if let Some(at) = order.u32(chunk, 0) { pending.push(at as usize); }
                        }
                    }
                }
                // Exif 子 IFD：绝大多数厂商的 MakerNote 在这里，必须走下去
                TAG_EXIF_IFD => {
                    if let Some(at) = entry.int_value(order).and_then(|v| usize::try_from(v).ok()) {
                        pending.push(at);
                    }
                }
                // Olympus / OM System：预览图不在标准 IFD 里，而在厂商 MakerNote 里
                TAG_MAKER_NOTE if entry.field_type == 7 && entry.count > 4 => {
                    if let Some(at) = order.u32(&entry.value_bytes, 0) {
                        spans.extend(olympus_maker_note_spans(bytes, order, at as usize));
                    }
                }
                _ => {}
            }
        }
        if let (Some(start), Some(len)) = (start, len)
            && start > 0 && len > 0 { spans.push((start as u64, len as usize)); }
        if compression.is_some_and(|value| COMPRESSION_JPEG.contains(&value))
            && !matches!(photometric, Some(PHOTOMETRIC_CFA | PHOTOMETRIC_LINEAR_RAW))
            && let (Some(start), Some(len)) = (strip_start, strip_len)
            && start > 0 && len > 0
        {
            spans.push((start as u64, len as usize));
        }
        if let Some(at) = order.u32(bytes, offset + 2 + count * 12) { pending.push(at as usize); }
    }
    spans.sort_unstable_by_key(|(_, len)| std::cmp::Reverse(*len));
    spans.dedup();
    spans
}

/// MakerNote 自己的字节序标记（`OLYMPUS\0` / `OM SYSTEM\0` 后面那两字节）。
fn maker_note_order(note: &[u8], at: usize) -> Option<ByteOrder> {
    match note.get(at..at + 2)? {
        b"II" => Some(ByteOrder::Little),
        b"MM" => Some(ByteOrder::Big),
        _ => None,
    }
}

/// 「MakerNote 相对偏移 + 长度」→ 绝对区间；正负离谱或超限一律 `None`。
fn checked_span(base: usize, offset: i64, len: i64) -> Option<(u64, usize)> {
    if offset <= 0 || len < 4 { return None; }
    let at = base.checked_add(usize::try_from(offset).ok()?)?;
    let len = usize::try_from(len).ok()?;
    (len <= JPEG_SPAN_LIMIT).then_some((at as u64, len))
}

/// 只认「单值整数」（SHORT/LONG，count=1）。厂商段里同编号的数组用法很多
/// —— 例：ImageProcessing 的 0x0100/0x0101/0x0102 是 `WB_RBLevels` 这类 count=4 的数组，
/// 不筛就会拿数组第一项当偏移/长度，白读一大段。
fn single_int(entry: Entry, order: ByteOrder) -> Option<i64> {
    (matches!(entry.field_type, 3 | 4) && entry.count == 1)
        .then(|| entry.int_value(order))
        .flatten()
}

/// Olympus / OM System 厂商 MakerNote 里的内嵌 JPEG。
///
/// ORF **不写**标准 TIFF 的 `0x0201/0x0202`（缩略图指针）。奥林巴斯把图放在这里：
///
/// * 主 IFD 的 `0x0100 ThumbnailImage` —— 160×120 小图（UNDEFINED，值槽是偏移）；
/// * CameraSettings 子 IFD 的 `0x0100/0x0101/0x0102` =
///   `PreviewImageValid/Start/Length` —— 大预览（E-M5 Mark II 实测 3200×2400）；
/// * 新机型也可能写在主 IFD 的 `0x1035/0x1036/0x1037`（ExifTool 同一组标签的新位置）。
///
/// 偏移语义照 ExifTool 的 `MakerNoteOlympus2/3`：`OLYMPUS\0`（IFD 在 +12）与
/// `OM SYSTEM\0`（+16）的**所有偏移都相对 MakerNote 起点**。实测（2026-10-05，
/// `/mnt/c/src/tmp/pic/AM300135.ORF`）：`PreviewImageStart=48652` + MakerNote 起点 3572
/// = 文件 offset 52224 —— 正是那张 3200×2400 JPEG 的 SOI。
/// 老式 `OLYMP\0`（IFD 在 +8，E-1/E-300 那代）没有 Base 覆盖，偏移按文件起点解释。
///
/// 依据：ExifTool `Image::ExifTool::MakerNotes` 的 MakerNoteOlympus2/3 与
/// `Olympus::Main` / `Olympus::CameraSettings` 标签表
/// （https://exiftool.sourceforge.net/TagNames/Olympus.html）。
///
/// 全程 `bytes.get`，MakerNote 签名/起点/子 IFD/条目数任何一步越界就返回空 ——
/// RAW 是不可信输入，坏文件不许 panic。
fn olympus_maker_note_spans(bytes: &[u8], outer: ByteOrder, note_at: usize) -> Vec<(u64, usize)> {
    let Some(note) = bytes.get(note_at..) else { return Vec::new() };
    let (ifd_at, base, order) = if note.starts_with(b"OLYMPUS\0") {
        (note_at + 12, note_at, maker_note_order(note, 8).unwrap_or(outer))
    } else if note.starts_with(b"OM SYSTEM\0") {
        (note_at + 16, note_at, maker_note_order(note, 12).unwrap_or(outer))
    } else if note.starts_with(b"OLYMP\0") {
        (note_at + 8, 0, outer)
    } else {
        return Vec::new();
    };
    let Some(entries) = read_ifd(bytes, order, ifd_at) else { return Vec::new() };
    let mut spans = Vec::new();
    let mut sub_ifds = Vec::new();
    let (mut valid, mut start, mut len) = (None, None, None);
    for entry in entries {
        match entry.tag {
            // ThumbnailImage：UNDEFINED，>4 字节时值槽是偏移
            0x0100 if entry.field_type == 7 && entry.count > 4 => {
                if let Some(at) = order.u32(&entry.value_bytes, 0)
                    && let Some(span) = checked_span(base, i64::from(at), entry.count as i64)
                {
                    spans.push(span);
                }
            }
            0x1035 => valid = single_int(entry, order),
            0x1036 => start = single_int(entry, order),
            0x1037 => len = single_int(entry, order),
            // 子 IFD 指针（Equipment / CameraSettings / RawDevelopment / …）：
            // 新版是 IFD(13)，被旧 ExifTool 改写过的可能变成 LONG(4)
            0x2010..=0x2050 if matches!(entry.field_type, 4 | 13) && entry.count <= 1 => {
                if let Some(at) = order.u32(&entry.value_bytes, 0)
                    && let Some(at) = base.checked_add(at as usize)
                {
                    sub_ifds.push(at);
                }
            }
            _ => {}
        }
    }
    push_makernote_preview(&mut spans, base, valid, start, len);
    for ifd in sub_ifds.into_iter().take(8) {
        let Some(entries) = read_ifd(bytes, order, ifd) else { continue };
        let (mut valid, mut start, mut len) = (None, None, None);
        for entry in entries {
            match entry.tag {
                0x0100 => valid = single_int(entry, order),
                0x0101 => start = single_int(entry, order),
                0x0102 => len = single_int(entry, order),
                _ => {}
            }
        }
        push_makernote_preview(&mut spans, base, valid, start, len);
    }
    spans
}

/// `PreviewImageValid/Start/Length` 三件套都齐了才算数（`Valid` 缺省当「有」）。
fn push_makernote_preview(
    spans: &mut Vec<(u64, usize)>,
    base: usize,
    valid: Option<i64>,
    start: Option<i64>,
    len: Option<i64>,
) {
    if valid == Some(0) { return; }
    if let (Some(start), Some(len)) = (start, len)
        && let Some(span) = checked_span(base, start, len)
    {
        spans.push(span);
    }
}

/// Read typed EXIF/GPS/XMP fields from the same IFD families as parse/read_fields.
/// RW2 and ORF keep TIFF directory offsets/types but use a vendor magic. Adapt
/// only the owned parser buffer; never mutate source bytes or the original file.
/// Unknown headers and broken directories are still rejected by the parser.
pub(crate) fn read_exif(bytes: &[u8]) -> Result<exif::Exif, exif::Error> {
    let order =
        family_order(bytes).ok_or(exif::Error::InvalidFormat("Unsupported TIFF family header"))?;
    let mut data = bytes.to_vec();
    let magic = match order {
        ByteOrder::Little => MAGIC_TIFF.to_le_bytes(),
        ByteOrder::Big => MAGIC_TIFF.to_be_bytes(),
    };
    data[2..4].copy_from_slice(&magic);
    exif::Reader::new().read_raw(data)
}

/// 解析头部。`bytes` 至少要含 8 字节的 TIFF 头；给一大段（比如前 1 MiB）也行。
#[must_use]
pub fn parse(bytes: &[u8]) -> Option<TiffInfo> {
    let order = family_order(bytes)?;
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
        && let Some(entries) = read_ifd(bytes, order, offset)
    {
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

/// List all reachable TIFF/EXIF directories from RAW formats whose magic kamadak rejects.
/// The traversal is bounded and cycle-safe; vendor MakerNote payloads remain opaque.
#[must_use]
pub fn read_fields(bytes: &[u8]) -> Option<Vec<(String, String, String)>> {
    let order = family_order(bytes)?;
    let root = order.u32(bytes, 4)? as usize;
    let mut pending = vec![("IFD 0".to_owned(), root)];
    let mut visited = std::collections::HashSet::new();
    let mut out = Vec::new();
    while let Some((ifd, offset)) = pending.pop() {
        if visited.len() >= 16 || !visited.insert(offset) {
            continue;
        }
        let Some(entries) = read_ifd(bytes, order, offset) else {
            continue;
        };
        for entry in entries {
            if let Some(target) = entry.int_value(order).and_then(|v| usize::try_from(v).ok()) {
                match entry.tag {
                    TAG_EXIF_IFD => pending.push(("Exif".to_owned(), target)),
                    TAG_GPS_IFD => pending.push(("GPS".to_owned(), target)),
                    TAG_INTEROP_IFD => pending.push(("Interoperability".to_owned(), target)),
                    _ => {}
                }
            }
            let name = match (ifd.as_str(), entry.tag) {
                ("GPS", 0x0001) => "GPSLatitudeRef",
                ("GPS", 0x0002) => "GPSLatitude",
                ("GPS", 0x0003) => "GPSLongitudeRef",
                ("GPS", 0x0004) => "GPSLongitude",
                ("GPS", 0x0005) => "GPSAltitudeRef",
                ("GPS", 0x0006) => "GPSAltitude",
                (_, TAG_MAKE) => "Make",
                (_, TAG_MODEL) => "Model",
                (_, TAG_ORIENTATION) => "Orientation",
                (_, TAG_SOFTWARE) => "Software",
                (_, TAG_DATETIME) => "DateTime",
                (_, TAG_DATETIME_ORIGINAL) => "DateTimeOriginal",
                (_, TAG_OFFSET_TIME) => "OffsetTime",
                (_, TAG_OFFSET_TIME_ORIGINAL) => "OffsetTimeOriginal",
                (_, TAG_OFFSET_TIME_DIGITIZED) => "OffsetTimeDigitized",
                (_, TAG_EXPOSURE) => "ExposureTime",
                (_, TAG_FNUMBER) => "FNumber",
                (_, TAG_EXPOSURE_BIAS) => "ExposureBiasValue",
                (_, TAG_ISO) => "ISOSpeedRatings",
                (_, TAG_FOCAL) => "FocalLength",
                (_, TAG_LENS) => "LensModel",
                (_, TAG_IMAGE_WIDTH | TAG_RW2_WIDTH | TAG_PIXEL_X) => "ImageWidth",
                (_, TAG_IMAGE_LENGTH | TAG_RW2_HEIGHT | TAG_PIXEL_Y) => "ImageHeight",
                _ => "",
            };
            let tag = if name.is_empty() {
                format!("0x{:04X}", entry.tag)
            } else {
                name.to_owned()
            };
            let value = if let Some(value) = entry.ascii(bytes, order) {
                value
            } else if entry.field_type == 5 {
                entry
                    .rational(bytes, order)
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            } else if entry.field_type == 10 {
                entry
                    .srational(bytes, order)
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            } else if let Some(value) = entry.int_value(order) {
                value.to_string()
            } else if entry.count > 0 {
                format!("<{} values, TIFF type {}>", entry.count, entry.field_type)
            } else {
                String::new()
            };
            out.push((
                ifd.clone(),
                tag,
                if value.is_empty() {
                    "<empty>".to_owned()
                } else {
                    value.chars().take(4096).collect()
                },
            ));
        }
        if ifd == "IFD 0" {
            let count = usize::from(order.u16(bytes, offset)?);
            if let Some(next) = offset
                .checked_add(2)
                .and_then(|v| count.checked_mul(12).and_then(|n| v.checked_add(n)))
                .and_then(|pos| order.u32(bytes, pos))
                && next != 0
            {
                pending.push(("IFD 1".to_owned(), next as usize));
            }
        }
    }
    Some(out)
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

        let write_ifd = |out: &mut Vec<u8>,
                         tags: &[u16],
                         field_slice: &[[u8; 4]],
                         meta_slice: &[(u16, u32)]| {
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
        write_ifd(
            &mut out,
            &ifd0_tags,
            &fields[..ifd0_field_count],
            &meta[..ifd0_field_count],
        );
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
    fn raw_field_list_follows_gps_ifd_without_cycles() {
        let mut bytes = build(MAGIC_RW2, false, &[(TAG_GPS_IFD, Val::Long(26))], &[]);
        assert_eq!(bytes.len(), 26);
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // GPSLatitudeRef
        bytes.extend_from_slice(&2u16.to_le_bytes()); // ASCII
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(b"N\0\0\0");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let fields = read_fields(&bytes).unwrap();
        assert!(fields.contains(&("GPS".into(), "GPSLatitudeRef".into(), "N".into())));
    }

    #[test]
    fn raw_field_list_keeps_known_unknown_and_exif_ifd_entries() {
        let bytes = build(
            MAGIC_RW2,
            false,
            &[(TAG_MAKE, Val::Ascii("Panasonic")), (0x9999, Val::Long(42))],
            &[
                (TAG_LENS, Val::Ascii("DG Vario-Elmarit 12-60mm F2.8-4")),
                (TAG_FNUMBER, Val::Rational(28, 10)),
            ],
        );
        let fields = read_fields(&bytes).unwrap();
        assert!(fields.contains(&("IFD 0".into(), "Make".into(), "Panasonic".into())));
        assert!(fields.contains(&("IFD 0".into(), "0x9999".into(), "42".into())));
        assert!(fields.iter().any(|(ifd, tag, value)| ifd == "Exif"
            && tag == "LensModel"
            && value.contains("12-60")));
        assert!(
            fields
                .iter()
                .any(|(ifd, tag, value)| ifd == "Exif" && tag == "FNumber" && value == "2.8")
        );
        assert_eq!(read_fields(b"II"), None);
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
        assert_eq!(
            parse(&bytes).expect("应当能解析").exposure_bias_ev,
            Some(0.0)
        );
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
            &[
                (TAG_RW2_WIDTH, Val::Short(5264)),
                (TAG_RW2_HEIGHT, Val::Short(3904)),
            ],
            &[
                (TAG_DATETIME_ORIGINAL, Val::Ascii("2026:09:13 00:54:28")),
                (TAG_OFFSET_TIME_ORIGINAL, Val::Ascii("+08:00")),
            ],
        );
        let info = parse(&bytes).expect("RW2 必须能解析");
        assert_eq!(info.datetime.as_deref(), Some("2026:09:13 00:54:28"));
        assert_eq!(
            info.offset_time.as_deref(),
            Some("+08:00"),
            "时区偏移不能丢"
        );
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
        let plain = build(
            MAGIC_RW2,
            false,
            &[],
            &[(TAG_OFFSET_TIME, Val::Ascii("-05:30"))],
        );
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
        let bytes = build(
            MAGIC_TIFF,
            false,
            &[],
            &[(TAG_EXPOSURE, Val::Rational(1, 0))],
        );
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
        let mut bytes = build(
            MAGIC_TIFF,
            false,
            &[(TAG_MAKE, Val::Ascii("Panasonic"))],
            &[],
        );
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
        let bytes = build(
            MAGIC_TIFF,
            false,
            &[(TAG_ORIENTATION, Val::Ascii("abcd"))],
            &[],
        );
        assert_eq!(parse(&bytes).map(|i| i.orientation), Some(None));
    }

    #[test]
    fn empty_info_is_reported_as_empty() {
        let info = TiffInfo::default();
        assert!(info.is_empty());
        assert!(
            !TiffInfo {
                orientation: Some(1),
                ..Default::default()
            }
            .is_empty()
        );
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
        assert!(
            info.orientation.is_some(),
            "样本的方向应当读得出来：{info:?}"
        );
        assert!(
            info.width.unwrap_or(0) > 0,
            "样本的宽度应当读得出来：{info:?}"
        );
        assert!(info.model.is_some(), "样本的机身型号应当读得出来：{info:?}");
        assert!(
            info.datetime.is_some(),
            "样本的拍摄时间应当读得出来：{info:?}"
        );
    }

    /// 往 IFD 里写一条 12 字节的条目（小端）。
    fn put_le_entry(out: &mut [u8], at: usize, tag: u16, field_type: u16, count: u32, value: u32) {
        out[at..at + 2].copy_from_slice(&tag.to_le_bytes());
        out[at + 2..at + 4].copy_from_slice(&field_type.to_le_bytes());
        out[at + 4..at + 8].copy_from_slice(&count.to_le_bytes());
        out[at + 8..at + 12].copy_from_slice(&value.to_le_bytes());
    }

    /// 合成一个「ORF + Olympus MakerNote」的最小文件（小端）。
    ///
    /// `synth_orf` 的布局照真实样本（`AM300135.ORF`）简化而来：
    /// IFD0 →(0x8769) Exif → (0x927C) MakerNote「OLYMPUS\0II\x03\0」+ 主 IFD。
    /// 主 IFD 里一定有 `0x0100 ThumbnailImage`（160×120 那张小图）；
    /// 大预览按 `via_main_tags` 二选一：
    ///   * `false`：CameraSettings 子 IFD（0x2020）的 `0x0100/0x0101/0x0102`（E-M5 Mark II 写法）
    ///   * `true` ：主 IFD 的 `0x1035/0x1036/0x1037`（ExifTool 记的另一种写法）
    ///
    /// 返回（文件字节，预览图绝对偏移，预览图长度）—— 偏移**相对 MakerNote 起点**，
    /// 正是要验证的那条语义。
    #[allow(clippy::too_many_lines)]
    fn synth_orf(
        thumb: &[u8],
        preview: &[u8],
        valid: u32,
        via_main_tags: bool,
    ) -> (Vec<u8>, usize, usize) {
        const IFD0: usize = 8;
        const EXIF: usize = 26;
        const NOTE: usize = 44;
        const SIGNATURE: usize = 12;
        // CameraSettings 子 IFD：count + 3 条 + next
        const CS_SIZE: usize = 2 + 3 * 12 + 4;
        let main_entries = if via_main_tags { 4 } else { 2 };
        let main_ifd = NOTE + SIGNATURE;
        let thumb_at = main_ifd + 2 + main_entries * 12 + 4;
        let preview_at = if via_main_tags {
            thumb_at + thumb.len()
        } else {
            thumb_at + thumb.len() + CS_SIZE
        };
        let note_len = preview_at + preview.len() - NOTE;

        let mut out = vec![0u8; preview_at + preview.len()];
        out[0..2].copy_from_slice(b"II");
        out[2..4].copy_from_slice(&MAGIC_ORF_RO.to_le_bytes());
        out[4..8].copy_from_slice(&(IFD0 as u32).to_le_bytes());
        // IFD0 → Exif IFD → MakerNote
        out[IFD0..IFD0 + 2].copy_from_slice(&1u16.to_le_bytes());
        put_le_entry(&mut out, IFD0 + 2, TAG_EXIF_IFD, 4, 1, EXIF as u32);
        out[EXIF..EXIF + 2].copy_from_slice(&1u16.to_le_bytes());
        put_le_entry(
            &mut out,
            EXIF + 2,
            TAG_MAKER_NOTE,
            7,
            note_len as u32,
            NOTE as u32,
        );
        out[NOTE..NOTE + SIGNATURE].copy_from_slice(b"OLYMPUS\0II\x03\0");
        out[main_ifd..main_ifd + 2].copy_from_slice(&(main_entries as u16).to_le_bytes());
        // 主 IFD 第一條：小缩略图（值槽 = 相对 NOTE 的偏移）
        put_le_entry(
            &mut out,
            main_ifd + 2,
            0x0100,
            7,
            thumb.len() as u32,
            (thumb_at - NOTE) as u32,
        );
        let second = main_ifd + 2 + 12;
        if via_main_tags {
            put_le_entry(&mut out, second, 0x1035, 4, 1, valid);
            put_le_entry(&mut out, second + 12, 0x1036, 4, 1, (preview_at - NOTE) as u32);
            put_le_entry(&mut out, second + 24, 0x1037, 4, 1, preview.len() as u32);
        } else {
            let cs_at = thumb_at + thumb.len();
            put_le_entry(&mut out, second, 0x2020, 13, 1, (cs_at - NOTE) as u32);
            out[cs_at..cs_at + 2].copy_from_slice(&3u16.to_le_bytes());
            put_le_entry(&mut out, cs_at + 2, 0x0100, 4, 1, valid);
            put_le_entry(
                &mut out,
                cs_at + 14,
                0x0101,
                4,
                1,
                (preview_at - NOTE) as u32,
            );
            put_le_entry(&mut out, cs_at + 26, 0x0102, 4, 1, preview.len() as u32);
        }
        out[thumb_at..thumb_at + thumb.len()].copy_from_slice(thumb);
        out[preview_at..preview_at + preview.len()].copy_from_slice(preview);
        (out, preview_at, preview.len())
    }

    /// 合成一个「DNG 形状」的最小文件：IFD0 是 **CFA 原始条带**（Compression=7 —— 真 RAW 也用
    /// JPEG 系 lossless 压缩，正是要排除的那种），SubIFD 是 **JPEG 预览条带**（Photometric=YCbCr）。
    /// 返回（字节，预览偏移，预览长度）。
    fn synth_dng_with_strip_preview(
        preview: &[u8],
        raw_photometric: i64,
    ) -> (Vec<u8>, usize, usize) {
        const IFD0: usize = 8;
        const IFD0_ENTRIES: usize = 5;
        const SUB_ENTRIES: usize = 4;
        let sub_ifd = IFD0 + 2 + IFD0_ENTRIES * 12 + 4;
        let raw_at = sub_ifd + 2 + SUB_ENTRIES * 12 + 4;
        let raw_len = 32usize;
        let preview_at = raw_at + raw_len;
        let mut out = vec![0u8; preview_at + preview.len()];
        out[0..2].copy_from_slice(b"II");
        out[2..4].copy_from_slice(&MAGIC_TIFF.to_le_bytes());
        out[4..8].copy_from_slice(&(IFD0 as u32).to_le_bytes());
        // IFD0 = 真 RAW 条带（Photometric 由调用方给：CFA / LinearRaw）
        out[IFD0..IFD0 + 2].copy_from_slice(&(IFD0_ENTRIES as u16).to_le_bytes());
        put_le_entry(&mut out, IFD0 + 2, 0x0103, 3, 1, 7);
        put_le_entry(&mut out, IFD0 + 14, 0x0106, 3, 1, raw_photometric as u32);
        put_le_entry(&mut out, IFD0 + 26, 0x0111, 4, 1, raw_at as u32);
        put_le_entry(&mut out, IFD0 + 38, 0x0117, 4, 1, raw_len as u32);
        put_le_entry(&mut out, IFD0 + 50, 0x014a, 4, 1, sub_ifd as u32);
        // SubIFD = JPEG 预览条带
        out[sub_ifd..sub_ifd + 2].copy_from_slice(&(SUB_ENTRIES as u16).to_le_bytes());
        put_le_entry(&mut out, sub_ifd + 2, 0x0103, 3, 1, 7);
        put_le_entry(&mut out, sub_ifd + 14, 0x0106, 3, 1, 6); // YCbCr
        put_le_entry(&mut out, sub_ifd + 26, 0x0111, 4, 1, preview_at as u32);
        put_le_entry(&mut out, sub_ifd + 38, 0x0117, 4, 1, preview.len() as u32);
        // 真 RAW 条带故意也以 FFD8 开头 —— 只看魔术字节会误判成预览
        out[raw_at] = 0xff;
        out[raw_at + 1] = 0xd8;
        out[preview_at..preview_at + preview.len()].copy_from_slice(preview);
        (out, preview_at, preview.len())
    }

    /// ORF 的预览图在厂商 MakerNote 里，且**相对 MakerNote 起点**；
    /// 3200×2400 那张必须排在小缩略图前面（小图放大到 384 只会糊）。
    #[test]
    fn orf_maker_note_preview_is_found_and_beats_the_tiny_thumbnail() {
        let thumb = [0xff, 0xd8, 0xff, 0xd9, 1, 2, 3, 4];
        let preview = [0xff, 0xd8, 0xff, 0xd9, 9, 8, 7, 6, 5, 4, 3, 2];
        for via_main_tags in [false, true] {
            let (bytes, preview_at, preview_len) = synth_orf(&thumb, &preview, 1, via_main_tags);
            let spans = embedded_jpeg_spans(&bytes);
            assert_eq!(
                spans.first(),
                Some(&(preview_at as u64, preview_len)),
                "大预览要排在最前面（via_main_tags={via_main_tags}）：{spans:?}"
            );
            assert!(
                spans.iter().any(|(_, len)| *len == thumb.len()),
                "小图要保留在列表里兜底：{spans:?}"
            );
        }
    }

    #[test]
    fn orf_preview_is_read_end_to_end() {
        use crate::media::embedded;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("竖片样片.ORF");
        let thumb = [0xff, 0xd8, 0xff, 0xd9, 1, 2, 3, 4];
        let preview = vec![0xff, 0xd8, 0xff, 0xd9, 5, 6, 7, 8, 9, 10, 11, 12];
        let (bytes, _, _) = synth_orf(&thumb, &preview, 1, false);
        std::fs::write(&path, &bytes).unwrap();
        let found = embedded::read(&path).expect("应当找到内嵌预览");
        assert_eq!(found.bytes, preview, "要读到大预览那一段，不是小图");
    }

    #[test]
    fn orf_maker_note_preview_valid_zero_falls_back_to_the_thumbnail() {
        let thumb = [0xff, 0xd8, 0xff, 0xd9, 1, 2, 3, 4];
        let preview = [0xff, 0xd8, 0xff, 0xd9, 9, 8, 7, 6, 5, 4, 3, 2];
        let (bytes, _, _) = synth_orf(&thumb, &preview, 0, false);
        let spans = embedded_jpeg_spans(&bytes);
        assert_eq!(spans, vec![(spans[0].0, thumb.len())], "Valid=0 时只剩小图");
    }

    #[test]
    fn foreign_maker_note_is_not_parsed_as_olympus() {
        let thumb = [0xff, 0xd8, 0xff, 0xd9, 1, 2, 3, 4];
        let preview = [0xff, 0xd8, 0xff, 0xd9, 9, 8, 7, 6, 5, 4, 3, 2];
        let (mut bytes, _, _) = synth_orf(&thumb, &preview, 1, false);
        // 把签名换成别家 —— 别的厂商的 MakerNote 布局完全不同，不能硬按 Olympus 解
        bytes[44..51].copy_from_slice(b"NOTOLYM");
        assert!(
            embedded_jpeg_spans(&bytes).is_empty(),
            "非 Olympus 签名不许产出任何 span"
        );
    }

    #[test]
    fn dng_strip_preview_is_extracted_and_the_cfa_strip_is_not() {
        let preview = vec![0xff, 0xd8, 0xff, 0xd9, 5, 6, 7, 8, 9, 10, 11, 12];
        for raw_photometric in [PHOTOMETRIC_CFA, PHOTOMETRIC_LINEAR_RAW] {
            let (bytes, preview_at, preview_len) =
                synth_dng_with_strip_preview(&preview, raw_photometric);
            let spans = embedded_jpeg_spans(&bytes);
            assert_eq!(
                spans,
                vec![(preview_at as u64, preview_len)],
                "Photometric={raw_photometric} 的条带是真 RAW，不许当预览"
            );
        }
        // 端到端：`read()` 拿到的就是预览那一段（不是那个也以 FFD8 开头的 raw 条带）
        let (bytes, _, _) = synth_dng_with_strip_preview(&preview, PHOTOMETRIC_CFA);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("样片.DNG");
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            super::super::embedded::read(&path).unwrap().bytes,
            preview
        );
    }

    #[test]
    fn parses_a_real_orf_preview_when_the_sample_is_present() {
        // 样本在这台机器上（`AGENTS.md` 记的样本目录）；别的机器上跳过。
        let path = std::path::Path::new("/mnt/c/src/tmp/pic/AM300135.ORF");
        if !path.exists() {
            return;
        }
        let found = super::super::embedded::read(path).expect("真 ORF 应当找得到内嵌预览");
        assert!(found.bytes.starts_with(&[0xff, 0xd8]));
        assert!(
            found.bytes.len() > 1_000_000,
            "要的是 3200×2400 那张大预览（实测 1096496 B），不是 160×120：{} B",
            found.bytes.len()
        );
        let image = image::load_from_memory_with_format(&found.bytes, image::ImageFormat::Jpeg)
            .expect("预览图要能解码");
        assert_eq!((image.width(), image.height()), (3200, 2400));
    }
}
