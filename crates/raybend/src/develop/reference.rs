//! 对比数据侧：复用显影入口生成同尺寸、同镜头几何的未调整参考帧。
use super::{
    curve::CurveSet,
    geometry::{EditGeometry, apply_rgb8},
    lens::{LensCorrection, LensMap},
    params::DevelopParams,
    pipeline::{DevelopStages, LinearImage, render_develop},
};
use crate::display::{self, PixelSize};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
static NEXT_FRAME: AtomicU64 = AtomicU64::new(1);
#[derive(Debug)]
pub struct ReferenceFrame {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}
impl ReferenceFrame {
    /// 已显影的定稿预览可直接作为对比参照；保留唯一帧 ID 供 GPU 纹理缓存。
    pub fn from_rgb(width: u32, height: u32, rgb: Vec<u8>) -> Option<Self> {
        let expected = usize::try_from(width)
            .ok()?
            .checked_mul(usize::try_from(height).ok()?)?
            .checked_mul(3)?;
        if width == 0 || height == 0 || rgb.len() != expected {
            return None;
        }
        Some(Self {
            id: NEXT_FRAME.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            rgb,
        })
    }
}
#[derive(Default)]
pub struct ReferenceCache {
    cached: Option<(
        (u32, u32, LensCorrection, Option<EditGeometry>),
        Arc<ReferenceFrame>,
    )>,
    sooc: Option<((PathBuf, Option<EditGeometry>), Arc<ReferenceFrame>)>,
}
impl ReferenceCache {
    /// 有配对位图时优先显示真正的机内 SOOC。解码和方向处理复用 display 入口。
    /// 失败返回 None，由调用方退回同源的未调整参考帧。
    pub fn get_sooc(
        &mut self,
        path: &Path,
        edit_geometry: Option<EditGeometry>,
    ) -> Option<Arc<ReferenceFrame>> {
        let key = (path.to_path_buf(), edit_geometry);
        if let Some((old, frame)) = &self.sooc
            && *old == key
        {
            return Some(Arc::clone(frame));
        }
        let pixels = display::pixels(path, PixelSize::Screen).ok()??;
        let (width, height, rgb) = match edit_geometry {
            Some(geometry) => apply_rgb8((pixels.width, pixels.height), &pixels.rgb, geometry)?,
            None => (pixels.width, pixels.height, pixels.rgb),
        };
        let frame = Arc::new(ReferenceFrame {
            id: NEXT_FRAME.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            rgb,
        });
        self.sooc = Some((key, Arc::clone(&frame)));
        Some(frame)
    }

    /// 缓存由一次解码源独占，切换源时丢弃；色调、降噪不影响参考帧。
    pub fn get(
        &mut self,
        source: &LinearImage,
        correction: &LensCorrection,
        edit_geometry: Option<EditGeometry>,
    ) -> Arc<ReferenceFrame> {
        let mut geometry = correction.clone();
        geometry.vignetting = None;
        geometry.manual.vignette = 0.0;
        geometry.manual.vignette_range = 0.5;
        let key = (source.width, source.height, geometry.clone(), edit_geometry);
        if let Some((old, frame)) = &self.cached
            && *old == key
        {
            return Arc::clone(frame);
        }
        let map = LensMap::new(&geometry);
        let rgb = render_develop(
            source,
            &DevelopParams::new(None),
            &CurveSet::identity(),
            &DevelopStages {
                lens: Some(&map),
                ..Default::default()
            },
        );
        let (width, height, rgb) = match edit_geometry {
            Some(geometry) => apply_rgb8((source.width, source.height), &rgb, geometry)
                .expect("调用方已校验成片几何"),
            None => (source.width, source.height, rgb),
        };
        let frame = Arc::new(ReferenceFrame {
            id: NEXT_FRAME.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            rgb,
        });
        self.cached = Some((key, Arc::clone(&frame)));
        frame
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::develop::lens::ManualLens;
    #[test]
    fn named_issue_reference_validates_dimensions_and_uses_unique_ids() {
        assert!(ReferenceFrame::from_rgb(0, 1, vec![]).is_none());
        assert!(ReferenceFrame::from_rgb(1, 0, vec![]).is_none());
        assert!(ReferenceFrame::from_rgb(1, 1, vec![1, 2]).is_none());
        assert!(ReferenceFrame::from_rgb(u32::MAX, u32::MAX, vec![]).is_none());
        let first = ReferenceFrame::from_rgb(1, 1, vec![1, 2, 3]).expect("valid RGB");
        let second = ReferenceFrame::from_rgb(1, 1, vec![1, 2, 3]).expect("another valid RGB");
        assert_ne!(first.id, second.id);
        assert_eq!(first.rgb, [1, 2, 3]);
    }

    #[test]
    fn paired_sooc_pixels_are_used_and_cached_with_geometry() {
        use image::{ImageFormat, Rgb, RgbImage};
        let path = std::env::temp_dir().join(format!("raybend-w5-sooc-{}.png", std::process::id()));
        let img = RgbImage::from_fn(8, 4, |x, _| {
            if x < 4 {
                Rgb([20, 0, 0])
            } else {
                Rgb([200, 0, 0])
            }
        });
        img.save_with_format(&path, ImageFormat::Png)
            .expect("写样本");
        let mut cache = ReferenceCache::default();
        let crop = EditGeometry {
            rotation: 0.0,
            crop: Some(super::super::geometry::CropRect {
                x: 0.5,
                y: 0.0,
                width: 0.5,
                height: 1.0,
            }),
            crop_ratio: None,
        };
        let frame = cache.get_sooc(&path, Some(crop)).expect("SOOC 帧");
        assert_eq!((frame.width, frame.height), (4, 4));
        assert!(frame.rgb.chunks_exact(3).all(|pixel| pixel == [200, 0, 0]));
        assert!(Arc::ptr_eq(
            &frame,
            &cache.get_sooc(&path, Some(crop)).expect("缓存")
        ));
        let other = cache.get_sooc(&path, None).expect("原图帧");
        assert_ne!(frame.id, other.id);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reference_has_shared_geometry_but_no_tone_or_vignette_edits() {
        let source = LinearImage {
            width: 9,
            height: 7,
            rgb: (0..63)
                .flat_map(|i| [1000 + i * 700, 2000 + i * 500, 4000 + i * 300])
                .collect(),
        };
        let mut correction = LensCorrection::manual_only(
            9,
            7,
            ManualLens {
                distortion: 0.7,
                vignette: 1.0,
                ..Default::default()
            },
        );
        let mut cache = ReferenceCache::default();
        let first = cache.get(&source, &correction, None);
        correction.manual.vignette = -1.0;
        assert!(Arc::ptr_eq(&first, &cache.get(&source, &correction, None)));
        correction.manual.vignette = 0.0;
        let expected = render_develop(
            &source,
            &DevelopParams::new(None),
            &CurveSet::identity(),
            &DevelopStages {
                lens: Some(&LensMap::new(&correction)),
                ..Default::default()
            },
        );
        assert_eq!(first.rgb, expected);
        correction.manual.distortion = -0.7;
        assert_ne!(first.id, cache.get(&source, &correction, None).id);
    }
}
