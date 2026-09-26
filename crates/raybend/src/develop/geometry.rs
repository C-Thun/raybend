//! 无损编辑的成片几何。裁切框位于**旋转后的水平画框**坐标；
//! 预览、缩略图和看图使用这一份逆映射，前端只展示 Rust 返回的框。

use serde::{Deserialize, Serialize};

/// 原图宽高归一化的水平成片框。旋转后允许 x/y 小于 0。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl CropRect {
    #[must_use]
    pub fn full() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        }
    }

    #[must_use]
    pub fn corners(self) -> [(f32, f32); 4] {
        [
            (self.x, self.y),
            (self.x + self.width, self.y),
            (self.x + self.width, self.y + self.height),
            (self.x, self.y + self.height),
        ]
    }

    #[must_use]
    pub fn valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
            && self.width <= 2.0
            && self.height <= 2.0
            && self.x.abs() <= 2.0
            && self.y.abs() <= 2.0
    }
}

/// 裁切确认时的面板配置；只影响下次编辑，不参与像素计算。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CropRatioId {
    #[serde(rename = "free")]
    Free,
    #[serde(rename = "original")]
    Original,
    #[serde(rename = "1:1")]
    Square,
    #[serde(rename = "3:2")]
    ThreeTwo,
    #[serde(rename = "2:3")]
    TwoThree,
    #[serde(rename = "4:3")]
    FourThree,
    #[serde(rename = "3:4")]
    ThreeFour,
    #[serde(rename = "16:9")]
    SixteenNine,
    #[serde(rename = "9:16")]
    NineSixteen,
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropRatioSetting {
    pub id: CropRatioId,
    pub width: f64,
    pub height: f64,
}

impl CropRatioSetting {
    #[must_use]
    pub fn valid(self) -> bool {
        self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
            && (0.01..=100.0).contains(&(self.width / self.height))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditGeometry {
    /// 顺时针角度；0 = 不旋转。
    pub rotation: f32,
    /// `None` = 当前角度的最大水平内接展示框。
    pub crop: Option<CropRect>,
    /// 已确认的比例选项；旧编辑栈缺失时按自由比例恢复。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop_ratio: Option<CropRatioSetting>,
}

impl EditGeometry {
    pub fn validate(self, source: (u32, u32)) -> Result<(), String> {
        if !self.rotation.is_finite() || self.rotation.abs() > 360.0 {
            return Err("旋转角度必须是 -360..360 的有限数".into());
        }
        if source.0 == 0 || source.1 == 0 {
            return Err("图像尺寸不能为零".into());
        }
        if let Some(crop) = self.crop
            && (!crop.valid() || !contains_rect(source, self.rotation, crop))
        {
            return Err("裁切框超出旋转后的有像素区域".into());
        }
        if self.crop_ratio.is_some_and(|setting| !setting.valid()) {
            return Err("裁切比例配置无效".into());
        }
        Ok(())
    }

    #[must_use]
    pub fn output_rect(self, source: (u32, u32)) -> CropRect {
        self.crop.unwrap_or_else(|| {
            largest_centered_rect(
                source,
                self.rotation,
                source.0 as f32 / source.1.max(1) as f32,
            )
        })
    }

    #[must_use]
    pub fn output_size(self, source: (u32, u32)) -> (u32, u32) {
        let crop = self.output_rect(source);
        (
            (crop.width * source.0 as f32).round().max(1.0) as u32,
            (crop.height * source.1 as f32).round().max(1.0) as u32,
        )
    }

    #[must_use]
    pub fn is_identity(self) -> bool {
        self.rotation == 0.0 && self.crop.is_none_or(|c| c == CropRect::full())
    }
}

fn sin_cos_degrees(rotation: f32) -> (f32, f32) {
    let quarter = (rotation / 90.0).round();
    if (rotation - quarter * 90.0).abs() < 1e-5 {
        match (quarter as i32).rem_euclid(4) {
            0 => (0.0, 1.0),
            1 => (1.0, 0.0),
            2 => (0.0, -1.0),
            _ => (-1.0, 0.0),
        }
    } else {
        rotation.to_radians().sin_cos()
    }
}

/// 一个水平框的四角都必须在旋转后的原图四边形内部。
/// 用逆旋转检验，避免扫描图像像素或为四条斜边写四份特殊情况。
#[must_use]
pub fn contains_rect(source: (u32, u32), rotation: f32, rect: CropRect) -> bool {
    if source.0 == 0 || source.1 == 0 || !rotation.is_finite() || !rect.valid() {
        return false;
    }
    let (w, h) = (source.0 as f32, source.1 as f32);
    let (sin, cos) = sin_cos_degrees(rotation);
    rect.corners().iter().all(|&(nx, ny)| {
        let x = (nx - 0.5) * w;
        let y = (ny - 0.5) * h;
        let src_x = x * cos + y * sin;
        let src_y = -x * sin + y * cos;
        src_x.abs() <= w * 0.5 + 1.5 && src_y.abs() <= h * 0.5 + 1.5
    })
}

/// 固定成片宽高比时，求旋转图像内能放下的最大居中水平框。
/// 这是 `|cos|·width + |sin|·height <= source_width` 等两条约束的解析解。
#[must_use]
pub fn largest_centered_rect(source: (u32, u32), rotation: f32, aspect: f32) -> CropRect {
    let (w, h) = (source.0.max(1) as f32, source.1.max(1) as f32);
    let ratio = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        w / h
    };
    let (sin, cos) = sin_cos_degrees(rotation);
    let (sin, cos) = (sin.abs(), cos.abs());
    let height = (w / (cos * ratio + sin)).min(h / (sin * ratio + cos));
    let width = ratio * height;
    CropRect {
        x: (w - width) / (2.0 * w),
        y: (h - height) / (2.0 * h),
        width: width / w,
        height: height / h,
    }
}

/// 八把手与框内移动；命中与拖动都在 Rust，不让前端复制边界数学。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CropHandle {
    Nw,
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
    Move,
}

#[must_use]
pub fn hit_crop(rect: CropRect, point: (f32, f32), tolerance: (f32, f32)) -> Option<CropHandle> {
    if !rect.valid() || !point.0.is_finite() || !point.1.is_finite() {
        return None;
    }
    let left = (point.0 - rect.x).abs() <= tolerance.0;
    let right = (point.0 - rect.x - rect.width).abs() <= tolerance.0;
    let top = (point.1 - rect.y).abs() <= tolerance.1;
    let bottom = (point.1 - rect.y - rect.height).abs() <= tolerance.1;
    let mid_x = (point.0 - rect.x - rect.width * 0.5).abs() <= tolerance.0;
    let mid_y = (point.1 - rect.y - rect.height * 0.5).abs() <= tolerance.1;
    match (left, right, top, bottom, mid_x, mid_y) {
        (true, _, true, _, _, _) => Some(CropHandle::Nw),
        (_, true, true, _, _, _) => Some(CropHandle::Ne),
        (_, true, _, true, _, _) => Some(CropHandle::Se),
        (true, _, _, true, _, _) => Some(CropHandle::Sw),
        (_, _, true, _, true, _) => Some(CropHandle::N),
        (_, true, _, _, _, true) => Some(CropHandle::E),
        (_, _, _, true, true, _) => Some(CropHandle::S),
        (true, _, _, _, _, true) => Some(CropHandle::W),
        _ if point.0 >= rect.x
            && point.0 <= rect.x + rect.width
            && point.1 >= rect.y
            && point.1 <= rect.y + rect.height =>
        {
            Some(CropHandle::Move)
        }
        _ => None,
    }
}

/// 从按下时的框和指针出发生成新框；越界时沿本次拖动二分到最后合法位置。
/// `delta` 以原图宽高归一化，ratio 是最终**物理像素**宽高比。
#[must_use]
pub fn drag_crop(
    source: (u32, u32),
    rotation: f32,
    start: CropRect,
    handle: CropHandle,
    delta: (f32, f32),
    ratio: Option<f32>,
) -> CropRect {
    if !contains_rect(source, rotation, start)
        || !delta.0.is_finite()
        || !delta.1.is_finite()
        || ratio.is_some_and(|value| !value.is_finite() || value <= 0.0)
    {
        return start;
    }
    let proposal = |t: f32| {
        let (dx, dy) = (delta.0 * t, delta.1 * t);
        if handle == CropHandle::Move {
            return CropRect {
                x: start.x + dx,
                y: start.y + dy,
                ..start
            };
        }
        let (mut x0, mut y0) = (start.x, start.y);
        let (mut x1, mut y1) = (start.x + start.width, start.y + start.height);
        let west = matches!(handle, CropHandle::Nw | CropHandle::W | CropHandle::Sw);
        let east = matches!(handle, CropHandle::Ne | CropHandle::E | CropHandle::Se);
        let north = matches!(handle, CropHandle::Nw | CropHandle::N | CropHandle::Ne);
        let south = matches!(handle, CropHandle::Sw | CropHandle::S | CropHandle::Se);
        if west {
            x0 += dx;
        }
        if east {
            x1 += dx;
        }
        if north {
            y0 += dy;
        }
        if south {
            y1 += dy;
        }
        if let Some(aspect) = ratio {
            let axis = source.0 as f32 / (aspect * source.1 as f32);
            let horizontal_only = !north && !south;
            let vertical_only = !west && !east;
            let width_drives = horizontal_only
                || (!vertical_only && (dx * source.0 as f32).abs() >= (dy * source.1 as f32).abs());
            if width_drives {
                let height = (x1 - x0) * axis;
                if north {
                    y0 = y1 - height;
                } else if south {
                    y1 = y0 + height;
                } else {
                    let middle = (y0 + y1) * 0.5;
                    y0 = middle - height * 0.5;
                    y1 = middle + height * 0.5;
                }
            } else {
                let width = (y1 - y0) / axis;
                if west {
                    x0 = x1 - width;
                } else if east {
                    x1 = x0 + width;
                } else {
                    let middle = (x0 + x1) * 0.5;
                    x0 = middle - width * 0.5;
                    x1 = middle + width * 0.5;
                }
            }
        }
        CropRect {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        }
    };
    let valid = |candidate: CropRect| {
        candidate.width >= 1.0 / source.0.max(1) as f32
            && candidate.height >= 1.0 / source.1.max(1) as f32
            && contains_rect(source, rotation, candidate)
    };
    let end = proposal(1.0);
    if valid(end) {
        return end;
    }
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    for _ in 0..24 {
        let middle = (low + high) * 0.5;
        if valid(proposal(middle)) {
            low = middle;
        } else {
            high = middle;
        }
    }
    proposal(low)
}

/// 显示域 RGB8 成片像素；预览和缓存图调用同一份采样入口。
/// 输入图像已按 EXIF 摆正。无效尺寸/缓冲区返回 `None`，不构造畸形图片。
#[must_use]
pub fn apply_rgb8(
    source: (u32, u32),
    rgb: &[u8],
    geometry: EditGeometry,
) -> Option<(u32, u32, Vec<u8>)> {
    apply_rgb(source, rgb, geometry)
}

pub fn apply_rgb<T: super::sample::RgbSample>(
    source: (u32, u32),
    rgb: &[T],
    geometry: EditGeometry,
) -> Option<(u32, u32, Vec<T>)> {
    let (w, h) = source;
    if w == 0
        || h == 0
        || (w as usize)
            .checked_mul(h as usize)
            .and_then(|n| n.checked_mul(3))
            != Some(rgb.len())
    {
        return None;
    }
    geometry.validate(source).ok()?;
    if geometry.is_identity() {
        return Some((w, h, rgb.to_vec()));
    }
    let crop = geometry.output_rect(source);
    let out_w = (crop.width * w as f32).round().max(1.0) as u32;
    let out_h = (crop.height * h as f32).round().max(1.0) as u32;
    let capacity = (out_w as usize)
        .checked_mul(out_h as usize)?
        .checked_mul(3)?;
    let mut result = vec![T::default(); capacity];
    let (sin, cos) = sin_cos_degrees(geometry.rotation);
    for y in 0..out_h {
        let frame_y = (crop.y + crop.height * (y as f32 + 0.5) / out_h as f32 - 0.5) * h as f32;
        for x in 0..out_w {
            let frame_x = (crop.x + crop.width * (x as f32 + 0.5) / out_w as f32 - 0.5) * w as f32;
            let sx = frame_x * cos + frame_y * sin + w as f32 * 0.5 - 0.5;
            let sy = -frame_x * sin + frame_y * cos + h as f32 * 0.5 - 0.5;
            let (sx, sy) = (sx.clamp(0.0, (w - 1) as f32), sy.clamp(0.0, (h - 1) as f32));
            let (x0, y0) = (sx.floor() as usize, sy.floor() as usize);
            let (x1, y1) = ((x0 + 1).min(w as usize - 1), (y0 + 1).min(h as usize - 1));
            let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
            let target = (y as usize * out_w as usize + x as usize) * 3;
            for channel in 0..3 {
                let at = |px: usize, py: usize| rgb[(py * w as usize + px) * 3 + channel].value();
                let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
                let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
                result[target + channel] = T::encode(top * (1.0 - fy) + bottom * fy);
            }
        }
    }
    Some((out_w, out_h, result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_crop_ratio_id_from_the_panel_is_serializable() {
        for id in [
            "free", "original", "1:1", "3:2", "2:3", "4:3", "3:4", "16:9", "9:16", "custom",
        ] {
            let json = format!(r#"{{"id":"{id}","width":3,"height":2}}"#);
            let setting: CropRatioSetting = serde_json::from_str(&json).expect(id);
            let roundtrip: CropRatioSetting =
                serde_json::from_str(&serde_json::to_string(&setting).expect("序列化"))
                    .expect("读回");
            assert_eq!(roundtrip, setting);
        }
    }

    #[test]
    fn crop_ratio_setting_round_trips_without_changing_pixels() {
        let old: EditGeometry = serde_json::from_str(
            r#"{"rotation":0,"crop":{"x":0.2,"y":0.2,"width":0.6,"height":0.6}}"#,
        )
        .expect("旧几何记录仍能读");
        assert_eq!(old.crop_ratio, None);
        let saved = EditGeometry {
            crop_ratio: Some(CropRatioSetting {
                id: CropRatioId::Custom,
                width: 5.0,
                height: 4.0,
            }),
            ..old
        };
        let json = serde_json::to_string(&saved).expect("保存");
        assert_eq!(
            serde_json::from_str::<EditGeometry>(&json).expect("读回"),
            saved
        );
        assert_eq!(saved.output_rect((400, 300)), old.output_rect((400, 300)));
        assert_eq!(saved.is_identity(), old.is_identity());
        assert!(
            EditGeometry {
                crop_ratio: Some(CropRatioSetting {
                    id: CropRatioId::Custom,
                    width: 0.0,
                    height: 4.0,
                }),
                ..old
            }
            .validate((400, 300))
            .is_err()
        );
    }

    #[test]
    fn inscribed_rect_stays_on_pixels_across_angles_and_aspects() {
        for source in [(4000, 3000), (3000, 4000), (1, 1)] {
            for angle in [-360.0, -90.0, -33.0, 0.0, 15.0, 45.0, 90.0, 360.0] {
                for aspect in [1.0, 1.5, 2.0 / 3.0] {
                    let rect = largest_centered_rect(source, angle, aspect);
                    assert!(
                        contains_rect(source, angle, rect),
                        "{source:?} {angle}: {rect:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn invalid_values_and_crossing_edges_are_rejected() {
        let source = (400, 300);
        assert!(!contains_rect(source, 30.0, CropRect::full()));
        for rotation in [f32::NAN, f32::INFINITY, 361.0] {
            assert!(
                EditGeometry {
                    rotation,
                    crop: None,
                    crop_ratio: None
                }
                .validate(source)
                .is_err()
            );
        }
        assert!(
            EditGeometry {
                rotation: 0.0,
                crop: Some(CropRect {
                    x: 0.9,
                    y: 0.0,
                    width: 0.2,
                    height: 1.0
                }),
                crop_ratio: None
            }
            .validate(source)
            .is_err()
        );
    }

    #[test]
    fn eight_handles_and_move_respect_rotated_pixel_boundary() {
        let source = (4000, 3000);
        let rotation = 27.0;
        let rect = largest_centered_rect(source, rotation, 1.0);
        let handles = [
            (CropHandle::Nw, (rect.x, rect.y)),
            (CropHandle::N, (rect.x + rect.width * 0.5, rect.y)),
            (CropHandle::Ne, (rect.x + rect.width, rect.y)),
            (
                CropHandle::E,
                (rect.x + rect.width, rect.y + rect.height * 0.5),
            ),
            (CropHandle::Se, (rect.x + rect.width, rect.y + rect.height)),
            (
                CropHandle::S,
                (rect.x + rect.width * 0.5, rect.y + rect.height),
            ),
            (CropHandle::Sw, (rect.x, rect.y + rect.height)),
            (CropHandle::W, (rect.x, rect.y + rect.height * 0.5)),
            (CropHandle::Move, (0.5, 0.5)),
        ];
        for (handle, point) in handles {
            assert_eq!(hit_crop(rect, point, (0.001, 0.001)), Some(handle));
            for delta in [(-1.0, -1.0), (1.0, 1.0), (0.05, -0.1)] {
                let next = drag_crop(source, rotation, rect, handle, delta, Some(1.0));
                assert!(
                    contains_rect(source, rotation, next),
                    "{handle:?} {delta:?}: {next:?}"
                );
                let aspect = next.width * source.0 as f32 / (next.height * source.1 as f32);
                assert!((aspect - 1.0).abs() < 0.01, "{handle:?}: {aspect}");
            }
        }
        assert_eq!(hit_crop(rect, (-5.0, -5.0), (0.001, 0.001)), None);
    }

    #[test]
    fn identity_and_crop_preserve_exact_pixels() {
        let rgb = vec![10, 0, 0, 20, 0, 0, 30, 0, 0, 40, 0, 0];
        assert_eq!(
            apply_rgb8((4, 1), &rgb, EditGeometry::default()),
            Some((4, 1, rgb.clone()))
        );
        let crop = CropRect {
            x: 0.25,
            y: 0.0,
            width: 0.5,
            height: 1.0,
        };
        assert_eq!(
            apply_rgb8(
                (4, 1),
                &rgb,
                EditGeometry {
                    crop: Some(crop),
                    rotation: 0.0,
                    crop_ratio: None
                }
            ),
            Some((2, 1, vec![20, 0, 0, 30, 0, 0]))
        );
        assert!(apply_rgb8((4, 1), &rgb[..11], EditGeometry::default()).is_none());
    }
}
