//! Exact, prepared LCMS input. Bounded profiles and conversion concurrency;
//! neither CLUT resampling nor integer intermediates change the working pixels.
use super::{
    ProfileId,
    icc::{IccError, IccRole, RgbIcc, linear_rec2020_thread},
};
use lcms2::{DisallowCache, Flags, Intent, PixelFormat, Profile, ThreadContext, Transform};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

type Input<T> = Transform<[T; 3], [f32; 3], ThreadContext, DisallowCache>;
struct Prepared {
    rgb8: Input<u8>,
    rgb16: Input<u16>,
    // LCMS context must outlive transforms. It is not mutated after creation;
    // keep it behind a mutex because ThreadContext deliberately is not Sync.
    _context: Mutex<ThreadContext>,
}
static CACHE: OnceLock<Mutex<VecDeque<(ProfileId, Arc<Prepared>)>>> = OnceLock::new();
// Only one large conversion fans out at a time, including concurrent library jobs.
static PARALLEL: Mutex<()> = Mutex::new(());
const CACHE_CAPACITY: usize = 8;
fn prepared(profile: &RgbIcc) -> Result<Arc<Prepared>, IccError> {
    if !profile.accepts_role(IccRole::PhotoInput) {
        return Err(IccError::WrongRole);
    }
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(i) = cache.iter().position(|(id, _)| id == profile.id()) {
        let entry = cache.remove(i).unwrap();
        let result = entry.1.clone();
        cache.push_back(entry);
        return Ok(result);
    }
    let context = ThreadContext::new();
    let source = Profile::new_icc_context(&context, profile.bytes())
        .map_err(|_| IccError::InvalidProfile)?;
    let target = linear_rec2020_thread(&context)?;
    let flags = Flags::NO_CACHE | Flags::NO_OPTIMIZE;
    let rgb8 = Transform::new_flags_context(
        &context,
        &source,
        PixelFormat::RGB_8,
        &target,
        PixelFormat::RGB_FLT,
        Intent::RelativeColorimetric,
        flags,
    )
    .map_err(|_| IccError::InvalidProfile)?;
    let rgb16 = Transform::new_flags_context(
        &context,
        &source,
        PixelFormat::RGB_16,
        &target,
        PixelFormat::RGB_FLT,
        Intent::RelativeColorimetric,
        flags,
    )
    .map_err(|_| IccError::InvalidProfile)?;
    drop((source, target));
    let result = Arc::new(Prepared {
        rgb8,
        rgb16,
        _context: Mutex::new(context),
    });
    cache.push_back((profile.id().clone(), result.clone()));
    if cache.len() > CACHE_CAPACITY {
        cache.pop_front();
    }
    Ok(result)
}
fn apply<T: Copy + Send + Sync>(
    pixels: &[[T; 3]],
    convert: impl Fn(&[[T; 3]], &mut [[f32; 3]]) + Sync,
) -> Result<Vec<[f32; 3]>, IccError> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(4);
    let parallel = pixels.len() >= 128 * 1024 && threads > 1;
    let mut output = vec![[0.0; 3]; pixels.len()];
    if !parallel {
        convert(pixels, &mut output);
    } else {
        let chunk = pixels.len().div_ceil(threads);
        std::thread::scope(|scope| {
            for (src, dst) in pixels.chunks(chunk).zip(output.chunks_mut(chunk)) {
                let convert = &convert;
                scope.spawn(move || convert(src, dst));
            }
        });
    }
    if output.iter().flatten().any(|v| !v.is_finite()) {
        return Err(IccError::NonFinitePixel);
    }
    Ok(output)
}
pub(super) fn rgb16(profile: &RgbIcc, pixels: &[[u16; 3]]) -> Result<Vec<[f32; 3]>, IccError> {
    // Acquire before preparing/allocating: waiting large jobs hold no output or
    // extra transform Arc outside the bounded cache.
    let _permit =
        (pixels.len() >= 128 * 1024).then(|| PARALLEL.lock().unwrap_or_else(|e| e.into_inner()));
    let prepared = prepared(profile)?;
    apply(pixels, |src, dst| prepared.rgb16.transform_pixels(src, dst))
}
pub(super) fn rgb8(profile: &RgbIcc, pixels: &[[u8; 3]]) -> Result<Vec<[f32; 3]>, IccError> {
    // Acquire before preparing/allocating: waiting large jobs hold no output or
    // extra transform Arc outside the bounded cache.
    let _permit =
        (pixels.len() >= 128 * 1024).then(|| PARALLEL.lock().unwrap_or_else(|e| e.into_inner()));
    let prepared = prepared(profile)?;
    apply(pixels, |src, dst| prepared.rgb8.transform_pixels(src, dst))
}
