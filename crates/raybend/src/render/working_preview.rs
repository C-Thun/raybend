//! V2 live preparation. Global adjustments update only small GPU data. Expensive
//! spatial recipes reuse the shared float reference path, never a per-frame ICC.
use super::RenderImage;
use crate::develop::working::{GpuToneDescription, WorkingReferenceImage, WorkingTonePlan};
use crate::develop::{CurveSet, DevelopParams, DevelopPlans, DevelopStages};
use crate::develop::{
    denoise::NrMethod,
    geometry::EditGeometry,
    lens::{LensCorrection, LensMap},
    lut::Lut,
};
use std::sync::Arc;

pub struct PreparedWorkingFrame {
    pub image: Arc<RenderImage>,
    pub tone: Option<Arc<GpuToneDescription>>,
}

struct TierCache {
    source: Arc<WorkingReferenceImage>,
    neutral: Option<(
        LensCorrection,
        crate::develop::denoise::DenoisePlan,
        Arc<RenderImage>,
    )>,
}
pub struct WorkingPreview {
    full: TierCache,
    preview: Option<(u32, TierCache)>,
    reference: Option<(
        (u32, LensCorrection, Option<EditGeometry>),
        u64,
        Arc<RenderImage>,
    )>,
}

impl WorkingPreview {
    pub fn new(source: WorkingReferenceImage) -> Self {
        Self::from_shared(Arc::new(source))
    }
    pub fn from_shared(source: Arc<WorkingReferenceImage>) -> Self {
        Self {
            full: TierCache {
                source,
                neutral: None,
            },
            preview: None,
            reference: None,
        }
    }
    pub fn tier_source(&mut self, edge: u32) -> Arc<WorkingReferenceImage> {
        self.prepare_tier(edge);
        Arc::clone(if edge == 0 {
            &self.full.source
        } else {
            &self.preview.as_ref().expect("prepared").1.source
        })
    }
    fn prepare_tier(&mut self, edge: u32) {
        if edge > 0 && self.preview.as_ref().is_none_or(|(old, _)| *old != edge) {
            self.preview = Some((
                edge,
                TierCache {
                    source: Arc::new(self.full.source.downscaled_to(edge)),
                    neutral: None,
                },
            ));
        }
    }
    pub fn dimensions(&self) -> (u32, u32) {
        (self.full.source.width, self.full.source.height)
    }

    pub fn render(
        &mut self,
        edge: u32,
        params: &DevelopParams,
        curves: &CurveSet,
        nr_method: NrMethod,
        lens: Option<&LensCorrection>,
        lut: Option<&Lut>,
        geometry: Option<EditGeometry>,
    ) -> crate::Result<PreparedWorkingFrame> {
        if let Some(geometry) = geometry {
            geometry
                .validate(self.dimensions())
                .map_err(crate::Error::Unsupported)?;
        }
        self.prepare_tier(edge);
        let tier = if edge == 0 {
            &mut self.full
        } else {
            &mut self.preview.as_mut().expect("prepared").1
        };
        let plans = DevelopPlans::from_params(tier.source.width, tier.source.height, params);
        let mut correction = lens.cloned().unwrap_or_else(|| {
            LensCorrection::manual_only(tier.source.width, tier.source.height, plans.lens.manual)
        });
        correction.manual = plans.lens.manual;
        let fast = nr_method != NrMethod::High
            && plans.sharpen.is_identity()
            && plans.local_tone == 0.0
            && lut.is_none()
            && geometry.is_none_or(|value| value.is_identity());
        if fast {
            if tier
                .neutral
                .as_ref()
                .is_none_or(|(lens, nr, _)| lens != &correction || nr != &plans.denoise)
            {
                // Release obsolete full-resolution packing before its replacement.
                tier.neutral = None;
                let map = LensMap::new(&correction);
                let image = if map.is_identity() && plans.denoise.is_identity() {
                    RenderImage::from_shared_linear(Arc::clone(&tier.source))
                } else {
                    let neutral = crate::develop::working::render_working_reference(
                        &tier.source,
                        &WorkingTonePlan::new(&DevelopParams::default(), &CurveSet::identity()),
                        &DevelopStages { lens:Some(&map),denoise:Some(&plans.denoise),..Default::default() },
                    ).map_err(|e| crate::Error::Unsupported(e.to_string()))?;
                    RenderImage::from_working(&neutral)
                }.ok_or_else(|| crate::Error::Unsupported("工作图超出 GPU 浮点纹理范围".into()))?;
                tier.neutral = Some((correction, plans.denoise, Arc::new(image)));
            }
            Ok(PreparedWorkingFrame {
                image: Arc::clone(&tier.neutral.as_ref().expect("prepared").2),
                tone: Some(Arc::new(
                    WorkingTonePlan::new(params, curves).gpu_description(),
                )),
            })
        } else {
            let image = crate::display::output::render_working_params(
                &tier.source,
                params,
                curves,
                nr_method,
                lens,
                lut,
                geometry,
                0,
            )?;
            Ok(PreparedWorkingFrame {
                image: Arc::new(RenderImage::from_working(&image).ok_or_else(|| {
                    crate::Error::Unsupported("工作图超出 GPU 浮点纹理范围".into())
                })?),
                tone: None,
            })
        }
    }

    /// Comparison preserves the same source interpretation and geometric correction,
    /// without tone, vignette, noise reduction or sharpening. Cached across slider changes.
    pub fn reference(
        &mut self,
        edge: u32,
        params: &DevelopParams,
        lens: Option<&LensCorrection>,
        geometry: Option<EditGeometry>,
    ) -> crate::Result<(u64, Arc<RenderImage>)> {
        let (width, height) = self.dimensions();
        let mut neutral = DevelopParams::default();
        neutral
            .set("distortion", params.value("distortion"))
            .map_err(crate::Error::Unsupported)?;
        let plans = DevelopPlans::from_params(width, height, &neutral);
        let mut correction = lens
            .cloned()
            .unwrap_or_else(|| LensCorrection::manual_only(width, height, plans.lens.manual));
        correction.manual = plans.lens.manual;
        correction.vignetting = None;
        let key = (edge, correction.clone(), geometry);
        if let Some((cached, id, image)) = &self.reference
            && *cached == key
        {
            return Ok((*id, Arc::clone(image)));
        }
        // Reference must not evict the edited lens/NR source. Prepare independently;
        // when geometry/noise are neutral, both frames share the same f32 allocation.
        self.reference = None;
        self.prepare_tier(edge);
        let tier = if edge == 0 { &self.full } else { &self.preview.as_ref().expect("prepared").1 };
        let image = if geometry.is_none_or(|g| g.is_identity()) && LensMap::new(&correction).is_identity() {
            if let Some((_, nr, image)) = &tier.neutral
                && nr.is_identity() && image.linear_source.is_some() { Arc::clone(image) }
            else { Arc::new(RenderImage::from_shared_linear(Arc::clone(&tier.source)).ok_or_else(|| crate::Error::Unsupported("工作图超出 GPU 浮点纹理范围".into()))?) }
        } else {
            let image = crate::display::output::render_working_params(&tier.source, &neutral, &CurveSet::identity(), NrMethod::Fast, Some(&correction), None, geometry, 0)?;
            Arc::new(RenderImage::from_working(&image).ok_or_else(|| crate::Error::Unsupported("工作图超出 GPU 浮点纹理范围".into()))?)
        };
        let id = crate::develop::reference::next_frame_id();
        self.reference = Some((key, id, Arc::clone(&image)));
        Ok((id, image))
    }

    /// Settled histogram is bounded to a 256px analysis sample and uses the same
    /// recipe and sRGB proxy convention as cache output, never the monitor ICC.
    pub fn histogram_rgb8(
        &self,
        params: &DevelopParams,
        curves: &CurveSet,
        nr_method: NrMethod,
        lens: Option<&LensCorrection>,
        lut: Option<&Lut>,
        geometry: Option<EditGeometry>,
    ) -> crate::Result<Vec<u8>> {
        let sample = self.full.source.downscaled_to(256);
        let image = crate::display::output::render_working_params(
            &sample, params, curves, nr_method, lens, lut, geometry, 0,
        )?;
        let srgb =
            crate::color::icc::srgb_icc().map_err(|e| crate::Error::Unsupported(e.to_string()))?;
        Ok(image
            .to_rgb16_flat(&srgb)
            .map_err(|e| crate::Error::Unsupported(e.to_string()))?
            .into_iter()
            .map(|v| ((u32::from(v) + 128) / 257) as u8)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_does_not_evict_spatial_frame_and_neutral_source_has_no_half_copy() {
        let image=crate::color::working::WorkingImage::new(9,7,vec![[0.2,0.4,0.8];63]).unwrap();
        let mut preview=WorkingPreview::new(WorkingReferenceImage::from_working(image).unwrap());
        let mut params=DevelopParams::default();let curves=CurveSet::identity();
        let first=preview.render(0,&params,&curves,NrMethod::Fast,None,None,None).unwrap();
        assert!(first.image.pixels.is_empty());assert!(first.image.linear_source.is_some());assert!(first.image.is_consistent());
        let (_,reference)=preview.reference(0,&params,None,None).unwrap();assert!(Arc::ptr_eq(&first.image,&reference));
        params.set("lumaNr",35.0).unwrap();
        let denoised=preview.render(0,&params,&curves,NrMethod::Fast,None,None,None).unwrap();
        preview.reference(0,&params,None,None).unwrap();
        let second=preview.render(0,&params,&curves,NrMethod::Fast,None,None,None).unwrap();
        assert!(Arc::ptr_eq(&denoised.image,&second.image));
        let mut packed=super::super::image::HalfUpload::default();first.image.pack_rows(0,7,&mut packed);
        // Direct source packing must match shared reference math, including signed range.
        let expected=crate::develop::working::render_working_reference(&preview.full.source,&WorkingTonePlan::new(&DevelopParams::default(),&curves),&DevelopStages::default()).unwrap();
        assert_eq!(packed.bytes,RenderImage::from_working(&expected).unwrap().pixels);
    }

    #[test]
    fn global_updates_reuse_source_pixels_and_spatial_changes_use_the_shared_recipe() {
        let image = crate::color::working::WorkingImage::from_linear_srgb(
            2,
            1,
            vec![[-0.1, 0.2, 1.5], [0.3, 0.5, 0.7]],
        )
        .unwrap();
        let mut preview = WorkingPreview::new(WorkingReferenceImage::from_working(image).unwrap());
        let mut params = DevelopParams::default();
        let first = preview
            .render(
                0,
                &params,
                &CurveSet::identity(),
                NrMethod::Fast,
                None,
                None,
                None,
            )
            .unwrap();
        params.set("exposure", 1.0).unwrap();
        let adjusted = preview
            .render(
                0,
                &params,
                &CurveSet::identity(),
                NrMethod::Fast,
                None,
                None,
                None,
            )
            .unwrap();
        assert!(Arc::ptr_eq(&first.image, &adjusted.image));
        assert_eq!(
            first.image.encoding,
            super::super::image::RenderEncoding::LinearRec2020Half
        );
        assert!(adjusted.tone.is_some());
        params.set("sharpenAmount", 20.0).unwrap();
        params.set("sharpenRadius", 30.0).unwrap();
        let spatial = preview
            .render(
                0,
                &params,
                &CurveSet::identity(),
                NrMethod::Fast,
                None,
                None,
                None,
            )
            .unwrap();
        assert!(spatial.tone.is_none());
        assert!(!Arc::ptr_eq(&adjusted.image, &spatial.image));
        let offline = crate::display::output::render_working_params(
            &preview.full.source,
            &params,
            &CurveSet::identity(),
            NrMethod::Fast,
            None,
            None,
            None,
            0,
        )
        .unwrap();
        assert_eq!(*spatial.image, RenderImage::from_working(&offline).unwrap());
        assert_eq!(
            preview
                .histogram_rgb8(
                    &params,
                    &CurveSet::identity(),
                    NrMethod::Fast,
                    None,
                    None,
                    None
                )
                .unwrap()
                .len(),
            6
        );
    }
    #[test]
    fn tone_changes_with_noise_or_lens_reuse_the_prepared_spatial_source() {
        let image =
            crate::color::working::WorkingImage::from_linear_srgb(9, 7, vec![[0.1, 0.4, 0.8]; 63])
                .unwrap();
        let mut preview = WorkingPreview::new(WorkingReferenceImage::from_working(image).unwrap());
        let mut params = DevelopParams::default();
        params.set("lumaNr", 35.0).unwrap();
        params.set("distortion", 20.0).unwrap();
        let first = preview
            .render(
                0,
                &params,
                &CurveSet::identity(),
                NrMethod::Fast,
                None,
                None,
                None,
            )
            .unwrap();
        params.set("exposure", 1.0).unwrap();
        let changed = preview
            .render(
                0,
                &params,
                &CurveSet::identity(),
                NrMethod::Fast,
                None,
                None,
                None,
            )
            .unwrap();
        assert!(Arc::ptr_eq(&first.image, &changed.image));
        assert!(changed.tone.is_some());
        params.set("lumaNr", 50.0).unwrap();
        let different = preview
            .render(
                0,
                &params,
                &CurveSet::identity(),
                NrMethod::Fast,
                None,
                None,
                None,
            )
            .unwrap();
        assert!(!Arc::ptr_eq(&changed.image, &different.image));
    }
}
