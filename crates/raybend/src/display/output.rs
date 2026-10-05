//! Shared output recipe: oriented full source → develop → geometry → optional shrink.
//! Preview caches use the same recipe at their own input resolution.
use crate::develop::curve::{Curve, CurveChannel, CurveSet};
use crate::develop::local_tone::{LocalToneOpts, LocalToneState};
use crate::develop::params::DevelopParams;
use crate::develop::pipeline::{
    DevelopPlans, DevelopStages, LinearImage, chain_image, render_develop16,
};
use crate::store::develop::DevelopStack;
use crate::{Error, Result};
use std::path::Path;
pub type Rgb16Image = image::ImageBuffer<image::Rgb<u16>, Vec<u16>>;

/// Strict full-size source. RAW never falls back to an embedded JPEG or a placeholder.
pub fn decode_linear_source(path: &Path) -> Result<(LinearImage, Option<f32>)> {
    use crate::media::kind::{MediaKind, kind_of_file};
    if path
        .file_name()
        .is_some_and(|name| kind_of_file(&name.to_string_lossy()) == MediaKind::Raw)
    {
        let request = crate::raw::DecodeRequest::full(path).with_preview(false);
        let mut decoded = crate::raw::worker::shared()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .decode_linear(&request)
            .map_err(|e| Error::Unsupported(format!("RAW 完整解码失败：{e}")))?;
        if decoded.source != crate::raw::backend::PixelSource::Decoded {
            return Err(Error::Unsupported("导出不能使用 RAW 内嵌预览".into()));
        }
        decoded.orientation = crate::media::exif::raw_orientation(path, decoded.orientation);
        let shot = decoded.as_shot_temperature;
        return LinearImage::from_raw16(decoded)
            .map(|linear| (linear, shot))
            .ok_or_else(|| Error::Unsupported("RAW 完整像素尺寸不合法".into()));
    }
    let decoded =
        crate::thumbnail::render::decode_file(path, crate::thumbnail::render::DecodeSpec::full())?
            .ok_or_else(|| Error::Unsupported("无法完整解码导出源文件".into()))?;
    let oriented = match decoded.orientation {
        Some(value) if value != 1 => {
            crate::thumbnail::render::apply_orientation(&decoded.image, value)
        }
        _ => decoded.image,
    };
    let rgb = oriented.to_rgb16();
    LinearImage::from_srgb16(rgb.width(), rgb.height(), rgb.as_raw())
        .map(|linear| (linear, None))
        .ok_or_else(|| Error::Unsupported("导出源像素尺寸不合法".into()))
}

/// Long edge 0 preserves the full developed geometry; nonzero limits only shrink.
#[must_use]
pub fn resize_output(image: Rgb16Image, max_edge: u32) -> Rgb16Image {
    let edge = image.width().max(image.height());
    if max_edge == 0 || edge <= max_edge {
        return image;
    }
    let scaled = |n: u32| {
        ((u64::from(n) * u64::from(max_edge) + u64::from(edge) / 2) / u64::from(edge)).max(1) as u32
    };
    image::imageops::resize(
        &image,
        scaled(image.width()),
        scaled(image.height()),
        image::imageops::FilterType::Lanczos3,
    )
}

pub fn render_linear(
    linear: &LinearImage,
    stack: &DevelopStack,
    lens: Option<&crate::develop::lens::LensCorrection>,
    decoded_as_shot_k: Option<f32>,
    lut: Option<&crate::develop::lut::Lut>,
    max_edge: u32,
) -> Result<Rgb16Image> {
    if stack.color.is_some() {
        return Err(Error::Unsupported("新版色彩编辑栈必须使用高精度显影入口".into()));
    }
    if !linear.is_consistent() {
        return Err(Error::Unsupported("显影输入尺寸与像素不一致".into()));
    }
    let (width, height) = (linear.width, linear.height);
    let (params, curves) = parameters_and_curves(stack, decoded_as_shot_k)?;
    // 可选阶段（降噪 / 锐化 / 镜头手动微调）与动态反差：
    // **缩略图也必须反映编辑结果**（M3 的 DoD）——
    // 动态反差的分解在缩略图尺寸上很便宜，不做的话网格与编辑器会显示两张不同的图。
    let plans = DevelopPlans::from_params(width, height, &params);
    let lens_map = {
        // 镜头配置文件（调用方解析后传进来）+ **手动三根拉杆**（从参数里合并）
        let manual = plans.lens.manual;
        let mut correction = lens.cloned().unwrap_or_else(|| {
            crate::develop::lens::LensCorrection::manual_only(width, height, manual)
        });
        correction.manual = manual;
        crate::develop::lens::LensMap::new(&correction)
    };
    let local_state = (plans.local_tone > 0.0).then(|| {
        let chained = chain_image(linear, &params);
        LocalToneState::analyze(&chained, &LocalToneOpts::default())
    });
    let stages = DevelopStages {
        nr_method: stack.nr_method.unwrap_or_default(),
        lens: Some(&lens_map),
        denoise: Some(&plans.denoise),
        local_tone: local_state.as_ref().map(|state| (state, plans.local_tone)),
        sharpen: Some(&plans.sharpen),
        lut,
    };
    let rgb = render_develop16(linear, &params, &curves, &stages);
    let (out_w, out_h, rgb) = match stack.geometry.filter(|geometry| !geometry.is_identity()) {
        Some(geometry) => crate::develop::geometry::apply_rgb((width, height), &rgb, geometry)
            .ok_or_else(|| Error::Unsupported("编辑栈成片几何不合法".to_string()))?,
        None => (width, height, rgb),
    };
    let image = Rgb16Image::from_raw(out_w, out_h, rgb)
        .ok_or_else(|| Error::Unsupported("编辑渲染：输出尺寸对不上".to_string()))?;
    Ok(resize_output(image, max_edge))
}

fn parameters_and_curves(stack: &DevelopStack, decoded_as_shot_k: Option<f32>) -> Result<(DevelopParams, CurveSet)> {
    let params = DevelopParams::from_values(
        stack.params.iter().map(|(id, value)| (id.clone(), *value)),
        decoded_as_shot_k.or(stack.as_shot_k),
    )
    .map_err(|e| Error::Unsupported(format!("编辑栈里的参数不合法：{e}")))?;
    let mut curves = CurveSet::identity();
    if stack.source_base == crate::store::develop::EditBase::Raw
        && let Some(points) = &stack.base_curve_points
    {
        curves.base = Curve::from_points(points.clone())
            .map_err(|e| Error::Unsupported(format!("编辑栈里的基础曲线不合法：{e}")))?;
    }
    for (channel, points) in &stack.curves {
        let Some(channel) = CurveChannel::parse(channel) else {
            return Err(Error::Unsupported(format!(
                "编辑栈里有未知的曲线通道：{channel}"
            )));
        };
        let curve = Curve::from_points(points.clone()).map_err(|e| {
            Error::Unsupported(format!("编辑栈里的曲线不合法（{}）：{e}", channel.as_str()))
        })?;
        curves.set_channel(channel, curve);
    }
    Ok((params, curves))
}

/// Strict V2 source resolution: frozen identity is checked on replay; missing
/// assets or changed embedded metadata never silently fall back to sRGB.
pub fn decode_working_source(
    path: &Path,
    color: &crate::color::PhotoColorState,
    mut profile: impl FnMut(&crate::color::ProfileId, crate::color::icc::IccRole) -> Result<crate::color::icc::RgbIcc>,
) -> Result<(crate::color::working::WorkingImage, Option<f32>)> {
    use crate::color::SourceColor;
    color.validate_frozen().map_err(|e| Error::Unsupported(e.into()))?;
    let unsupported = || Error::Unsupported("照片输入类型、配置身份与固化色彩状态不符".into());
    let raw = path.file_name().is_some_and(|name|
        crate::media::kind::kind_of_file(&name.to_string_lossy()) == crate::media::kind::MediaKind::Raw);
    if raw {
        let SourceColor::RawCameraMatrix { matrix_id } = &color.source else { return Err(unsupported()); };
        let request = crate::raw::DecodeRequest::full(path).with_preview(false).with_camera_matrix(matrix_id.clone());
        let decoded = crate::raw::worker::shared().lock().unwrap_or_else(|e| e.into_inner())
            .decode_working(&request).map_err(|e| Error::Unsupported(format!("RAW 高精度解码失败：{e}")))?;
        if decoded.source != crate::raw::backend::PixelSource::Decoded || &decoded.camera_matrix_id != matrix_id {
            return Err(unsupported());
        }
        let orientation = crate::media::exif::raw_orientation(path, decoded.orientation).unwrap_or(1);
        return Ok((decoded.image.into_oriented(orientation), decoded.as_shot_temperature));
    }
    let bytes = crate::fs_asset::read_limited(path, crate::raw::precheck::MAX_BYTES, "位图")?;
    let decoded = crate::color::input::decode_bitmap(&bytes)
        .map_err(|e| Error::Unsupported(format!("色彩位图解码失败：{e}")))?;
    let input = match &color.source {
        SourceColor::EmbeddedIcc { profile_id } => decoded.embedded_icc.as_ref()
            .filter(|icc| icc.id() == profile_id).cloned().ok_or_else(unsupported)?,
        SourceColor::AssignedRgbIcc { profile_id } => {
            let icc = profile(profile_id, crate::color::icc::IccRole::PhotoInput)?;
            if icc.id() != profile_id { return Err(unsupported()); }
            icc
        }
        SourceColor::TaggedSrgb if decoded.evidence == crate::color::input::SourceEvidence::TaggedPngSrgb =>
            crate::color::icc::srgb_icc().map_err(|e| Error::Unsupported(e.to_string()))?,
        SourceColor::AssumedSrgb if decoded.embedded_icc.is_none() => crate::color::icc::srgb_icc().map_err(|e| Error::Unsupported(e.to_string()))?,
        _ => return Err(unsupported()),
    };
    decoded.working_image_with(&input).map(|image| (image, None))
        .map_err(|e| Error::Unsupported(format!("输入色彩转换失败：{e}")))
}

/// Offline V2 recipe shares parameter, curve and every spatial operator with the
/// existing pipeline. It intentionally is not installed as the realtime path.
pub fn render_working(
    source: &crate::develop::working::WorkingReferenceImage,
    stack: &DevelopStack,
    lens: Option<&crate::develop::lens::LensCorrection>,
    decoded_as_shot_k: Option<f32>,
    lut: Option<&crate::develop::lut::Lut>,
    max_edge: u32,
) -> Result<crate::color::working::WorkingImage> {
    let color = stack.color.as_ref().ok_or_else(|| Error::Unsupported("高精度显影需要固化色彩状态".into()))?;
    stack.validate()?;
    color.validate_frozen().map_err(|e| Error::Unsupported(e.into()))?;
    let (params, curves) = parameters_and_curves(stack, decoded_as_shot_k)?;
    render_working_params(source, &params, &curves, stack.nr_method.unwrap_or_default(), lens, lut, stack.geometry, max_edge)
}

/// Shared prepared recipe for offline export and the explicit V2 editor path.
pub fn render_working_params(
    source: &crate::develop::working::WorkingReferenceImage,
    params: &crate::develop::DevelopParams, curves: &crate::develop::CurveSet,
    nr_method: crate::develop::denoise::NrMethod,
    lens: Option<&crate::develop::lens::LensCorrection>, lut: Option<&crate::develop::lut::Lut>,
    geometry: Option<crate::develop::geometry::EditGeometry>, max_edge: u32,
) -> Result<crate::color::working::WorkingImage> {
    use crate::develop::working::{WorkingTonePlan, render_working_reference};
    let tone = WorkingTonePlan::new(params, curves);
    let plans = DevelopPlans::from_params(source.width, source.height, params);
    let mut correction = lens.cloned().unwrap_or_else(|| crate::develop::lens::LensCorrection::manual_only(source.width, source.height, plans.lens.manual));
    correction.manual = plans.lens.manual;
    let lens_map = crate::develop::lens::LensMap::new(&correction);
    let local = (plans.local_tone > 0.0).then(|| {
        let chained = tone.chain_reference(source);
        LocalToneState::analyze_rgb(crate::develop::sample::RgbView { rgb: &chained, ..source.view() }, &LocalToneOpts::default())
    });
    let stages = DevelopStages { nr_method, lens: Some(&lens_map),
        denoise: Some(&plans.denoise), local_tone: local.as_ref().map(|state| (state, plans.local_tone)),
        sharpen: Some(&plans.sharpen), lut };
    let image = render_working_reference(source, &tone, &stages).map_err(|e| Error::Unsupported(e.to_string()))?;
    let image = match geometry.filter(|geometry| !geometry.is_identity()) {
        Some(geometry) => {
            let (w, h) = image.dimensions();
            let (w, h, rgb) = crate::develop::geometry::apply_rgb((w, h), image.as_view().rgb, geometry)
                .ok_or_else(|| Error::Unsupported("高精度成片几何不合法".into()))?;
            crate::color::working::WorkingImage::from_flat(w, h, rgb).map_err(|e| Error::Unsupported(e.to_string()))?
        }
        None => image,
    };
    Ok(image.into_downscaled(max_edge))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::develop::{
        denoise::NrMethod,
        geometry::{CropRect, EditGeometry},
    };
    use std::collections::{BTreeMap, BTreeSet};

    fn gradient() -> LinearImage {
        LinearImage::new(
            64,
            32,
            (0..2048u32)
                .flat_map(|i| [((i * 23) + 500) as u16; 3])
                .collect(),
        )
        .unwrap()
    }
    #[test]
    fn full_precision_survives_every_display_stage_and_geometry() {
        let source = gradient();
        let original = source.clone();
        let lut = crate::develop::lut::Lut::parse_cube("LUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
        let stack = DevelopStack {
            params: BTreeMap::from([
                ("exposure".into(), 0.2),
                ("sharpenAmount".into(), 35.0),
                ("dynamicContrast".into(), 20.0),
                ("lumaNr".into(), 15.0),
                ("colorNr".into(), 10.0),
                ("vignette".into(), 5.0),
            ]),
            geometry: Some(EditGeometry {
                rotation: 0.0,
                crop: Some(CropRect {
                    x: 0.1,
                    y: 0.1,
                    width: 0.8,
                    height: 0.8,
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let output = render_linear(&source, &stack, None, None, Some(&lut), 0).unwrap();
        assert_eq!(output.dimensions(), (51, 26));
        let levels: BTreeSet<_> = output.as_raw().iter().copied().collect();
        assert!(
            levels.len() > 256,
            "16-bit stages collapsed to {} levels",
            levels.len()
        );
        assert_eq!(source, original);
        let resized = render_linear(&source, &stack, None, None, Some(&lut), 20).unwrap();
        assert_eq!(resized.dimensions(), (20, 10));
        assert_eq!(resize_output(output.clone(), 65535), output);
    }
    #[test]
    fn high_nr_and_curves_preserve_precision() {
        let stack = DevelopStack {
            nr_method: Some(NrMethod::High),
            params: BTreeMap::from([("lumaNr".into(), 10.0)]),
            curves: BTreeMap::from([("rgb".into(), vec![[0.0, 0.0], [0.4, 0.5], [1.0, 1.0]])]),
            ..Default::default()
        };
        let output = render_linear(&gradient(), &stack, None, None, None, 0).unwrap();
        assert!(
            output
                .as_raw()
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                > 256
        );
    }
    #[test]
    fn invalid_source_stack_geometry_and_no_upscale() {
        let bad = LinearImage {
            width: u32::MAX,
            height: u32::MAX,
            rgb: vec![],
        };
        assert!(!bad.is_consistent());
        assert!(render_linear(&bad, &DevelopStack::default(), None, None, None, 0).is_err());
        let source = gradient();
        for stack in [
            DevelopStack {
                params: BTreeMap::from([("unknown".into(), 1.0)]),
                ..Default::default()
            },
            DevelopStack {
                curves: BTreeMap::from([("bad".into(), vec![[0.0, 0.0], [1.0, 1.0]])]),
                ..Default::default()
            },
            DevelopStack {
                geometry: Some(EditGeometry {
                    rotation: f32::NAN,
                    ..Default::default()
                }),
                ..Default::default()
            },
        ] {
            assert!(render_linear(&source, &stack, None, None, None, 0).is_err());
        }
        let image = render_linear(&source, &DevelopStack::default(), None, None, None, 0).unwrap();
        assert_eq!(resize_output(image.clone(), 0), image);
        assert_eq!(resize_output(image.clone(), 100), image);
        let thin = Rgb16Image::new(1, 100);
        assert_eq!(resize_output(thin, 1).dimensions(), (1, 1));
    }
}
