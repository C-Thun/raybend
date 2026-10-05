//! Capability negotiation for native SDR wide-gamut presentation. The OS adapter
//! supplies facts; only this adapter chooses a supported swapchain encoding.
use super::presentation::PresentationAdapter;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputSurface {
    pub format: wgpu::TextureFormat,
    pub space: wgpu::SurfaceColorSpace,
    pub sc_rgb_scale: Option<f32>,
}
pub fn negotiate(
    adapter: PresentationAdapter,
    caps: &wgpu::SurfaceCapabilities,
    standard_format: wgpu::TextureFormat,
    system_managed: bool,
    white_nits: Option<f32>,
) -> OutputSurface {
    let white = white_nits.filter(|value| value.is_finite() && (80.0..=10000.0).contains(value));
    if adapter == PresentationAdapter::WindowsComposition
        && system_managed
        && let Some(white) = white
        && caps
            .color_spaces(wgpu::TextureFormat::Rgba16Float)
            .contains(wgpu::SurfaceColorSpaces::EXTENDED_SRGB_LINEAR)
    {
        return OutputSurface {
            format: wgpu::TextureFormat::Rgba16Float,
            space: wgpu::SurfaceColorSpace::ExtendedSrgbLinear,
            sc_rgb_scale: Some(white / 80.0),
        };
    }
    OutputSurface {
        format: standard_format,
        space: wgpu::SurfaceColorSpace::Auto,
        sc_rgb_scale: None,
    }
}

/// SDR white remains the upper luminance bound; negative channels and values above
/// one needed for wide-gamut chromaticity survive. This does not enable HDR editing.
pub fn sdr_sc_rgb(rgb: [f32; 3], scale: f32) -> [f32; 3] {
    let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
    if luma <= 0.0 {
        return [0.0; 3];
    }
    rgb.map(|value| value / luma.max(1.0) * scale)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn capabilities() -> wgpu::SurfaceCapabilities {
        wgpu::SurfaceCapabilities {
            formats: vec![wgpu::TextureFormat::Bgra8UnormSrgb],
            format_capabilities: vec![wgpu::SurfaceFormatCapabilities {
                format: wgpu::TextureFormat::Rgba16Float,
                color_spaces: wgpu::SurfaceColorSpaces::EXTENDED_SRGB_LINEAR,
            }],
            present_modes: vec![],
            alpha_modes: vec![],
            usages: wgpu::TextureUsages::RENDER_ATTACHMENT,
        }
    }
    #[test]
    fn sc_rgb_requires_os_management_valid_white_and_exact_format_space_pair() {
        let mut caps = capabilities();
        let standard = wgpu::TextureFormat::Bgra8UnormSrgb;
        let choose = |caps: &wgpu::SurfaceCapabilities, managed, white| {
            negotiate(
                PresentationAdapter::WindowsComposition,
                caps,
                standard,
                managed,
                white,
            )
        };
        assert_eq!(
            choose(&caps, true, Some(203.0)).sc_rgb_scale,
            Some(203.0 / 80.0)
        );
        for white in [
            None,
            Some(f32::NAN),
            Some(f32::INFINITY),
            Some(0.0),
            Some(79.0),
            Some(10001.0),
        ] {
            assert!(choose(&caps, true, white).sc_rgb_scale.is_none());
        }
        assert!(choose(&caps, false, Some(203.0)).sc_rgb_scale.is_none());
        assert!(
            negotiate(
                PresentationAdapter::PlatformDefault,
                &caps,
                standard,
                true,
                Some(203.0)
            )
            .sc_rgb_scale
            .is_none()
        );
        caps.format_capabilities[0].format = wgpu::TextureFormat::Rgba8Unorm;
        assert!(choose(&caps, true, Some(203.0)).sc_rgb_scale.is_none());
        caps = capabilities();
        caps.format_capabilities[0].color_spaces = wgpu::SurfaceColorSpaces::SRGB;
        assert_eq!(choose(&caps, true, Some(203.0)).format, standard);
    }
    #[test]
    fn wide_gamut_channels_survive_and_sdr_luminance_matches_ui_white() {
        let rgb = [1.2, 0.2, -0.1];
        let result = sdr_sc_rgb(rgb, 203.0 / 80.0);
        assert!(result[0] > 1.0 && result[2] < 0.0);
        assert_eq!(sdr_sc_rgb([1.0; 3], 2.0), [2.0; 3]);
        assert_eq!(sdr_sc_rgb([2.0; 3], 2.0), [2.0; 3]);
        assert_eq!(sdr_sc_rgb([-1.0; 3], 2.0), [0.0; 3]);
    }
}
