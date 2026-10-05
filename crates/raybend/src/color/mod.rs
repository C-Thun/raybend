//! 色彩管理的跨管线契约（CM-W1）。这里先描述身份与边界；像素转换从 CM-W2 接入。
//!
//! `SourceColor` 是照片自身的解释依据，`WorkingSpace` 是处理版本固定的数学域，
//! `OutputColor` 是导出目标。显示器配置不属于照片或 issue，见 [`system`]。

pub mod icc;
pub mod display;
pub mod proof;
pub mod defaults;
pub mod assets;
pub mod input;
mod matrix_trc;
mod lcms_input;
pub(crate) mod display_clut;
pub(crate) mod display_native;
#[cfg(test)]
pub(crate) mod icc_fixtures;
pub mod system;
pub mod working;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// 旧定稿在没有色彩字段时必须继续按原来的 sRGB 8-bit 路径解释。
/// 版本切换必须是显式的，不能通过“缺字段就当新管线”改变旧照片外观。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ColorProcessVersion {
    #[default]
    LegacySrgb8V1,
    LinearRec2020V2,
}

/// 一个可携带的配置文件身份；同名或移动位置不改变身份。
/// 只接受 32 字节 SHA-256 的小写十六进制，避免大小写/路径参与缓存键。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProfileId(String);

impl ProfileId {
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        Self(digest.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ProfileId {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("profile id must be 64 lowercase hexadecimal characters");
        }
        Ok(Self(value))
    }
}

impl From<ProfileId> for String {
    fn from(value: ProfileId) -> Self {
        value.0
    }
}

/// 导入的资产类型；RGB ICC 与 RAW 相机配置不能在 UI 中互相冒充。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileKind {
    RgbIcc,
    RawCamera,
}

/// 照片输入解释选择。`Auto` 是自动判定，不等于“强行当作 sRGB”。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceColor {
    #[default]
    Auto,
    EmbeddedIcc {
        profile_id: ProfileId,
    },
    AssignedRgbIcc {
        profile_id: ProfileId,
    },
    AssumedSrgb,
    TaggedSrgb,
    RawCameraMatrix { matrix_id: ProfileId },
    AssignedRawCamera {
        profile_id: ProfileId,
    },
}

/// 固定的处理域。未来若更换白点/原色，必须新增处理版本，不能原地改值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkingSpace {
    LinearRec2020D65,
}

/// 导出成片的目标，和照片的源配置、屏幕配置互不混淆。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OutputColor {
    #[default]
    Srgb,
    DisplayP3,
    AdobeRgb,
    CustomRgbIcc {
        profile_id: ProfileId,
    },
}

/// 纯数据的照片级色彩状态；不包含本机显示器 ID/ICC 路径。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PhotoColorState {
    #[serde(default)]
    pub process_version: ColorProcessVersion,
    #[serde(default)]
    pub source: SourceColor,
}

impl PhotoColorState {
    pub fn validate_frozen(&self) -> Result<(), &'static str> {
        if self.process_version != ColorProcessVersion::LinearRec2020V2 {
            return Err("legacy color state must remain absent from the stack");
        }
        if matches!(self.source, SourceColor::Auto) {
            return Err("input color interpretation must be resolved before saving");
        }
        Ok(())
    }

    /// 新照片在高精度主链真正接通后才显式选用此版本。
    #[must_use]
    pub fn new_pipeline(source: SourceColor) -> Self {
        Self {
            process_version: ColorProcessVersion::LinearRec2020V2,
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_color_state_is_legacy_and_rejects_unknown_fields() {
        let empty: PhotoColorState = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, PhotoColorState::default());
        assert_eq!(empty.process_version, ColorProcessVersion::LegacySrgb8V1);
        assert!(serde_json::from_str::<PhotoColorState>(r#"{"monitor_profile":"x"}"#).is_err());
    }

    #[test]
    fn profile_identity_is_bytes_not_name_and_serialization_is_canonical() {
        let id = ProfileId::of_bytes("ICC\0camera / 中文".as_bytes());
        assert_eq!(id.as_str().len(), 64);
        assert_eq!(
            serde_json::to_string(&id).unwrap(),
            format!("\"{}\"", id.as_str())
        );
        assert_eq!(
            serde_json::from_str::<ProfileId>(&serde_json::to_string(&id).unwrap()).unwrap(),
            id
        );
        for invalid in [
            String::new(),
            "a".into(),
            "A".repeat(64),
            "z".repeat(64),
            "a".repeat(65),
        ] {
            assert!(ProfileId::try_from(invalid).is_err());
        }
    }

    #[test]
    fn source_working_and_output_are_distinct() {
        let id = ProfileId::of_bytes(b"profile");
        let state = PhotoColorState::new_pipeline(SourceColor::AssignedRgbIcc {
            profile_id: id.clone(),
        });
        let output = OutputColor::CustomRgbIcc { profile_id: id };
        assert_eq!(state.process_version, ColorProcessVersion::LinearRec2020V2);
        assert!(
            serde_json::to_string(&state)
                .unwrap()
                .contains("assigned_rgb_icc")
        );
        assert!(
            serde_json::to_string(&output)
                .unwrap()
                .contains("custom_rgb_icc")
        );
    }
}
