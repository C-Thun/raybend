//! **渲染用的图像**：`GpuContext` / `OffscreenRenderer` 唯一接受的像素形态。
//!
//! 它原来是 `scene.rs` 里的 `TestImage`（spike 的合成测试图专用）。
//! M3-W2 编辑器要往同一套 wgpu 上下文里塞**真实照片**，于是把它提到模块级、
//! 补上「从 RGB8 来」「夹到设备上限」两件事 —— 而不是另造一个「照片类型」
//! （`AGENTS.md` §2.12：同一个能力只允许有一套实现）。
//!
//! 旧图保留 RGBA8 sRGB。V2 空间处理结果可打包 RGBA16F；无空间处理时
//! 直接共享 f32 参考图，上传时按有界行块转换成 Rec.2020 half，不常驻整图打包副本。
//! wgpu queue.write_texture 不要求 256 字节行对齐（encoder copy 才有此约束）。

use image::{DynamicImage, RgbaImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderEncoding {
    #[default]
    Srgb8,
    LinearRec2020Half,
}

impl RenderEncoding {
    pub const fn bytes_per_pixel(self) -> u32 {
        match self { Self::Srgb8 => 4, Self::LinearRec2020Half => 8 }
    }
    pub const fn texture_format(self) -> wgpu::TextureFormat {
        match self {
            Self::Srgb8 => wgpu::TextureFormat::Rgba8UnormSrgb,
            Self::LinearRec2020Half => wgpu::TextureFormat::Rgba16Float,
        }
    }
}

/// 同一渲染器的编码字节或共享浮点源。
#[derive(Debug, Clone, PartialEq)]
pub struct RenderImage {
    pub encoding: RenderEncoding,
    pub width: u32,
    pub height: u32,
    /// Encoded bytes; empty when `linear_source` owns the deferred pixels.
    pub pixels: Vec<u8>,
    /// Shared f32 source for deferred, bounded upload. No resident half copy.
    pub(crate) linear_source: Option<std::sync::Arc<crate::develop::working::WorkingReferenceImage>>,
}

/// Reused bounded upload scratch. SIMD conversion lives in the existing half crate.
#[derive(Default)]
pub(crate) struct HalfUpload {linear:Vec<f32>,half:Vec<half::f16>,pub bytes:Vec<u8>}

impl RenderImage {
    /// 1×1 全透明 —— **「还没有照片」的占位纹理**。
    ///
    /// 为什么要有它：`GpuContext` 的纹理与绑定组必须始终是合法的（wgpu 不接受 0 尺寸纹理），
    /// 而「没有照片」这件事由 [`crate::render::GpuContext`] 的 `has_image` 表达 ——
    /// 那才是「画不画这个四边形」的开关。占位纹理只是让资源合法。
    #[must_use]
    pub fn transparent_1x1() -> Self {
        Self {
            encoding: RenderEncoding::Srgb8,
            width: 1,
            height: 1,
            pixels: vec![0, 0, 0, 0],
            linear_source: None,
        }
    }

    /// 一块纯色（测试与「降级到纯色」用）。
    #[must_use]
    pub fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let mut pixels = Vec::with_capacity((width as usize) * (height as usize) * 4);
        for _ in 0..(width as usize) * (height as usize) {
            pixels.extend_from_slice(&rgba);
        }
        Self {
            encoding: RenderEncoding::Srgb8,
            width,
            height,
            pixels,
            linear_source: None,        }
    }

    /// RGB8（解码器给的形态）→ RGBA8，alpha 一律 255。
    ///
    /// 长度对不上返回 `None`（**不猜、不补零**：长度错说明上游已经错了，
    /// 静默修好只会把错带进纹理 —— `AGENTS.md` §7.9 的老教训）。
    #[must_use]
    pub fn from_rgb8(width: u32, height: u32, rgb: &[u8]) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let expected = (width as usize) * (height as usize) * 3;
        if rgb.len() != expected {
            return None;
        }
        let mut pixels = Vec::with_capacity((width as usize) * (height as usize) * 4);
        for chunk in rgb.as_chunks::<3>().0 {
            pixels.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
        }
        Some(Self {
            encoding: RenderEncoding::Srgb8,
            width,
            height,
            pixels,
            linear_source: None,        })
    }

    /// 长度与尺寸对不对得上（跨进程 / 跨线程边界上来的数据要防一手）。
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        if let Some(source) = &self.linear_source {
            return self.encoding == RenderEncoding::LinearRec2020Half && self.pixels.is_empty()
                && (source.width,source.height) == (self.width,self.height)
                && source.view().rgb.len() == self.width as usize * self.height as usize * 3;
        }
        self.width > 0
            && self.height > 0
            && (self.width as usize).checked_mul(self.height as usize)
                .and_then(|n| n.checked_mul(self.encoding.bytes_per_pixel() as usize)) == Some(self.pixels.len())
    }

    /// Only the presentation texture is half precision. Negative and over-one
    /// values survive; values beyond finite half range are rejected, never clipped.
    pub fn from_working(image: &crate::color::working::WorkingImage) -> Option<Self> {
        let (width, height) = image.dimensions();
        let mut pixels = Vec::with_capacity(image.pixels().len() * 8);
        for rgb in image.pixels() {
            for value in rgb.iter().copied().chain(std::iter::once(1.0)) {
                let value = half::f16::from_f32(value);
                if !value.is_finite() { return None; }
                pixels.extend_from_slice(&value.to_bits().to_le_bytes());
            }
        }
        Some(Self { encoding: RenderEncoding::LinearRec2020Half, width, height, pixels, linear_source: None })
    }

    /// Share the prepared linear-sRGB source. Device upload converts only a tile
    /// at a time into Rec.2020 half, and recovery can reuse this same allocation.
    pub(crate) fn from_shared_linear(source: std::sync::Arc<crate::develop::working::WorkingReferenceImage>) -> Option<Self> {
        for pixel in source.view().rgb.as_chunks::<3>().0 {
            let rgb = crate::develop::color::mat3_vec3(crate::color::working::LINEAR_SRGB_TO_REC2020, *pixel);
            if rgb.iter().any(|v| !v.is_finite() || v.abs() >= 65520.0) { return None; }
        }
        Some(Self { encoding: RenderEncoding::LinearRec2020Half, width:source.width,height:source.height,pixels:Vec::new(),linear_source:Some(source) })
    }

    pub(crate) fn shares_source(&self, other: &Self) -> bool {
        match (&self.linear_source,&other.linear_source) {
            (Some(a),Some(b)) => std::sync::Arc::ptr_eq(a,b),
            _ => false,
        }
    }

    /// Pack a bounded group of rows. Vec capacity is reused by the uploader.
    pub(crate) fn pack_rows(&self, first:u32, rows:u32, output:&mut HalfUpload) {
        use half::slice::HalfFloatSliceExt;
        let source=self.linear_source.as_ref().expect("deferred float source");
        let start=first as usize*self.width as usize*3;
        let end=start+rows as usize*self.width as usize*3;
        let count=rows as usize*self.width as usize*4;
        output.linear.clear();output.linear.reserve(count);
        for pixel in source.view().rgb[start..end].as_chunks::<3>().0 {
            let rgb=crate::develop::color::mat3_vec3(crate::color::working::LINEAR_SRGB_TO_REC2020,*pixel);
            output.linear.extend_from_slice(&[rgb[0],rgb[1],rgb[2],1.0]);
        }
        output.half.resize(count,half::f16::ZERO);
        output.half.convert_from_f32_slice(&output.linear);
        output.bytes.clear();output.bytes.reserve(count*2);
        for value in &output.half {output.bytes.extend_from_slice(&value.to_bits().to_le_bytes());}
    }

    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.pixels.len()
    }

    /// 长边超过 `max_edge` 就缩到它（Lanczos3）；否则**原样返回**（不放大）。
    ///
    /// 用途只有一个：**夹到设备的 `max_texture_dimension_2d`**（默认 8192）。
    /// 一张 100MP 的照片（12000×9000）会超限 —— 那种图宁可降采样也不能让 `create_texture` 抛错。
    /// 这不是缩略图质量管线（那一份在 `thumbnail::render::resize_for_thumb`，两段式 + 实测数据），
    /// 所以这里就一句 `image::resize` 足够。
    #[must_use]
    pub fn clamped_to_long_edge(&self, max_edge: u32) -> Self {
        let long = self.width.max(self.height);
        if long <= max_edge || max_edge == 0 {
            return self.clone();
        }
        if let Some(source) = &self.linear_source {
            return Self::from_shared_linear(std::sync::Arc::new(source.downscaled_to(max_edge))).expect("averaging finite half values remains finite");
        }
        if self.encoding == RenderEncoding::LinearRec2020Half {
            let rgb = self.pixels.as_chunks::<8>().0.iter().map(|pixel| std::array::from_fn(|c| {
                half::f16::from_bits(u16::from_le_bytes([pixel[c * 2], pixel[c * 2 + 1]])).to_f32()
            })).collect();
            return crate::color::working::WorkingImage::new(self.width, self.height, rgb)
                .ok().and_then(|image| Self::from_working(&image.into_downscaled(max_edge)))
                .unwrap_or_else(|| self.clone());
        }
        let source = RgbaImage::from_raw(self.width, self.height, self.pixels.clone());
        let Some(source) = source else {
            return self.clone(); // 长度不合法：原样返回，让调用方自己报错
        };
        let resized = DynamicImage::ImageRgba8(source).resize(max_edge, max_edge, image::imageops::FilterType::Lanczos3);
        let rgba = resized.to_rgba8();
        let (width, height) = (rgba.width(), rgba.height());
        Self {
            encoding: RenderEncoding::Srgb8,
            width,
            height,
            pixels: rgba.into_raw(),
            linear_source: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_working_texture_preserves_range_and_rejects_overflow() {
        let image = crate::color::working::WorkingImage::new(2, 1, vec![[-0.2, 0.18, 2.0], [0.1; 3]]).unwrap();
        let texture = RenderImage::from_working(&image).unwrap();
        assert!(texture.is_consistent());
        assert_eq!(texture.byte_len(), 16);
        assert_eq!(texture.encoding.texture_format(), wgpu::TextureFormat::Rgba16Float);
        for (actual, expected) in texture.pixels.chunks_exact(8).zip(image.pixels()) {
            for c in 0..3 {
                let value = half::f16::from_bits(u16::from_le_bytes([actual[c*2], actual[c*2+1]])).to_f32();
                assert!((value - expected[c]).abs() < 0.001);
            }
        }
        let enormous = crate::color::working::WorkingImage::new(1, 1, vec![[70_000.0, 0.0, 0.0]]).unwrap();
        assert!(RenderImage::from_working(&enormous).is_none());
        let smaller = texture.clamped_to_long_edge(1);
        assert_eq!((smaller.width, smaller.height), (1, 1));
        assert!(smaller.is_consistent());
        assert_eq!(smaller.encoding, texture.encoding);
    }

    #[test]
    fn deferred_tiles_match_scalar_half_at_range_and_rounding_boundaries() {
        use crate::develop::working::WorkingReferenceImage;
        let source = std::sync::Arc::new(WorkingReferenceImage {
            width: 3, height: 2,
            rgb: vec![-0.2, 0.18, 2.0, 1e-8, 0.000061, 0.3333, 65500.0, 65500.0, 65500.0,
                -65500.0, -65500.0, -65500.0, 0.0, 1.0, 0.5, 12.0, -1.0, 0.2],
        });
        let image = RenderImage::from_shared_linear(source.clone()).unwrap();
        let mut scratch = HalfUpload::default();
        let mut tiles = Vec::new();
        for row in 0..2 { image.pack_rows(row, 1, &mut scratch); tiles.extend_from_slice(&scratch.bytes); }
        let expected: Vec<u8> = source.rgb.as_chunks::<3>().0.iter().flat_map(|rgb| {
            crate::develop::color::mat3_vec3(crate::color::working::LINEAR_SRGB_TO_REC2020, *rgb)
                .into_iter().chain([1.0]).flat_map(|v| half::f16::from_f32(v).to_bits().to_le_bytes())
        }).collect();
        assert_eq!(tiles, expected);
        assert_eq!(image.byte_len(), 0);
        for value in [65520.0, -65520.0, f32::INFINITY, f32::NAN] {
            let source = WorkingReferenceImage { width:1, height:1, rgb:vec![value;3] };
            let finite = crate::develop::color::mat3_vec3(crate::color::working::LINEAR_SRGB_TO_REC2020, [value;3])
                .iter().all(|v| half::f16::from_f32(*v).is_finite());
            assert_eq!(RenderImage::from_shared_linear(std::sync::Arc::new(source)).is_some(), finite);
        }
    }

    #[test]
    fn from_rgb8_expands_to_opaque_rgba() {
        let rgb = vec![10, 20, 30, 40, 50, 60];
        let image = RenderImage::from_rgb8(2, 1, &rgb).expect("长度对得上");
        assert_eq!(image.width, 2);
        assert_eq!(image.height, 1);
        assert_eq!(image.pixels, vec![10, 20, 30, 255, 40, 50, 60, 255]);
        assert!(image.is_consistent());
    }

    #[test]
    fn from_rgb8_rejects_bad_shapes_instead_of_padding() {
        // 少一个字节 / 多一个字节 / 0 尺寸 —— 一律 `None`，绝不「凑一个能用的」
        assert!(RenderImage::from_rgb8(2, 1, &[1, 2, 3, 4, 5]).is_none());
        assert!(RenderImage::from_rgb8(2, 1, &[1, 2, 3, 4, 5, 6, 7]).is_none());
        assert!(RenderImage::from_rgb8(0, 1, &[]).is_none());
        assert!(RenderImage::from_rgb8(1, 0, &[]).is_none());
        assert!(RenderImage::from_rgb8(1, 1, &[]).is_none());
    }

    #[test]
    fn solid_and_transparent_placeholders_are_consistent() {
        let solid = RenderImage::solid(3, 2, [1, 2, 3, 4]);
        assert!(solid.is_consistent());
        assert_eq!(solid.byte_len(), 3 * 2 * 4);
        assert_eq!(&solid.pixels[..4], &[1, 2, 3, 4]);
        let placeholder = RenderImage::transparent_1x1();
        assert!(placeholder.is_consistent());
        assert_eq!(placeholder.pixels, vec![0, 0, 0, 0]);
        // 0 尺寸也不许造出空纹理（wgpu 会拒绝）
        let tiny = RenderImage::solid(0, 0, [9, 9, 9, 9]);
        assert_eq!((tiny.width, tiny.height), (1, 1));
        assert!(tiny.is_consistent());
    }

    #[test]
    fn clamp_leaves_small_images_untouched() {
        let image = RenderImage::solid(8, 4, [7, 7, 7, 255]);
        let same = image.clamped_to_long_edge(64);
        assert_eq!(same, image, "没超限就该原样返回（连拷贝都不做）");
        let no_limit = image.clamped_to_long_edge(0);
        assert_eq!(no_limit, image, "0 = 不限制");
    }

    #[test]
    fn clamp_shrinks_long_edge_and_keeps_aspect() {
        let image = RenderImage::solid(800, 400, [0, 128, 255, 255]);
        let small = image.clamped_to_long_edge(200);
        assert!(small.is_consistent());
        assert_eq!(small.width.max(small.height), 200, "长边正好压到上限");
        assert_eq!(small.height, 100, "比例保持（800×400 → 200×100）");
        assert_eq!(small.pixels.len(), 200 * 100 * 4);
    }
}
