//! Shared display sample adapter; all RGB8/RGB16 spatial math uses this contract.
pub trait RgbSample: Copy + Default + Send + Sync {
    const MAX: f32;
    type Blur: Copy + Default + Send + Sync;
    const BLUR_SCALE: f32;
    fn store_blur(value: f32) -> Self::Blur;
    fn blur_value(value: Self::Blur) -> f32;
    fn value(self) -> f32;
    fn encode(value: f32) -> Self;
}

/// Borrowed interleaved RGB; spatial algorithms share this view across integer legacy
/// samples and floating-point working samples without allocating an adapter image.
#[derive(Clone, Copy)]
pub struct RgbView<'a, T> {
    pub width: u32,
    pub height: u32,
    pub rgb: &'a [T],
}

impl<'a, T> RgbView<'a, T> {
    pub fn new(width: u32, height: u32, rgb: &'a [T]) -> Option<Self> {
        let expected = (width as usize).checked_mul(height as usize)?.checked_mul(3)?;
        (width > 0 && height > 0 && rgb.len() == expected)
            .then_some(Self { width, height, rgb })
    }
}

/// The EXIF orientation table shared by integer and float pixel containers.
pub fn orient_rgb<T: RgbSample>(source: RgbView<'_, T>, orientation: u16) -> (u32, u32, Vec<T>) {
    let (w, h) = (source.width, source.height);
    let (ow, oh) = if matches!(orientation, 5..=8) { (h, w) } else { (w, h) };
    let mut rgb = vec![T::default(); source.rgb.len()];
    for dy in 0..oh {
        for dx in 0..ow {
            let (sx, sy) = match orientation {
                2 => (w - 1 - dx, dy), 3 => (w - 1 - dx, h - 1 - dy),
                4 => (dx, h - 1 - dy), 5 => (dy, dx),
                6 => (dy, h - 1 - dx), 7 => (w - 1 - dy, h - 1 - dx),
                8 => (w - 1 - dy, dx), _ => (dx, dy),
            };
            let from = (sy as usize * w as usize + sx as usize) * 3;
            let to = (dy as usize * ow as usize + dx as usize) * 3;
            rgb[to..to + 3].copy_from_slice(&source.rgb[from..from + 3]);
        }
    }
    (ow, oh, rgb)
}
macro_rules! sample {
    ($t:ty, $max:expr) => {
        impl RgbSample for $t {
            const MAX: f32 = $max;
            type Blur = u16;
            const BLUR_SCALE: f32 = 257.0;
            fn store_blur(value: f32) -> Self::Blur {
                (value * Self::BLUR_SCALE + 0.5).clamp(0.0, 65535.0) as u16
            }
            fn blur_value(value: Self::Blur) -> f32 { f32::from(value) }
            fn value(self) -> f32 {
                f32::from(self)
            }
            fn encode(value: f32) -> Self {
                value.round().clamp(0.0, <Self as RgbSample>::MAX) as Self
            }
        }
    };
}
sample!(u8, 255.0);
sample!(u16, 65535.0);
// Working-space samples retain negative and over-one values until output conversion.
impl RgbSample for f32 {
    const MAX: f32 = 1.0;
    type Blur = f32;
    const BLUR_SCALE: f32 = 1.0;
    fn store_blur(value: f32) -> Self::Blur { value }
    fn blur_value(value: Self::Blur) -> f32 { value }
    fn value(self) -> f32 {
        self
    }
    fn encode(value: f32) -> Self {
        value
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn samples_clamp_round_and_quantize_at_boundaries() {
        assert_eq!(<u16 as RgbSample>::encode(f32::NAN), 0);
        assert_eq!(<u16 as RgbSample>::encode(f32::INFINITY), 65535);
        assert_eq!(<u8 as RgbSample>::encode(-1.0), 0);
        assert_eq!(<u16 as RgbSample>::encode(12345.6), 12346);
        assert_eq!(
            super::super::pipeline::quantize_rgb8(&[0, 128, 129, 257, 65535]),
            [0, 0, 1, 1, 255]
        );
        assert!(super::super::pipeline::LinearImage::from_srgb16(1, 1, &[]).is_none());
        assert!(
            super::super::pipeline::LinearImage::from_srgb16(u32::MAX, u32::MAX, &[]).is_none()
        );
    }
}
