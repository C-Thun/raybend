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
use rawler::RawLoader;
use rawler::decoders::{Decoder, RawDecodeParams};
use rawler::imgop::develop::{ProcessingStep, RawDevelop};
use rawler::rawsource::RawSource;

use super::backend::{DecodeRequest, PixelSource, RawBackend, RawError, RawImage8, RawResult};
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
/// 将来做显影管线时要的就是「到 `Calibrate` 为止的线性 f32」，届时用
/// `RawDevelop::new_with(&LINEAR_STEPS)` 即可（本模块已经把它拆成常量）。
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

/// rawler 后端。
#[derive(Debug, Default, Clone, Copy)]
pub struct RawlerBackend;

impl RawlerBackend {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl RawBackend for RawlerBackend {
    fn name(&self) -> &'static str {
        "rawler"
    }

    fn decode(&self, req: &DecodeRequest) -> RawResult<RawImage8> {
        // 分步计时（只在 `RAYBEND_RAW_TRACE=1` 时输出到 stderr —— 这条路径跑在
        // worker 进程里，stderr 是继承的，所以父进程的日志里能直接看到）
        let trace = std::env::var_os("RAYBEND_RAW_TRACE").is_some();
        let t0 = std::time::Instant::now();

        // 预检（本后端自己也做一道：它可能被单独测试/单跑，不能指望调用方已经做过）
        match precheck(&req.path) {
            Precheck::Ok { .. } => {}
            Precheck::Rejected(why) => return Err(RawError::NotRaw(why)),
        }
        let mark = |label: &str| {
            if trace {
                eprintln!("[raw] {label}: {:?}", t0.elapsed());
            }
        };
        mark("precheck");

        let source = RawSource::new(&req.path)
            .map_err(|e| RawError::Io(format!("{}：{e}", req.path.display())))?;
        mark("open");
        let loader = loader();
        mark("loader");
        let decoder = loader
            .get_decoder(&source)
            .map_err(|e| RawError::Unsupported(e.to_string()))?;
        mark("detect");
        let params = RawDecodeParams::default();

        let need = req.max_edge.unwrap_or(u32::MAX);

        if req.allow_preview
            && let Some(found) = pick_embedded(decoder.as_ref(), &source, &params, need)
        {
            let (img, source_kind) = found;
            mark("embedded");
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
                read_orientation(decoder.as_ref(), &source, &params)
            } else {
                None
            };
            mark("metadata");
            let out = finish(img, req.max_edge, source_kind, orientation);
            mark("finish");
            return out;
        }

        // ── 真实解码 ──
        let raw = decoder
            .raw_image(&source, &params, false)
            .map_err(|e| classify(e.to_string()))?;
        mark("raw_image");
        let orientation = Some(raw.orientation.to_u16());
        let develop = RawDevelop::new_with(BROWSING_STEPS);
        let intermediate = develop
            .develop_intermediate(&raw)
            .map_err(|e| RawError::Decode(e.to_string()))?;
        mark("develop");
        let img = intermediate
            .to_dynamic_image()
            .ok_or_else(|| RawError::Empty("显影结果无法转成图像".to_string()))?;
        let out = finish(img, req.max_edge, PixelSource::Decoded, orientation);
        mark("finish");
        out
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
