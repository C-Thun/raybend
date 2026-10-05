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
use rawler::imgop::chromatic_adaption::adapt_bradford;
use rawler::imgop::develop::{Intermediate, ProcessingStep, RawDevelop};
use rawler::imgop::matrix::{IDENTITY_MATRIX_3, multiply, normalize, pseudo_inverse, transform_1d};
use rawler::imgop::xyz::Illuminant;
use rawler::rawsource::RawSource;

use super::backend::{
    DecodeRequest, PixelSource, RawBackend, RawError, RawImage8, RawImage16, RawResult,
    RawWorkingImage,
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
/// 刻意**不含**降噪 / 镜头校正 / 色调映射（那些属于编辑里程碑，见 `memory/FUTURE.md` §D）。
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

/// 新版工作域停在 `Calibrate` 之前：该上游步骤会把负值裁到 0，并对 >1
/// 执行非线性色度压缩。保持去马赛克/裁切一致，随后由本 adapter 映射到 Rec.2020。
const WORKING_STEPS: &[ProcessingStep] = &[
    ProcessingStep::Rescale,
    ProcessingStep::Demosaic,
    ProcessingStep::FujiRotate,
    ProcessingStep::CropActiveArea,
    ProcessingStep::WhiteBalance,
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

    /// Metadata/dummy decode supplies the exact selected matrix without sensor
    /// decompression, demosaic, f32 image allocation or transfer to the host.
    pub fn working_color_identity(path:&Path)->RawResult<crate::color::ProfileId> {
        let opened=open(&DecodeRequest::full(path))?;
        let raw=opened.decoder.raw_image(&opened.source,&opened.params,true).map_err(|e|classify(e.to_string()))?;
        if !matches!(raw.cpp,1|3|4) {return Err(RawError::Unsupported("RAW 通道数不支持工作域".into()));}
        if let Some((illuminant,matrix))=selected_matrix(&raw) {
            match matrix.len() {9=>{d65_matrix::<3>(&matrix,illuminant)?;},12=>{d65_matrix::<4>(&matrix,illuminant)?;},_=>return Err(RawError::Unsupported("RAW 相机矩阵维度不支持工作域".into()))};
        }
        Ok(working_matrix_id(&camera_matrix_id(&raw)))
    }

    /// 只解析 RAW 头和厂商 MakerNote；镜头识别留在隔离 worker 内。
    pub fn lens_name(path: &Path) -> RawResult<Option<String>> {
        let opened = open(&DecodeRequest::full(path))?;
        let metadata = opened
            .decoder
            .raw_metadata(&opened.source, &opened.params)
            .map_err(|e| classify(e.to_string()))?;
        Ok(metadata
            .exif
            .lens_model
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
        if req.embedded_only
            && let Some(embedded) = crate::media::embedded::read(&req.path)
            && let Ok(image) =
                image::load_from_memory_with_format(&embedded.bytes, image::ImageFormat::Jpeg)
        {
            return finish(
                image,
                req.max_edge,
                PixelSource::EmbeddedPreview,
                embedded.orientation,
            );
        }
        let opened = open(req)?;
        let Opened {
            source,
            decoder,
            params,
        } = &opened;
        trace.mark("open");
        let need = req.max_edge.unwrap_or(u32::MAX);

        if req.allow_preview
            && let Some(found) = pick_embedded(
                (0..2)
                    .map(|index| {
                        if index == 0 {
                            decoder.thumbnail_image(source, params)
                        } else {
                            decoder.preview_image(source, params)
                        }
                    })
                    .map(|candidate| candidate.ok().flatten()),
                need,
                req.embedded_only,
            )
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
            let orientation = if req.embedded_only && !needs_metadata_orientation(&req.path) {
                // 兼容兜底仍只读有界头部，不让缺 EXIF 导致整 RAW 扫描。
                use std::io::Read;
                let mut head = Vec::new();
                std::fs::File::open(&req.path)
                    .ok()
                    .and_then(|file| file.take(256 * 1024).read_to_end(&mut head).ok());
                crate::media::tiff::parse(&head)
                    .and_then(|info| info.orientation)
                    .map(|value| crate::media::meta::normalize_orientation(Some(value)))
            } else if needs_metadata_orientation(&req.path) {
                read_orientation(decoder.as_ref(), source, params)
            } else {
                None
            };
            trace.mark("metadata");
            let out = finish(img, req.max_edge, source_kind, orientation);
            trace.mark("finish");
            return out;
        }

        if req.embedded_only {
            return Err(RawError::Empty("RAW 没有可用内嵌图".into()));
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
        // 拿它当「场景参考」的数据源等于自欺（`memory/FUTURE.md` D1）。
        let raw = decoder
            .raw_image(source, params, false)
            .map_err(|e| classify(e.to_string()))?;
        trace.mark("raw_image");

        // 色温估计要在 `raw` 还在的时候算（后面 develop 只是借用它）
        let as_shot_temperature = as_shot_temperature(&raw);
        let camera_matrix_id = camera_matrix_id(&raw);
        let orientation = Some(raw.orientation.to_u16());
        let develop = RawDevelop::new_with(LINEAR_STEPS);
        let intermediate = develop
            .develop_intermediate(&raw)
            .map_err(|e| RawError::Decode(e.to_string()))?;
        trace.mark("develop");

        let mut image = linear_from_intermediate(
            intermediate,
            orientation,
            as_shot_temperature,
            camera_matrix_id,
        )?;
        if let Some(max_edge) = req.max_edge.filter(|edge| *edge > 0) {
            let long = image.width.max(image.height);
            if long > max_edge {
                image = downscale(image, max_edge);
            }
        }
        trace.mark("finish");
        Ok(image)
    }

    fn decode_working(&self, req: &DecodeRequest) -> RawResult<RawWorkingImage> {
        let opened = open(req)?;
        let mut raw = opened
            .decoder
            .raw_image(&opened.source, &opened.params, false)
            .map_err(|error| classify(error.to_string()))?;
        let as_shot_temperature = as_shot_temperature(&raw);
        let legacy_id = camera_matrix_id(&raw);
        let current_id = working_matrix_id(&legacy_id);
        let legacy = use_legacy_scaling(req.expected_camera_matrix_id.as_ref(), &legacy_id, &current_id)?;
        let camera_matrix_id = if legacy { legacy_id } else { current_id };
        if !legacy { normalize_raw_unbounded(&mut raw)?; }
        let orientation = Some(raw.orientation.to_u16());
        let steps: Vec<_> = WORKING_STEPS.iter().copied()
            .filter(|step| legacy || *step != ProcessingStep::Rescale).collect();
        let intermediate = RawDevelop::new_with(&steps)
            .develop_intermediate(&raw)
            .map_err(|error| RawError::Decode(error.to_string()))?;
        // Only metadata is needed after demosaic; release full sensor storage now.
        raw.data = rawler::RawImageData::Float(Vec::new());
        let image = camera_intermediate_to_working(intermediate, &raw)?
            .into_downscaled(req.max_edge.unwrap_or(0));
        Ok(RawWorkingImage {
            image,
            source: PixelSource::Decoded,
            orientation,
            as_shot_temperature,
            camera_matrix_id,
        })
    }
}

fn working_matrix_id(legacy: &crate::color::ProfileId) -> crate::color::ProfileId {
    crate::color::ProfileId::of_bytes(format!("rawler-0.8-unbounded-scale-v2\0{}", legacy.as_str()).as_bytes())
}
fn use_legacy_scaling(expected: Option<&crate::color::ProfileId>, legacy: &crate::color::ProfileId, current: &crate::color::ProfileId) -> RawResult<bool> {
    match expected {
        Some(id) if id == legacy => Ok(true),
        Some(id) if id != current => Err(RawError::Unsupported("固化 RAW 相机矩阵/标度身份不匹配".into())),
        _ => Ok(false),
    }
}

/// Adapter replaces upstream's lower clipping only. BlackLevelRepeatDim is
/// honored for odd dimensions, CFA patterns and interleaved linear/mono data.
/// White level is normalization, not an instruction to clip saturated samples.
fn normalize_raw_unbounded(raw: &mut RawImage) -> RawResult<()> {
    let black = raw.blacklevel.as_vec();
    let white = raw.whitelevel.as_vec();
    let grid = (raw.blacklevel.width, raw.blacklevel.height, raw.blacklevel.cpp);
    let shape = (raw.width, raw.height, raw.cpp);
    let cfa = matches!(raw.photometric, rawler::rawimage::RawPhotometricInterpretation::Cfa(_));
    let mut pixels = match std::mem::replace(&mut raw.data, rawler::RawImageData::Float(Vec::new())) {
        rawler::RawImageData::Integer(values) => values.into_iter().map(f32::from).collect(),
        rawler::RawImageData::Float(values) => values,
    };
    normalize_sensor(&mut pixels, shape, grid, &black, &white, cfa)?;
    raw.data = rawler::RawImageData::Float(pixels);
    // Rescale is omitted for this revision; downstream receives normalized data.
    Ok(())
}
fn normalize_sensor(pixels: &mut [f32], (width,height,cpp): (usize,usize,usize), (bw,bh,bc): (usize,usize,usize), black: &[f32], white: &[f32], cfa: bool) -> RawResult<()> {
    let invalid = || RawError::Unsupported("RAW 黑/白电平网格、通道或范围无效".into());
    if width == 0 || height == 0 || !matches!(cpp,1|3|4) || bw == 0 || bh == 0 || !matches!(bc,1) && bc != cpp
        || width.checked_mul(height).and_then(|v|v.checked_mul(cpp)) != Some(pixels.len())
        || bw.checked_mul(bh).and_then(|v|v.checked_mul(bc)) != Some(black.len())
        || !(white.len()==1 || white.len()==cpp || cfa && cpp==1 && white.len()==4)
        || black.iter().chain(white).any(|v|!v.is_finite()) || pixels.iter().any(|v|!v.is_finite()) { return Err(invalid()); }
    // Validate all actual pairings before changing a sample (no partial success).
    // CFA white pattern repeats at 2×2; odd black periods need two repeats.
    let repeat_w=if cfa && white.len()==4 && bw%2==1 {bw.saturating_mul(2)} else {bw};
    let repeat_h=if cfa && white.len()==4 && bh%2==1 {bh.saturating_mul(2)} else {bh};
    for y in 0..height.min(repeat_h) { for x in 0..width.min(repeat_w) { for c in 0..cpp {
        let b=black[((y%bh)*bw+x%bw)*bc+if bc==1 {0}else{c}];
        let w=white[if white.len()==1 {0}else if cfa {y%2*2+x%2}else{c}];
        if w<=b || !(w-b).is_finite() {return Err(invalid());}
    }}}
    for (i,pixel) in pixels.chunks_exact_mut(cpp).enumerate() {
        let (x,y)=(i%width,i/width);
        for (c,value) in pixel.iter_mut().enumerate() {
            let b=black[((y%bh)*bw+x%bw)*bc+if bc==1 {0}else{c}];
            let w=white[if white.len()==1 {0}else if cfa {y%2*2+x%2}else{c}];
            *value=(*value-b)/(w-b);
            if !value.is_finite() {return Err(invalid());}
        }
    }
    Ok(())
}

fn camera_intermediate_to_working(
    intermediate: Intermediate,
    raw: &RawImage,
) -> RawResult<crate::color::working::WorkingImage> {
    let (illuminant, matrix) = selected_matrix(raw).unwrap_or_else(|| {
        (
            Illuminant::D65,
            IDENTITY_MATRIX_3.into_iter().flatten().collect(),
        )
    });
    let wb = if raw.wb_coeffs[0].is_nan() {
        [1.0; 4]
    } else {
        raw.wb_coeffs
    };
    match intermediate {
        Intermediate::Monochrome(pixels) => {
            let width = u32::try_from(pixels.dim().w)
                .map_err(|_| RawError::Empty("RAW 宽度溢出".into()))?;
            let height = u32::try_from(pixels.dim().h)
                .map_err(|_| RawError::Empty("RAW 高度溢出".into()))?;
            let rgb = pixels.data.into_iter().map(|value| [value; 3]).collect();
            crate::color::working::WorkingImage::from_linear_srgb(width, height, rgb)
                .map_err(|error| RawError::Empty(error.to_string()))
        }
        Intermediate::ThreeColor(pixels) => {
            let matrix = d65_matrix::<3>(&matrix, illuminant)?;
            let rgb = camera_rgb_to_srgb(&pixels.data, wb, matrix)?;
            crate::color::working::WorkingImage::from_linear_srgb(
                u32::try_from(pixels.width).map_err(|_| RawError::Empty("RAW 宽度溢出".into()))?,
                u32::try_from(pixels.height).map_err(|_| RawError::Empty("RAW 高度溢出".into()))?,
                rgb,
            )
            .map_err(|error| RawError::Empty(error.to_string()))
        }
        Intermediate::FourColor(pixels) => {
            let matrix = d65_matrix::<4>(&matrix, illuminant)?;
            let rgb = camera_rgb_to_srgb(&pixels.data, wb, matrix)?;
            crate::color::working::WorkingImage::from_linear_srgb(
                u32::try_from(pixels.width).map_err(|_| RawError::Empty("RAW 宽度溢出".into()))?,
                u32::try_from(pixels.height).map_err(|_| RawError::Empty("RAW 高度溢出".into()))?,
                rgb,
            )
            .map_err(|error| RawError::Empty(error.to_string()))
        }
    }
}

fn d65_matrix<const N: usize>(matrix: &[f32], illuminant: Illuminant) -> RawResult<[[f32; 3]; N]> {
    let mut matrix = transform_1d::<N, 3>(matrix)
        .ok_or_else(|| RawError::Unsupported(format!("RAW 相机矩阵需要 {} 个系数", N * 3)))?;
    if !matrix.iter().flatten().all(|value| value.is_finite()) {
        return Err(RawError::Unsupported("RAW 相机矩阵含非有限数".into()));
    }
    if illuminant != Illuminant::D65 {
        if N != 3 {
            return Err(RawError::Unsupported(
                "非 D65 四通道相机矩阵尚不能安全适配".into(),
            ));
        }
        let first_three = [matrix[0], matrix[1], matrix[2]];
        let adapted = adapt_bradford(&illuminant, &Illuminant::D65, &first_three);
        matrix[0..3].copy_from_slice(&adapted);
    }
    Ok(matrix)
}

/// 与 rawler 相同的矩阵归一化/伪逆和白平衡，但保留相机空间映射产生的负值和 >1。
fn camera_rgb_to_srgb<const N: usize>(
    pixels: &[[f32; N]],
    white_balance: [f32; 4],
    xyz_to_camera: [[f32; 3]; N],
) -> RawResult<Vec<[f32; 3]>> {
    if white_balance[..N].iter().any(|value| !value.is_finite()) {
        return Err(RawError::Unsupported("RAW 白平衡含非有限数".into()));
    }
    let rgb_to_camera = normalize(multiply(
        &xyz_to_camera,
        &crate::develop::color::SRGB_TO_XYZ_D65,
    ));
    let camera_to_rgb = pseudo_inverse(rgb_to_camera);
    if camera_to_rgb
        .iter()
        .flatten()
        .any(|value| !value.is_finite())
    {
        return Err(RawError::Unsupported("RAW 相机矩阵不可逆".into()));
    }
    pixels
        .iter()
        .map(|pixel| {
            let mut rgb = [0.0_f32; 3];
            for output in 0..3 {
                for channel in 0..N {
                    rgb[output] +=
                        camera_to_rgb[output][channel] * pixel[channel] * white_balance[channel];
                }
            }
            if rgb.iter().any(|value| !value.is_finite()) {
                return Err(RawError::Decode("RAW 校准结果含非有限数".into()));
            }
            Ok(rgb)
        })
        .collect()
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
    camera_matrix_id: crate::color::ProfileId,
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
        camera_matrix_id,
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

/// 与 rawler Calibrate 相同的照明优先序。缺矩阵时 rawler 用 identity，
/// 该退路也有稳定身份；矩阵本身不被错误地称作 ICC。
fn camera_matrix_id(raw: &RawImage) -> crate::color::ProfileId {
    let Some((illuminant, matrix)) = selected_matrix(raw) else {
        return crate::color::ProfileId::of_bytes(b"rawler-0.8-calibrate-identity-v1");
    };
    matrix_id_from_parts(&format!("{illuminant:?}"), &matrix)
}

fn selected_matrix(raw: &RawImage) -> Option<(Illuminant, Vec<f32>)> {
    raw.color_matrix_find_first(CALIBRATION_ILLUMINANTS.iter().copied())
}

fn matrix_id_from_parts(illuminant: &str, matrix: &[f32]) -> crate::color::ProfileId {
    let mut bytes = b"rawler-0.8-calibrate-matrix-v1\0".to_vec();
    bytes.extend_from_slice(illuminant.as_bytes());
    bytes.push(0);
    for value in matrix {
        bytes.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    crate::color::ProfileId::of_bytes(&bytes)
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
    let Some(linear) =
        crate::develop::LinearImage::new(image.width, image.height, image.rgb.clone())
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
        camera_matrix_id: image.camera_matrix_id,
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
/// 规则是「**最小够用**」——够 `need` 就立刻用（剩下的大图**不去解码**）；都不够时
/// `embedded_only`（没有真解码兜底）拿**最大**的那张，否则要求 ≥[`PREVIEW_MIN_PERCENT`]%。
///
/// 「embedded_only 时代取最大」这条是 2026-10-05 补的：原先 embedded_only 会**无条件接受
/// 第一张**（哪怕它只有 160×120），于是 Ricoh/Sigma 的 DNG 明明有 6000×4000 预览，网格却
/// 拿 160×120 铺满 —— 小图挡大图不是「够用」，是错误。
///
/// 候选是**惰性迭代器**：这里只按需 `next()`，拿够就早退；测试可以直接喂现成的图。
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
/// 真正的收敛点是「Rust 按规范渲染出位图 + 渲染结果缓存」（`memory/FUTURE.md` C 段、
/// W3 显影视口），那时两条路会并成一条。在此之前**别把这里当 bug 改**：
/// 把阀值提到 100% 会让看图永远吃预览（糊），降下来会让小图也走完整解码（慢）。
fn pick_embedded(
    candidates: impl Iterator<Item = Option<DynamicImage>>,
    need: u32,
    embedded_only: bool,
) -> Option<(DynamicImage, PixelSource)> {
    let mut best: Option<DynamicImage> = None;
    for image in candidates.flatten() {
        let long = long_edge(&image);
        if long == 0 {
            continue;
        }
        // 够用就早退：`need` 之后的候选（大预览）根本不去解码
        if long >= need {
            return Some((image, PixelSource::EmbeddedPreview));
        }
        if best.as_ref().is_none_or(|b| long_edge(b) < long) {
            best = Some(image);
        }
    }
    let image = best?;
    let long = long_edge(&image);
    // 够了 75%：放大一点用（浏览场景比「等一次完整解码」划算）
    (embedded_only || long.saturating_mul(100) >= need.saturating_mul(PREVIEW_MIN_PERCENT))
        .then_some((image, PixelSource::EmbeddedPreview))
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

    use crate::develop::color::XYZ_TO_SRGB_D65;

    #[test]
    fn unbounded_sensor_scaling_covers_repeat_grid_odd_sizes_and_old_identity() {
        let mut values=vec![0.0,10.0,120.0,30.0,0.0,230.0,40.0,50.0,60.0];
        normalize_sensor(&mut values,(3,3,1),(2,2,1),&[10.0,20.0,30.0,40.0],&[110.0,120.0,130.0,140.0],true).unwrap();
        assert_eq!(values,vec![-0.1,-0.1,1.1,0.0,-0.4,2.0,0.3,0.3,0.5]);
        let mut rgb=vec![0.0,60.0,230.0];
        normalize_sensor(&mut rgb,(1,1,3),(1,1,3),&[10.0,20.0,30.0],&[110.0,120.0,130.0],false).unwrap();
        assert_eq!(rgb,vec![-0.1,0.4,2.0]);
        for (black,white) in [(vec![10.0],vec![10.0]),(vec![f32::NAN],vec![100.0]),(vec![0.0],vec![])] {
            let mut v=vec![1.0];assert!(normalize_sensor(&mut v,(1,1,1),(1,1,1),&black,&white,false).is_err());assert_eq!(v,[1.0]);
        }
        assert!(normalize_sensor(&mut [],(0,1,1),(1,1,1),&[0.0],&[1.0],false).is_err());
        let legacy=crate::color::ProfileId::of_bytes(b"matrix");let current=working_matrix_id(&legacy);
        assert_ne!(legacy,current);
        assert!(!use_legacy_scaling(None,&legacy,&current).unwrap());
        assert!(use_legacy_scaling(Some(&legacy),&legacy,&current).unwrap());
        assert!(!use_legacy_scaling(Some(&current),&legacy,&current).unwrap());
        assert!(use_legacy_scaling(Some(&crate::color::ProfileId::of_bytes(b"wrong")),&legacy,&current).is_err());
    }

    #[test]
    fn camera_calibration_keeps_out_of_range_values() {
        let input = [[-0.25, 0.5, 1.5], [2.0, 0.0, 0.0]];
        let output =
            camera_rgb_to_srgb(&input, [1.0, 1.0, 1.0, f32::NAN], XYZ_TO_SRGB_D65).unwrap();
        for (actual, expected) in output[0].iter().zip(input[0]) {
            assert!((actual - expected).abs() < 1e-5);
        }
        assert!(output[0][0] < 0.0);
        assert!(output[0][2] > 1.0);
        assert!(output[1][0] > 1.0);
        assert!(camera_rgb_to_srgb(&input, [f32::NAN; 4], XYZ_TO_SRGB_D65).is_err());
        assert!(camera_rgb_to_srgb(&input, [1.0; 4], [[0.0; 3]; 3]).is_err());
    }

    #[test]
    fn camera_matrix_rejects_unsupported_channel_and_illuminant_combinations() {
        assert!(d65_matrix::<3>(&[1.0; 8], Illuminant::D65).is_err());
        assert!(d65_matrix::<3>(&[f32::NAN; 9], Illuminant::D65).is_err());
        assert!(d65_matrix::<4>(&[1.0; 12], Illuminant::D50).is_err());
    }

    #[test]
    fn camera_matrix_identity_changes_with_illuminant_and_float_bits() {
        let a = matrix_id_from_parts("D65", &[1.0, 0.0, -0.25]);
        assert_eq!(a, matrix_id_from_parts("D65", &[1.0, 0.0, -0.25]));
        assert_ne!(a, matrix_id_from_parts("D50", &[1.0, 0.0, -0.25]));
        assert_ne!(a, matrix_id_from_parts("D65", &[1.0, -0.0, -0.25]));
    }

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
        use crate::develop::color::{
            SRGB_TO_XYZ_D65, XYZ_TO_SRGB_D65, kelvin_to_xy, mat3_inverse, mat3_vec3, xy_to_xyz,
        };
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
    fn embedded_pick_uses_the_smallest_sufficient_then_the_largest() {
        let img = |w: u32, h: u32| {
            Some(image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                w,
                h,
                image::Rgb([0, 0, 0]),
            )))
        };
        // 颁略图够用：拿它，**不去解码大预览**（迭代器惰性：第二项根本不会被取）
        let picked = pick_embedded([img(1024, 768), img(6000, 4000)].into_iter(), 384, true).unwrap();
        assert_eq!(picked.0.width(), 1024);
        // 颁略图太小：即便 embedded_only 也要拿大的那张（160×120 不许挡住 6000×4000）
        let picked = pick_embedded([img(160, 120), img(6000, 4000)].into_iter(), 384, true).unwrap();
        assert_eq!(picked.0.width(), 6000);
        // 都不够 75%（非 embedded_only）→ 回去真解码
        assert!(pick_embedded([img(160, 120), img(200, 150)].into_iter(), 384, false).is_none());
        // 够 75% 就用（300/384 ≈ 78%）
        let picked = pick_embedded([img(160, 120), img(300, 225)].into_iter(), 384, false).unwrap();
        assert_eq!(picked.0.width(), 300);
        // 一张都没有 / 尺寸为 0 → None
        assert!(pick_embedded([None, None].into_iter(), 384, true).is_none());
        assert!(pick_embedded([img(0, 0)].into_iter(), 384, true).is_none());
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
