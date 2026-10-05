//! 新处理版本的像素容器。旧版 `develop::LinearImage` 仍是 u16 线性 sRGB；
//! 两者绝不隐式互转，以免旧 issue 被新的色域和截断规则改变。

use thiserror::Error;

use super::WorkingSpace;
use super::icc::{IccError, RgbIcc, working_to_output_rgb16, working_to_output_rgb16_flat};

/// D65 线性 sRGB → D65 线性 Rec.2020。RAW 老后端已映射到 sRGB 原色时可用；
/// 新 RAW 路径须在 rawler 的高光/负值裁切前接入，不能靠本矩阵恢复已丢的颜色。
pub(crate) const LINEAR_SRGB_TO_REC2020: [[f32; 3]; 3] = [
    [0.627_404, 0.329_282, 0.043_313_6],
    [0.069_097, 0.919_54, 0.011_361_2],
    [0.016_391_6, 0.088_013_2, 0.895_595],
];

pub(crate) const LINEAR_REC2020_TO_SRGB: [[f32; 3]; 3] = [
    [1.660_491, -0.587_641_1, -0.072_849_86],
    [-0.124_550_5, 1.132_899_9, -0.008_349_42],
    [-0.018_150_76, -0.100_578_9, 1.118_729_7],
];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorkingImageError {
    #[error("working image dimensions or pixel count are invalid")]
    InvalidShape,
    #[error("working image has a non-finite channel")]
    NonFinite,
    #[error("ICC output conversion failed: {0}")]
    Icc(#[from] IccError),
}

/// f32 线性 Rec.2020 D65。允许合法的负值和大于 1 的值，直到导出边界才量化。
#[derive(Debug, Clone, PartialEq)]
pub struct WorkingImage {
    width: u32,
    height: u32,
    rgb: Vec<f32>,
}

impl WorkingImage {
    pub fn new(width: u32, height: u32, rgb: Vec<[f32; 3]>) -> Result<Self, WorkingImageError> {
        Self::validate(width, height, &rgb)?;
        Ok(Self { width, height, rgb: rgb.into_flattened() })
    }

    fn validate(width: u32, height: u32, rgb: &[[f32; 3]]) -> Result<(), WorkingImageError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or(WorkingImageError::InvalidShape)?;
        if width == 0 || height == 0 || rgb.len() != expected {
            return Err(WorkingImageError::InvalidShape);
        }
        if rgb.iter().flatten().any(|value| !value.is_finite()) {
            return Err(WorkingImageError::NonFinite);
        }
        Ok(())
    }

    /// 只做原色矩阵变换；不加伽马、不做高光压缩、不截断范围。
    pub fn from_linear_srgb(
        width: u32,
        height: u32,
        mut rgb: Vec<[f32; 3]>,
    ) -> Result<Self, WorkingImageError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or(WorkingImageError::InvalidShape)?;
        if width == 0 || height == 0 || rgb.len() != expected {
            return Err(WorkingImageError::InvalidShape);
        }
        for pixel in &mut rgb {
            if pixel.iter().any(|value| !value.is_finite()) {
                return Err(WorkingImageError::NonFinite);
            }
            *pixel = crate::develop::color::mat3_vec3(LINEAR_SRGB_TO_REC2020, *pixel);
            if pixel.iter().any(|value| !value.is_finite()) {
                return Err(WorkingImageError::NonFinite);
            }
        }
        Ok(Self { width, height, rgb: rgb.into_flattened() })
    }

    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    #[must_use]
    pub fn space(&self) -> WorkingSpace {
        WorkingSpace::LinearRec2020D65
    }

    #[must_use]
    pub fn pixels(&self) -> &[[f32; 3]] {
        self.rgb.as_chunks::<3>().0
    }

    pub fn pixels_mut(&mut self) -> &mut [[f32; 3]] {
        self.rgb.as_chunks_mut::<3>().0
    }

    pub fn as_view(&self) -> crate::develop::sample::RgbView<'_, f32> {
        crate::develop::sample::RgbView {
            width: self.width,
            height: self.height,
            rgb: &self.rgb,
        }
    }

    pub fn into_flat(self) -> Vec<f32> {
        self.rgb
    }

    pub fn into_oriented(self, orientation: u16) -> Self {
        if !(2..=8).contains(&orientation) { return self; }
        let (width, height, rgb) = crate::develop::sample::orient_rgb(self.as_view(), orientation);
        Self { width, height, rgb }
    }

    /// Move an interleaved spatial result into the working container, without a full copy.
    pub fn from_flat(width: u32, height: u32, rgb: Vec<f32>) -> Result<Self, WorkingImageError> {
        let view = crate::develop::sample::RgbView::new(width, height, &rgb)
            .ok_or(WorkingImageError::InvalidShape)?;
        if view.rgb.iter().any(|value| !value.is_finite()) {
            return Err(WorkingImageError::NonFinite);
        }
        Ok(Self { width, height, rgb })
    }

    /// 唯一的 RGB 输出量化入口；调用方还须嵌入同一目标 ICC。
    pub fn to_rgb16(&self, target: &RgbIcc) -> Result<Vec<[u16; 3]>, WorkingImageError> {
        working_to_output_rgb16(target, self.pixels()).map_err(Into::into)
    }

    /// 与 `to_rgb16` 数值一致，直接给现有编码器的交错缓冲，省掉整图二次复制。
    pub fn to_rgb16_flat(&self, target: &RgbIcc) -> Result<Vec<u16>, WorkingImageError> {
        working_to_output_rgb16_flat(target, self.pixels()).map_err(Into::into)
    }

    /// 预览降采样保留浮点工作域，不量化到 8/16 位；长边已达标时直接移动原缓冲。
    /// 使用区域箱式平均，复杂度为 O(源像素 + 目标像素)。
    pub fn into_downscaled(self, long_edge: u32) -> Self {
        let long = self.width.max(self.height);
        if long_edge == 0 || long <= long_edge {
            return self;
        }
        let (width, height, rgb) = downscale_float_rgb(self.as_view(), long_edge);
        Self { width, height, rgb }
    }
}

/// Shared borrowed area sampler; no full-resolution clone while preparing a preview.
pub(crate) fn downscale_float_rgb(source: crate::develop::sample::RgbView<'_, f32>, long_edge: u32) -> (u32, u32, Vec<f32>) {
    let long = source.width.max(source.height);
    if long_edge == 0 || long <= long_edge { return (source.width, source.height, source.rgb.to_vec()); }
        let scale = f64::from(long_edge) / f64::from(long);
        let width = ((f64::from(source.width) * scale).round() as u32).max(1);
        let height = ((f64::from(source.height) * scale).round() as u32).max(1);
        let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
        for y in 0..height {
            let start_y = (u64::from(y) * u64::from(source.height) / u64::from(height)) as usize;
            let end_y = (u64::from(y + 1) * u64::from(source.height) / u64::from(height)) as usize;
            for x in 0..width {
                let start_x = (u64::from(x) * u64::from(source.width) / u64::from(width)) as usize;
                let end_x = (u64::from(x + 1) * u64::from(source.width) / u64::from(width)) as usize;
                let mut total = [0.0_f64; 3];
                let mut count = 0_usize;
                for source_y in start_y..end_y.max(start_y + 1) {
                    for source_x in start_x..end_x.max(start_x + 1) {
                        let pixel = source.rgb.as_chunks::<3>().0[source_y * source.width as usize + source_x];
                        for channel in 0..3 {
                            total[channel] += f64::from(pixel[channel]);
                        }
                        count += 1;
                    }
                }
                rgb.push(total.map(|value| (value / count as f64) as f32));
            }
        }
        (width, height, rgb.into_flattened())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::icc::srgb_icc;

    #[test]
    fn shape_nonfinite_and_unbounded_values_have_distinct_outcomes() {
        assert_eq!(
            WorkingImage::new(0, 1, vec![]).unwrap_err(),
            WorkingImageError::InvalidShape
        );
        assert_eq!(
            WorkingImage::new(u32::MAX, u32::MAX, vec![]).unwrap_err(),
            WorkingImageError::InvalidShape
        );
        assert_eq!(
            WorkingImage::new(1, 1, vec![[f32::NAN, 0.0, 0.0]]).unwrap_err(),
            WorkingImageError::NonFinite
        );
        let image = WorkingImage::new(1, 2, vec![[-0.25, 1.5, 0.18], [0.0, 1.0, 8.0]]).unwrap();
        assert_eq!(image.space(), WorkingSpace::LinearRec2020D65);
        assert_eq!(image.dimensions(), (1, 2));
        assert_eq!(image.pixels()[0], [-0.25, 1.5, 0.18]);
        assert_eq!(image.pixels()[1][2], 8.0);
        assert_eq!(image.to_rgb16(&srgb_icc().unwrap()).unwrap().len(), 2);
    }

    #[test]
    fn linear_srgb_matrix_keeps_negative_and_over_one_before_output() {
        let image = WorkingImage::from_linear_srgb(
            3,
            1,
            vec![[1.0, 0.0, 0.0], [-0.5, 0.0, 0.0], [2.0, 0.0, 0.0]],
        )
        .unwrap();
        assert!((image.pixels()[0][0] - 0.627_404).abs() < 1e-6);
        assert!(image.pixels()[1].iter().all(|channel| *channel < 0.0));
        assert!(image.pixels()[2][0] > 1.0);
    }

    #[test]
    fn downscale_averages_float_pixels_without_clamping_or_upscaling() {
        let pixels = vec![
            [-1.0, 2.0, 0.0],
            [1.0, 4.0, 0.0],
            [-3.0, 6.0, 0.0],
            [3.0, 8.0, 0.0],
        ];
        let image = WorkingImage::new(4, 1, pixels).unwrap();
        assert_eq!(image.clone().into_downscaled(8), image);
        let smaller = image.into_downscaled(2);
        assert_eq!(smaller.dimensions(), (2, 1));
        assert_eq!(smaller.pixels(), &[[0.0, 3.0, 0.0], [0.0, 7.0, 0.0]]);
    }
}
