//! **渲染用的图像**：`GpuContext` / `OffscreenRenderer` 唯一接受的像素形态。
//!
//! 它原来是 `scene.rs` 里的 `TestImage`（spike 的合成测试图专用）。
//! M3-W2 编辑器要往同一套 wgpu 上下文里塞**真实照片**，于是把它提到模块级、
//! 补上「从 RGB8 来」「夹到设备上限」两件事 —— 而不是另造一个「照片类型」
//! （`AGENTS.md` §2.12：同一个能力只允许有一套实现）。
//!
//! ## 为什么是 RGBA8、而不是直接用解码器给的 RGB8
//!
//! wgpu 的 `write_texture` 要求**每行字节数是 256 的倍数**
//! （`COPY_BYTES_PER_ROW_ALIGNMENT`）。6000 宽的 RGB8 是 18000 字节/行 —— 不是 256 的倍数，
//! 直接传会校验失败；按行补 padding 又要整块重排，代价与「扩成 4 通道」差不多。
//! 而 WebGPU 里根本没有三通道的可采样格式（`rgba8unorm` 是标准），所以：
//! **扩成 RGBA8 是唯一不需要额外缓冲的走法**（代价是 24MP 图多占 24MB 显存）。
//!
//! 色彩空间：纹理按 `Rgba8UnormSrgb` 采样（见 `create_image_texture`），
//! 所以这里的字节就是「文件里/解码器给的那组 8bit 值」，不做任何转换 ——
//! 色彩管理整体是后期里程碑（`AGENTS.md` §6.1）。

use image::{DynamicImage, RgbaImage};

/// 一张 RGBA8（行主序紧密排列）的图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderImage {
    pub width: u32,
    pub height: u32,
    /// 长度必须是 `width × height × 4`。
    pub pixels: Vec<u8>,
}

impl RenderImage {
    /// 1×1 全透明 —— **「还没有照片」的占位纹理**。
    ///
    /// 为什么要有它：`GpuContext` 的纹理与绑定组必须始终是合法的（wgpu 不接受 0 尺寸纹理），
    /// 而「没有照片」这件事由 [`crate::render::GpuContext`] 的 `has_image` 表达 ——
    /// 那才是「画不画这个四边形」的开关。占位纹理只是让资源合法。
    #[must_use]
    pub fn transparent_1x1() -> Self {
        Self {
            width: 1,
            height: 1,
            pixels: vec![0, 0, 0, 0],
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
            width,
            height,
            pixels,
        }
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
            width,
            height,
            pixels,
        })
    }

    /// 长度与尺寸对不对得上（跨进程 / 跨线程边界上来的数据要防一手）。
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.pixels.len() == (self.width as usize) * (self.height as usize) * 4
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
        let source = RgbaImage::from_raw(self.width, self.height, self.pixels.clone());
        let Some(source) = source else {
            return self.clone(); // 长度不合法：原样返回，让调用方自己报错
        };
        let resized = DynamicImage::ImageRgba8(source).resize(max_edge, max_edge, image::imageops::FilterType::Lanczos3);
        let rgba = resized.to_rgba8();
        let (width, height) = (rgba.width(), rgba.height());
        Self {
            width,
            height,
            pixels: rgba.into_raw(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
