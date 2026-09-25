//! 首选后端：**rawler**（上游 [dnglab](https://github.com/dnglab/dnglab)，LGPL-2.1-only）。
//!
//! ⚠️ **本文件是唯一允许出现 rawler 类型的地方**（`AGENTS.md` §6.3）——
//! 它 API 不稳定且不遵循 SemVer，换后端时只改这一个文件。
//!
//! ⚠️ **只应在 worker 进程里调用**：rawler 的显影路径里有 `todo!()` / `panic!()` /
//! `unimplemented!()` 分支（CFA 形状意外、色彩矩阵长度不是 9、旋转与活跃区冲突……），
//! 这些在真实相机文件上不是理论风险。主进程解析 panic 的代价太大，进程隔离是对的。
//!
//! # 两条路径
//!
//! * **内嵌预览**：相机写在 RAW 里的 JPEG。取图 → 缩放。毫秒级。
//! * **真实解码**：`raw_image` 拿 CFA，再走 `RawDevelop`（缩放 → 去马赛克 → 富士旋转 →
//!   裁活跃区 → 白平衡 → 色彩校准 → 裁默认区 → sRGB 伽马）。秒级。
//!
//! 选择规则见 [`RawlerBackend::decode`]：能达到要求尺寸的内嵌图优先；
//! 差得不多（≥ 75%）也用（浏览场景下比完整解码划算得多）；差太多才真解码。

use std::path::Path;

use image::{DynamicImage, GenericImageView};
use rawler::RawImage;
use rawler::RawLoader;
use rawler::decoders::{Decoder, RawDecodeParams};
use rawler::imgop::develop::{Intermediate, ProcessingStep, RawDevelop};
use rawler::imgop::xyz::Illuminant;
use rawler::rawsource::RawSource;

use super::backend::{
    DecodeRequest, PixelSource, RawBackend, RawError, RawImage16, RawImage8, RawResult,
};
use super::precheck::{Precheck, precheck};

/// 内嵌预览**至少**要有目标尺寸的这个百分比才用它（否则真解码）。
///
/// 75% 是个取舍：内嵌预览普遍比完整解码的画质略差（压缩更狠），但现代相机的
/// 全尺寸预览质量相当好；为了浏览速度，宁可稍微放大一点也别让用户每次等一秒。
/// 想要绝对画质时（将来的显影 / 1:1），调用方给 `allow_preview = false`。
pub const PREVIEW_MIN_PERCENT: u32 = 75;

/// 浏览用显影步骤。
///
/// 刻意**不含**降噪 / 镜头校正 / 色调映射（那些属于编辑里程碑，见 `FUTURE.md` §D）。
/// 末了一步 `SRgb` 是伽马编码 —— 输出给显示器看的，不是线性数据。
/// 显影管线要的是「到 `Calibrate` 为止的线性 f32」——就是 [`LINEAR_STEPS`]。
const BROWSING_STEPS: &[ProcessingStep] = &[
    ProcessingStep::Rescale,
    ProcessingStep::Demosaic,
    ProcessingStep::FujiRotate,
    ProcessingStep::CropActiveArea,
    ProcessingStep::WhiteBalance,
    ProcessingStep::Calibrate,
    ProcessingStep::CropDefault,
    ProcessingStep::SRgb,
];

/// **显影管线用的步骤**（M3-W3）：与浏览那条**只差最后一步** —— 不做 sRGB 伽马。
///
/// 于是输出的就是「线性 sRGB f32」（`Calibrate` 已经把相机空间映射到 sRGB 原色，
/// 见 `map_3ch_to_rgb`），再转成 u16 交给 `develop::pipeline`。
///
/// 两条步骤表**必须只差 `SRgb` 一项**：这是「浏览看到的图」与「显影管线的输入」
/// 来自同一套解码的保证（否则同一张 RAW 在两个地方会解出不同的像素）。
const LINEAR_STEPS: &[ProcessingStep] = &[
    ProcessingStep::Rescale,
    ProcessingStep::Demosaic,
    ProcessingStep::FujiRotate,
    ProcessingStep::CropActiveArea,
    ProcessingStep::WhiteBalance,
    ProcessingStep::Calibrate,
    ProcessingStep::CropDefault,
];

/// 找色彩矩阵时优先要的照明（与 `develop_intermediate` 里的 `Calibrate` 同一张表）。
const CALIBRATION_ILLUMINANTS: &[Illuminant] = &[
    Illuminant::D65,
    Illuminant::A,
    Illuminant::B,
    Illuminant::C,
    Illuminant::D50,
    Illuminant::D55,
    Illuminant::D75,
    Illuminant::Daylight,
    Illuminant::Flash,
];

/// rawler 后端。
#[derive(Debug, Default, Clone, Copy)]
pub struct RawlerBackend;

impl RawlerBackend {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// 只解析 RAW 头和厂商 MakerNote；镜头识别留在隔离 worker 内。
    pub fn lens_name(path: &Path) -> RawResult<Option<String>> {
        let opened = open(&DecodeRequest::full(path))?;
        let metadata = opened.decoder.raw_metadata(&opened.source, &opened.params)
            .map_err(|e| classify(e.to_string()))?;
        Ok(metadata.exif.lens_model
            .or_else(|| metadata.lens.map(|lens| lens.lens_name))
            .filter(|name| !name.trim().is_empty()))
    }
}

impl RawBackend for RawlerBackend {
    fn name(&self) -> &'static str {
        "rawler"
    }

    fn decode(&self, req: &DecodeRequest) -> RawResult<RawImage8> {
        let trace = Tracer::new();
        let opened = open(req)?;
        let Opened {
            source,
            decoder,
            params,
        } = &opened;
        trace.mark("open");
        let need = req.max_edge.unwrap_or(u32::MAX);

        if req.allow_preview
            && let Some(found) = pick_embedded(decoder.as_ref(), source, params, need)
        {
            let (img, source_kind) = found;
            trace.mark("embedded");
            /*
             * 方向：**只在读不到外层 EXIF 的容器上才花这 50ms**。
             *
             * `raw_metadata` 实测要 ~50ms（要重走一道 IFD），而 TIFF 家族（RW2/NEF/ARW/DNG/ORF…）
             * 的 EXIF 在主进程用 kamadak-exif 读文件头就有了（缩略图管线一直这么做）。
             * CR3 是 ISO-BMFF，外层读不到 EXIF，只能靠 rawler —— 所以只给它付这笔钱。
             *
             * 拿不到方向不会让图崩，最多是「竖拍看起来是躺着的」；管线那边拿不到方向会按 1 处理。
             */
            let orientation = if needs_metadata_orientation(&req.path) {
                read_orientation(decoder.as_ref(), source, params)
            } else {
                None
            };
            trace.mark("metadata");
            let out = finish(img, req.max_edge, source_kind, orientation);
            trace.mark("finish");
            return out;
        }

        // ── 真实解码（8bit sRGB：黑电平 / 白平衡 / 色彩矩阵 / sRGB 伽马）──
        let raw = decoder
            .raw_image(source, params, false)
            .map_err(|e| classify(e.to_string()))?;
        trace.mark("raw_image");
        let orientation = Some(raw.orientation.to_u16());
        let develop = RawDevelop::new_with(BROWSING_STEPS);
        let intermediate = develop
            .develop_intermediate(&raw)
            .map_err(|e| RawError::Decode(e.to_string()))?;
        trace.mark("develop");
        let img = intermediate
            .to_dynamic_image()
            .ok_or_else(|| RawError::Empty("显影结果无法转成图像".to_string()))?;
        let out = finish(img, req.max_edge, PixelSource::Decoded, orientation);
        trace.mark("finish");
        out
    }

    /// **线性解码**（M3-W3 的显影输入）：与 `decode` 同一条链，只是不做最后那步 sRGB 伽马。
    ///
    /// 输出是线性 sRGB 的 u16（见 [`RawImage16`]），外加**拍摄色温估计**
    /// （色温拉杆的基线，`AGENTS.md` §11.5）。
    fn decode_linear(&self, req: &DecodeRequest) -> RawResult<RawImage16> {
        let trace = Tracer::new();
        let opened = open(req)?;
        let Opened {
            source,
            decoder,
            params,
        } = &opened;
        trace.mark("open");

        // 线性这条路**不走内嵌预览**：相机写的 JPEG 已经被机内处理过，
        // 拿它当「场景参考」的数据源等于自欺（`FUTURE.md` D1）。
        let raw = decoder
            .raw_image(source, params, false)
            .map_err(|e| classify(e.to_string()))?;
        trace.mark("raw_image");

        // 色温估计要在 `raw` 还在的时候算（后面 develop 只是借用它）
        let as_shot_temperature = as_shot_temperature(&raw);
        let orientation = Some(raw.orientation.to_u16());
        let develop = RawDevelop::new_with(LINEAR_STEPS);
        let intermediate = develop
            .develop_intermediate(&raw)
            .map_err(|e| RawError::Decode(e.to_string()))?;
        trace.mark("develop");

        let mut image = linear_from_intermediate(intermediate, orientation, as_shot_temperature)?;
        if let Some(max_edge) = req.max_edge.filter(|edge| *edge > 0) {
            let long = image.width.max(image.height);
            if long > max_edge {
                image = downscale(image, max_edge);
            }
        }
        trace.mark("finish");
        Ok(image)
    }
}

/// 打开一次解码所需的全部东西（两条解码路径共用）。
struct Opened {
    source: RawSource,
    decoder: Box<dyn Decoder>,
    params: RawDecodeParams,
}

/// 预检 + 打开 + 认容器 + 取参数（**两条路径共用的唯一一份**）。
fn open(req: &DecodeRequest) -> RawResult<Opened> {
    // 预检（本后端自己也做一道：它可能被单独测试/单跑，不能指望调用方已经做过）
    match precheck(&req.path) {
        Precheck::Ok { .. } => {}
        Precheck::Rejected(why) => return Err(RawError::NotRaw(why)),
    }
    let source = RawSource::new(&req.path)
        .map_err(|e| RawError::Io(format!("{}：{e}", req.path.display())))?;
    let loader = loader();
    let decoder = loader
        .get_decoder(&source)
        .map_err(|e| RawError::Unsupported(e.to_string()))?;
    Ok(Opened {
        source,
        decoder,
        params: RawDecodeParams::default(),
    })
}

/// 分步计时（只在 `RAYBEND_RAW_TRACE=1` 时输出到 stderr —— 这条路径跑在
/// worker 进程里，stderr 是继承的，所以父进程的日志里能直接看到）。
struct Tracer {
    enabled: bool,
    started: std::time::Instant,
}

impl Tracer {
    fn new() -> Self {
        Self {
            enabled: std::env::var_os("RAYBEND_RAW_TRACE").is_some(),
            started: std::time::Instant::now(),
        }
    }

    fn mark(&self, label: &str) {
        if self.enabled {
            eprintln!("[raw] {label}: {:?}", self.started.elapsed());
        }
    }
}

/// `Intermediate`（f32）→ [`RawImage16`]（线性 u16）。
///
/// 走 `to_dynamic_image()` 再 `into_raw()`：**不做第二份拷贝**（`DynamicImage` 的
/// 16bit 变体内部就是 `ImageBuffer`，`into_raw()` 直接把 `Vec` 交出来）。
/// 单色（去马赛克关掉 / 单色传感器）与四通道（四色滤镜）都归一成三通道。
fn linear_from_intermediate(
    intermediate: Intermediate,
    orientation: Option<u16>,
    as_shot_temperature: Option<f32>,
) -> RawResult<RawImage16> {
    let Some(image) = intermediate.to_dynamic_image() else {
        return Err(RawError::Empty("显影结果无法转成图像".to_string()));
    };
    let (width, height, rgb) = match image {
        DynamicImage::ImageRgb16(buffer) => {
            let (width, height) = buffer.dimensions();
            (width, height, buffer.into_raw())
        }
        DynamicImage::ImageRgba16(buffer) => {
            let (width, height) = buffer.dimensions();
            // 丢掉 alpha（管线不吃它），保留前三个通道
            let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
            for pixel in buffer.as_raw().as_chunks::<4>().0 {
                rgb.extend_from_slice(&pixel[..3]);
            }
            (width, height, rgb)
        }
        DynamicImage::ImageLuma16(buffer) => {
            let (width, height) = buffer.dimensions();
            let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
            for value in buffer.as_raw() {
                rgb.extend_from_slice(&[*value, *value, *value]);
            }
            (width, height, rgb)
        }
        other => {
            // `to_dynamic_image` 只会给 16bit 的三种形态；真出现别的说明上游变了
            return Err(RawError::Empty(format!(
                "线性解码拿到了意外的像素形态：{:?}",
                other.color()
            )));
        }
    };
    let out = RawImage16 {
        width,
        height,
        rgb,
        source: PixelSource::Decoded,
        orientation,
        as_shot_temperature,
    };
    if !out.is_consistent() {
        return Err(RawError::Empty(format!(
            "线性解码结果尺寸不合法：{width}×{height}"
        )));
    }
    Ok(out)
}

/// 从相机元数据估**拍摄色温**（K）—— 色温拉杆的基线。
///
/// 做法（与 `develop::color` 里的数学同一份）：
/// 相机中性 = `(1/wb_r, 1/wb_g, 1/wb_b)`（`wb_coeffs` 是「把这张照片的照明变成中性」的倍率），
/// 乘上色彩矩阵的逆得到 XYZ，取色度再反查色温。
///
/// 读不到 / 算不出就返回 `None`（调用方退回默认值）—— **不许猜**。
fn as_shot_temperature(raw: &RawImage) -> Option<f32> {
    let (_, matrix) = raw.color_matrix_find_first(CALIBRATION_ILLUMINANTS.iter().copied())?;
    if matrix.len() < 9 {
        return None;
    }
    let xyz_to_cam = [
        [matrix[0], matrix[1], matrix[2]],
        [matrix[3], matrix[4], matrix[5]],
        [matrix[6], matrix[7], matrix[8]],
    ];
    let neutral = camera_neutral_from_wb(&raw.wb_coeffs)?;
    crate::develop::color::cct_from_camera_neutral(neutral, xyz_to_cam)
}

/// `wb_coeffs` → **相机空间的中性方向**（= 照明在相机里的响应）。
///
/// ❗ 只看**前三个**系数：四色传感器才有第 4 个，三色机器上它是 `NaN`
/// （实测 Panasonic DC-G9 的 RW2：`[2.0859375, 1.0, 2.1953125, NaN]`）。
/// 早先对整个数组查 `is_finite` ⇒ **所有三色机器都算不出色温**（基线永远是 `None`）。
///
/// `wb_coeffs` 的语义是「把这张照片的照明乘成中性」的倍率，所以照明的相机响应
/// 就是它的**倒数**（绿归一为 1，与 rawler 的口径一致）。
fn camera_neutral_from_wb(wb: &[f32; 4]) -> Option<[f32; 3]> {
    let rgb = [wb[0], wb[1], wb[2]];
    if !rgb.iter().all(|value| value.is_finite()) {
        return None;
    }
    if rgb.iter().any(|value| value.abs() < 1e-6) {
        return None;
    }
    Some([1.0 / rgb[0], 1.0 / rgb[1], 1.0 / rgb[2]])
}

/// 线性图的快速降采样（箱式平均，与 `develop::pipeline` 同一份实现）。
fn downscale(image: RawImage16, long_edge: u32) -> RawImage16 {
    let Some(linear) = crate::develop::LinearImage::new(image.width, image.height, image.rgb.clone())
    else {
        return image;
    };
    let small = linear.downscaled_to(long_edge);
    RawImage16 {
        width: small.width,
        height: small.height,
        rgb: small.rgb,
        source: image.source,
        orientation: image.orientation,
        as_shot_temperature: image.as_shot_temperature,
    }
}

/// 全局共享的 `RawLoader`（它内部要建相机数据库，**建一次就够**）。
///
/// worker 是常驻进程，把 loader 缓存在这里可以省掉每张图重复建库的开销。
fn loader() -> &'static RawLoader {
    static LOADER: std::sync::OnceLock<RawLoader> = std::sync::OnceLock::new();
    LOADER.get_or_init(RawLoader::new)
}

/// 挑一张够用的内嵌图。
///
/// 顺序：先问小的（`thumbnail_image`，解码便宜），不够大再问大的（`preview_image`）；
/// 都不够大就把**最大的那张**带回去（够 75% 就用它，否则调用方去真解码）。
///
/// # 这个 75% 阀值是**观感分岔点**（人类 2026-09-17 定：保持现状）
///
/// 两条路的颜色与亮度**本来就不一样**：
///
/// * 内嵌预览是**相机自己处理的**（带机内风格），看起来像机内 JPEG；
/// * 完整解码是**我们自己的最简处理**（黑电平/白平衡/色彩矩阵，无机内风格、无色彩管理）。
///
/// 于是同一张 RAW 在小档位（预览）与大档位（完整解码）之间会「跳一下」——
/// 实测症状：双击点开 RAW 时看到模糊图变清晰的同时颜色/亮度也变了。
/// **这是有意保留的**（崔总 2026-09-17 拍）：看图档位要的是清晰度。
///
/// 真正的收敛点是「Rust 按规范渲染出位图 + 渲染结果缓存」（`FUTURE.md` C 段、
/// W3 显影视口），那时两条路会并成一条。在此之前**别把这里当 bug 改**：
/// 把阀值提到 100% 会让看图永远吃预览（糊），降下来会让小图也走完整解码（慢）。
fn pick_embedded(
    decoder: &dyn Decoder,
    source: &RawSource,
    params: &RawDecodeParams,
    need: u32,
) -> Option<(DynamicImage, PixelSource)> {
    let mut best: Option<DynamicImage> = None;

    // `thumbnail_image` 通常是几百像素的小图；先问它，命中就完全不必解码大预览
    for candidate in [
        decoder.thumbnail_image(source, params),
        decoder.preview_image(source, params),
    ] {
        let Ok(Some(img)) = candidate else { continue };
        let long = long_edge(&img);
        if long >= need {
            return Some((img, PixelSource::EmbeddedPreview));
        }
        if best.as_ref().is_none_or(|b| long_edge(b) < long) {
            best = Some(img);
        }
    }

    if let Some(img) = best {
        let long = long_edge(&img);
        // 够了 75%：放大一点用（浏览场景比「等一次完整解码」划算）
        if long.saturating_mul(100) >= need.saturating_mul(PREVIEW_MIN_PERCENT) {
            return Some((img, PixelSource::EmbeddedPreview));
        }
    }
    None
}

/// 从 RAW 自己的元数据里取 EXIF 方向 —— CR3 这类容器读不到外层 EXIF，只能靠它。
fn read_orientation(
    decoder: &dyn Decoder,
    source: &RawSource,
    params: &RawDecodeParams,
) -> Option<u16> {
    decoder
        .raw_metadata(source, params)
        .ok()
        .and_then(|md| md.exif.orientation)
}

/// 缩放到目标尺寸并转成 8 位 RGB。
fn finish(
    img: DynamicImage,
    max_edge: Option<u32>,
    source: PixelSource,
    orientation: Option<u16>,
) -> RawResult<RawImage8> {
    let img = match max_edge {
        Some(max) if max > 0 && long_edge(&img) > max => resize_to_long_edge(&img, max),
        _ => img,
    };
    let rgb = img.to_rgb8();
    let (width, height) = rgb.dimensions();
    if width == 0 || height == 0 {
        return Err(RawError::Empty("尺寸为 0".to_string()));
    }
    Ok(RawImage8 {
        width,
        height,
        rgb: rgb.into_raw(),
        source,
        orientation,
    })
}

fn long_edge(img: &DynamicImage) -> u32 {
    let (w, h) = img.dimensions();
    w.max(h)
}

/// 这个文件的方向**只能**从 rawler 的元数据里拿吗？
///
/// 只有 CR3（ISO-BMFF）是这样：它的 EXIF 在 BMFF box 里，主进程的 kamadak-exif 读不到。
/// TIFF 家族（含 RW2/NEF/ARW/DNG/ORF）外层就能读到，不必多花 50ms。
fn needs_metadata_orientation(path: &Path) -> bool {
    path.extension()
        .map(|e| e.eq_ignore_ascii_case("cr3"))
        .unwrap_or(false)
}

/// 等比例缩放到长边 = `max`。
///
/// 用 **`thumbnail`（区域平均）而不是 Lanczos3**：实测同一张 RW2 的内嵌预览，
/// Lanczos3 要 ~195ms、区域平均要几毫秒 —— 而这里只是**第一步降采样**，
/// 最终尺寸的 Lanczos 由缩略图管线在很小的图上做（两道降采样的画质通常还更好：
/// 大比例下 Lanczos 容易出振铃，区域平均是干净的抗锯齿）。
fn resize_to_long_edge(img: &DynamicImage, max: u32) -> DynamicImage {
    let (w, h) = img.dimensions();
    let scale = f64::from(max) / f64::from(w.max(h));
    // 至少 1 像素 —— 极窄的图（全景）按比例算出来的短边可能被截成 0
    let nw = ((f64::from(w) * scale).round() as u32).max(1);
    let nh = ((f64::from(h) * scale).round() as u32).max(1);
    img.thumbnail_exact(nw, nh)
}

/// rawler 的 `DecoderFailed` 消息里既有「文件损坏」也有「相机不支持」，
/// 按关键词粗分一下，只为了日志好读（用户看到的文案在同一层统一处理）。
fn classify(message: String) -> RawError {
    let lower = message.to_lowercase();
    if lower.contains("unsupported") || lower.contains("model") || lower.contains("make") {
        RawError::Unsupported(message)
    } else {
        RawError::Decode(message)
    }
}

/// `RawSource` 在 Windows 上用 mmap；这里只核对文件确实存在（预检已做，这里是兜底）。
#[must_use]
pub fn is_readable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 假的 RAW 文件（TIFF 头 + 填充）：预检能过，解码必然失败 —— 用来验证
    /// 「失败是干净的错误，不是 panic、不是崩溃」。
    fn fake_raw(path: &Path) {
        let mut bytes = b"II\x2a\x00".to_vec();
        bytes.resize(8192, 0);
        std::fs::write(path, &bytes).unwrap();
    }

    fn close(a: f32, b: f32, tolerance: f32) -> bool {
        (a - b).abs() <= tolerance
    }

    #[test]
    fn missing_file_is_not_raw_error() {
        let backend = RawlerBackend::new();
        let err = backend
            .decode(&DecodeRequest::thumb("/definitely/not/here.RW2", 256))
            .unwrap_err();
        assert!(matches!(err, RawError::NotRaw(_)), "应为 NotRaw：{err:?}");
    }

    #[test]
    fn a_non_raw_file_is_rejected_before_decoding() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, b"hello, this is not a photo at all").unwrap();
        let backend = RawlerBackend::new();
        let err = backend
            .decode(&DecodeRequest::thumb(&path, 256))
            .unwrap_err();
        assert!(matches!(err, RawError::NotRaw(_)), "应为 NotRaw：{err:?}");
    }

    #[test]
    fn a_fake_raw_fails_cleanly_without_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake.RW2");
        fake_raw(&path);
        let backend = RawlerBackend::new();
        // 不关心是哪一类错误（rawler 不同版本措辞不同），只要求**是错误**、不 panic
        let result = backend.decode(&DecodeRequest::thumb(&path, 256));
        assert!(result.is_err(), "假 RAW 不该解出图来");
    }

    #[test]
    fn is_readable_checks_regular_files_only() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_readable(dir.path()), "目录不算可读的 RAW");
        let file = dir.path().join("a.RW2");
        fake_raw(&file);
        assert!(is_readable(&file));
    }

    #[test]
    fn resize_keeps_aspect_and_never_zero() {
        let img = DynamicImage::new_rgb8(4000, 1000);
        let out = resize_to_long_edge(&img, 1000);
        assert_eq!(out.dimensions(), (1000, 250));

        // 极端窄：短边不得被算成 0
        let thin = DynamicImage::new_rgb8(6000, 10);
        let out = resize_to_long_edge(&thin, 1000);
        assert!(out.height() >= 1, "短边至少 1 像素：{:?}", out.dimensions());
    }

    #[test]
    fn only_cr3_needs_the_expensive_metadata_read() {
        assert!(needs_metadata_orientation(Path::new("/x/IMG.CR3")));
        assert!(needs_metadata_orientation(Path::new("/x/IMG.cr3")));
        for name in ["IMG.RW2", "IMG.NEF", "IMG.ARW", "IMG.DNG", "IMG.ORF"] {
            assert!(
                !needs_metadata_orientation(Path::new(name)),
                "{name} 外层就能读到 EXIF，不该再花 50ms"
            );
        }
    }

    #[test]
    fn camera_neutral_survives_the_nan_fourth_coefficient() {
        // 真机实测值（Panasonic DC-G9 的 RW2）：第 4 个系数是 NaN ——
        // 早先的写法在这里返回 None，于是**所有三色机器都没有色温基线**。
        let neutral = camera_neutral_from_wb(&[2.085_937_5, 1.0, 2.195_312_5, f32::NAN])
            .expect("前三个系数有效就该算得出来");
        assert!(close(neutral[0], 1.0 / 2.085_937_5, 1e-6));
        assert!(close(neutral[1], 1.0, 1e-6));
        assert!(close(neutral[2], 1.0 / 2.195_312_5, 1e-6));
        // 前三个里出现非法值 / 0 才算读不出来
        assert!(camera_neutral_from_wb(&[f32::NAN, 1.0, 1.0, 1.0]).is_none());
        assert!(camera_neutral_from_wb(&[2.0, 0.0, 2.0, 1.0]).is_none());
    }

    #[test]
    fn as_shot_temperature_follows_a_known_illuminant() {
        // 造一张「相机空间 = sRGB」的假图：相机中性 = 某色温的白 ⇒ 反查要回到那个色温
        use crate::develop::color::{SRGB_TO_XYZ_D65, XYZ_TO_SRGB_D65, kelvin_to_xy, mat3_inverse, mat3_vec3, xy_to_xyz};
        let xyz_to_cam = mat3_inverse(SRGB_TO_XYZ_D65).expect("可逆");
        let matrix: Vec<f32> = xyz_to_cam.iter().flatten().copied().collect();
        for kelvin in [3000.0f32, 5000.0, 6500.0] {
            let (x, y) = kelvin_to_xy(kelvin);
            let white = mat3_vec3(XYZ_TO_SRGB_D65, xy_to_xyz(x, y));
            let wb = [1.0 / white[0], 1.0, 1.0 / white[2], f32::NAN];
            let neutral = camera_neutral_from_wb(&wb).expect("有效");
            let back = crate::develop::color::cct_from_camera_neutral(neutral, xyz_to_cam)
                .expect("能反查");
            // 容差 5%：这是个**估计值**（只用来把拉杆挪到位），不是色彩学意义上的精确 CCT ——
            // 近似公式 + sRGB 矩阵往返都会带几十到一百多 K 的误差（3000K 实测差 ~100K）。
            assert!(
                (back - kelvin).abs() / kelvin < 0.05,
                "{kelvin}K 反查成 {back}K"
            );
        }
        let _ = &matrix;
    }

    #[test]
    fn classify_maps_unsupported_wording() {
        assert!(matches!(
            classify("Unsupported camera model".to_string()),
            RawError::Unsupported(_)
        ));
        assert!(matches!(
            classify("Failed to decode".to_string()),
            RawError::Decode(_)
        ));
    }
}
