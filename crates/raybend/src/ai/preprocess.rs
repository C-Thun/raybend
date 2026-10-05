//! 模型独有的 resize/normalize；照片解码、朝向、ICC 仍由既有服务负责。
use crate::thumbnail::render::{apply_orientation, resize_for_thumb};
use crate::{Error, Result};
use image::{RgbImage, imageops::FilterType};

pub const EDGE: u32 = 224;
pub const INPUT_LEN: usize = 3 * EDGE as usize * EDGE as usize;
const MEAN: [f32; 3] = [0.48145466, 0.4578275, 0.40821073];
const STD: [f32; 3] = [0.26862954, 0.261_302_6, 0.275_777_1];

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResizePolicy {
    CenterCrop,
    Fit,
}

/// worker 与离线 probe 共用原片入口。产品只能在隔离 worker 调用；RAW 不做全尺寸解码。
pub fn original_path(path: &std::path::Path, raw: bool) -> Result<RgbImage> {
    if !path.is_absolute() {
        return Err(Error::Unsupported("照片路径必须为绝对路径".into()));
    }
    let (bytes, orientation) = if raw {
        let jpeg = crate::media::embedded::read(path)
            .ok_or_else(|| Error::Unsupported("RAW 缺少可用内嵌预览".into()))?;
        (jpeg.bytes, jpeg.orientation)
    } else {
        (
            crate::fs_asset::read_limited(path, 256 * 1024 * 1024, "AI 原始位图")?,
            None,
        )
    };
    original_bitmap(&bytes, orientation)
}

/// bytes 是已选定原始位图/RAW 内嵌 JPEG，不得来自 latest 或编辑缓存。
/// RAW 字节的抽取必须由隔离 worker 完成，不能在此偷偷完整解码 RAW。
pub fn original_bitmap(bytes: &[u8], orientation_override: Option<u16>) -> Result<RgbImage> {
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(Error::Unsupported(
            "AI 原始图片超过 256 MiB 输入上限".into(),
        ));
    }
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let (w, h) = reader
        .into_dimensions()
        .map_err(|e| Error::Unsupported(e.to_string()))?;
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 100_000_000 {
        return Err(Error::Unsupported("AI 图片尺寸无效或超过一亿像素".into()));
    }
    let orientation = orientation_override
        .or_else(|| {
            crate::media::exif::read_bytes(bytes)
                .and_then(|e| e.orientation)
                .map(|e| crate::media::meta::normalize_orientation(Some(e)))
        })
        .unwrap_or(1);
    let mut decoded = crate::color::input::decode_bitmap(bytes)
        .map_err(|e| Error::Unsupported(format!("AI 原始色彩：{e}")))?;
    // 先在输入编码域缩到代理尺度，避免为大图分配一份全尺寸浮点工作像素。
    decoded.image = resize_for_thumb(apply_orientation(&decoded.image, orientation), 384);
    let linear = decoded
        .working_image()
        .map_err(|e| Error::Unsupported(format!("AI 原始色彩：{e}")))?;
    let rgb16 = linear
        .to_rgb16(&crate::color::icc::srgb_icc().map_err(|e| Error::Unsupported(e.to_string()))?)
        .map_err(|e| Error::Unsupported(e.to_string()))?;
    let data = rgb16
        .into_iter()
        .flat_map(|p| p.map(|c| ((u32::from(c) + 128) / 257) as u8))
        .collect();
    RgbImage::from_raw(decoded.image.width(), decoded.image.height(), data)
        .ok_or_else(|| Error::Unsupported("AI 原始像素长度不一致".into()))
}

/// 全图模式用 CLIP 均值作为边框，归一化后接近零；中心裁切与全图模式需分别校准。
pub fn tensor(rgb: &RgbImage, policy: ResizePolicy) -> Result<Vec<f32>> {
    tensor_with(rgb, policy, false)
}
/// SigLIP 对照严格用它的 224 方形 resize / mean=std=.5，不套 CLIP 归一化。
pub fn siglip_tensor(rgb: &RgbImage) -> Result<Vec<f32>> {
    tensor_with(rgb, ResizePolicy::Fit, true)
}
fn tensor_with(rgb: &RgbImage, policy: ResizePolicy, square: bool) -> Result<Vec<f32>> {
    let (w, h) = rgb.dimensions();
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 100_000_000 {
        return Err(Error::Unsupported("AI 预处理尺寸无效".into()));
    }
    let denominator = match policy {
        ResizePolicy::CenterCrop => w.min(h),
        ResizePolicy::Fit => w.max(h),
    };
    let nw = if square {
        EDGE
    } else {
        (u64::from(w) * u64::from(EDGE) / u64::from(denominator)).max(1) as u32
    };
    let nh = if square {
        EDGE
    } else {
        (u64::from(h) * u64::from(EDGE) / u64::from(denominator)).max(1) as u32
    };
    // 不让极端窄图在中心裁切时分配数 GB 的中间图。
    if u64::from(nw) * u64::from(nh) > 16_000_000 {
        return Err(Error::Unsupported("AI 图片宽高比超出中心裁切预算".into()));
    }
    let resized = image::imageops::resize(rgb, nw, nh, FilterType::CatmullRom);
    let (mean, std) = if square {
        ([0.5; 3], [0.5; 3])
    } else {
        (MEAN, STD)
    };
    let mut values = vec![0_f32; INPUT_LEN];
    let (crop_x, crop_y, pad_x, pad_y) = match policy {
        ResizePolicy::CenterCrop => ((nw - EDGE) / 2, (nh - EDGE) / 2, 0, 0),
        ResizePolicy::Fit => (0, 0, (EDGE - nw) / 2, (EDGE - nh) / 2),
    };
    for y in 0..EDGE {
        for x in 0..EDGE {
            let p =
                if x >= pad_x && y >= pad_y && x - pad_x + crop_x < nw && y - pad_y + crop_y < nh {
                    Some(resized.get_pixel(x - pad_x + crop_x, y - pad_y + crop_y).0)
                } else {
                    None
                };
            for c in 0..3 {
                values[c * (EDGE * EDGE) as usize + (y * EDGE + x) as usize] =
                    p.map_or(0., |p| (f32::from(p[c]) / 255. - mean[c]) / std[c]);
            }
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shape_channel_order_normalization_and_fit_padding() {
        let rgb = RgbImage::from_pixel(2, 1, image::Rgb([255, 0, 128]));
        let t = tensor(&rgb, ResizePolicy::Fit).unwrap();
        assert_eq!(t.len(), INPUT_LEN);
        assert_eq!(t[0], 0.);
        let middle = (112 * EDGE + 112) as usize;
        assert!((t[middle] - (1. - MEAN[0]) / STD[0]).abs() < 1e-6);
        assert!((t[(EDGE * EDGE) as usize + middle] - (-MEAN[1]) / STD[1]).abs() < 1e-6);
        assert!(tensor(&RgbImage::new(0, 1), ResizePolicy::Fit).is_err());
        assert!(tensor(&RgbImage::new(1, 100_000), ResizePolicy::CenterCrop).is_err());
    }
    #[test]
    fn original_proxy_reuses_exif_and_rejects_corruption() {
        let image = image::DynamicImage::ImageRgb8(RgbImage::from_pixel(
            400,
            200,
            image::Rgb([50, 60, 70]),
        ));
        let mut cursor = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut cursor, image::ImageFormat::Jpeg)
            .unwrap();
        let upright = original_bitmap(cursor.get_ref(), Some(6)).unwrap();
        assert_eq!(upright.dimensions(), (192, 384));
        assert!(original_bitmap(b"broken", None).is_err());
        let t = tensor(&upright, ResizePolicy::CenterCrop).unwrap();
        assert!(t.iter().all(|v| v.is_finite()));
    }
}
