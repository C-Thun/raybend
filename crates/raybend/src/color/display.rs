//! A prepared screen transform. Built on display invalidation, never per frame.
//! ICC matrix/TRC comes from the very same checked transform as file output.

use super::system::{
    ColorSystemAdapter, ColorSystemError, DisplayColorState, SystemOutputSpace,
    VersionedDisplaySnapshot,
};
use super::{
    icc::{IccError, IccRole, RgbIcc},
    matrix_trc,
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum DisplayStrategy {
    Icc(Arc<ScreenTransform>),
    SystemManaged {
        space: SystemOutputSpace,
        sdr_white_nits: Option<f32>,
    },
    SrgbFallback {
        reason: String,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct PreparedDisplay {
    pub generation: u64,
    pub snapshot: super::system::DisplaySnapshot,
    pub strategy: DisplayStrategy,
}

pub fn prepare_display(
    adapter: &dyn ColorSystemAdapter,
    snapshot: &VersionedDisplaySnapshot,
) -> Result<PreparedDisplay, ColorSystemError> {
    let strategy = match &snapshot.snapshot.state {
        DisplayColorState::Icc { profile_path } => {
            let profile = adapter.read_display_profile(profile_path)?;
            DisplayStrategy::Icc(Arc::new(
                ScreenTransform::from_icc(&profile)
                    .map_err(|e| ColorSystemError::Unavailable(e.to_string()))?,
            ))
        }
        DisplayColorState::SystemManaged {
            output_space,
            sdr_white_nits,
        } => DisplayStrategy::SystemManaged {
            space: *output_space,
            sdr_white_nits: *sdr_white_nits,
        },
        DisplayColorState::SrgbFallback { reason } => DisplayStrategy::SrgbFallback {
            reason: reason.clone(),
        },
        DisplayColorState::Unavailable { reason } => DisplayStrategy::Unavailable {
            reason: reason.clone(),
        },
    };
    Ok(PreparedDisplay {
        generation: snapshot.generation,
        snapshot: snapshot.snapshot.clone(),
        strategy,
    })
}

#[derive(Debug, Clone)]
pub struct ScreenTransform {
    pub(crate) matrix: [[f32; 3]; 3],
    pub(crate) compensation: Arc<Vec<[f32; 4]>>,
    pub(crate) identity_compensation: bool,
    pub(crate) clut: Option<super::display_clut::DisplayClut>,
    pub(crate) native: Option<super::display_native::NativeDisplay>,
}

impl ScreenTransform {
    pub fn from_icc(profile: &RgbIcc) -> Result<Self, IccError> {
        // Profiles are still re-read on every display invalidation. Only identical
        // byte identities reuse the expensive preparation, including rejected bakes.
        type Cache = std::collections::VecDeque<(super::ProfileId, Result<ScreenTransform,IccError>)>;
        static CACHE: std::sync::OnceLock<std::sync::Mutex<Cache>> = std::sync::OnceLock::new();
        let mut cache = CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e|e.into_inner());
        if let Some(index) = cache.iter().position(|(id,_)| id==profile.id()) {
            let entry=cache.remove(index).unwrap(); let result=entry.1.clone();cache.push_back(entry);return result;
        }
        let result=Self::build(profile);
        cache.push_back((profile.id().clone(),result.clone()));
        while cache.len()>8 || cache.iter().map(|(_,v)|v.as_ref().map_or(0,|s|s.compensation.len()*16+s.clut.as_ref().map_or(0,|c|c.samples.len()*16)+s.native.as_ref().map_or(0,|c|c.samples.len()*16))).sum::<usize>() > 48*1024*1024 {
            cache.pop_front();
        }
        result
    }
    fn build(profile: &RgbIcc) -> Result<Self, IccError> {
        if !profile.accepts_role(IccRole::Display) {
            return Err(IccError::WrongRole);
        }
        let Some(transform) = matrix_trc::output_transform(profile)? else {
            if let Some((matrix,native))=super::display_native::NativeDisplay::build(profile)? {
                return Ok(Self {matrix,compensation:Arc::new(Vec::new()),identity_compensation:false,clut:None,native:Some(native)});
            }
            let (matrix, clut) = super::display_clut::DisplayClut::build(profile)?;
            return Ok(Self { matrix, compensation: Arc::new(Vec::new()), identity_compensation: false, clut: Some(clut), native:None });
        };
        let (matrix, compensation) = transform.display_description();
        let steps = (compensation.len() - 1) as f32;
        let encode = crate::develop::color::linear_to_srgb;
        let identity_compensation = compensation.iter().enumerate().all(|(i, pixel)| {
            pixel[..3].iter().all(|value| {
                (encode(*value) - encode((i as f32 / steps).powi(2))).abs() <= 1.0 / 2048.0
            })
        });
        Ok(Self {
            matrix,
            compensation: Arc::new(compensation),
            identity_compensation,
            clut: None,
            native:None,
        })
    }

    pub fn method(&self)->&'static str {
        if self.native.is_some() {"native-mBA"} else if self.clut.is_some() {"sampled-PCS-CLUT"} else {"matrix-TRC"}
    }

    /// CPU inspection of exactly the description consumed by WGSL.
    pub fn surface_linear(&self, working: [f32; 3]) -> [f32; 3] {
        let linear = crate::develop::color::mat3_vec3(self.matrix, working);
        if let Some(native)=&self.native {return native.surface_linear(linear);}
        if let Some(clut) = &self.clut { return clut.surface_linear(linear); }
        if self.identity_compensation {
            return linear.map(|v| v.clamp(0.0, 1.0));
        }
        std::array::from_fn(|c| {
            let position = linear[c].clamp(0.0, 1.0).sqrt() * (self.compensation.len() - 1) as f32;
            let low = position as usize;
            let high = (low + 1).min(self.compensation.len() - 1);
            self.compensation[low][c]
                + (self.compensation[high][c] - self.compensation[low][c]) * (position - low as f32)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_srgb_transfer_avoids_redundant_screen_lookups() {
        for profile in [
            super::super::icc::srgb_icc().unwrap(),
            super::super::icc::display_p3_icc().unwrap(),
        ] {
            assert!(
                ScreenTransform::from_icc(&profile)
                    .unwrap()
                    .identity_compensation
            );
        }
        assert!(
            !ScreenTransform::from_icc(&super::super::icc::adobe_rgb_icc().unwrap())
                .unwrap()
                .identity_compensation
        );
    }
    #[test]
    fn screen_description_matches_independent_lcms_file_transform() {
        for profile in [
            super::super::icc::srgb_icc().unwrap(),
            super::super::icc::display_p3_icc().unwrap(),
            super::super::icc::adobe_rgb_icc().unwrap(),
        ] {
            let transform = ScreenTransform::from_icc(&profile).unwrap();
            let mut samples = vec![[-0.4, 0.2, 1.6], [1.7, -0.2, 0.0]];
            for i in 0..2048 {
                let v = i as f32 / 2047.0;
                samples.push([
                    v,
                    ((i * 701) % 2048) as f32 / 2047.0,
                    ((i * 137) % 2048) as f32 / 2047.0,
                ]);
                samples.push([v * 0.005; 3]);
            }
            let reference =
                super::super::icc::working_to_output_rgb16_reference(&profile, &samples).unwrap();
            for (input, expected) in samples.iter().zip(reference) {
                let displayed = transform.surface_linear(*input).map(|v| {
                    (crate::develop::color::linear_to_srgb(v).clamp(0.0, 1.0) * 65535.0).round()
                        as u16
                });
                for c in 0..3 {
                    assert!(
                        displayed[c].abs_diff(expected[c]) <= 128,
                        "{} {input:?} {displayed:?} {expected:?}",
                        profile.id().as_str()
                    );
                }
            }
        }
    }
}
