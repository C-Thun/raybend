//! Explicit V2 adaptation of the shared editor worker, not a second pixel pipeline.
use super::*;
use raybend::color::{
    PhotoColorState, SourceColor,
    icc::{IccRole, RgbIcc},
};
use raybend::render::working_preview::WorkingPreview;

#[derive(Debug)]
pub(super) struct Input {
    pub state: PhotoColorState,
    profile: Option<RgbIcc>,
}

pub(super) fn prepare<R: Runtime>(
    app: &AppHandle<R>,
    state: PhotoColorState,
) -> Result<Arc<Input>, String> {
    state.validate_frozen().map_err(str::to_owned)?;
    let profile = match &state.source {
        SourceColor::AssignedRgbIcc { profile_id } => Some(
            crate::color_profiles::resolve(app, profile_id, IccRole::PhotoInput)
                .map_err(|e| e.to_string())?,
        ),
        _ => None,
    };
    Ok(Arc::new(Input { state, profile }))
}

pub(super) fn saved<R: Runtime>(
    app: &AppHandle<R>,
    path: &Path,
) -> Result<Option<Arc<Input>>, String> {
    let Some(asset) = crate::develop::resolve_asset(app, path) else {
        return Ok(None);
    };
    let catalog = app
        .state::<crate::browse::BrowseState>()
        .lease(app, &asset.repository_id)?;
    let color = catalog
        .read(|conn| raybend::store::develop::load(conn, asset.asset_id))
        .map_err(|e| e.to_string())?
        .color;
    color.map(|color| prepare(app, color)).transpose()
}

pub(super) struct Cached {
    path: String,
    color: PhotoColorState,
    signature: Option<String>,
    storage: Option<raybend::store::session::TaskAccess>,
    preview: WorkingPreview,
    original: (u32, u32),
    shot: Option<f32>,
    sooc: Option<std::path::PathBuf>,
    reference: ReferenceCache,
    revision: u64,
    high_preview: Option<(DenoiseKey, WorkingPreview)>,
}

pub(super) fn run(
    cached: &mut Option<Cached>,
    wanted: &Option<String>,
    job: &DevelopJob,
    max_texture: u32,
    high: &mut raybend::develop::denoise_job::WorkingHighDenoise,
) -> Option<DevelopOutcome> {
    let path = wanted.as_deref()?;
    let input = job.color.as_ref()?;
    let storage = job
        .transition
        .as_ref()
        .and_then(|plan| plan._permit.as_ref().map(|permit| permit.access()))
        .or_else(|| cached.as_ref().and_then(|source| source.storage.clone()));
    let _permit = storage
        .as_ref()
        .map(|access| access.enter())
        .transpose()
        .ok()?;
    let started = Instant::now();
    let mut decode_ms = None;
    let mut shot = None;
    let mut working_reference = None;
    let mut reference = None;
    let mut reference_base = None;
    let mut nr_pending = false;
    let mut nr_error = None;
    let result = (|| -> Result<DevelopedImage, String> {
        let signature =
            if job.photo.is_some() || cached.as_ref().is_none_or(|entry| entry.path != path) {
                Some(
                    raybend::media::source::source_signature(Path::new(path))
                        .map_err(|e| e.to_string())?,
                )
            } else {
                cached.as_ref().and_then(|entry| entry.signature.clone())
            };
        if cached.as_ref().is_none_or(|entry| {
            entry.path != path
                || entry.color != input.state
                || entry.signature != signature
                || job.photo.is_some()
        }) {
            *cached = None;
            high.clear();
            let begin = Instant::now();
            let (image, decoded_shot) = raybend::display::output::decode_working_source(
                Path::new(path),
                &input.state,
                |id, role| {
                    input
                        .profile
                        .as_ref()
                        .filter(|profile| profile.id() == id && profile.accepts_role(role))
                        .cloned()
                        .ok_or_else(|| raybend::Error::Unsupported("固化输入 ICC 不可用".into()))
                },
            )
            .map_err(|e| e.to_string())?;
            let original = image.dimensions();
            let source = raybend::develop::working::WorkingReferenceImage::from_working(
                image.into_downscaled(max_texture),
            )
            .map_err(|e| e.to_string())?;
            *cached = Some(Cached {
                path: path.into(),
                color: input.state.clone(),
                signature,
                storage,
                preview: WorkingPreview::new(source),
                original,
                shot: decoded_shot,
                sooc: job.transition.as_ref().and_then(|plan| plan.sooc.clone()),
                reference: ReferenceCache::default(),
                revision: raybend::develop::reference::next_frame_id(),
                high_preview: None,
            });
            decode_ms = Some(begin.elapsed().as_secs_f64() * 1000.0);
        }
        let source = cached.as_mut().expect("decoded");
        shot = source.shot;
        let edge = if job.tier == ImageTier::Preview {
            PixelSize::SCREEN_EDGE
        } else {
            0
        };
        if !job.interactive {
            if let Some(frame) = job
                .prefer_sooc
                .then_some(source.sooc.as_deref())
                .flatten()
                .and_then(|path| source.reference.get_sooc(path, job.geometry))
            {
                reference = Some(frame);
                reference_base = Some("sooc");
            } else {
                working_reference = Some(
                    source
                        .preview
                        .reference(edge, &job.params, job.lens.as_ref(), job.geometry)
                        .map_err(|e| e.to_string())?,
                );
                reference_base = Some(
                    if raybend::media::kind::kind_of_file(path)
                        == raybend::media::kind::MediaKind::Raw
                    {
                        "raw"
                    } else {
                        "sooc"
                    },
                );
            }
        }
        let plans = DevelopPlans::from_params(source.original.0, source.original.1, &job.params);
        let mut correction = job.lens.clone().unwrap_or_else(|| {
            LensCorrection::manual_only(source.original.0, source.original.1, plans.lens.manual)
        });
        correction.manual = plans.lens.manual;
        let high_frame = if job.nr_method == NrMethod::High && !plans.denoise.is_identity() {
            let tier = source.preview.tier_source(edge);
            let key = DenoiseKey {
                source: source.revision,
                width: tier.width,
                height: tier.height,
                plan: plans.denoise,
                lens: correction,
            };
            match high.get_or_request(key.clone(), &tier, !job.interactive) {
                Some(Ok(image)) => {
                    if source
                        .high_preview
                        .as_ref()
                        .is_none_or(|(old, _)| old != &key)
                    {
                        source.high_preview = Some((key, WorkingPreview::from_shared(image)));
                    }
                    let mut params = job.params.clone();
                    for id in [
                        "lumaNr",
                        "colorNr",
                        "distortion",
                        "vignette",
                        "vignetteRange",
                        "chromatic",
                    ] {
                        params.clear(id);
                    }
                    Some(
                        source
                            .high_preview
                            .as_mut()
                            .unwrap()
                            .1
                            .render(
                                0,
                                &params,
                                &job.curves,
                                NrMethod::Fast,
                                None,
                                job.lut.as_deref(),
                                job.geometry,
                            )
                            .map_err(|e| e.to_string())?,
                    )
                }
                Some(Err(error)) => {
                    nr_error = Some(error);
                    None
                }
                None => {
                    nr_pending = true;
                    None
                }
            }
        } else {
            high.cancel();
            None
        };
        let frame = match high_frame {
            Some(frame) => frame,
            None => source
                .preview
                .render(
                    edge,
                    &job.params,
                    &job.curves,
                    NrMethod::Fast,
                    job.lens.as_ref(),
                    job.lut.as_deref(),
                    job.geometry,
                )
                .map_err(|e| e.to_string())?,
        };
        let size = job.geometry.map_or(source.original, |geometry| {
            geometry.output_size(source.original)
        });
        Ok(DevelopedImage {
            width: frame.image.width,
            height: frame.image.height,
            pixels: DevelopedPixels::Working(frame),
            source_width: size.0,
            source_height: size.1,
            original_width: source.original.0,
            original_height: source.original.1,
        })
    })();
    let histogram = if result.is_ok() && !job.interactive {
        cached.as_ref().and_then(|source| {
            let mut params = job.params.clone();
            let mut lens = job.lens.as_ref();
            let has_noise =
                !DevelopPlans::from_params(source.original.0, source.original.1, &job.params)
                    .denoise
                    .is_identity();
            let preview = if job.nr_method == NrMethod::High
                && has_noise
                && !nr_pending
                && nr_error.is_none()
                && let Some((_, preview)) = &source.high_preview
            {
                for id in [
                    "lumaNr",
                    "colorNr",
                    "distortion",
                    "vignette",
                    "vignetteRange",
                    "chromatic",
                ] {
                    params.clear(id);
                }
                lens = None;
                preview
            } else {
                &source.preview
            };
            preview
                .histogram_rgb8(
                    &params,
                    &job.curves,
                    NrMethod::Fast,
                    lens,
                    job.lut.as_deref(),
                    job.geometry,
                )
                .map(|rgb| raybend::display::histogram_of_rgb8(&rgb, display::DEFAULT_BINS).into())
                .map_err(|e| eprintln!("[color] histogram: {e}"))
                .ok()
        })
    } else {
        None
    };
    Some(DevelopOutcome {
        working_reference,
        nr_pending,
        nr_error,
        reference,
        reference_base,
        id: job.id,
        rev: job.rev,
        path: path.into(),
        tier: job.tier,
        origin: "working-rec2020-v2".into(),
        transition: false,
        as_shot_temperature: shot,
        decode_ms,
        develop_ms: started.elapsed().as_secs_f64() * 1000.0 - decode_ms.unwrap_or(0.0),
        histogram,
        result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn high_denoise_settles_in_background_and_global_updates_reuse_its_pixels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("浮点降噪.png");
        image::RgbImage::from_fn(9, 9, |x, y| {
            image::Rgb([40 + (x * 3) as u8, 70 + (y * 2) as u8, 220])
        })
        .save(&path)
        .unwrap();
        let wanted = Some(path.to_string_lossy().into_owned());
        let mut job = super::super::tests::job(11, 1, Some(path.to_str().unwrap()));
        job.color = Some(Arc::new(Input {
            state: PhotoColorState::new_pipeline(SourceColor::AssumedSrgb),
            profile: None,
        }));
        job.nr_method = NrMethod::High;
        job.params.set("lumaNr", 40.0).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut high = raybend::develop::denoise_job::WorkingHighDenoise::new(move || {
            let _ = tx.send(());
        });
        let mut cached = None;
        let first = run(&mut cached, &wanted, &job, 8192, &mut high).unwrap();
        assert!(first.nr_pending);
        assert!(first.result.is_ok());
        assert!(first.nr_error.is_none());
        rx.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
        job.photo = None;
        let ready = run(&mut cached, &wanted, &job, 8192, &mut high).unwrap();
        assert!(!ready.nr_pending);
        assert!(ready.nr_error.is_none());
        assert!(ready.histogram.is_some());
        let DevelopedPixels::Working(first) = ready.result.unwrap().pixels else {
            panic!("legacy frame")
        };
        job.params.set("exposure", 0.5).unwrap();
        let again = run(&mut cached, &wanted, &job, 8192, &mut high).unwrap();
        assert!(again.decode_ms.is_none());
        assert!(!again.nr_pending);
        let DevelopedPixels::Working(again) = again.result.unwrap().pixels else {
            panic!("legacy frame")
        };
        assert!(Arc::ptr_eq(&first.image, &again.image));
        job.params.clear("lumaNr");
        let neutral = run(&mut cached, &wanted, &job, 8192, &mut high).unwrap();
        assert!(!neutral.nr_pending);
        assert!(neutral.histogram.is_some());
    }
    #[test]
    fn frozen_photo_uses_float_gpu_frames_and_parameter_updates_reuse_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("色彩原片.png");
        image::RgbImage::from_pixel(3, 2, image::Rgb([250, 32, 120]))
            .save(&path)
            .unwrap();
        let mut job = super::super::tests::job(1, 1, Some(path.to_str().unwrap()));
        job.color = Some(Arc::new(Input {
            state: PhotoColorState::new_pipeline(SourceColor::AssumedSrgb),
            profile: None,
        }));
        let mut cached = None;
        let mut high = raybend::develop::denoise_job::WorkingHighDenoise::new(|| {});
        let wanted = Some(path.to_string_lossy().into_owned());
        let first = run(&mut cached, &wanted, &job, 8192, &mut high).unwrap();
        assert!(first.histogram.is_some());
        let first_reference = first.working_reference.as_ref().unwrap().clone();
        let first = first.result.unwrap();
        let DevelopedPixels::Working(first) = first.pixels else {
            panic!("legacy frame");
        };
        job.photo = None;
        job.params.set("exposure", 1.0).unwrap();
        let second = run(&mut cached, &wanted, &job, 8192, &mut high).unwrap();
        assert!(second.decode_ms.is_none());
        let second_reference = second.working_reference.as_ref().unwrap();
        assert_eq!(first_reference.0, second_reference.0);
        assert!(Arc::ptr_eq(&first_reference.1, &second_reference.1));
        let DevelopedPixels::Working(second) = second.result.unwrap().pixels else {
            panic!("legacy frame");
        };
        assert!(Arc::ptr_eq(&first.image, &second.image));
        assert!(second.tone.is_some());
    }
}
