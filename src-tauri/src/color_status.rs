//! Actual native presentation, kept separate from the OS detection snapshot.
use raybend::color::{
    display::{DisplayStrategy, PreparedDisplay},
    system::{ColorSystemError, DisplayColorState, SystemOutputSpace},
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayPresentationKind {
    Icc,
    SystemManaged,
    SrgbFallback,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayPresentationReason {
    MissingProfile,
    PreparationFailed,
    LimitedOutput,
    SystemUnavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayPresentation {
    pub kind: DisplayPresentationKind,
    pub display_id: Option<String>,
    pub profile_path: Option<String>,
    pub output_space: SystemOutputSpace,
    pub sdr_white_nits: Option<f32>,
    pub reason: Option<DisplayPresentationReason>,
    pub diagnostic: Option<String>,
    pub generation: Option<u64>,
}

impl DisplayPresentation {
    /// Called only after the renderer installs the transform and negotiates its surface.
    pub fn applied(
        prepared: &Result<PreparedDisplay, ColorSystemError>,
        sc_rgb_active: bool,
    ) -> Self {
        let mut result = Self {
            kind: DisplayPresentationKind::SrgbFallback,
            display_id: None,
            profile_path: None,
            output_space: SystemOutputSpace::Srgb,
            sdr_white_nits: None,
            reason: None,
            diagnostic: None,
            generation: None,
        };
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                result.reason = Some(DisplayPresentationReason::PreparationFailed);
                result.diagnostic = Some(error.to_string());
                return result;
            }
        };
        result.display_id = prepared.snapshot.display_id.clone();
        result.generation = Some(prepared.generation);
        if let DisplayColorState::Icc { profile_path } = &prepared.snapshot.state {
            result.profile_path = Some(profile_path.to_string_lossy().into_owned());
        }
        match &prepared.strategy {
            DisplayStrategy::Icc(_) => result.kind = DisplayPresentationKind::Icc,
            DisplayStrategy::SystemManaged { sdr_white_nits, .. } => {
                result.kind = DisplayPresentationKind::SystemManaged;
                result.sdr_white_nits = *sdr_white_nits;
                if sc_rgb_active {
                    result.output_space = SystemOutputSpace::ScRgb;
                } else {
                    result.reason = Some(DisplayPresentationReason::LimitedOutput);
                }
            }
            DisplayStrategy::SrgbFallback { reason } => {
                result.reason = Some(DisplayPresentationReason::MissingProfile);
                result.diagnostic = Some(reason.clone());
            }
            DisplayStrategy::Unavailable { reason } => {
                result.kind = DisplayPresentationKind::Unavailable;
                result.reason = Some(DisplayPresentationReason::SystemUnavailable);
                result.diagnostic = Some(reason.clone());
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raybend::color::system::DisplaySnapshot;

    fn prepared(state: DisplayColorState, strategy: DisplayStrategy) -> Result<PreparedDisplay, ColorSystemError> {
        Ok(PreparedDisplay {
            generation: u64::MAX,
            snapshot: DisplaySnapshot { display_id: Some("摄影棚屏幕".into()), state },
            strategy,
        })
    }

    #[test]
    fn output_reports_the_negotiated_surface_instead_of_the_requested_space() {
        let request = prepared(
            DisplayColorState::SystemManaged { output_space: SystemOutputSpace::ScRgb, sdr_white_nits: Some(203.0) },
            DisplayStrategy::SystemManaged { space: SystemOutputSpace::ScRgb, sdr_white_nits: Some(203.0) },
        );
        let limited = DisplayPresentation::applied(&request, false);
        assert_eq!(limited.kind, DisplayPresentationKind::SystemManaged);
        assert_eq!(limited.output_space, SystemOutputSpace::Srgb);
        assert_eq!(limited.reason, Some(DisplayPresentationReason::LimitedOutput));
        assert_eq!(limited.generation, Some(u64::MAX));
        let wide = DisplayPresentation::applied(&request, true);
        assert_eq!(wide.output_space, SystemOutputSpace::ScRgb);
        assert_eq!(wide.reason, None);
        assert_eq!(wide.sdr_white_nits, Some(203.0));
        assert_eq!(wide.display_id.as_deref(), Some("摄影棚屏幕"));
    }

    #[test]
    fn fallback_and_unavailable_keep_the_actual_srgb_output_and_diagnostics() {
        for (strategy,kind,reason) in [
            (DisplayStrategy::SrgbFallback { reason: "无配置\nmissing profile".into() }, DisplayPresentationKind::SrgbFallback, DisplayPresentationReason::MissingProfile),
            (DisplayStrategy::Unavailable { reason: "平台未连接".into() }, DisplayPresentationKind::Unavailable, DisplayPresentationReason::SystemUnavailable),
        ] {
            let value = DisplayPresentation::applied(&prepared(DisplayColorState::Unavailable { reason: "检测不可用".into() },strategy),true);
            assert_eq!(value.kind,kind);
            assert_eq!(value.reason,Some(reason));
            assert_eq!(value.output_space,SystemOutputSpace::Srgb);
            assert!(value.diagnostic.as_ref().is_some_and(|s|!s.is_empty()));
        }
        let failure = DisplayPresentation::applied(&Err(ColorSystemError::Unavailable("CLUT error".into())),false);
        assert_eq!(failure.kind,DisplayPresentationKind::SrgbFallback);
        assert_eq!(failure.reason,Some(DisplayPresentationReason::PreparationFailed));
        assert_eq!(failure.generation,None);
        assert_eq!(failure.profile_path,None);
        assert!(failure.diagnostic.unwrap().contains("CLUT error"));
    }

    #[test]
    fn icc_target_and_long_unicode_path_survive_status_serialization() {
        let path = format!("C:/{}显示器.icc","摄影棚/".repeat(150));
        let profile = raybend::color::icc::srgb_icc().unwrap();
        let transform = raybend::color::display::ScreenTransform::from_icc(&profile).unwrap();
        let value = DisplayPresentation::applied(&prepared(
            DisplayColorState::Icc {profile_path:path.clone().into()}, DisplayStrategy::Icc(std::sync::Arc::new(transform)),
        ),false);
        assert_eq!(value.kind,DisplayPresentationKind::Icc);
        assert_eq!(value.profile_path.as_deref(),Some(path.as_str()));
        let json = serde_json::to_value(value).unwrap();
        assert_eq!(json["profilePath"],path);
        assert_eq!(json["kind"],"icc");
        assert_eq!(json["outputSpace"],"srgb");
        assert!(json["reason"].is_null());
    }
}
