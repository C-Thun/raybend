//! Shared display sample adapter; all RGB8/RGB16 spatial math uses this contract.
pub trait RgbSample: Copy + Default + Send + Sync {
    const MAX: f32;
    fn value(self) -> f32;
    fn encode(value: f32) -> Self;
}
macro_rules! sample {
    ($t:ty, $max:expr) => {
        impl RgbSample for $t {
            const MAX: f32 = $max;
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
