//! RGB ICC 的受限读取与 CPU 参考转换。这里只接受照片输入、RGB 输出或显示器
//! 可能使用的 ICC v2/v4；打印/CMYK、DeviceLink 与 iccMAX 不假装兼容。

use lcms2::{
    CIExyY, CIExyYTRIPLE, ColorSpaceSignature, Flags, Intent, PixelFormat, Profile,
    ProfileClassSignature, ToneCurve, Transform,
};
use thiserror::Error;

use super::ProfileId;

pub const MAX_ICC_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IccRole {
    PhotoInput,
    RgbOutput,
    Display,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum IccError {
    #[error("ICC file is empty or shorter than its header")]
    TooShort,
    #[error("ICC file exceeds the size limit")]
    TooLarge,
    #[error("ICC header has an invalid length or signature")]
    BadHeader,
    #[error("only RGB ICC v2/v4 is supported")]
    UnsupportedFormat,
    #[error("ICC device class does not match its intended use")]
    WrongRole,
    #[error("Little CMS could not parse or transform this ICC")]
    InvalidProfile,
    #[error("RGB pixel contains a non-finite channel")]
    NonFinitePixel,
    #[error("this ICC needs a validated GPU CLUT transform")]
    UnsupportedGpuTransform,
    #[error("ICC display CLUT exceeds the validated GPU interpolation error budget")]
    GpuClutAccuracy,
}

/// 经完整长度、签名、版本、通道与设备类别校验的 ICC 原始字节。
#[derive(Debug, Clone)]
pub struct RgbIcc {
    id: ProfileId,
    bytes: Vec<u8>,
    declared_size: usize,
    class: ProfileClassSignature,
}

impl RgbIcc {
    pub fn parse(bytes: &[u8], role: IccRole) -> Result<Self, IccError> {
        if bytes.len() < 128 {
            return Err(IccError::TooShort);
        }
        if bytes.len() > MAX_ICC_BYTES {
            return Err(IccError::TooLarge);
        }
        let declared = u32::from_be_bytes(bytes[0..4].try_into().unwrap()) as usize;
        if declared < 128 || declared > bytes.len() || &bytes[36..40] != b"acsp" {
            return Err(IccError::BadHeader);
        }
        if !matches!(bytes[8], 2 | 4) || &bytes[16..20] != b"RGB " {
            return Err(IccError::UnsupportedFormat);
        }
        let profile = Profile::new_icc(&bytes[..declared]).map_err(|_| IccError::InvalidProfile)?;
        if profile.color_space() != ColorSpaceSignature::RgbData {
            return Err(IccError::UnsupportedFormat);
        }
        let class = profile.device_class();
        if !class_accepts_role(class, role) {
            return Err(IccError::WrongRole);
        }
        Ok(Self {
            // 资源身份属于原始文件字节；合法的尾随填充也不能被悄悄归一化掉。
            id: ProfileId::of_bytes(bytes),
            bytes: bytes.to_vec(),
            declared_size: declared,
            class,
        })
    }

    #[must_use]
    pub fn id(&self) -> &ProfileId {
        &self.id
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn class(&self) -> ProfileClassSignature {
        self.class
    }

    /// 已通过 LCMS 解析的配置只需检查设备类，无须为每次导出再次复制/解析 ICC 字节。
    #[must_use]
    pub fn accepts_role(&self, role: IccRole) -> bool {
        class_accepts_role(self.class, role)
    }

    pub(crate) fn tag_bytes(&self, signature:&[u8;4])->Option<&[u8]> {
        let bytes=&self.bytes[..self.declared_size];
        let count=u32::from_be_bytes(bytes.get(128..132)?.try_into().ok()?) as usize;
        let table=bytes.get(132..132_usize.checked_add(count.checked_mul(12)?)?)?;
        for entry in table.as_chunks::<12>().0 {
            if &entry[..4]==signature {
                let offset=u32::from_be_bytes(entry[4..8].try_into().ok()?) as usize;
                let size=u32::from_be_bytes(entry[8..12].try_into().ok()?) as usize;
                return bytes.get(offset..offset.checked_add(size)?);
            }
        }
        None
    }

    pub(crate) fn profile(&self) -> Result<Profile, IccError> {
        Profile::new_icc(&self.bytes[..self.declared_size]).map_err(|_| IccError::InvalidProfile)
    }
}

fn class_accepts_role(class: ProfileClassSignature, role: IccRole) -> bool {
    match role {
        IccRole::PhotoInput => matches!(
            class,
            ProfileClassSignature::InputClass
                | ProfileClassSignature::DisplayClass
                | ProfileClassSignature::ColorSpaceClass
        ),
        IccRole::RgbOutput => matches!(
            class,
            ProfileClassSignature::OutputClass
                | ProfileClassSignature::DisplayClass
                | ProfileClassSignature::ColorSpaceClass
        ),
        IccRole::Display => class == ProfileClassSignature::DisplayClass,
    }
}

fn rec2020_colorimetry() -> (CIExyY, CIExyYTRIPLE) {
    let d65 = CIExyY {
        x: 0.3127,
        y: 0.3290,
        Y: 1.0,
    };
    let primaries = CIExyYTRIPLE {
        Red: CIExyY {
            x: 0.708,
            y: 0.292,
            Y: 1.0,
        },
        Green: CIExyY {
            x: 0.170,
            y: 0.797,
            Y: 1.0,
        },
        Blue: CIExyY {
            x: 0.131,
            y: 0.046,
            Y: 1.0,
        },
    };
    (d65, primaries)
}
pub(crate) fn linear_rec2020() -> Result<Profile, IccError> {
    let (d65, primaries) = rec2020_colorimetry();
    let linear = ToneCurve::new(1.0);
    Profile::new_rgb(&d65, &primaries, &[&linear, &linear, &linear]).map_err(|_| IccError::InvalidProfile)
}
pub(super) fn linear_rec2020_thread(context: &lcms2::ThreadContext) -> Result<Profile<lcms2::ThreadContext>, IccError> {
    let (d65, primaries) = rec2020_colorimetry();
    let linear = ToneCurve::new(1.0);
    Profile::new_rgb_context(context, &d65, &primaries, &[&linear, &linear, &linear]).map_err(|_| IccError::InvalidProfile)
}

/// 16-bit 输入经源 ICC 一次转换到线性 Rec.2020；浮点工作值不在这里截断。
pub fn input_rgb16_to_working(
    source: &RgbIcc,
    pixels: &[[u16; 3]],
) -> Result<Vec<[f32; 3]>, IccError> {
    if let Some(transform) = super::matrix_trc::input_transform(source)? {
        return transform.apply(pixels);
    }
    super::lcms_input::rgb16(source, pixels)
}

/// RGB8 常见路径直接借用解码器缓冲；矩阵/TRC 快路径不分配整图 u16 副本。
/// CLUT 回退仍按原 8→16 精确扩展后走同一 LCMS 参考语义。
pub(crate) fn input_rgb8_to_working(
    source: &RgbIcc,
    pixels: &[[u8; 3]],
) -> Result<Vec<[f32; 3]>, IccError> {
    if let Some(transform) = super::matrix_trc::input_transform(source)? {
        return transform.apply_rgb8(pixels);
    }
    super::lcms_input::rgb8(source, pixels)
}

/// Little CMS 全精度基准；复杂 CLUT/不吻合快路径的 ICC 走这里。
pub(crate) fn input_rgb16_to_working_reference(
    source: &RgbIcc,
    pixels: &[[u16; 3]],
) -> Result<Vec<[f32; 3]>, IccError> {
    let from = source.profile()?;
    let to = linear_rec2020()?;
    let transform = Transform::new_flags(
        &from,
        PixelFormat::RGB_16,
        &to,
        PixelFormat::RGB_FLT,
        Intent::RelativeColorimetric,
        Flags::NO_OPTIMIZE,
    )
    .map_err(|_| IccError::InvalidProfile)?;
    let mut output = vec![[0.0_f32; 3]; pixels.len()];
    transform.transform_pixels(pixels, &mut output);
    if output.iter().flatten().any(|channel| !channel.is_finite()) {
        return Err(IccError::NonFinitePixel);
    }
    Ok(output)
}

/// Floating encoded RGB keeps its precision; conversion happens once on load.
/// The ICC engine decides the transfer extension, never an 8/16-bit intermediate.
pub fn input_rgbf32_to_working(
    source: &RgbIcc,
    pixels: &[[f32; 3]],
) -> Result<Vec<[f32; 3]>, IccError> {
    if !source.accepts_role(IccRole::PhotoInput) {
        return Err(IccError::WrongRole);
    }
    if pixels.iter().flatten().any(|v| !v.is_finite()) {
        return Err(IccError::NonFinitePixel);
    }
    let from = source.profile()?;
    let to = linear_rec2020()?;
    let transform = Transform::new_flags(
        &from,
        PixelFormat::RGB_FLT,
        &to,
        PixelFormat::RGB_FLT,
        Intent::RelativeColorimetric,
        Flags::NO_OPTIMIZE,
    )
    .map_err(|_| IccError::InvalidProfile)?;
    let mut output: Vec<[f32; 3]> = vec![[0.0; 3]; pixels.len()];
    transform.transform_pixels(pixels, &mut output);
    if output.iter().flatten().any(|v| !v.is_finite()) {
        return Err(IccError::NonFinitePixel);
    }
    Ok(output)
}

/// 线性工作值转 RGB 输出配置。量化只在这一最终边界发生；调用方再写与像素匹配的 ICC。
pub fn working_to_output_rgb16(
    target: &RgbIcc,
    pixels: &[[f32; 3]],
) -> Result<Vec<[u16; 3]>, IccError> {
    if pixels.iter().flatten().any(|channel| !channel.is_finite()) {
        return Err(IccError::NonFinitePixel);
    }
    if let Some(transform) = super::matrix_trc::output_transform(target)? {
        return Ok(transform.apply(pixels));
    }
    working_to_output_rgb16_reference(target, pixels)
}

/// 与上方相同的像素/ICC 语义，交错平面直接供现有 16-bit 编码器使用。
/// 常见矩阵/TRC 路径不再同时分配两张完整的 u16 输出图。
pub(crate) fn working_to_output_rgb16_flat(
    target: &RgbIcc,
    pixels: &[[f32; 3]],
) -> Result<Vec<u16>, IccError> {
    if pixels.iter().flatten().any(|channel| !channel.is_finite()) {
        return Err(IccError::NonFinitePixel);
    }
    if let Some(transform) = super::matrix_trc::output_transform(target)? {
        return Ok(transform.apply_flat(pixels));
    }
    // CLUT 基准路径暂沿用 Little CMS 的整图结果；未默认接入实时渲染，
    // 其吞吐与峰值内存仍属 CM-W2 未验收项。
    Ok(working_to_output_rgb16_reference(target, pixels)?.into_flattened())
}

pub(crate) fn working_to_output_rgb16_reference(
    target: &RgbIcc,
    pixels: &[[f32; 3]],
) -> Result<Vec<[u16; 3]>, IccError> {
    let from = linear_rec2020()?;
    let to = target.profile()?;
    let transform = Transform::new_flags(
        &from,
        PixelFormat::RGB_FLT,
        &to,
        PixelFormat::RGB_16,
        Intent::RelativeColorimetric,
        Flags::NO_OPTIMIZE,
    )
    .map_err(|_| IccError::InvalidProfile)?;
    let mut output = vec![[0_u16; 3]; pixels.len()];
    transform.transform_pixels(pixels, &mut output);
    Ok(output)
}

// Built-in v1 identities must be reproducible across process launches and machines.
// Only generated profiles use this header normalization; imported bytes remain untouched.
#[cfg(test)]
fn builtin_profile(mut bytes: Vec<u8>) -> Result<RgbIcc, IccError> {
    for (offset, value) in [2000_u16, 1, 1, 0, 0, 0].into_iter().enumerate() {
        bytes[24 + offset * 2..26 + offset * 2].copy_from_slice(&value.to_be_bytes());
    }
    RgbIcc::parse(&bytes, IccRole::RgbOutput)
}

macro_rules! cached_builtin {
    ($public:ident, $bytes:literal) => {
        pub fn $public() -> Result<RgbIcc, IccError> {
            static PROFILE: std::sync::OnceLock<Result<RgbIcc, IccError>> =
                std::sync::OnceLock::new();
            PROFILE
                .get_or_init(|| RgbIcc::parse(include_bytes!($bytes), IccRole::RgbOutput))
                .clone()
        }
    };
}
cached_builtin!(srgb_icc, "../../assets/color/srgb-v1.icc");
cached_builtin!(display_p3_icc, "../../assets/color/display-p3-v1.icc");
cached_builtin!(adobe_rgb_icc, "../../assets/color/adobe-rgb-v1.icc");

/// 标准 sRGB ICC 字节可用于默认输入解释、参考测试及导出嵌入。
#[cfg(test)]
fn make_srgb_icc() -> Result<RgbIcc, IccError> {
    let bytes = Profile::new_srgb()
        .icc()
        .map_err(|_| IccError::InvalidProfile)?;
    builtin_profile(bytes)
}

/// Display P3：D65 + P3 原色 + IEC 61966-2-1（sRGB）传递曲线。
/// 与某些把 P3 简写成纯 gamma 2.2 的配置不同，暗部必须保持分段曲线。
#[cfg(test)]
fn make_display_p3_icc() -> Result<RgbIcc, IccError> {
    let d65 = CIExyY {
        x: 0.3127,
        y: 0.3290,
        Y: 1.0,
    };
    let primaries = CIExyYTRIPLE {
        Red: CIExyY {
            x: 0.680,
            y: 0.320,
            Y: 1.0,
        },
        Green: CIExyY {
            x: 0.265,
            y: 0.690,
            Y: 1.0,
        },
        Blue: CIExyY {
            x: 0.150,
            y: 0.060,
            Y: 1.0,
        },
    };
    let trc =
        ToneCurve::new_parametric(4, &[2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045])
            .map_err(|_| IccError::InvalidProfile)?;
    let bytes = Profile::new_rgb(&d65, &primaries, &[&trc, &trc, &trc])
        .map_err(|_| IccError::InvalidProfile)?
        .icc()
        .map_err(|_| IccError::InvalidProfile)?;
    builtin_profile(bytes)
}

/// Adobe RGB (1998)：D65、其标准原色、2.19921875 gamma。
#[cfg(test)]
fn make_adobe_rgb_icc() -> Result<RgbIcc, IccError> {
    let d65 = CIExyY {
        x: 0.3127,
        y: 0.3290,
        Y: 1.0,
    };
    let primaries = CIExyYTRIPLE {
        Red: CIExyY {
            x: 0.640,
            y: 0.330,
            Y: 1.0,
        },
        Green: CIExyY {
            x: 0.210,
            y: 0.710,
            Y: 1.0,
        },
        Blue: CIExyY {
            x: 0.150,
            y: 0.060,
            Y: 1.0,
        },
    };
    let trc = ToneCurve::new(563.0 / 256.0);
    let bytes = Profile::new_rgb(&d65, &primaries, &[&trc, &trc, &trc])
        .map_err(|_| IccError::InvalidProfile)?
        .icc()
        .map_err(|_| IccError::InvalidProfile)?;
    builtin_profile(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-built ICC lut8 tag, independent of the matrix/TRC extraction code.
    /// All three input axes are mapped by a real 2×2×2 CLUT into PCS XYZ.
    fn clut_profile() -> RgbIcc {
        let original = srgb_icc().unwrap();
        let mut bytes = original.bytes().to_vec();
        let count = u32::from_be_bytes(bytes[128..132].try_into().unwrap()) as usize;
        let table_end = 132 + count * 12;
        bytes.splice(table_end..table_end, [0_u8; 12]);
        bytes[128..132].copy_from_slice(&((count + 1) as u32).to_be_bytes());
        for i in 0..count {
            let at = 132 + i * 12 + 4;
            let offset = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) + 12;
            bytes[at..at + 4].copy_from_slice(&offset.to_be_bytes());
        }
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
        let offset = bytes.len();
        let mut lut = b"mft1\0\0\0\0".to_vec();
        lut.extend([3, 3, 2, 0]);
        for i in 0..9 {
            lut.extend((if i % 4 == 0 { 65536_i32 } else { 0 }).to_be_bytes());
        }
        for _ in 0..3 {
            lut.extend(0_u8..=255);
        }
        for r in [0, 127] {
            for g in [0, 127] {
                for b in [0, 127] {
                    lut.extend([r, g, b]);
                }
            }
        }
        for _ in 0..3 {
            lut.extend(0_u8..=255);
        }
        bytes[table_end..table_end + 4].copy_from_slice(b"A2B0");
        bytes[table_end + 4..table_end + 8].copy_from_slice(&(offset as u32).to_be_bytes());
        bytes[table_end + 8..table_end + 12].copy_from_slice(&(lut.len() as u32).to_be_bytes());
        bytes.extend(lut);
        let length = bytes.len() as u32;
        bytes[..4].copy_from_slice(&length.to_be_bytes());
        bytes[84..100].fill(0);
        RgbIcc::parse(&bytes, IccRole::PhotoInput).unwrap()
    }
    #[test]
    fn independent_lut8_uses_lcms_reference_and_never_masquerades_as_gpu_matrix() {
        let profile = clut_profile();
        let samples = [
            [0, 0, 0],
            [65535, 0, 0],
            [0, 65535, 0],
            [0, 0, 65535],
            [12000, 24000, 36000],
            [65535; 3],
        ];
        assert!(
            super::super::matrix_trc::input_transform(&profile)
                .unwrap()
                .is_none()
        );
        let actual = input_rgb16_to_working(&profile, &samples).unwrap();
        let expected = input_rgb16_to_working_reference(&profile, &samples).unwrap();
        assert_eq!(actual, expected);
        assert!(actual.iter().flatten().all(|value| value.is_finite()));
        let matrix_only = input_rgb16_to_working(&srgb_icc().unwrap(), &samples).unwrap();
        assert_ne!(actual, matrix_only);
        // A2B-only fixture does not define a display CLUT: LCMS may use the
        // retained output matrix. No GPU matrix extraction is claimed here.
        assert!(matches!(
            super::super::proof::ProofTransform::from_icc(&profile),
            Err(IccError::UnsupportedGpuTransform)
        ));
        let mut v2 = profile.bytes().to_vec();
        v2[8] = 2;
        v2[9..12].fill(0);
        let v2 = RgbIcc::parse(&v2, IccRole::PhotoInput).unwrap();
        assert_eq!(
            input_rgb16_to_working(&v2, &samples).unwrap(),
            input_rgb16_to_working_reference(&v2, &samples).unwrap()
        );
    }

    #[test]
    fn builtin_profile_identity_ignores_generated_wall_clock_and_is_cached() {
        for profile in [
            srgb_icc().unwrap(),
            display_p3_icc().unwrap(),
            adobe_rgb_icc().unwrap(),
        ] {
            let mut next_date = profile.bytes().to_vec();
            next_date[24..36].copy_from_slice(&[0x07, 0xea, 0, 10, 0, 4, 0, 12, 0, 34, 0, 56]);
            assert_ne!(ProfileId::of_bytes(&next_date), *profile.id());
            assert_eq!(builtin_profile(next_date).unwrap().id(), profile.id());
        }
        for (canonical, generated) in [
            (srgb_icc().unwrap(), make_srgb_icc().unwrap()),
            (display_p3_icc().unwrap(), make_display_p3_icc().unwrap()),
            (adobe_rgb_icc().unwrap(), make_adobe_rgb_icc().unwrap()),
        ] {
            let samples = [
                [65535, 0, 0],
                [0, 65535, 0],
                [0, 0, 65535],
                [123, 32768, 60000],
            ];
            let a = input_rgb16_to_working_reference(&canonical, &samples).unwrap();
            let b = input_rgb16_to_working_reference(&generated, &samples).unwrap();
            for (a, b) in a.iter().flatten().zip(b.iter().flatten()) {
                assert!((a - b).abs() < 2e-5);
            }
        }
    }

    #[test]
    #[ignore]
    fn bench_two_megapixel_clut_input() {
        let profile=std::env::var_os("RAYBEND_ICC_SAMPLE").map(|path|RgbIcc::parse(&std::fs::read(path).unwrap(),IccRole::PhotoInput).unwrap()).unwrap_or_else(clut_profile);
        let pixels: Vec<[u16; 3]> = (0..2_000_000_u32)
            .map(|i| {
                [
                    (i * 31 % 65536) as u16,
                    (i * 113 % 65536) as u16,
                    (i * 211 % 65536) as u16,
                ]
            })
            .collect();
        let prepare=std::time::Instant::now();let _=input_rgb16_to_working(&profile,&pixels[..1]).unwrap();eprintln!("input prepare {:?}",prepare.elapsed());
        let baseline=std::time::Instant::now();let reference=input_rgb16_to_working_reference(&profile,&pixels).unwrap();eprintln!("LCMS serial 2MP {:?}",baseline.elapsed());
        let started = std::time::Instant::now();
        let result = input_rgb16_to_working(&profile, &pixels).unwrap();
        eprintln!(
            "independent RGB CLUT 2MP input: {:?}; {} pixels (once per source load)",
            started.elapsed(),
            result.len()
        );
        assert_eq!(result,reference);
        std::hint::black_box(result);
    }

    /// 手动诊断：不参与常规秒级测试，不设跨硬件不稳定的绝对耗时门槛。
    #[test]
    #[ignore]
    fn bench_two_megapixel_icc_round_trip() {
        use std::time::Instant;
        let profile = srgb_icc().unwrap();
        let pixels: Vec<[u16; 3]> = (0..2_000_000_u32)
            .map(|index| {
                [
                    (index.wrapping_mul(31) % 65_536) as u16,
                    (index.wrapping_mul(113) % 65_536) as u16,
                    (index.wrapping_mul(211) % 65_536) as u16,
                ]
            })
            .collect();
        // 曲线建表与 LCMS 网格核验属于配置首次载入，不在照片像素吞吐计时内。
        let setup_started = Instant::now();
        let _ = super::super::matrix_trc::input_transform(&profile).unwrap();
        let _ = super::super::matrix_trc::output_transform(&profile).unwrap();
        let setup_elapsed = setup_started.elapsed();
        let started = Instant::now();
        let working = input_rgb16_to_working(&profile, &pixels).unwrap();
        let input_elapsed = started.elapsed();
        let started = Instant::now();
        let output = working_to_output_rgb16(&profile, &working).unwrap();
        let output_elapsed = started.elapsed();
        let started = Instant::now();
        let flat = working_to_output_rgb16_flat(&profile, &working).unwrap();
        let flat_elapsed = started.elapsed();
        assert_eq!(flat, output.iter().flatten().copied().collect::<Vec<_>>());
        std::hint::black_box(output);
        eprintln!(
            "ICC profile setup {:?}; 2MP input→Rec2020 {:?}, Rec2020→output {:?}, flat export {:?}",
            setup_elapsed, input_elapsed, output_elapsed, flat_elapsed
        );
    }

    #[test]
    fn valid_srgb_and_invalid_files_are_distinct() {
        let srgb = srgb_icc().unwrap();
        assert_eq!(srgb.class(), ProfileClassSignature::DisplayClass);
        assert_eq!(
            RgbIcc::parse(&[], IccRole::PhotoInput).unwrap_err(),
            IccError::TooShort
        );
        let mut bad = srgb.bytes().to_vec();
        bad[36] = 0;
        assert_eq!(
            RgbIcc::parse(&bad, IccRole::PhotoInput).unwrap_err(),
            IccError::BadHeader
        );
        let mut cmyk = srgb.bytes().to_vec();
        cmyk[16..20].copy_from_slice(b"CMYK");
        assert_eq!(
            RgbIcc::parse(&cmyk, IccRole::PhotoInput).unwrap_err(),
            IccError::UnsupportedFormat
        );
        let mut huge = vec![0_u8; MAX_ICC_BYTES + 1];
        huge[36..40].copy_from_slice(b"acsp");
        assert_eq!(
            RgbIcc::parse(&huge, IccRole::PhotoInput).unwrap_err(),
            IccError::TooLarge
        );
        let mut padded = srgb.bytes().to_vec();
        padded.push(0);
        let padded = RgbIcc::parse(&padded, IccRole::PhotoInput).unwrap();
        assert_ne!(padded.id(), srgb.id());
    }

    #[test]
    fn icc_v2_and_v4_are_parsed_from_real_lcms_serialization() {
        for version in [2.1, 4.3] {
            let mut profile = Profile::new_srgb();
            profile.set_version(version);
            let bytes = profile.icc().unwrap();
            let parsed = RgbIcc::parse(&bytes, IccRole::PhotoInput).unwrap();
            assert_eq!(bytes[8], version as u8);
            assert_eq!(parsed.bytes(), bytes);
            let converted = input_rgb16_to_working(&parsed, &[[0, 32768, 65535]]).unwrap();
            assert!(converted[0].iter().all(|value| value.is_finite()));
        }
    }

    #[test]
    fn srgb_primaries_match_independent_rec2020_matrix_values() {
        let srgb = srgb_icc().unwrap();
        let work = input_rgb16_to_working(
            &srgb,
            &[[65535, 0, 0], [0, 65535, 0], [0, 0, 65535], [0, 0, 0]],
        )
        .unwrap();
        for (actual, expected) in work.iter().zip([
            [0.6274, 0.0691, 0.0164],
            [0.3293, 0.9195, 0.0880],
            [0.0433, 0.0114, 0.8956],
            [0.0, 0.0, 0.0],
        ]) {
            for (a, e) in actual.iter().zip(expected) {
                assert!((a - e).abs() < 0.008, "{a} != {e}");
            }
        }
        let roundtrip = working_to_output_rgb16(&srgb, &work).unwrap();
        for (actual, expected) in
            roundtrip
                .iter()
                .zip([[65535, 0, 0], [0, 65535, 0], [0, 0, 65535], [0, 0, 0]])
        {
            for (a, e) in actual.iter().zip(expected) {
                assert!(a.abs_diff(e) <= 150);
            }
        }
    }

    #[test]
    fn non_finite_working_values_never_reach_encoder() {
        let srgb = srgb_icc().unwrap();
        assert_eq!(
            working_to_output_rgb16(&srgb, &[[f32::NAN, 0.0, 0.0]]).unwrap_err(),
            IccError::NonFinitePixel
        );
        assert_eq!(
            working_to_output_rgb16(&srgb, &[[f32::INFINITY, 0.0, 0.0]]).unwrap_err(),
            IccError::NonFinitePixel
        );
    }

    #[test]
    fn built_in_wide_gamut_profiles_are_distinct_and_round_trip() {
        let srgb = srgb_icc().unwrap();
        for target in [display_p3_icc().unwrap(), adobe_rgb_icc().unwrap()] {
            assert_ne!(target.id(), srgb.id());
            let work = input_rgb16_to_working(&target, &[[10000, 30000, 50000]]).unwrap();
            let returned = working_to_output_rgb16(&target, &work).unwrap();
            for (a, b) in returned[0].iter().zip([10000_u16, 30000, 50000]) {
                assert!(a.abs_diff(b) < 200, "round-trip {a} vs {b}");
            }
        }
    }
}
