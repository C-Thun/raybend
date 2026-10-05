//! 位图的色彩解释入口。新处理版本使用它；旧处理版本继续按原解码路径重放。

use std::borrow::Cow;
use std::io::Cursor;

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use thiserror::Error;

use super::SourceColor;
use super::icc::{IccError, IccRole, RgbIcc, input_rgb8_to_working, input_rgb16_to_working};
use super::working::{WorkingImage, WorkingImageError};

#[derive(Debug, Error)]
pub enum BitmapColorError {
    #[error("cannot decode bitmap: {0}")]
    Decode(#[from] image::ImageError),
    #[error("cannot inspect bitmap: {0}")]
    Inspect(#[from] std::io::Error),
    #[error("invalid embedded RGB ICC: {0}")]
    Icc(#[from] IccError),
    #[error("cannot inspect PNG color metadata: {0}")]
    PngMetadata(String),
    #[error("bitmap has conflicting color descriptions")]
    ConflictingSource,
    #[error("bitmap has no known RGB color interpretation")]
    UnresolvedSource,
    #[error("bitmap pixel format is not yet supported by the linear RGB path")]
    UnsupportedPixels,
    #[error("working image is invalid: {0}")]
    WorkingImage(#[from] WorkingImageError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceEvidence {
    EmbeddedIcc,
    TaggedPngSrgb,
    AssumedJpegSrgb,
    Unresolved,
}

#[derive(Debug)]
pub struct DecodedBitmap {
    pub image: DynamicImage,
    pub format: ImageFormat,
    pub evidence: SourceEvidence,
    pub embedded_icc: Option<RgbIcc>,
}

#[derive(Debug)]
pub struct BitmapDescription {
    pub evidence: SourceEvidence,
    pub embedded_icc: Option<RgbIcc>,
    pub supports_rgb: bool,
}
impl BitmapDescription {
    pub fn source(&self) -> Option<SourceColor> { source_color(self.evidence,self.embedded_icc.as_ref()) }
}
fn source_color(evidence: SourceEvidence, embedded_icc: Option<&RgbIcc>) -> Option<SourceColor> {
        match evidence {
            SourceEvidence::EmbeddedIcc => Some(SourceColor::EmbeddedIcc {
                profile_id: embedded_icc?.id().clone(),
            }),
            SourceEvidence::AssumedJpegSrgb => Some(SourceColor::AssumedSrgb),
            SourceEvidence::TaggedPngSrgb => Some(SourceColor::TaggedSrgb),
            SourceEvidence::Unresolved => None,
        }
}

impl DecodedBitmap {
    /// 未知来源不能被隐式当作 sRGB；用户显式指派/配置默认值在上层解决。
    pub fn source(&self) -> Option<SourceColor> {
        source_color(self.evidence,self.embedded_icc.as_ref())
    }

    /// 无 alpha 的 RGB/灰度 8/16-bit 与 RGB32F 建立高精度参考像素。
    /// 灰度扩为等值 RGB，16-bit/float 不经过低精度中间图；alpha 走 `linear_rgba`。
    pub fn linear_rgb(&self) -> Result<Vec<[f32; 3]>, BitmapColorError> {
        let profile = self.source_profile()?;
        self.linear_rgb_with(&profile)
    }

    /// 经已解析的输入依据创建高精度工作像素；未知来源需先明确指定 ICC。
    pub fn working_image(&self) -> Result<WorkingImage, BitmapColorError> {
        WorkingImage::new(self.image.width(), self.image.height(), self.linear_rgb()?)
            .map_err(Into::into)
    }

    /// 用户明确指定输入 ICC 时，像素数值不变，只更换解释并转换到工作域。
    pub fn working_image_with(&self, profile: &RgbIcc) -> Result<WorkingImage, BitmapColorError> {
        WorkingImage::new(
            self.image.width(),
            self.image.height(),
            self.linear_rgb_with(profile)?,
        )
        .map_err(Into::into)
    }

    fn source_profile(&self) -> Result<Cow<'_, RgbIcc>, BitmapColorError> {
        match self.evidence {
            SourceEvidence::EmbeddedIcc => self
                .embedded_icc
                .as_ref()
                .map(Cow::Borrowed)
                .ok_or(BitmapColorError::UnresolvedSource),
            SourceEvidence::AssumedJpegSrgb | SourceEvidence::TaggedPngSrgb => {
                Ok(Cow::Owned(super::icc::srgb_icc()?))
            }
            SourceEvidence::Unresolved => Err(BitmapColorError::UnresolvedSource),
        }
    }

    pub fn linear_rgb_with(&self, profile: &RgbIcc) -> Result<Vec<[f32; 3]>, BitmapColorError> {
        if let DynamicImage::ImageRgb32F(image) = &self.image {
            return super::icc::input_rgbf32_to_working(profile, image.as_raw().as_chunks::<3>().0).map_err(Into::into);
        }
        if matches!(self.image, DynamicImage::ImageLuma8(_) | DynamicImage::ImageLuma16(_)) {
            let rgb = self.image.to_rgb16();
            return input_rgb16_to_working(profile, rgb.as_raw().as_chunks::<3>().0).map_err(Into::into);
        }
        if let DynamicImage::ImageRgb16(image) = &self.image {
            // ImageBuffer 的 RGB16 是紧密交错的 u16。借出三通道块即可，
            // 不为 60MP 图再复制约 343 MiB 的输入缓冲。
            let (pixels, remainder) = image.as_raw().as_chunks::<3>();
            debug_assert!(remainder.is_empty());
            return input_rgb16_to_working(profile, pixels).map_err(Into::into);
        }
        if let DynamicImage::ImageRgb8(image) = &self.image {
            let (pixels, remainder) = image.as_raw().as_chunks::<3>();
            debug_assert!(remainder.is_empty());
            return input_rgb8_to_working(profile, pixels).map_err(Into::into);
        }
        Err(BitmapColorError::UnsupportedPixels)
    }

    /// RGBA alpha 是覆盖度，不参与 ICC；色彩和 alpha 的 16-bit 精度各自保留。
    pub fn linear_rgba(&self) -> Result<Vec<[f32; 4]>, BitmapColorError> {
        let profile = self.source_profile()?;
        self.linear_rgba_with(&profile)
    }

    pub fn linear_rgba_with(&self, profile: &RgbIcc) -> Result<Vec<[f32; 4]>, BitmapColorError> {
        let (rgb, alpha): (Vec<[u16; 3]>, Vec<u16>) = match &self.image {
            DynamicImage::ImageRgba16(image) => image
                .pixels()
                .map(|p| ([p.0[0], p.0[1], p.0[2]], p.0[3]))
                .unzip(),
            DynamicImage::ImageRgba8(image) => image
                .pixels()
                .map(|p| {
                    (
                        [p.0[0], p.0[1], p.0[2]].map(|v| u16::from(v) * 257),
                        u16::from(p.0[3]) * 257,
                    )
                })
                .unzip(),
            _ => return Err(BitmapColorError::UnsupportedPixels),
        };
        let linear = input_rgb16_to_working(profile, &rgb)?;
        Ok(linear
            .into_iter()
            .zip(alpha)
            .map(|(rgb, alpha)| [rgb[0], rgb[1], rgb[2], f32::from(alpha) / 65535.0])
            .collect())
    }
}

/// 只解码一次：先从同一 decoder 读取 ICC，再交给 DynamicImage 保留原本位深。
pub fn decode_bitmap(bytes: &[u8]) -> Result<DecodedBitmap, BitmapColorError> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let format = reader.format().ok_or(BitmapColorError::UnresolvedSource)?;
    let mut decoder = reader.into_decoder()?;
    let BitmapDescription { evidence,embedded_icc,.. } = describe_decoder(&mut decoder,bytes,format)?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(DecodedBitmap {
        image,
        format,
        evidence,
        embedded_icc,
    })
}

fn describe_decoder(decoder: &mut impl ImageDecoder, bytes: &[u8], format: ImageFormat) -> Result<BitmapDescription,BitmapColorError> {
    let mut embedded_icc = decoder
        .icc_profile()?
        .map(|bytes| RgbIcc::parse(&bytes, IccRole::PhotoInput))
        .transpose()?;
    // image::TiffDecoder 目前可能拿不到一个合法的 TIFF 34675 ICC tag；
    // 仓内已有 EXIF/TIFF 解析器可从同一容器读出这个原始 tag。
    if embedded_icc.is_none() && format == ImageFormat::Tiff
        && let Ok(parsed) = exif::Reader::new().read_from_container(&mut Cursor::new(bytes))
            && let Some(field) =
                parsed.get_field(exif::Tag(exif::Context::Tiff, 34675), exif::In::PRIMARY)
            {
                let bytes = match &field.value {
                    exif::Value::Byte(bytes) | exif::Value::Undefined(bytes, _) => bytes,
                    _ => return Err(BitmapColorError::ConflictingSource),
                };
                embedded_icc = Some(RgbIcc::parse(bytes, IccRole::PhotoInput)?);
            }
    let tagged_png_srgb = if format == ImageFormat::Png {
        png::Decoder::new(Cursor::new(bytes))
            .read_info()
            .map_err(|error| BitmapColorError::PngMetadata(error.to_string()))?
            .info()
            .srgb
            .is_some()
    } else {
        false
    };
    if embedded_icc.is_some() && tagged_png_srgb {
        return Err(BitmapColorError::ConflictingSource);
    }
    let evidence = if embedded_icc.is_some() {
        SourceEvidence::EmbeddedIcc
    } else if tagged_png_srgb {
        SourceEvidence::TaggedPngSrgb
    } else if format == ImageFormat::Jpeg {
        SourceEvidence::AssumedJpegSrgb
    } else {
        SourceEvidence::Unresolved
    };
    Ok(BitmapDescription { evidence,embedded_icc,supports_rgb: !decoder.color_type().has_alpha() })
}
/// Reads container metadata without decoding a full image or allocating pixel buffers.
pub fn inspect_bitmap(bytes: &[u8]) -> Result<BitmapDescription,BitmapColorError> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let format = reader.format().ok_or(BitmapColorError::UnresolvedSource)?;
    describe_decoder(&mut reader.into_decoder()?,bytes,format)
}

/// Resolve a new input choice in a background preparation task. Frozen states
/// never call this on replay; they retain their exact source identity.
pub fn prepare_source(path: &std::path::Path, defaults: &super::defaults::ColorDefaults, assigned: Option<&RgbIcc>) -> crate::Result<super::PhotoColorState> {
    let error = |e: BitmapColorError| crate::Error::Unsupported(e.to_string());
    if crate::media::kind::kind_of_file(&path.to_string_lossy()) == crate::media::kind::MediaKind::Raw {
        if assigned.is_some() { return Err(crate::Error::Unsupported("RGB ICC 不能指定到 RAW 相机数据".into())); }
        let matrix_id = crate::raw::worker::shared().lock().unwrap_or_else(|e|e.into_inner())
            .working_color_identity(path)
            .map_err(|e|crate::Error::Unsupported(e.to_string()))?;
        return Ok(super::PhotoColorState::new_pipeline(SourceColor::RawCameraMatrix {matrix_id}));
    }
    let bytes = crate::fs_asset::read_limited(path,crate::raw::precheck::MAX_BYTES,"位图")?;
    let description = inspect_bitmap(&bytes).map_err(error)?;
    if !description.supports_rgb { return Err(crate::Error::Unsupported("含透明通道的位图暂不支持新版色彩编辑".into())); }
    match assigned {
        Some(profile) if profile.accepts_role(IccRole::PhotoInput) => Ok(super::PhotoColorState::new_pipeline(SourceColor::AssignedRgbIcc {profile_id:profile.id().clone()})),
        Some(_) => Err(crate::Error::Unsupported("该配置不能用于照片输入".into())),
        None => defaults.resolve_bitmap(description.source()).map_err(|e|crate::Error::Unsupported(e.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ExtendedColorType, ImageEncoder};

    #[test]
    fn grayscale_and_float_pixels_are_not_silently_reduced_to_eight_bits() {
        let profile = super::super::icc::srgb_icc().unwrap();
        let gray = DecodedBitmap { image: DynamicImage::ImageLuma16(image::ImageBuffer::from_raw(2, 1, vec![30000, 30001]).unwrap()),
            format: ImageFormat::Png, evidence: SourceEvidence::Unresolved, embedded_icc: None };
        let working = gray.working_image_with(&profile).unwrap();
        assert_ne!(working.pixels()[0], working.pixels()[1]);
        let float = DecodedBitmap { image: DynamicImage::ImageRgb32F(image::ImageBuffer::from_raw(2, 1, vec![0.2, 0.2, 0.2, 0.200001, 0.200001, 0.200001]).unwrap()),
            format: ImageFormat::OpenExr, evidence: SourceEvidence::Unresolved, embedded_icc: None };
        assert!(float.working_image().is_err(), "floating format does not invent its RGB interpretation");
        let working = float.working_image_with(&profile).unwrap();
        assert_ne!(working.pixels()[0], working.pixels()[1]);
        let bad = [[f32::NAN, 0.0, 0.0]];
        assert!(super::super::icc::input_rgbf32_to_working(&profile, &bad).is_err());
    }

    #[test]
    fn borrowed_rgb8_and_rgb16_buffers_keep_identical_color_values() {
        let profile = super::super::icc::display_p3_icc().unwrap();
        let bytes8 = vec![31_u8, 127, 255, 240, 11, 63];
        let bytes16: Vec<u16> = bytes8.iter().map(|value| u16::from(*value) * 257).collect();
        let make = |image| DecodedBitmap {
            image,
            format: ImageFormat::Png,
            evidence: SourceEvidence::EmbeddedIcc,
            embedded_icc: Some(profile.clone()),
        };
        let eight = make(DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(2, 1, bytes8).unwrap(),
        ));
        let sixteen = make(DynamicImage::ImageRgb16(
            image::ImageBuffer::from_raw(2, 1, bytes16).unwrap(),
        ));
        assert_eq!(eight.linear_rgb().unwrap(), sixteen.linear_rgb().unwrap());
    }

    #[test]
    fn embedded_icc_and_sixteen_bit_samples_survive_one_decode() {
        let icc = super::super::icc::srgb_icc().unwrap();
        let mut bytes = Vec::new();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
        encoder.set_icc_profile(icc.bytes().to_vec()).unwrap();
        let samples: Vec<u8> = [0x1234_u16, 0xabcd, 0xffff]
            .into_iter()
            .flat_map(u16::to_ne_bytes)
            .collect();
        encoder
            .write_image(&samples, 1, 1, ExtendedColorType::Rgb16)
            .unwrap();
        let decoded = decode_bitmap(&bytes).unwrap();
        assert_eq!(decoded.evidence, SourceEvidence::EmbeddedIcc);
        assert_eq!(decoded.embedded_icc.as_ref().unwrap().id(), icc.id());
        assert!(matches!(
            decoded.source(),
            Some(SourceColor::EmbeddedIcc { .. })
        ));
        let DynamicImage::ImageRgb16(image) = &decoded.image else {
            panic!("16-bit precision lost")
        };
        assert_eq!(image.get_pixel(0, 0).0, [0x1234, 0xabcd, 0xffff]);
        let linear = decoded.linear_rgb().unwrap();
        assert_eq!(linear.len(), 1);
        assert!(linear[0].iter().all(|v| v.is_finite()));
        let working = decoded.working_image().unwrap();
        assert_eq!(working.dimensions(), (1, 1));
        assert_eq!(working.pixels(), linear.as_slice());
    }

    #[test]
    fn untagged_png_is_unresolved_and_explicit_assignment_works() {
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(&[31, 127, 255], 1, 1, ExtendedColorType::Rgb8)
            .unwrap();
        let decoded = decode_bitmap(&bytes).unwrap();
        assert_eq!(decoded.evidence, SourceEvidence::Unresolved);
        assert!(decoded.source().is_none());
        assert!(matches!(
            decoded.linear_rgb(),
            Err(BitmapColorError::UnresolvedSource)
        ));
        assert_eq!(
            decoded
                .linear_rgb_with(&super::super::icc::srgb_icc().unwrap())
                .unwrap()
                .len(),
            1
        );
        assert!(matches!(
            decoded.working_image(),
            Err(BitmapColorError::UnresolvedSource)
        ));
        assert_eq!(
            decoded
                .working_image_with(&super::super::icc::srgb_icc().unwrap())
                .unwrap()
                .dimensions(),
            (1, 1)
        );
    }

    #[test]
    fn png_srgb_chunk_is_an_explicit_source() {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[31, 127, 255])
            .unwrap();
        let decoded = decode_bitmap(&bytes).unwrap();
        assert_eq!(decoded.evidence, SourceEvidence::TaggedPngSrgb);
        assert!(matches!(decoded.source(), Some(SourceColor::TaggedSrgb)));
        assert_eq!(decoded.linear_rgb().unwrap().len(), 1);
    }

    #[test]
    fn rgba16_keeps_alpha_separate_from_color_transform() {
        let mut bytes = Vec::new();
        let icc = super::super::icc::srgb_icc().unwrap();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
        encoder.set_icc_profile(icc.bytes().to_vec()).unwrap();
        let samples: Vec<u8> = [0x8000_u16, 0x4000, 0x2000, 0x8000]
            .into_iter()
            .flat_map(u16::to_ne_bytes)
            .collect();
        encoder
            .write_image(&samples, 1, 1, ExtendedColorType::Rgba16)
            .unwrap();
        let decoded = decode_bitmap(&bytes).unwrap();
        assert!(matches!(&decoded.image, DynamicImage::ImageRgba16(_)));
        assert!(matches!(
            decoded.linear_rgb(),
            Err(BitmapColorError::UnsupportedPixels)
        ));
        let linear = decoded.linear_rgba().unwrap();
        assert_eq!(linear.len(), 1);
        assert!((linear[0][3] - 0.5).abs() < 0.00002);
        assert!(linear[0][..3].iter().all(|v| v.is_finite()));
    }

    #[test]
    fn untagged_jpeg_has_explicit_srgb_assumption() {
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut bytes)
            .write_image(&[31, 127, 255], 1, 1, ExtendedColorType::Rgb8)
            .unwrap();
        let decoded = decode_bitmap(&bytes).unwrap();
        assert_eq!(decoded.evidence, SourceEvidence::AssumedJpegSrgb);
        assert!(matches!(decoded.source(), Some(SourceColor::AssumedSrgb)));
        assert_eq!(decoded.linear_rgb().unwrap().len(), 1);
    }
}
