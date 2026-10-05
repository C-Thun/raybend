//! Relative colorimetric RGB proof: clip in target device gamut, return to the
//! working space, then let the independent window display transform present it.
//! Preparation is validated against two Little CMS reference transforms. CLUT
//! and paper/ink simulation are rejected until their own interpolation is tested.
use super::{ProfileId,icc::{RgbIcc,IccError,IccRole},matrix_trc};
use crate::develop::color::{mat3_vec3,mat3_inverse};

#[derive(Debug,Clone)]
pub struct ProofTransform {
    pub target_id: ProfileId,
    pub(crate) forward: [[f32;3];3],
    pub(crate) backward: [[f32;3];3],
}
impl ProofTransform {
    pub fn from_icc(target: &RgbIcc) -> Result<Self,IccError> {
        if !target.accepts_role(IccRole::RgbOutput) { return Err(IccError::WrongRole); }
        let forward=matrix_trc::output_transform(target)?.ok_or(IccError::UnsupportedGpuTransform)?.proof_matrix();
        let backward=mat3_inverse(forward).ok_or(IccError::UnsupportedGpuTransform)?;
        let candidate=Self {target_id:target.id().clone(),forward,backward};
        let mut samples=Vec::new();
        for r in 0..9 { for g in 0..9 { for b in 0..9 { samples.push([r,g,b].map(|v|v as f32/8.0)); }}}
        samples.extend([[-0.2,0.3,1.5],[2.0,-0.1,0.5],[0.00001;3],[0.004;3]]);
        let encoded=super::icc::working_to_output_rgb16_reference(target,&samples)?;
        let expected=super::icc::input_rgb16_to_working_reference(target,&encoded)?;
        for (sample,expected) in samples.into_iter().zip(expected) {
            let (actual,_)=candidate.simulate(sample);
            if actual.into_iter().zip(expected).any(|(a,b)| (a-b).abs()>0.002) {
                return Err(IccError::UnsupportedGpuTransform);
            }
        }
        Ok(candidate)
    }
    pub fn simulate(&self,working: [f32;3]) -> ([f32;3],bool) {
        let device=mat3_vec3(self.forward,working);
        let warning=device.iter().any(|v| *v < -0.00001 || *v > 1.00001);
        (mat3_vec3(self.backward,device.map(|v|v.clamp(0.0,1.0))),warning)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rgb_proof_preserves_inside_gamut_and_flags_only_output_gamut() {
        for profile in [super::super::icc::srgb_icc().unwrap(),super::super::icc::display_p3_icc().unwrap(),super::super::icc::adobe_rgb_icc().unwrap()] {
            let transform=ProofTransform::from_icc(&profile).unwrap();
            let (gray,warning)=transform.simulate([0.18;3]);
            assert!(!warning);
            for v in gray { assert!((v-0.18).abs()<0.00001); }
            let (clipped,warning)=transform.simulate([-0.2,0.8,2.0]);
            assert!(warning); assert!(clipped.iter().all(|v|v.is_finite()));
        }
        let srgb=super::super::icc::srgb_icc().unwrap();
        let p3=super::super::icc::display_p3_icc().unwrap();
        let sample=super::super::icc::input_rgb8_to_working(&p3,&[[255,0,0]]).unwrap()[0];
        assert!(ProofTransform::from_icc(&srgb).unwrap().simulate(sample).1);
        assert!(!ProofTransform::from_icc(&p3).unwrap().simulate(sample).1);
    }
}
