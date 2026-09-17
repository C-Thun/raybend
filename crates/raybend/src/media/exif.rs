//! EXIF 抽取：从照片里读出拍摄时间、器材、曝光参数、尺寸、朝向与地理坐标。
//!
//! 用 `kamadak-exif`（纯 Rust / MIT）—— 与 RapidRAW 的选择一致（它另引 `little_exif`
//! 只用于**写回**，本项目第一阶段不写 EXIF）。
//!
//! # 两条重要口径
//!
//! 1. **拍摄时间没有时区**：`DateTimeOriginal` 只是相机钟表的读数。本项目的处理是：
//!    有 `OffsetTimeOriginal`（`+08:00`）就按它换算成 UTC 毫秒并记下偏移；没有就
//!    **把墙上时间原样当 UTC 存**、偏移留空，显示端遇空按「无偏移」渲染 —— 这样用户
//!    看到的数字与相机/其它软件一致，而不是被我们本机的时区悄悄改掉。
//! 2. **读不到 EXIF 不是错误**：扫描、视频、截图、微信导出的图都可能没有，甚至文件
//!    本身损坏。这时按 [`TakenAtSource`] 的顺序退到文件名、再到文件修改时间，
//!    并把**来源**一并记下来（`assets.taken_at_source`），UI 才能如实标注。

use std::path::Path;

use exif::{In, Tag, Value};

use crate::store::time;

/// 拍摄时间的来源（写进 `assets.taken_at_source`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TakenAtSource {
    /// 照片自带的 EXIF 拍摄时间（最可信）。
    Exif,
    /// 文件名里带的日期时间（手机与部分相机的命名习惯）。
    Filename,
    /// 文件修改时间（最后手段：至少能把照片排进时间轴）。
    FileMtime,
}

/// 解析出来的拍摄时间。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct TakenAt {
    /// Unix 毫秒（UTC）。
    pub millis: i64,
    /// 换算用的时区偏移（分钟，东八区 = 480）；`None` = 原值没有时区信息，
    /// 已按「墙上时间当 UTC」处理。
    pub offset_min: Option<i32>,
    pub source: TakenAtSource,
}

/// 地理坐标（十进制度，北纬/东经为正）。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Gps {
    pub lat: f64,
    pub lon: f64,
}

/// 一张照片的元数据。**所有字段都可空** —— 相机型号各异，缺什么是常态。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct ExifData {
    pub taken_at: Option<TakenAt>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub focal_mm: Option<f64>,
    pub f_number: Option<f64>,
    /// 快门时间（毫秒；1/250 秒 → 4.0）。
    pub exposure_ms: Option<f64>,
    pub iso: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// EXIF orientation（1..8）。
    pub orientation: Option<i64>,
    pub gps: Option<Gps>,
    pub software: Option<String>,
    /// 人可读的拍摄时间原样字符串（供 `easy copy` 展示与排错）。
    pub datetime_raw: Option<String>,
}

impl ExifData {
    /// 什么都没读到。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// 读一个文件的 EXIF。**任何失败都返回空值**（不是错误）——
/// 没有 EXIF 的文件是常态，调用方不该为此写错误分支。
#[must_use]
pub fn read_file(path: &Path) -> ExifData {
    read_file_with_probe(path, HEAD_PROBE_BYTES)
}

/// **RAW 专用**的读法：在通用读法之外多一层「TIFF 家族兜底」（RW2 / ORF 的魔数不是 `0x2A`，
/// `kamadak-exif` 直接放弃 —— 见 [`crate::media::tiff`]）。
///
/// 为什么不直接把它并进 `read_file`：兜底只解得出**方向与尺寸**，其它字段是空的。
/// 通用的 `read_bytes` 若在「开头读到一半」时就靠兜底返回，会把「整读能拿到全部 EXIF」
/// 这条更好的路堵掉（`read_file_probe_then_full_read_fallback` 那条测试正是钉这个的）。
/// 所以：**确切知道是 RAW 的地方**才用这个入口。
#[must_use]
pub fn read_file_raw(path: &Path) -> ExifData {
    read_file_with_probe_raw(path, HEAD_PROBE_BYTES, true)
}

/// [`read_file`] 的可测版本：先只读 `probe_bytes` 个字节试解析，失败再整读。
fn read_file_with_probe(path: &Path, probe_bytes: usize) -> ExifData {
    read_file_with_probe_raw(path, probe_bytes, false)
}

/// 共同实现。`raw_family = true` 时启用 TIFF 家族兜底。
fn read_file_with_probe_raw(path: &Path, probe_bytes: usize, raw_family: bool) -> ExifData {
    use std::io::Read;

    // ① 只读开头一段试一次。EXIF 就在 JPEG 的 APP1 里（前几 KB），RAW 的 IFD 绝大多数
    //    也在开头 —— 实测这一步把每张照片的读取量从「整个文件」降到 1 MiB 以下，
    //    在 9p/SMB 这类慢盘上是数量级的差别（实测 46ms/张 → 个位数 ms）。
    if let Ok(mut f) = std::fs::File::open(path) {
        let mut head = vec![0u8; probe_bytes];
        if let Ok(n) = f.read(&mut head) {
            head.truncate(n);
            if let Some(data) = read_bytes_inner(&head, raw_family) {
                return data;
            }
        }
    }

    // ② 开头读不出来（IFD 偏移指向文件尾部、或容器格式要求看完整文件）才读整个文件
    let Ok(bytes) = std::fs::read(path) else {
        return ExifData::default();
    };
    read_bytes_inner(&bytes, raw_family).unwrap_or_default()
}

/// 探测时先读多少字节。1 MiB 足以覆盖 JPEG 的全部头部段与绝大多数 RAW 的主 IFD。
const HEAD_PROBE_BYTES: usize = 1 << 20;

/// 从内存里的字节读 EXIF。
///
/// 先按容器格式找（JPEG / PNG / WebP / HEIF），失败再按**裸 TIFF** 试 ——
/// RW2 / NEF / CR2 / DNG 这些 RAW 都是 TIFF 家族，走的就是后一条路。
#[must_use]
pub fn read_bytes(bytes: &[u8]) -> Option<ExifData> {
    read_bytes_inner(bytes, false)
}

/// [`read_bytes`] 的 RAW 版（多一层 TIFF 家族兜底，说明见 [`read_file_raw`]）。
#[must_use]
pub fn read_bytes_raw(bytes: &[u8]) -> Option<ExifData> {
    read_bytes_inner(bytes, true)
}

fn read_bytes_inner(bytes: &[u8], raw_family: bool) -> Option<ExifData> {
    if !raw_family {
        // 通用路径：**保持原语义**（容器 → 裸 TIFF），不做任何兜底
        let reader = exif::Reader::new();
        let exif = std::io::Cursor::new(bytes)
            .pipe(|mut c| reader.read_from_container(&mut c))
            .or_else(|_| reader.read_raw(bytes.to_vec()))
            .ok()?;
        return Some(from_exif(&exif));
    }
    let reader = exif::Reader::new();
    let exif = std::io::Cursor::new(bytes)
        .pipe(|mut c| reader.read_from_container(&mut c))
        .or_else(|_| reader.read_raw(bytes.to_vec()))
        .ok();
    if let Some(exif) = exif {
        return Some(from_exif(&exif));
    }

    /*
     * 兜底：**TIFF 家族但魔数不是 0x002A** 的那几种 RAW（Panasonic RW2 = `IIU\0` / 0x0055、
     * Olympus ORF 的 `RO`/`RS`）。`kamadak-exif` 的 `tiff.rs` 把魔数写死成 `TIFF_FORTY_TWO`，
     * 于是这些文件的 EXIF **整体静默读不到** —— 实测 RW2 的后果是方向丢成默认 1（竖拍躺着显示）
     * 与尺寸 0×0（tile 比例退回占位）。
     *
     * 这里只取方向与尺寸（见 `media::tiff` 的范围说明），其余字段仍然留空 ——
     * 那是刻意的：宁可少几项，也不去猜。
     */
    let info = crate::media::tiff::parse(bytes)?;
    if info.is_empty() {
        return None;
    }
    Some(ExifData {
        orientation: info.orientation,
        width: info.width,
        height: info.height,
        ..ExifData::default()
    })
}

/// 把 `kamadak-exif` 的结果搬进我们自己的结构。
#[must_use]
pub fn from_exif(exif: &exif::Exif) -> ExifData {
    let mut out = ExifData {
        camera_make: ascii(exif, Tag::Make, In::PRIMARY),
        camera_model: ascii(exif, Tag::Model, In::PRIMARY),
        lens: ascii(exif, Tag::LensModel, In::PRIMARY)
            .or_else(|| ascii(exif, Tag::LensMake, In::PRIMARY)),
        orientation: uint(exif, Tag::Orientation, In::PRIMARY).map(i64::from),
        software: ascii(exif, Tag::Software, In::PRIMARY),
        ..ExifData::default()
    };

    out.focal_mm = rational(exif, Tag::FocalLength, In::PRIMARY);
    out.f_number = rational(exif, Tag::FNumber, In::PRIMARY);
    // 快门时间是**秒**（1/250 = 0.004），库里统一存毫秒，便于排序与统计
    out.exposure_ms = rational(exif, Tag::ExposureTime, In::PRIMARY)
        .map(|secs| secs * 1000.0)
        .filter(|ms| ms.is_finite() && *ms >= 0.0);
    out.iso = uint(exif, Tag::PhotographicSensitivity, In::PRIMARY)
        .or_else(|| uint(exif, Tag::ISOSpeed, In::PRIMARY))
        .map(i64::from);

    // 尺寸：优先 Exif IFD 的 PixelX/YDimension，退回 IFD0 的 ImageWidth/Length
    out.width = uint(exif, Tag::PixelXDimension, In::PRIMARY)
        .or_else(|| uint(exif, Tag::ImageWidth, In::PRIMARY))
        .map(i64::from);
    out.height = uint(exif, Tag::PixelYDimension, In::PRIMARY)
        .or_else(|| uint(exif, Tag::ImageLength, In::PRIMARY))
        .map(i64::from);

    // 拍摄时间：DateTimeOriginal → DateTimeDigitized → DateTime
    let dt_field = [Tag::DateTimeOriginal, Tag::DateTimeDigitized, Tag::DateTime]
        .into_iter()
        .find_map(|tag| exif.get_field(tag, In::PRIMARY));
    if let Some(field) = dt_field {
        let raw = field
            .display_value()
            .with_unit(field)
            .to_string()
            .trim()
            .to_string();
        if let Some((y, mo, d, hh, mi, ss)) = parse_datetime(&raw) {
            // 优先用 OffsetTimeOriginal；其次 OffsetTime；都没有就是「无时区」
            let offset_min = ascii(exif, Tag::OffsetTimeOriginal, In::PRIMARY)
                .as_deref()
                .and_then(parse_offset)
                .or_else(|| {
                    ascii(exif, Tag::OffsetTime, In::PRIMARY)
                        .as_deref()
                        .and_then(parse_offset)
                });
            let millis = match offset_min {
                Some(off) => time::from_civil_with_offset(y, mo, d, hh, mi, ss, off),
                None => time::from_civil(y, mo, d, hh, mi, ss),
            };
            if let Some(millis) = millis {
                out.taken_at = Some(TakenAt {
                    millis,
                    offset_min,
                    source: TakenAtSource::Exif,
                });
                out.datetime_raw = Some(raw);
            }
        }
    }

    out.gps = read_gps(exif);
    out
}

/// 判定拍摄时间：EXIF → 文件名 → 文件修改时间。
///
/// **纯函数**（不碰磁盘），便于单测与复用。`mtime_ms` 传 `None` 表示连修改时间都没有。
#[must_use]
pub fn resolve_taken_at(
    exif: Option<&ExifData>,
    file_name: &str,
    mtime_ms: Option<i64>,
) -> Option<TakenAt> {
    if let Some(t) = exif.and_then(|e| e.taken_at) {
        return Some(t);
    }
    if let Some(millis) = from_filename(file_name) {
        return Some(TakenAt {
            millis,
            offset_min: None,
            source: TakenAtSource::Filename,
        });
    }
    mtime_ms.map(|millis| TakenAt {
        millis,
        offset_min: None,
        source: TakenAtSource::FileMtime,
    })
}

/// 从文件名里猜拍摄时间。
///
/// 覆盖常见命名：`IMG_20260815_123456`、`PXL_20260815_123456789`、
/// `2026-08-15 12.34.56`、`20260815`、`20260815123456`。
/// 只在字符串里**扫出**一段合法的日期（而不是死抠固定模板）—— 相机前缀五花八门。
#[must_use]
pub fn from_filename(file_name: &str) -> Option<i64> {
    let name = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let bytes = name.as_bytes();
    let digit = |i: usize| i < bytes.len() && bytes[i].is_ascii_digit();
    let sep = |i: usize| i < bytes.len() && matches!(bytes[i], b'-' | b':' | b'/' | b'.');

    // ① 紧凑形式：YYYYMMDD（相机与手机最常用）
    for i in 0..bytes.len().saturating_sub(7) {
        if !digit(i) || (i > 0 && digit(i - 1)) {
            continue;
        }
        if !(0..8).all(|k| digit(i + k)) {
            continue;
        }
        let (Some(y), Some(mo), Some(d)) = (
            number(&name[i..i + 4]).map(i64::from),
            number(&name[i + 4..i + 6]),
            number(&name[i + 6..i + 8]),
        ) else {
            continue;
        };
        let (hh, mi, ss) = parse_time_suffix(&name[i + 8..]);
        if let Some(ms) = time::from_civil(y, mo, d, hh, mi, ss) {
            return Some(ms);
        }
    }

    // ② 分隔形式：YYYY-MM-DD / YYYY:MM:DD / YYYY.MM.DD（视频与部分相机）
    for i in 0..bytes.len().saturating_sub(9) {
        if !digit(i) || (i > 0 && digit(i - 1)) {
            continue;
        }
        if !(digit(i)
            && digit(i + 1)
            && digit(i + 2)
            && digit(i + 3)
            && sep(i + 4)
            && digit(i + 5)
            && digit(i + 6)
            && sep(i + 7)
            && digit(i + 8)
            && digit(i + 9))
        {
            continue;
        }
        let (Some(y), Some(mo), Some(d)) = (
            number(&name[i..i + 4]).map(i64::from),
            number(&name[i + 5..i + 7]),
            number(&name[i + 8..i + 10]),
        ) else {
            continue;
        };
        let (hh, mi, ss) = parse_time_suffix(&name[i + 10..]);
        if let Some(ms) = time::from_civil(y, mo, d, hh, mi, ss) {
            return Some(ms);
        }
    }
    None
}

/// 一段全是数字的切片 → 数值（含非数字或空则 `None`）。
fn number(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// 时间后缀：`_123456` / `-12.34.56` / `123456` / `_123456789`（后三位是毫秒）。
fn parse_time_suffix(rest: &str) -> (u32, u32, u32) {
    // 允许 `_123456`、`-12.34.56`、`123456789`（后三位是毫秒）等写法：
    // 先剥掉前导分隔符，再收集「数字与分隔符」，最后只留数字。
    let tail: String = rest
        .trim_start_matches(['_', '-', ' ', 'T', 't', '.', ':'])
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | ':' | '_'))
        .filter(char::is_ascii_digit)
        .collect();
    if tail.len() < 4 {
        return (0, 0, 0); // 没有时间部分：当天的 00:00:00
    }
    let take = |a: usize, b: usize| -> u32 { tail[a..b.min(tail.len())].parse().unwrap_or(0) };
    let hh = take(0, 2);
    let mi = take(2, 4);
    let ss = if tail.len() >= 6 { take(4, 6) } else { 0 };
    (hh, mi, ss)
}

/// 解析 EXIF 的日期串：`2026:08:15 12:34:56`（也容忍 `-` 分隔与前后空白）。
///
/// 全 0（`0000:00:00 00:00:00`）是相机「没设时间」的写法，返回 `None`。
#[must_use]
pub fn parse_datetime(raw: &str) -> Option<(i64, u32, u32, u32, u32, u32)> {
    let cleaned = raw.trim().replace('-', ":");
    let (date, time_part) = cleaned
        .split_once(' ')
        .map_or((cleaned.as_str(), ""), |(a, b)| (a, b.trim()));
    let mut d = date.split(':');
    let y: i64 = d.next()?.trim().parse().ok()?;
    let mo: u32 = d.next()?.trim().parse().ok()?;
    let day: u32 = d.next()?.trim().parse().ok()?;
    let (hh, mi, ss) = if time_part.is_empty() {
        (0, 0, 0)
    } else {
        let mut t = time_part.split(':');
        let hh: u32 = t.next()?.trim().parse().ok()?;
        let mi: u32 = t.next().unwrap_or("0").trim().parse().unwrap_or(0);
        let ss: u32 = t.next().unwrap_or("0").trim().parse().unwrap_or(0);
        (hh, mi, ss)
    };
    if y == 0 && mo == 0 && day == 0 {
        return None; // 相机没设时间
    }
    // 用 time::from_civil 兜底校验范围
    time::from_civil(y, mo, day, hh, mi, ss)?;
    Some((y, mo, day, hh, mi, ss))
}

/// 解析时区偏移串：`+08:00` / `-05:00` / `+0800` → 分钟数。
#[must_use]
pub fn parse_offset(raw: &str) -> Option<i32> {
    let s = raw.trim();
    let sign = match s.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits: String = s[1..].chars().filter(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return None;
    }
    let h: i32 = digits[0..2].parse().ok()?;
    let m: i32 = digits[2..4].parse().ok()?;
    if h > 14 || m > 59 {
        return None;
    }
    Some(sign * (h * 60 + m))
}

// ══════════════════════════════════════════════════════════════════
// 取值小工具
// ══════════════════════════════════════════════════════════════════

fn field(exif: &exif::Exif, tag: Tag, in_ifd: In) -> Option<&exif::Field> {
    exif.get_field(tag, in_ifd)
}

fn ascii(exif: &exif::Exif, tag: Tag, in_ifd: In) -> Option<String> {
    let f = field(exif, tag, in_ifd)?;
    if let Value::Ascii(parts) = &f.value {
        let s = parts
            .first()
            .map(|b| String::from_utf8_lossy(b).trim().to_string())?;
        // 空串、以及相机写的「全空格」占位都当没有
        return (!s.is_empty()).then_some(s);
    }
    None
}

fn uint(exif: &exif::Exif, tag: Tag, in_ifd: In) -> Option<u32> {
    field(exif, tag, in_ifd)?.value.get_uint(0)
}

fn rational(exif: &exif::Exif, tag: Tag, in_ifd: In) -> Option<f64> {
    let v = &field(exif, tag, in_ifd)?.value;
    match v {
        Value::Rational(items) => items.first().map(exif::Rational::to_f64),
        Value::SRational(items) => items.first().map(exif::SRational::to_f64),
        Value::Short(items) => items.first().map(|n| f64::from(*n)),
        _ => None,
    }
    .filter(|v| v.is_finite() && *v != 0.0)
}

/// GPS：3 个有理数（度分秒）+ 半球标记 → 十进制度。
fn read_gps(exif: &exif::Exif) -> Option<Gps> {
    let lat = gps_coord(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, "S")?;
    let lon = gps_coord(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, "W")?;
    Some(Gps { lat, lon })
}

fn gps_coord(exif: &exif::Exif, coord: Tag, r#ref: Tag, negative: &str) -> Option<f64> {
    let v = &field(exif, coord, In::PRIMARY)?.value;
    let nums = match v {
        Value::Rational(items) => items.iter().map(exif::Rational::to_f64).collect::<Vec<_>>(),
        Value::SRational(items) => items
            .iter()
            .map(exif::SRational::to_f64)
            .collect::<Vec<_>>(),
        _ => return None,
    };
    if nums.len() < 3 || nums.iter().any(|n| !n.is_finite()) {
        return None;
    }
    let mut deg = nums[0] + nums[1] / 60.0 + nums[2] / 3600.0;
    // 0,0 是「没定位」的写法（GPS 未锁定）而不是真的在几内亚湾
    if deg == 0.0 {
        return None;
    }
    if let Some(dir) = ascii(exif, r#ref, In::PRIMARY)
        && dir.to_uppercase().starts_with(negative)
    {
        deg = -deg;
    }
    Some(deg)
}

/// 小工具：`x.pipe(f)` 写法（`std` 没有 `pipe`，这里补一个）。
trait Pipe: Sized {
    fn pipe<R>(self, f: impl FnOnce(Self) -> R) -> R {
        f(self)
    }
}
impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 构造一个最小 TIFF（RW2/NEF/CR2/DNG 都是这个家族）──
    // 这样测试不依赖任何外部样本文件，且顺带把「裸 TIFF」这条路走通。

    struct Entry {
        tag: u16,
        ty: u16,
        count: u32,
        data: Vec<u8>,
    }

    fn ascii_entry(tag: u16, s: &str) -> Entry {
        let mut data = s.as_bytes().to_vec();
        data.push(0); // ASCII 类型要带结尾 NUL
        Entry {
            tag,
            ty: 2,
            count: data.len() as u32,
            data,
        }
    }

    fn short_entry(tag: u16, v: u16) -> Entry {
        Entry {
            tag,
            ty: 3,
            count: 1,
            data: v.to_le_bytes().to_vec(),
        }
    }

    fn long_entry(tag: u16, v: u32) -> Entry {
        Entry {
            tag,
            ty: 4,
            count: 1,
            data: v.to_le_bytes().to_vec(),
        }
    }

    fn rational_entry(tag: u16, num: u32, den: u32) -> Entry {
        let mut data = num.to_le_bytes().to_vec();
        data.extend_from_slice(&den.to_le_bytes());
        Entry {
            tag,
            ty: 5,
            count: 1,
            data,
        }
    }

    fn rational3_entry(tag: u16, nums: [(u32, u32); 3]) -> Entry {
        let mut data = Vec::new();
        for (n, d) in nums {
            data.extend_from_slice(&n.to_le_bytes());
            data.extend_from_slice(&d.to_le_bytes());
        }
        Entry {
            tag,
            ty: 5,
            count: 3,
            data,
        }
    }

    /// 拼一个「小端 TIFF + IFD0 + Exif 子 IFD」的最小文件。
    /// 拼一个「小端 TIFF + IFD0 + 若干子 IFD」的最小文件。
    ///
    /// `subs` 是 (IFD0 里的指针标签, 子 IFD 条目)：Exif 用 `0x8769`、GPS 用 `0x8825`。
    /// 真实相机（以及 RW2/NEF/CR2/DNG 这类 TIFF 家族 RAW）就是这么组织的。
    fn build_tiff_with(ifd0_in: Vec<Entry>, subs: Vec<(u16, Vec<Entry>)>) -> Vec<u8> {
        let mut ifd0 = ifd0_in;
        let mut subs: Vec<(u16, Vec<Entry>)> = subs
            .into_iter()
            .map(|(tag, mut es)| {
                es.sort_by_key(|e| e.tag);
                (tag, es)
            })
            .collect();
        subs.sort_by_key(|(tag, _)| *tag);
        for (tag, _) in &subs {
            ifd0.push(long_entry(*tag, 0)); // 指针先占位
        }
        ifd0.sort_by_key(|e| e.tag);

        let ifd_size = |n: usize| 2 + n * 12 + 4; // 条目数 + 每条 12 字节 + next 偏移
        let ifd0_off = 8usize;
        let mut cur = ifd0_off + ifd_size(ifd0.len());
        let mut sub_offsets: Vec<(u16, usize)> = Vec::new();
        for (tag, es) in &subs {
            sub_offsets.push((*tag, cur));
            cur += ifd_size(es.len());
        }
        let data_off = cur;
        for e in ifd0.iter_mut() {
            if let Some((_, off)) = sub_offsets.iter().find(|(t, _)| *t == e.tag) {
                e.data = (*off as u32).to_le_bytes().to_vec();
                e.count = 1;
            }
        }

        fn write_ifd(entries: &[Entry], out: &mut Vec<u8>, blobs: &mut Vec<u8>, data_off: usize) {
            out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
            for e in entries {
                out.extend_from_slice(&e.tag.to_le_bytes());
                out.extend_from_slice(&e.ty.to_le_bytes());
                out.extend_from_slice(&e.count.to_le_bytes());
                if e.data.len() <= 4 {
                    let mut pad = e.data.clone();
                    pad.resize(4, 0);
                    out.extend_from_slice(&pad);
                } else {
                    let at = data_off + blobs.len();
                    out.extend_from_slice(&(at as u32).to_le_bytes());
                    blobs.extend_from_slice(&e.data);
                    if blobs.len() % 2 == 1 {
                        blobs.push(0); // TIFF 要求字对齐
                    }
                }
            }
            out.extend_from_slice(&0u32.to_le_bytes()); // 没有下一个 IFD
        }

        let mut out = Vec::new();
        let mut blobs: Vec<u8> = Vec::new();
        out.extend_from_slice(b"II"); // 小端
        out.extend_from_slice(&0x002au16.to_le_bytes());
        out.extend_from_slice(&(ifd0_off as u32).to_le_bytes());
        write_ifd(&ifd0, &mut out, &mut blobs, data_off);
        for (_, es) in &subs {
            write_ifd(es, &mut out, &mut blobs, data_off);
        }
        out.extend_from_slice(&blobs);
        out
    }

    /// 只带 Exif 子 IFD 的快捷构造。
    fn build_tiff(ifd0: Vec<Entry>, exif_ifd: Vec<Entry>) -> Vec<u8> {
        build_tiff_with(ifd0, vec![(0x8769, exif_ifd)])
    }

    /// 只带 GPS 子 IFD 的快捷构造（GPS 标签住在自己的 IFD 里，不在 IFD0 也不在 Exif）。
    fn gps_tiff(gps_ifd: Vec<Entry>) -> Vec<u8> {
        build_tiff_with(Vec::new(), vec![(0x8825, gps_ifd)])
    }

    fn sample_tiff() -> Vec<u8> {
        build_tiff(
            vec![
                ascii_entry(0x010f, "Panasonic"), // Make
                ascii_entry(0x0110, "DC-S5M2"),   // Model
                short_entry(0x0112, 1),           // Orientation
                ascii_entry(0x0131, "Ver.1.0"),   // Software
                long_entry(0x0100, 6000),         // ImageWidth
                long_entry(0x0101, 4000),         // ImageLength
            ],
            vec![
                ascii_entry(0x9003, "2026:08:15 12:34:56"), // DateTimeOriginal
                ascii_entry(0x9011, "+08:00"),              // OffsetTimeOriginal
                ascii_entry(0xa434, "LUMIX S 20-60/F3.5-5.6"), // LensModel
                rational_entry(0x829a, 1, 250),             // ExposureTime = 1/250 s
                rational_entry(0x829d, 56, 10),             // FNumber = 5.6
                short_entry(0x8827, 800),                   // PhotographicSensitivity
                rational_entry(0x920a, 35, 1),              // FocalLength = 35mm
                long_entry(0xa002, 6000),                   // PixelXDimension
                long_entry(0xa003, 4000),                   // PixelYDimension
            ],
        )
    }

    // ── 裸 TIFF / RAW 路径 ──

    #[test]
    fn reads_a_bare_tiff_like_a_raw_file() {
        let data = read_bytes(&sample_tiff()).expect("应当能读裸 TIFF");
        assert_eq!(data.camera_make.as_deref(), Some("Panasonic"));
        assert_eq!(data.camera_model.as_deref(), Some("DC-S5M2"));
        assert_eq!(data.lens.as_deref(), Some("LUMIX S 20-60/F3.5-5.6"));
        assert_eq!(data.focal_mm, Some(35.0));
        assert!((data.f_number.unwrap() - 5.6).abs() < 1e-9);
        assert!(
            (data.exposure_ms.unwrap() - 4.0).abs() < 1e-9,
            "1/250 秒 = 4 毫秒"
        );
        assert_eq!(data.iso, Some(800));
        assert_eq!((data.width, data.height), (Some(6000), Some(4000)));
        assert_eq!(data.orientation, Some(1));
        assert_eq!(data.software.as_deref(), Some("Ver.1.0"));
        assert!(!data.is_empty());
    }

    #[test]
    fn taken_at_uses_the_offset_when_present() {
        let data = read_bytes(&sample_tiff()).unwrap();
        let t = data.taken_at.expect("应当有拍摄时间");
        assert_eq!(t.source, TakenAtSource::Exif);
        assert_eq!(t.offset_min, Some(480), "+08:00 = 480 分钟");
        // 东八区 12:34:56 = UTC 04:34:56
        assert_eq!(t.millis, time::from_civil(2026, 8, 15, 4, 34, 56).unwrap());
        assert_eq!(
            data.datetime_raw.as_deref(),
            Some("2026-08-15 12:34:56"),
            "display_value 会归一成这种形式"
        );
    }

    #[test]
    fn without_offset_the_wall_time_is_kept_as_is() {
        // 没有 OffsetTime*：墙上时间原样当 UTC 存，偏移留空
        let data = read_bytes(&build_tiff(
            vec![],
            vec![ascii_entry(0x9003, "2026:08:15 12:34:56")],
        ))
        .unwrap();
        let t = data.taken_at.unwrap();
        assert_eq!(t.offset_min, None);
        assert_eq!(
            t.millis,
            time::from_civil(2026, 8, 15, 12, 34, 56).unwrap(),
            "不能被本机时区改动"
        );
    }

    #[test]
    fn zero_datetime_means_not_set() {
        let data = read_bytes(&build_tiff(
            vec![],
            vec![ascii_entry(0x9003, "0000:00:00 00:00:00")],
        ))
        .unwrap();
        assert!(data.taken_at.is_none(), "相机没设时间 → 没有拍摄时间");
    }

    #[test]
    fn garbage_datetime_is_ignored() {
        for raw in [
            "",
            "not a date",
            "2026:13:45 99:99:99",
            "2026:02:31 10:00:00",
        ] {
            let data = read_bytes(&build_tiff(vec![], vec![ascii_entry(0x9003, raw)])).unwrap();
            assert!(data.taken_at.is_none(), "{raw:?} 不该被当成有效时间");
        }
    }

    #[test]
    fn no_exif_at_all_yields_empty_not_error() {
        assert!(read_bytes(b"not an image at all").is_none());
        assert!(read_bytes(&[]).is_none());
        assert!(read_file(Path::new("/definitely/not/here.jpg")).is_empty());
    }

    #[test]
    fn gps_is_converted_to_decimal_degrees() {
        let data = read_bytes(&gps_tiff(vec![
            rational3_entry(0x0002, [(22, 1), (18, 1), (0, 1)]), // 22°18'0" N
            ascii_entry(0x0001, "N"),
            rational3_entry(0x0004, [(114, 1), (10, 1), (12, 1)]), // 114°10'12" E
            ascii_entry(0x0003, "E"),
        ]))
        .unwrap();
        let gps = data.gps.expect("应当有坐标");
        assert!((gps.lat - 22.3).abs() < 1e-9, "lat={}", gps.lat);
        assert!((gps.lon - (114.0 + 10.0 / 60.0 + 12.0 / 3600.0)).abs() < 1e-9);
    }

    #[test]
    fn gps_south_west_is_negative() {
        let data = read_bytes(&gps_tiff(vec![
            rational3_entry(0x0002, [(33, 1), (52, 1), (0, 1)]),
            ascii_entry(0x0001, "S"),
            rational3_entry(0x0004, [(151, 1), (12, 1), (0, 1)]),
            ascii_entry(0x0003, "W"),
        ]))
        .unwrap();
        let gps = data.gps.unwrap();
        assert!(gps.lat < 0.0, "南纬应当是负数：{}", gps.lat);
        assert!(gps.lon < 0.0, "西经应当是负数：{}", gps.lon);
    }

    #[test]
    fn gps_zero_zero_means_no_fix() {
        let data = read_bytes(&build_tiff(
            vec![],
            vec![
                rational3_entry(0x0002, [(0, 1), (0, 1), (0, 1)]),
                ascii_entry(0x0001, "N"),
                rational3_entry(0x0004, [(0, 1), (0, 1), (0, 1)]),
                ascii_entry(0x0003, "E"),
            ],
        ))
        .unwrap();
        assert!(data.gps.is_none(), "0,0 是「没定位」，不是几内亚湾");
    }

    #[test]
    fn partial_gps_is_dropped() {
        // 只有纬度没有经度 → 整个丢弃（半条坐标没法用）
        let data = read_bytes(&build_tiff(
            vec![],
            vec![
                rational3_entry(0x0002, [(22, 1), (18, 1), (0, 1)]),
                ascii_entry(0x0001, "N"),
            ],
        ))
        .unwrap();
        assert!(data.gps.is_none());
    }

    #[test]
    fn read_file_probe_then_full_read_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.rw2");
        std::fs::write(&path, sample_tiff()).unwrap();

        // 探测长度够 → 只读开头就够（正常路径）
        let data = read_file_with_probe(&path, HEAD_PROBE_BYTES);
        assert_eq!(data.camera_model.as_deref(), Some("DC-S5M2"));

        // 探测长度给得极小（模拟「IFD 在文件更深处」）→ 必须回退到整读，仍然读得到
        for probe in [8usize, 64, 256] {
            let data = read_file_with_probe(&path, probe);
            assert_eq!(
                data.camera_model.as_deref(),
                Some("DC-S5M2"),
                "probe={probe} 时应当回退整读"
            );
            assert!(data.taken_at.is_some());
        }

        // 探测长度为 0 不能 panic（仍然应当回退整读）
        assert_eq!(
            read_file_with_probe(&path, 0).camera_model.as_deref(),
            Some("DC-S5M2")
        );

        // 不是图片：不 panic，返回空
        let junk = dir.path().join("junk.jpg");
        std::fs::write(&junk, b"hello world").unwrap();
        assert!(read_file(&junk).is_empty());

        // 文件不存在
        assert!(read_file(Path::new("/definitely/not/here.jpg")).is_empty());
    }

    // ── 时间来源判定 ──

    #[test]
    fn exif_wins_over_filename_and_mtime() {
        let data = read_bytes(&sample_tiff()).unwrap();
        let t = resolve_taken_at(Some(&data), "IMG_20200101_000000.jpg", Some(1)).unwrap();
        assert_eq!(t.source, TakenAtSource::Exif);
    }

    #[test]
    fn filename_is_used_when_exif_has_no_time() {
        let t = resolve_taken_at(None, "IMG_20260815_123456.jpg", Some(999)).unwrap();
        assert_eq!(t.source, TakenAtSource::Filename);
        assert_eq!(t.millis, time::from_civil(2026, 8, 15, 12, 34, 56).unwrap());
    }

    #[test]
    fn mtime_is_the_last_resort() {
        let t = resolve_taken_at(None, "DSC0001.JPG", Some(1_700_000_000_000)).unwrap();
        assert_eq!(t.source, TakenAtSource::FileMtime);
        assert_eq!(t.millis, 1_700_000_000_000);
        // 连修改时间都没有 → 没有
        assert!(resolve_taken_at(None, "DSC0001.JPG", None).is_none());
    }

    #[test]
    fn filename_time_patterns() {
        let expect = |name: &str, want: Option<i64>| {
            assert_eq!(from_filename(name), want, "{name}");
        };
        let dt = |y, mo, d, hh, mi, ss| time::from_civil(y, mo, d, hh, mi, ss);

        expect("IMG_20260815_123456.jpg", dt(2026, 8, 15, 12, 34, 56));
        expect("PXL_20260815_123456789.jpg", dt(2026, 8, 15, 12, 34, 56));
        expect("20260815_123456.jpg", dt(2026, 8, 15, 12, 34, 56));
        expect("20260815.jpg", dt(2026, 8, 15, 0, 0, 0));
        expect("20260815123456.jpg", dt(2026, 8, 15, 12, 34, 56));
        expect("VID_2026-08-15 12.34.56.mp4", dt(2026, 8, 15, 12, 34, 56));
        expect("照片20260815_1234.jpg", dt(2026, 8, 15, 12, 34, 0));
        // 没有日期 / 日期非法 → None
        expect("IMG_0001.jpg", None);
        expect("DSC_1234.JPG", None);
        expect("20261345_120000.jpg", None);
        expect("", None);
        // 超长数字串不该被当成日期（前一位是数字则跳过）
        expect("123456789.jpg", None);
    }

    #[test]
    fn datetime_parsing_tolerates_real_world_variants() {
        assert_eq!(
            parse_datetime("2026:08:15 12:34:56"),
            Some((2026, 8, 15, 12, 34, 56))
        );
        assert_eq!(
            parse_datetime("  2026:08:15 12:34:56  "),
            Some((2026, 8, 15, 12, 34, 56))
        );
        assert_eq!(
            parse_datetime("2026-08-15 12:34:56"),
            Some((2026, 8, 15, 12, 34, 56))
        );
        assert_eq!(parse_datetime("2026:08:15"), Some((2026, 8, 15, 0, 0, 0)));
        assert_eq!(parse_datetime("0000:00:00 00:00:00"), None);
        assert_eq!(parse_datetime(""), None);
        assert_eq!(parse_datetime("2026:08"), None);
    }

    #[test]
    fn offset_parsing() {
        assert_eq!(parse_offset("+08:00"), Some(480));
        assert_eq!(parse_offset("-05:00"), Some(-300));
        assert_eq!(parse_offset("+0800"), Some(480));
        assert_eq!(parse_offset("+00:00"), Some(0));
        assert_eq!(parse_offset("+8:00"), None, "位数不够");
        assert_eq!(parse_offset("08:00"), None, "缺符号");
        assert_eq!(parse_offset(""), None);
        assert_eq!(parse_offset("+99:00"), None, "时差不可能超过 14");
    }

    #[test]
    fn time_suffix_edges() {
        assert_eq!(parse_time_suffix(""), (0, 0, 0));
        assert_eq!(parse_time_suffix("_1"), (0, 0, 0), "不足 4 位当没有");
        assert_eq!(parse_time_suffix("_1234"), (12, 34, 0));
        assert_eq!(parse_time_suffix("_123456"), (12, 34, 56));
        assert_eq!(parse_time_suffix("_123456789"), (12, 34, 56), "毫秒被截掉");
    }
}
